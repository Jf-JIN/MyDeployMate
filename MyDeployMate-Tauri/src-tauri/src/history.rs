//! P08 run history: list with filters, delete, the retention cleanup, log export and the
//! changes shown before running a past run again (SRS OBS-02, NFR-02, RUN-07; IXD-34, IXD-36,
//! FLOW-07). Opening one run for the read-only P07 goes through `open_run`.

use tauri::State;

use mdm_core::history::retention_cutoff_ms;
use mdm_core::rerun::plan_changes as compare_plans;
use mdm_core::{Clock, Message, ResolvedPlan, Run, RunChoice, RunFilter, RunId, RunSummary};
use mdm_exec::SystemClock;
use mdm_store::{SharedHistory, StoreError, remove_run_log};

use crate::error::{AppError, AppErrorKind};
use crate::run::history;
use crate::state::AppState;

#[tauri::command]
pub fn list_runs(
    state: State<'_, AppState>,
    filter: RunFilter,
) -> Result<Vec<RunSummary>, AppError> {
    Ok(history(&state)?.lock().list_runs(&filter)?)
}

#[tauri::command]
pub fn run_choices(state: State<'_, AppState>) -> Result<Vec<RunChoice>, AppError> {
    Ok(history(&state)?.lock().run_choices()?)
}

/// The latest run of each project (IXD-06 "最近运行").
#[tauri::command]
pub fn latest_runs(state: State<'_, AppState>) -> Result<Vec<Run>, AppError> {
    Ok(history(&state)?.lock().latest_runs()?)
}

/// Delete one finished run and its log (OBS-02); the UI asks first.
#[tauri::command]
pub fn delete_run(state: State<'_, AppState>, run_id: RunId) -> Result<(), AppError> {
    let active = state.active.lock().unwrap_or_else(|e| e.into_inner());
    if active.as_ref().is_some_and(|a| a.run_id == run_id) {
        return Err(StoreError::RunUnfinished.into());
    }
    drop(active);
    if !history(&state)?.lock().delete_run(run_id)? {
        return Err(AppError::new(AppErrorKind::NotFound, Message::RunNotFound));
    }
    Ok(remove_run_log(&state.data_dir, run_id)?)
}

/// How many finished runs a retention of `days` would remove now (IXD-36: shown before saving).
#[tauri::command]
pub fn expired_run_count(state: State<'_, AppState>, days: u32) -> Result<u32, AppError> {
    let cutoff = retention_cutoff_ms(SystemClock.now_ms(), days);
    Ok(history(&state)?.lock().count_expired(cutoff)?)
}

/// Remove the runs past the saved retention period with their logs. Returns how many.
#[tauri::command]
pub fn purge_history(state: State<'_, AppState>) -> Result<u32, AppError> {
    let days = {
        let mut store = state.store()?;
        store.load()?.0.preferences.history_retention_days
    };
    purge(&state, &history(&state)?, days)
}

/// Delete the runs older than `days`, then their log files. A log that cannot be removed does
/// not bring the run back; the first such error is reported after the rest are removed.
pub fn purge(state: &AppState, history: &SharedHistory, days: u32) -> Result<u32, AppError> {
    let cutoff = retention_cutoff_ms(SystemClock.now_ms(), days);
    let removed = history.lock().purge_expired(cutoff)?;
    let mut first_error = None;
    for id in &removed {
        if let Err(e) = remove_run_log(&state.data_dir, *id) {
            first_error.get_or_insert(e);
        }
    }
    match first_error {
        Some(e) => Err(e.into()),
        None => Ok(removed.len() as u32),
    }
}

/// What differs between the plan `run_id` ran and `plan`, a preview of the same pipeline now
/// (FLOW-07). The preview still goes through D03 as usual.
#[tauri::command]
pub fn plan_changes(
    state: State<'_, AppState>,
    run_id: RunId,
    plan: ResolvedPlan,
) -> Result<Vec<Message>, AppError> {
    let old = history(&state)?.lock().load_plan(run_id)?;
    let old = old.ok_or_else(|| AppError::new(AppErrorKind::NotFound, Message::RunNotFound))?;
    Ok(compare_plans(&old, &plan))
}

/// Write an exported run log to the file the operator picked in the save dialog (IXD-27).
/// The UI builds the text so that app notes are in the operator's language.
#[tauri::command]
pub fn write_export(path: String, contents: String) -> Result<(), AppError> {
    let path = std::path::PathBuf::from(path);
    std::fs::write(&path, contents).map_err(|source| StoreError::Io { path, source })?;
    Ok(())
}
