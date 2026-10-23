//! Tauri commands: app info, workspace load / validate / save (ARC-35), defaults for new
//! objects, edits whose side effects the UI shows before saving (IXD-14, IXD-20), and local
//! checks for the server form (IXD-17), project creation and copy, and effective variables
//! (IXD-07, IXD-13), pipeline and step editing helpers (IXD-08–11), local shell lookup (STEP-02),
//! and the run preview (D03).
//! Drafts to validate or save arrive as raw JSON so type errors can be reported with their
//! field path; the edit helpers take typed drafts the UI built from a loaded workspace.

use serde::Serialize;
use tauri::State;
use ts_rs::TS;

use mdm_core::keyfile::KEY_HEAD_LEN;
use mdm_core::resolve::{VariableView, variable_views as resolve_variable_views};
use mdm_core::template::{BuiltinVariable, builtin_variables as template_builtins};
use mdm_core::wsl::parse_wsl_list;
use mdm_core::{
    Environment, EnvironmentId, KeyFormat, Pipeline, PlanRequest, Project, ProjectId, Reference,
    ResolvedPlan, Server, ServerEdit, ServerId, Shell, Step, StepKind, ValidationReport, Variable,
    VariableId, VariableValue, Workspace, build_plan, check_workspace_json, defaults,
    detect_key_format, edit, parse_workspace, validate_workspace,
};
use mdm_exec::resolve_shell;
use mdm_store::LoadOutcome;

use crate::connection::signature;
use crate::error::{AppError, AppErrorKind};
use crate::state::AppState;
use crate::values::ProcessValues;

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppInfo {
    pub version: String,
    pub data_dir: String,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LoadedWorkspace {
    pub workspace: Workspace,
    /// Set once when this load created the workspace or migrated it from an older schema.
    pub notice: Option<LoadNotice>,
}

#[derive(Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum LoadNotice {
    Created,
    Migrated { from: u32 },
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        data_dir: state.data_dir.root().display().to_string(),
    }
}

#[tauri::command]
pub fn load_workspace(state: State<'_, AppState>) -> Result<LoadedWorkspace, AppError> {
    let mut store = state.store()?;
    let (workspace, outcome) = store.load()?;
    let notice = match outcome {
        LoadOutcome::Loaded => None,
        LoadOutcome::Created => Some(LoadNotice::Created),
        LoadOutcome::Migrated { from } => Some(LoadNotice::Migrated { from }),
    };
    Ok(LoadedWorkspace { workspace, notice })
}

/// Every place a sensitive value is kept: variables whose value is a `secretRef`, and
/// environment overrides with a `secretRef`, keyed by the variable they override (ARC-10).
fn secret_slots(ws: &Workspace) -> Vec<(VariableId, Option<EnvironmentId>)> {
    let mut slots: Vec<_> = ws
        .global_variables
        .iter()
        .chain(ws.projects.iter().flat_map(|p| p.variables.iter()))
        .filter(|v| matches!(v.value, VariableValue::SecretRef))
        .map(|v| (v.id, None))
        .collect();
    for project in &ws.projects {
        for o in &project.overrides {
            if !matches!(o.value, VariableValue::SecretRef) {
                continue;
            }
            let base = project
                .variables
                .iter()
                .chain(&ws.global_variables)
                .find(|v| v.name == o.name);
            if let Some(base) = base {
                slots.push((base.id, Some(o.environment_id)));
            }
        }
    }
    slots
}

/// Check a draft without saving. Always succeeds; problems are in the report.
#[tauri::command]
pub fn validate_workspace_draft(draft: serde_json::Value) -> ValidationReport {
    match parse_workspace(draft) {
        Ok(ws) => validate_workspace(&ws),
        Err(report) => report,
    }
}

/// Validate and save a draft. The draft's `revision` must match the file on disk.
#[tauri::command]
pub fn save_workspace(
    state: State<'_, AppState>,
    draft: serde_json::Value,
) -> Result<Workspace, AppError> {
    let ws = check_workspace_json(draft).map_err(AppError::validation)?;
    check_active_run_servers(&state, &ws)?;
    let mut store = state.store()?;
    let (before, _) = store.load()?;
    let saved = store.save(ws)?;
    drop(store);
    state.drop_orphan_secrets(&before, &saved);
    Ok(saved)
}

