//! Configuration export and import (SRS CFG-01, CFG-02; IXD-36, IXD-37). Import is two steps:
//! the preview validates the file and lists what the full replacement changes; applying it
//! backs up the current file and replaces it atomically. Nothing is replaced while a run is
//! active, and an import never runs anything.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use tauri::State;
use ts_rs::TS;

use mdm_core::exchange::{DiffEntry, LocalPath, local_paths, workspace_diff};
use mdm_core::{Message, Workspace};
use mdm_store::{StoreError, export_json, parse_import};

use crate::error::{AppError, AppErrorKind};
use crate::state::AppState;

/// A validated import waiting for the operator's confirmation.
pub struct PendingImport {
    token: u32,
    workspace: Workspace,
    /// Revision of the configuration the preview compared against.
    base_revision: u64,
}

/// The pending import, and the token the next preview gets.
#[derive(Default)]
pub struct ImportSlot(Mutex<(Option<PendingImport>, u32)>);

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImportPreview {
    /// Passed back to `import_apply`; a new preview replaces the previous one.
    pub token: u32,
    /// Schema version of the file; older files are migrated.
    pub from_version: u32,
    pub changes: Vec<DiffEntry>,
    /// Project roots and key files that do not exist on this computer: to fix after import.
    pub missing_paths: Vec<LocalPath>,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImportApplied {
    pub workspace: Workspace,
    /// Where the replaced configuration was copied.
    pub backup: String,
}

/// Write the saved configuration to `path` (CFG-01). Local paths are left out unless asked.
#[tauri::command]
pub fn export_workspace(
    state: State<'_, AppState>,
    path: String,
    include_local_paths: bool,
) -> Result<(), AppError> {
    let ws = state.store()?.load()?.0;
    let path = PathBuf::from(path);
    std::fs::write(&path, export_json(&ws, include_local_paths))
        .map_err(|source| StoreError::Io { path, source })?;
    Ok(())
}

/// Validate `path` and compare it with the saved configuration (D05). Invalid files come back
/// as a validation error with every issue's object path; the current configuration is never
/// touched here.
#[tauri::command]
pub fn import_preview(state: State<'_, AppState>, path: String) -> Result<ImportPreview, AppError> {
    let file = PathBuf::from(&path);
    let bytes = std::fs::read(&file).map_err(|source| StoreError::Io { path: file, source })?;
    let (imported, from_version) = parse_import(&bytes)?;
    let current = state.store()?.load()?.0;
    let missing_paths = local_paths(&imported)
        .into_iter()
        .filter(|p| !Path::new(&p.path).exists())
        .collect();
    let mut slot = state.import.0.lock().unwrap_or_else(|e| e.into_inner());
    slot.1 = slot.1.wrapping_add(1);
    let token = slot.1;
    let preview = ImportPreview {
        token,
        from_version,
        changes: workspace_diff(&current, &imported),
        missing_paths,
    };
    slot.0 = Some(PendingImport {
        token,
        workspace: imported,
        base_revision: current.revision,
    });
    Ok(preview)
}

/// Back up the current configuration and replace it with the previewed import (IXD-37).
#[tauri::command]
pub fn import_apply(state: State<'_, AppState>, token: u32) -> Result<ImportApplied, AppError> {
    if state
        .active
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .is_some()
    {
        return Err(AppError::new(
            AppErrorKind::RunActive,
            Message::ImportRunActive,
        ));
    }
    let pending = {
        let mut slot = state.import.0.lock().unwrap_or_else(|e| e.into_inner());
        match slot.0.take() {
            Some(p) if p.token == token => p,
            other => {
                slot.0 = other;
                return Err(expired());
            }
        }
    };
    let mut store = state.store()?;
    let before = store.load()?.0;
    if before.revision != pending.base_revision {
        return Err(expired());
    }
    let (saved, backup) = store.replace(pending.workspace, pending.base_revision)?;
    drop(store);
    state.drop_orphan_secrets(&before, &saved);
    Ok(ImportApplied {
        workspace: saved,
        backup: backup.display().to_string(),
    })
}

fn expired() -> AppError {
    AppError::new(AppErrorKind::RevisionConflict, Message::ImportExpired)
}
