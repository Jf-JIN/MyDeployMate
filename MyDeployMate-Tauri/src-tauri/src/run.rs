//! Starting and controlling a run (SRS RUN-02, RUN-05; IXD-25; architecture ARC-13, ARC-35;
//! plan task 4.6).
//!
//! `start_run` rebuilds the plan from the saved workspace, checks it against what the operator
//! previewed, records the run and starts the coordinator. The plan built here is the frozen
//! snapshot: later edits do not reach the running tasks. The run page then calls `open_run`,
//! which returns everything recorded so far and, for the active run, sends later updates
//! through the Tauri channel passed in.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use mdm_core::log::LogBudget;
use mdm_core::{
    Clock, LogEntry, Message, OperatorPrompt, PlanRequest, ResolvedPlan, Run, RunId, RunMachine,
    Server, ServerId, StorageError, Task, UploadProgress, build_plan,
};
use mdm_exec::{
    AppExecutor, ConnectFuture, Control, Coordinator, LogPipeline, OperatorAnswer, OperatorRequest,
    RemoteConnector, RunView, SystemClock, TaskContext, TaskOutcome,
};
use mdm_store::{RunLogFile, SharedHistory, read_run_log};
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::connection::{SessionError, open_session};
use crate::credentials::{AskFuture, Operator, secret_answer};
use crate::error::{AppError, AppErrorKind};
use crate::state::AppState;

/// The run in progress, as the commands see it.
pub struct ActiveRun {
    pub run_id: RunId,
    /// The start request's idempotency key: a repeated click returns the same run (IXD-25).
    key: String,
    control: mpsc::UnboundedSender<Control>,
    view: Arc<ChannelView>,
    /// The servers as the run froze them; their connection settings stay locked until the run
    /// ends (AC-05, IXD-20).
    pub servers: Vec<Server>,
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
        cancel_reason: Option<Message>,
    },
    Logs {
        entries: Vec<LogEntry>,
    },
    /// A task waits for the operator: show D02 or D04 and answer with `answer_run_trust` or
    /// `answer_run_secret`.
    Request(RunRequest),
    StorageFailed {
        message: Message,
    },
    /// Progress of the running upload task; not stored.
    Progress {
        task: u32,
        progress: UploadProgress,
    },
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RunRequest {
    #[ts(type = "number")]
    pub request_id: u64,
    pub task: u32,
    pub prompt: OperatorPrompt,
}

impl From<&OperatorRequest> for RunRequest {
    fn from(r: &OperatorRequest) -> Self {
        Self {
            request_id: r.id,
            task: r.seq,
            prompt: r.prompt.clone(),
        }
    }
}

/// Everything recorded about a run, for the run page (IXD-26).
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RunSnapshot {
    pub run: Run,
    pub tasks: Vec<Task>,
    pub cancel_reason: Option<Message>,
    /// The frozen plan, for each task's parameters.
    pub plan: ResolvedPlan,
    pub logs: Vec<LogEntry>,
    /// Log lines that could not be read back, normally one cut off by a crash.
    pub damaged_lines: usize,
    /// Later updates arrive through the channel.
    pub active: bool,
    /// The open D02 or D04 request of the active run.
    pub request: Option<RunRequest>,
}

/// Sends run updates to the page that opened the run last; none until a page opens it.
#[derive(Default)]
struct ChannelView {
    channel: Mutex<Option<Channel<RunUpdate>>>,
    /// The open request, for a page that opens the run while it waits.
    request: Mutex<Option<RunRequest>>,
}