/// While a run is active, its servers cannot be deleted, moved to another environment, or have
/// their connection fields changed (AC-05, IXD-20). Name, tags and notes stay editable.
fn check_active_run_servers(state: &AppState, ws: &Workspace) -> Result<(), AppError> {
    let active = state.active.lock().unwrap_or_else(|e| e.into_inner());
    let Some(run) = active.as_ref() else {
        return Ok(());
    };
    for frozen in &run.servers {
        let message = match ws.servers.iter().find(|s| s.id == frozen.id) {
            None => "删除",
            Some(s) if s.environment_id != frozen.environment_id => "更换环境",
            Some(s) if signature(s) != signature(frozen) => "修改连接设置",
            Some(_) => continue,
        };
        return Err(AppError::new(
            AppErrorKind::RunActive,
            format!(
                "服务器 {} 正被当前运行使用，运行结束后才能{message}",
                frozen.name
            ),
        ));
    }
    Ok(())
}

/// Open the data directory in Explorer (IXD-36).
#[tauri::command]
pub fn open_data_dir(state: State<'_, AppState>) -> Result<(), AppError> {
    std::process::Command::new("explorer")
        .arg(state.data_dir.root())
        .spawn()
        .map(|_| ())
        .map_err(|err| {
            AppError::new(
                AppErrorKind::StorageUnavailable,
                format!("无法打开数据目录：{err}"),
            )
        })
}

/// A server with SRV-01 defaults in the given environment.
#[tauri::command]
pub fn new_server(environment_id: EnvironmentId) -> Server {
    defaults::new_server(environment_id)
}

#[tauri::command]
pub fn new_environment() -> Environment {
    defaults::new_environment("")
}

#[tauri::command]
pub fn new_variable() -> Variable {
    defaults::new_variable()
}

/// What keeps an environment from being deleted.
#[tauri::command]
pub fn environment_references(draft: Workspace, environment_id: EnvironmentId) -> Vec<Reference> {
    edit::environment_references(&draft, environment_id)
}

/// Project bindings of a server, shown before deleting it.
#[tauri::command]
pub fn server_references(draft: Workspace, server_id: ServerId) -> Vec<Reference> {
    edit::server_references(&draft, server_id)
}

/// Insert or replace a server in the draft; reports bindings an environment change removes.
/// The server form arrives as raw JSON so a mistyped field fails as a validation issue.
#[tauri::command]
pub fn put_server(draft: Workspace, server: serde_json::Value) -> Result<ServerEdit, AppError> {
    edit::put_server_json(draft, server).map_err(AppError::validation)
}

/// Remove a server and its bindings from the draft.
#[tauri::command]
pub fn remove_server(draft: Workspace, server_id: ServerId) -> ServerEdit {
    edit::remove_server(draft, server_id)
}

/// Recognise the private key file at `path` from its first bytes (IXD-17).
#[tauri::command]
pub fn inspect_key_file(path: String) -> Result<KeyFormat, AppError> {
    use std::io::Read;
    let mut head = Vec::with_capacity(KEY_HEAD_LEN);
    std::fs::File::open(&path)
        .and_then(|f| f.take(KEY_HEAD_LEN as u64).read_to_end(&mut head))
        .map_err(|err| {
            let message = if err.kind() == std::io::ErrorKind::NotFound {
                format!("找不到私钥文件：{path}")
            } else {
                format!("无法读取私钥文件：{path}（{err}）")
            };
            AppError::new(AppErrorKind::StorageUnavailable, message)
        })?;
    Ok(detect_key_format(&head))
}

/// Whether a client environment variable is set and not empty; the value never leaves Rust
/// (IXD-17, SRV-02).
#[tauri::command]
pub fn env_var_present(name: String) -> bool {
    if name.is_empty() || name.contains(['=', '\0']) {
        return false;
    }
    std::env::var_os(name).is_some_and(|v| !v.is_empty())
}

/// A project with one empty pipeline in the given default environment (IXD-07).
#[tauri::command]
pub fn new_project(default_environment_id: EnvironmentId) -> Project {
    defaults::new_project(default_environment_id)
}

/// Copy a project of the draft without its target bindings or credentials (IXD-07).
#[tauri::command]
pub fn copy_project(
    draft: Workspace,
    project_id: ProjectId,
    name: String,
) -> Result<Project, AppError> {
    draft
        .projects
        .iter()
        .find(|p| p.id == project_id)
        .map(|p| edit::copy_project(p, name))
        .ok_or_else(|| AppError::new(AppErrorKind::Internal, "要复制的项目不存在"))
}

/// Whether a local directory exists, to warn about a project root that is missing (IXD-07).
#[tauri::command]
pub fn local_dir_exists(path: String) -> bool {
    std::path::Path::new(&path).is_dir()
}

