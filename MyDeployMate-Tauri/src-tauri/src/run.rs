//! Starting and controlling a run (SRS RUN-02, RUN-05; IXD-25; architecture ARC-13, ARC-35;
//! plan task 4.6).
//!
//! `start_run` rebuilds the plan from the saved workspace, checks it against what the operator
//! previewed, records the run and starts the coordinator. The plan built here is the frozen
//! snapshot: later edits do not reach the running tasks. Updates reach the UI through the
//! Tauri channel passed in.

use std::sync::{Arc, Mutex};

use mdm_core::log::LogBudget;
use mdm_core::{
    Clock, LogEntry, PlanRequest, Run, RunId, RunMachine, StorageError, Task, build_plan,
};
use mdm_exec::{
    AppExecutor, Control, Coordinator, CredentialRequest, LogPipeline, RunView, SystemClock,
};
use mdm_store::RunLogFile;
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::error::{AppError, AppErrorKind};
use crate::state::AppState;
use crate::values::ProcessValues;

/// The run in progress, as the commands see it.
pub struct ActiveRun {
    pub run_id: RunId,
    /// The start request's idempotency key: a repeated click returns the same run (IXD-25).
    key: String,
    control: mpsc::UnboundedSender<Control>,
}

/// What the preview dialog sends when the operator clicks start.
#[derive(Clone, Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StartRunRequest {
    pub request: PlanRequest,
    /// Workspace revision the preview was built from; a different saved revision means the
    /// configuration changed and the operator must preview again (ARC-13).
    #[ts(type = "number")]
    pub revision: u64,
    /// The environment name as typed, for protected environments (RUN-02).
    pub confirmation: Option<String>,
    /// Same key for every click of one preview.
    pub key: String,
}

/// Pushed to the UI while a run is active. `state` carries the run and all its tasks after a
/// recorded change.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum RunUpdate {
    State {
        run: Box<Run>,
        tasks: Vec<Task>,
        cancel_reason: Option<String>,
    },
    Logs {
        entries: Vec<LogEntry>,
    },
    Credential {
        #[ts(type = "number")]
        request_id: u64,
        task: u32,
        prompt: String,
    },
    StorageFailed {
        message: String,
    },
}

struct ChannelView(Mutex<Channel<RunUpdate>>);

impl ChannelView {
    fn send(&self, update: RunUpdate) {
        let channel = self.0.lock().unwrap_or_else(|e| e.into_inner());
        // The window may be gone; the run goes on and its records stay complete.
        let _ = channel.send(update);
    }
}

impl RunView for ChannelView {
    fn state(&self, machine: &RunMachine) {
        self.send(RunUpdate::State {
            run: Box::new(machine.run().clone()),
            tasks: machine.tasks().to_vec(),
            cancel_reason: machine.cancel_reason().map(str::to_owned),
        });
    }

    fn logs(&self, entries: Vec<LogEntry>) {
        self.send(RunUpdate::Logs { entries });
    }

    fn credential_requested(&self, request: &CredentialRequest) {
        self.send(RunUpdate::Credential {
            request_id: request.id,
            task: request.seq,
            prompt: request.prompt.clone(),
        });
    }

    fn storage_failed(&self, error: &StorageError) {
        self.send(RunUpdate::StorageFailed {
            message: error.to_string(),
        });
    }
}