impl ChannelView {
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Channel<RunUpdate>>> {
        self.channel.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn send(&self, update: RunUpdate) {
        // The window may be gone; the run goes on and its records stay complete.
        if let Some(channel) = self.lock().as_ref() {
            let _ = channel.send(update);
        }
    }
}

impl RunView for ChannelView {
    fn state(&self, machine: &RunMachine) {
        if machine.run().status != mdm_core::RunStatus::WaitingCredential {
            *self.request.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
        self.send(RunUpdate::State {
            run: Box::new(machine.run().clone()),
            tasks: machine.tasks().to_vec(),
            cancel_reason: machine.cancel_reason().cloned(),
        });
    }

    fn logs(&self, entries: Vec<LogEntry>) {
        self.send(RunUpdate::Logs { entries });
    }

    fn requested(&self, request: &OperatorRequest) {
        let request = RunRequest::from(request);
        *self.request.lock().unwrap_or_else(|e| e.into_inner()) = Some(request.clone());
        self.send(RunUpdate::Request(request));
    }

    fn storage_failed(&self, error: &StorageError) {
        self.send(RunUpdate::StorageFailed {
            message: error.message(),
        });
    }

    fn progress(&self, seq: u32, progress: UploadProgress) {
        self.send(RunUpdate::Progress {
            task: seq,
            progress,
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
) -> Result<RunId, AppError> {
    let mut active = state
        .active
        .lock()
        .map_err(|_| AppError::new(AppErrorKind::Internal, Message::RunStateBroken))?;
    if let Some(run) = active.as_ref() {
        if run.key == start.key {
            return Ok(run.run_id);
        }
        return Err(AppError::new(
            AppErrorKind::RunActive,
            Message::AnotherRunActive,
        ));
    }
    let history = history(&state)?;

    let (workspace, _) = state.store()?.load()?;
    if workspace.revision != start.revision {
        return Err(AppError::new(
            AppErrorKind::RevisionConflict,
            Message::StalePreview,
        ));
    }
    let plan = state
        .with_values(|values| build_plan(&workspace, &start.request, values))
        .map_err(|e| AppError::new(AppErrorKind::Validation, e.message()))?;
    if !plan.issues.is_empty() {
        return Err(AppError::new(
            AppErrorKind::Validation,
            Message::PreflightIssues {
                n: plan.issues.len() as u32,
            },
        ));
    }
    if plan.tasks.is_empty() {
        return Err(AppError::new(AppErrorKind::Validation, Message::NoTasks));
    }
    if plan.protected
        && start.confirmation.as_deref().map(str::trim) != Some(plan.environment_name.as_str())
    {
        return Err(AppError::new(
            AppErrorKind::Validation,
            Message::ConfirmEnvironment {
                name: plan.environment_name.clone(),
            },
        ));
    }

    let servers: Vec<Server> = workspace
        .servers
        .iter()
        .filter(|s| plan.targets.iter().any(|t| t.server_id == s.id))
        .cloned()
        .collect();
    let plan = Arc::new(plan);
    let executor = Arc::new(AppExecutor::new(Arc::new(RunConnector {
        app: app.clone(),
        servers: servers.iter().map(|s| (s.id, s.clone())).collect(),
        plan: Arc::clone(&plan),
    })));

    let run_id = RunId::new();
    let log = RunLogFile::open(&state.data_dir, run_id)?;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let (control, control_rx) = mpsc::unbounded_channel();
    let view = Arc::new(ChannelView::default());
    let pipeline = Arc::new(LogPipeline::new(
        plan.secrets.0.clone(),
        Box::new(log),
        LogBudget::default(),
        view.clone(),
        Arc::clone(&clock),
        control.clone(),
    ));
    let coordinator = Coordinator::create(
        plan,
        run_id,
        history,
        clock,
        executor.clone(),
        pipeline.clone(),
        control_rx,
    )
    .map_err(|(e, _)| AppError::new(AppErrorKind::StorageUnavailable, e.message()))?;

    *active = Some(ActiveRun {
        run_id,
        key: start.key,
        control,
        view,
        servers,
    });
    drop(active);

    tauri::async_runtime::spawn(async move {
        let stop = CancellationToken::new();
        let flusher = tauri::async_runtime::spawn(pipeline.flush_every(stop.clone()));
        let _ = coordinator.run().await;
        executor.close().await;
        stop.cancel();
        let _ = flusher.await;
        let state = app.state::<AppState>();
        let mut active = state.active.lock().unwrap_or_else(|e| e.into_inner());
        if active.as_ref().is_some_and(|a| a.run_id == run_id) {
            *active = None;
        }
        drop(active);
        if state.quit_when_idle.load(Ordering::SeqCst) {
            app.exit(0);
        }
    });
    Ok(run_id)
}

/// Everything recorded about `run_id`. For the active run, later updates go to `on_update`
/// and no longer to the page that opened it before.
#[tauri::command]
pub fn open_run(
    state: State<'_, AppState>,
    run_id: RunId,
    on_update: Channel<RunUpdate>,
) -> Result<RunSnapshot, AppError> {
    let history = history(&state)?;
    let view = {
        let active = state.active.lock().unwrap_or_else(|e| e.into_inner());
        active
            .as_ref()
            .filter(|a| a.run_id == run_id)
            .map(|a| a.view.clone())
    };
    let Some(view) = view else {
        return read_snapshot(&state, &history, run_id, false);
    };
    // The coordinator records a change before it reaches the view, and the pipeline writes
    // output before it sends it. Reading while holding the view means the snapshot holds
    // everything already sent, and everything sent later is newer or a repeat (which the page
    // drops by log number).
    let mut channel = view.lock();
    let mut snapshot = read_snapshot(&state, &history, run_id, true)?;
    snapshot.request = view
        .request
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    *channel = Some(on_update);
    Ok(snapshot)
}

fn read_snapshot(
    state: &AppState,
    history: &SharedHistory,
    run_id: RunId,
    active: bool,
) -> Result<RunSnapshot, AppError> {
    let (stored, plan) = {
        let store = history.lock();
        (store.load_run(run_id)?, store.load_plan(run_id)?)
    };
    let (Some(stored), Some(plan)) = (stored, plan) else {
        return Err(AppError::new(AppErrorKind::NotFound, Message::RunNotFound));
    };
    let log = read_run_log(&state.data_dir, run_id)?;
    Ok(RunSnapshot {
        run: stored.run,
        tasks: stored.tasks,
        cancel_reason: stored.cancel_reason,
        plan,
        logs: log.entries,
        damaged_lines: log.damaged_lines,
        active,
        request: None,
    })
}

fn history(state: &AppState) -> Result<SharedHistory, AppError> {
    state.history.clone().ok_or_else(|| {
        AppError::new(
            AppErrorKind::StorageUnavailable,
            state
                .history_error
                .clone()
                .unwrap_or(Message::RunUnavailable),
        )
    })
}

/// The run in progress, if any (IXD-02 entry point).
#[tauri::command]
pub fn active_run(state: State<'_, AppState>) -> Option<RunId> {
    let active = state.active.lock().unwrap_or_else(|e| e.into_inner());
    active.as_ref().map(|a| a.run_id)
}

/// Servers the active run uses; their deletion and connection changes wait for it (IXD-20).
#[tauri::command]
pub fn active_run_servers(state: State<'_, AppState>) -> Vec<ServerId> {
    let active = state.active.lock().unwrap_or_else(|e| e.into_inner());
    active
        .as_ref()
        .map(|a| a.servers.iter().map(|s| s.id).collect())
        .unwrap_or_default()
}

/// Cancel the active run (IXD-29). Unknown or finished runs are an error.
#[tauri::command]
pub fn cancel_run(state: State<'_, AppState>, run_id: RunId) -> Result<(), AppError> {
    send_control(
        &state,
        run_id,
        Control::Cancel {
            reason: Message::UserCancelled,
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

/// D04 answer during a run. `secret: None` cancels the run (IXD-32).
#[tauri::command]
pub fn answer_run_secret(
    state: State<'_, AppState>,
    run_id: RunId,
    request_id: u64,
    secret: Option<String>,
    remember: bool,
) -> Result<(), AppError> {
    let answer = secret_answer(secret, remember);
    send_control(
        &state,
        run_id,
        Control::Answer {
            request: request_id,
            answer,
        },
    )
}

/// D02 answer during a run: `trust` after the operator checked the fingerprint; otherwise the
/// run is cancelled.
#[tauri::command]
pub fn answer_run_trust(
    state: State<'_, AppState>,
    run_id: RunId,
    request_id: u64,
    trust: bool,
) -> Result<(), AppError> {
    send_control(
        &state,
        run_id,
        Control::Answer {
            request: request_id,
            answer: trust.then_some(OperatorAnswer::Trust),
        },
    )
}

/// Connects remote tasks with the server settings frozen at the start; D02 and D04 go to the
/// run page through the coordinator, so the wait is recorded and cancel reaches it.
struct RunConnector {
    app: AppHandle,
    servers: HashMap<ServerId, Server>,
    plan: Arc<ResolvedPlan>,
}

impl RemoteConnector for RunConnector {
    fn connect<'a>(&'a self, server: ServerId, ctx: &'a TaskContext) -> ConnectFuture<'a> {
        Box::pin(async move {
            let Some(server) = self.servers.get(&server) else {
                return Err(TaskOutcome::Failed {
                    exit_code: None,
                    exit_signal: None,
                    reason: Message::ServerMissing,
                });
            };
            let task = &self.plan.tasks[ctx.seq as usize];
            let context = Message::TaskContext {
                n: ctx.seq + 1,
                step: task.step_name.clone(),
            };
            match open_session(&self.app, server, context, &TaskAsker(ctx), &ctx.cancel).await {
                Ok(session) => Ok(session),
                Err(SessionError::Cancelled) => Err(TaskOutcome::Cancelled),
                Err(SessionError::Failed(e)) => Err(TaskOutcome::Failed {
                    exit_code: None,
                    exit_signal: None,
                    reason: Message::ConnectFailed {
                        server: server.name.clone(),
                        reason: Box::new(e.message),
                    },
                }),
            }
        })
    }
}

struct TaskAsker<'a>(&'a TaskContext);

impl Operator for TaskAsker<'_> {
    fn ask(&self, prompt: OperatorPrompt) -> AskFuture<'_> {
        Box::pin(self.0.ask(prompt))
    }
}

/// Closing the window while a run is active (IXD-29): cancel the run and quit once it has
/// stopped. Without an active run the app quits at once.
#[tauri::command]
pub fn cancel_and_quit(app: AppHandle, state: State<'_, AppState>) {
    state.quit_when_idle.store(true, Ordering::SeqCst);
    let active = state.active.lock().unwrap_or_else(|e| e.into_inner());
    let sent = active.as_ref().is_some_and(|a| {
        a.control
            .send(Control::Cancel {
                reason: Message::QuitCancelled,
            })
            .is_ok()
    });
    drop(active);
    if !sent {
        app.exit(0);
    }
}

/// Window close requests are held back while a run is active; the page asks what to do.
pub fn close_requested(window: &tauri::Window, api: &tauri::CloseRequestApi) {
    let state = window.state::<AppState>();
    let active = state.active.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(run) = active.as_ref() {
        api.prevent_close();
        let _ = window.emit(CLOSE_EVENT, run.run_id);
    }
}

/// Emitted to the page when the window is closed during a run.
pub const CLOSE_EVENT: &str = "run://close-requested";

fn send_control(state: &AppState, run_id: RunId, control: Control) -> Result<(), AppError> {
    let active = state.active.lock().unwrap_or_else(|e| e.into_inner());
    match active.as_ref() {
        Some(a) if a.run_id == run_id => a
            .control
            .send(control)
            .map_err(|_| AppError::new(AppErrorKind::Internal, Message::RunEnded)),
        _ => Err(AppError::new(
            AppErrorKind::Validation,
            Message::RunAlreadyEnded,
        )),
    }
}