/// Effective variable values of a project in an environment, for P04 (IXD-13).
#[tauri::command]
pub fn variable_views(
    state: State<'_, AppState>,
    draft: Workspace,
    project_id: ProjectId,
    environment_id: EnvironmentId,
) -> Vec<VariableView> {
    state.with_values(|values| resolve_variable_views(&draft, project_id, environment_id, values))
}

/// An empty pipeline with RUN-03/RUN-04 defaults (IXD-08).
#[tauri::command]
pub fn new_pipeline(name: String) -> Pipeline {
    defaults::new_pipeline(name)
}

/// A copy of a pipeline with new IDs for it and its steps (IXD-08).
#[tauri::command]
pub fn copy_pipeline(pipeline: Pipeline, name: String) -> Pipeline {
    edit::copy_pipeline(&pipeline, name)
}

/// A step of `kind` with STEP-01 defaults and a name for its kind (IXD-10).
#[tauri::command]
pub fn new_step(kind: StepKind) -> Step {
    defaults::new_step(kind, defaults::step_kind_label(kind))
}

/// A copy of a step with a new ID (IXD-09).
#[tauri::command]
pub fn copy_step(step: Step) -> Step {
    edit::copy_step(&step)
}

/// Built-in expressions for the variable menu (IXD-11).
#[tauri::command]
pub fn builtin_variables() -> Vec<BuiltinVariable> {
    template_builtins()
}

/// Resolve a run plan without running anything (D03, RUN-01): no network, no password prompt.
/// The UI passes the page draft so an unsaved preview shows what the editor holds (IXD-21).
#[tauri::command]
pub fn preview_run(
    state: State<'_, AppState>,
    draft: serde_json::Value,
    request: PlanRequest,
) -> Result<ResolvedPlan, AppError> {
    let draft = parse_workspace(draft).map_err(AppError::validation)?;
    state
        .with_values(|values| build_plan(&draft, &request, values))
        .map_err(|e| AppError::new(AppErrorKind::Internal, e.to_string()))
}

/// Installed WSL distributions; empty when WSL is missing or fails (STEP-02).
#[tauri::command]
pub fn wsl_distros() -> Vec<String> {
    let mut command = std::process::Command::new("wsl.exe");
    command.args(["--list", "--quiet"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    match command.output() {
        Ok(out) if out.status.success() => parse_wsl_list(&out.stdout),
        _ => Vec::new(),
    }
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ShellProgram {
    pub shell: Shell,
    /// Full path of the executable, or `None` when it is not on PATH.
    pub path: Option<String>,
}

/// Which executable each local shell runs on this computer (STEP-02, ARC-17).
#[tauri::command]
pub fn shell_programs() -> Vec<ShellProgram> {
    [Shell::Powershell, Shell::Pwsh, Shell::Cmd, Shell::Wsl]
        .into_iter()
        .map(|shell| ShellProgram {
            shell,
            path: resolve_shell(shell).map(|p| p.display().to_string()),
        })
        .collect()
}

impl AppState {
    /// Run `f` with the value source of this computer (environment and Credential Manager).
    pub(crate) fn with_values<T>(&self, f: impl FnOnce(&ProcessValues<'_>) -> T) -> T {
        let secrets = self.secrets.lock().unwrap_or_else(|e| e.into_inner());
        f(&ProcessValues {
            secrets: secrets.as_ref().ok(),
        })
    }

    /// After a save: delete what Credential Manager still holds for deleted servers and for
    /// sensitive values that no longer exist (IXD-20, ARC-10, ARC-26). Best effort; a failure
    /// leaves an orphan entry, never a broken save.
    fn drop_orphan_secrets(&self, before: &Workspace, after: &Workspace) {
        let mut secrets = self.secrets.lock().unwrap_or_else(|e| e.into_inner());
        let Ok(store) = secrets.as_mut() else {
            return;
        };
        for server in &before.servers {
            if !after.servers.iter().any(|s| s.id == server.id) {
                let _ = store.forget_server(server.id);
            }
        }
        let still = secret_slots(after);
        for slot in secret_slots(before) {
            if !still.contains(&slot) {
                let _ = store.delete_variable(slot.0, slot.1);
            }
        }
    }

    pub(crate) fn store(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, mdm_store::WorkspaceStore>, AppError> {
        if let Some(message) = &self.startup_error {
            return Err(AppError::new(self.startup_error_kind, message.clone()));
        }
        self.store
            .lock()
            .map_err(|_| AppError::new(AppErrorKind::Internal, "工作区存储状态异常，请重启应用"))
    }
}
