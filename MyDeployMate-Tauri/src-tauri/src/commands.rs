//! Tauri commands for phase 1: app info and workspace load / validate / save (ARC-35).
//! Drafts arrive as raw JSON so type errors can be reported with their field path.

use serde::Serialize;
use tauri::State;
use ts_rs::TS;

use mdm_core::{
    ValidationReport, Workspace, check_workspace_json, parse_workspace, validate_workspace,
};
use mdm_store::LoadOutcome;

use crate::error::{AppError, AppErrorKind};
use crate::state::AppState;

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