/// Start the run the operator previewed. Returns the run ID; a repeated request with the same
/// key returns the run it already started.
#[tauri::command]
pub fn start_run(
    app: AppHandle,
    state: State<'_, AppState>,
    start: StartRunRequest,
    on_update: Channel<RunUpdate>,
) -> Result<RunId, AppError> {
    let mut active = state
        .active
        .lock()
        .map_err(|_| AppError::new(AppErrorKind::Internal, "运行状态异常，请重启应用"))?;
    if let Some(run) = active.as_ref() {
        if run.key == start.key {
            return Ok(run.run_id);
        }
        return Err(AppError::new(
            AppErrorKind::RunActive,
            "已有运行正在进行，同一时间只能有一个运行",
        ));
    }
    let history = state.history.clone().ok_or_else(|| {
        AppError::new(
            AppErrorKind::StorageUnavailable,
            state
                .history_error
                .clone()
                .unwrap_or_else(|| "运行记录不可用".into()),
        )
    })?;

    let (workspace, _) = state.store()?.load()?;
    if workspace.revision != start.revision {
        return Err(AppError::new(
            AppErrorKind::RevisionConflict,
            "配置在预览后已被修改，请重新预览后再开始",
        ));
    }
    let plan = build_plan(&workspace, &start.request, &ProcessValues)
        .map_err(|e| AppError::new(AppErrorKind::Validation, e.to_string()))?;
    if !plan.issues.is_empty() {
        return Err(AppError::new(
            AppErrorKind::Validation,
            format!(
                "预检有 {} 个问题，请在预览中处理后再开始",
                plan.issues.len()
            ),
        ));
    }
    if plan.tasks.is_empty() {
        return Err(AppError::new(AppErrorKind::Validation, "没有要执行的任务"));
    }
    if plan.protected
        && start.confirmation.as_deref().map(str::trim) != Some(plan.environment_name.as_str())
    {
        return Err(AppError::new(
            AppErrorKind::Validation,
            format!("请输入环境名称 {} 以确认", plan.environment_name),
        ));
    }

    let run_id = RunId::new();
    let log = RunLogFile::open(&state.data_dir, run_id)?;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let (control, control_rx) = mpsc::unbounded_channel();
    let pipeline = Arc::new(LogPipeline::new(
        plan.secrets.0.clone(),
        Box::new(log),
        LogBudget::default(),
        Arc::new(ChannelView(Mutex::new(on_update))),
        Arc::clone(&clock),
        control.clone(),
    ));
    let coordinator = Coordinator::create(
        Arc::new(plan),
        run_id,
        history,
        clock,
        Arc::new(AppExecutor),
        pipeline.clone(),
        control_rx,
    )
    .map_err(|(e, _)| AppError::new(AppErrorKind::StorageUnavailable, e.to_string()))?;

    *active = Some(ActiveRun {
        run_id,
        key: start.key,
        control,
    });
    drop(active);

    tauri::async_runtime::spawn(async move {
        let stop = CancellationToken::new();
        let flusher = tauri::async_runtime::spawn(pipeline.flush_every(stop.clone()));
        let _ = coordinator.run().await;
        stop.cancel();
        let _ = flusher.await;
        let state = app.state::<AppState>();
        let mut active = state.active.lock().unwrap_or_else(|e| e.into_inner());
        if active.as_ref().is_some_and(|a| a.run_id == run_id) {
            *active = None;
        }
    });
    Ok(run_id)
}

/// The run in progress, if any (IXD-02 entry point).
#[tauri::command]
pub fn active_run(state: State<'_, AppState>) -> Option<RunId> {
    let active = state.active.lock().unwrap_or_else(|e| e.into_inner());
    active.as_ref().map(|a| a.run_id)
}

/// Cancel the active run (IXD-29). Unknown or finished runs are an error.
#[tauri::command]
pub fn cancel_run(state: State<'_, AppState>, run_id: RunId) -> Result<(), AppError> {
    send_control(
        &state,
        run_id,
        Control::Cancel {
            reason: "用户取消".into(),
        },
    )
}

/// Answer the confirm task `seq` of the active run (IXD-28).
#[tauri::command]
pub fn answer_confirmation(
    state: State<'_, AppState>,
    run_id: RunId,
    seq: u32,
    proceed: bool,
) -> Result<(), AppError> {
    send_control(&state, run_id, Control::Confirm { seq, proceed })
}

fn send_control(state: &AppState, run_id: RunId, control: Control) -> Result<(), AppError> {
    let active = state.active.lock().unwrap_or_else(|e| e.into_inner());
    match active.as_ref() {
        Some(a) if a.run_id == run_id => a
            .control
            .send(control)
            .map_err(|_| AppError::new(AppErrorKind::Internal, "运行已结束")),
        _ => Err(AppError::new(AppErrorKind::Validation, "该运行已结束")),
    }
}
