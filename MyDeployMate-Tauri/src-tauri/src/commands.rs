//! Tauri commands: app info, workspace load / validate / save (ARC-35), defaults for new
//! objects, edits whose side effects the UI shows before saving (IXD-14, IXD-20), and local
//! checks for the server form (IXD-17), project creation and copy, and effective variables
//! (IXD-07, IXD-13).
//! Drafts to validate or save arrive as raw JSON so type errors can be reported with their
//! field path; the edit helpers take typed drafts the UI built from a loaded workspace.

use serde::Serialize;
use tauri::State;
use ts_rs::TS;

use mdm_core::keyfile::KEY_HEAD_LEN;
use mdm_core::resolve::{VariableView, variable_views as resolve_variable_views};
use mdm_core::{
    Environment, EnvironmentId, KeyFormat, Project, ProjectId, Reference, Server, ServerEdit,
    ServerId, ValidationReport, Variable, Workspace, check_workspace_json, defaults,
    detect_key_format, edit, parse_workspace, validate_workspace,
};
use mdm_store::LoadOutcome;

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
    let mut store = state.store()?;
    Ok(store.save(ws)?)
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
#[tauri::command]
pub fn put_server(draft: Workspace, server: Server) -> ServerEdit {
    edit::put_server(draft, server)
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
    draft: Workspace,
    project_id: ProjectId,
    environment_id: EnvironmentId,
) -> Vec<VariableView> {
    resolve_variable_views(&draft, project_id, environment_id, &ProcessValues)
}

impl AppState {
    fn store(&self) -> Result<std::sync::MutexGuard<'_, mdm_store::WorkspaceStore>, AppError> {
        if let Some(message) = &self.startup_error {
            return Err(AppError::new(self.startup_error_kind, message.clone()));
        }
        self.store
            .lock()
            .map_err(|_| AppError::new(AppErrorKind::Internal, "工作区存储状态异常，请重启应用"))
    }
}
