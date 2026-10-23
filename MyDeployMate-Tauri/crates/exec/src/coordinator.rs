//! Run coordinator (SRS RUN-04–07, architecture ARC-05, ARC-06, ARC-14–16).
//!
//! One coordinator drives one run, serially: it asks the core [`RunMachine`] what to do next,
//! persists every state change before acting on it (ARC-14), and hands tasks to a
//! [`TaskExecutor`]. While a task runs or waits for the operator it keeps reading the control
//! channel, so cancel always gets through (ARC-06).
//!
//! When a record cannot be written, nothing more is dispatched: a running task is cancelled,
//! and the run ends interrupted in memory (ARC-33). The next start marks it interrupted on disk
//! if the final write fails too.

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use mdm_core::{
    Clock, Next, PlannedTask, ResolvedPlan, RunEvent, RunId, RunMachine, RunStatus, RunStore,
    StepKind, StorageError, TaskStatus,
};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::local::OutputChunk;

/// Operator input for the active run.
#[derive(Debug)]
pub enum Control {
    Cancel {
        reason: String,
    },
    /// Answer to the confirm task `seq` (IXD-28).
    Confirm {
        seq: u32,
        proceed: bool,
    },
    /// Answer to a credential request; `None` cancels the prompt, which cancels the run (SEC-02).
    Credential {
        request: u64,
        value: Option<Credential>,
    },
}

/// A password or passphrase typed by the operator. Never printed.
pub struct Credential(String);

impl Credential {
    pub fn new(value: impl Into<String>) -> Self {
        Credential(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Credential(***)")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialRequest {
    pub id: u64,
    pub seq: u32,
    /// What to ask for, e.g. "deploy@10.0.0.5 的 SSH 密码".
    pub prompt: String,
}

/// How a task ended, as the executor saw it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskOutcome {
    Succeeded {
        exit_code: Option<i32>,
    },
    /// Includes timeouts and errors before the command started.
    Failed {
        exit_code: Option<i32>,
        exit_signal: Option<String>,
        reason: String,
    },
    /// Stopped after a cancel request, and known to have stopped.
    Cancelled,
    /// Whether the command still runs or took effect cannot be confirmed.
    Unknown {
        reason: String,
    },
}

/// Receives everything the UI shows about the run. Control updates (`changed`, credential
/// requests, storage failures) must not be dropped; output may be batched (ARC-07).
pub trait RunObserver: Send + Sync {
    /// The run after a state change that is already persisted.
    fn changed(&self, machine: &RunMachine);
    fn output(&self, seq: u32, chunk: OutputChunk);
    /// A line written by the app itself, not by the command (e.g. why a process was killed).
    fn note(&self, seq: u32, text: &str);
    fn credential_requested(&self, request: &CredentialRequest);
    fn storage_failed(&self, error: &StorageError);
}

pub type TaskFuture<'a> = Pin<Box<dyn Future<Output = TaskOutcome> + Send + 'a>>;

/// Runs one kind of task. Implementations must return soon after `ctx.cancel` fires.
pub trait TaskExecutor: Send + Sync {
    fn execute<'a>(&'a self, task: &'a PlannedTask, ctx: TaskContext) -> TaskFuture<'a>;
}

/// What an executor may use while it runs one task.
pub struct TaskContext {
    pub seq: u32,
    pub cancel: CancellationToken,
    observer: Arc<dyn RunObserver>,
    credentials: mpsc::UnboundedSender<(String, oneshot::Sender<Option<Credential>>)>,
}

impl TaskContext {
    pub fn output(&self, chunk: OutputChunk) {
        self.observer.output(self.seq, chunk);
    }

    pub fn note(&self, text: &str) {
        self.observer.note(self.seq, text);
    }

    /// Ask the operator for a credential. `None` means the prompt was cancelled; the run is
    /// then cancelled too and the executor should stop.
    pub async fn credential(&self, prompt: impl Into<String>) -> Option<Credential> {
        let (tx, rx) = oneshot::channel();
        self.credentials.send((prompt.into(), tx)).ok()?;
        rx.await.ok().flatten()
    }
}

/// Why the coordinator gave up on the run.
enum Abort {
    Storage(StorageError),
    /// A transition the state machine refused; a coordinator bug.
    Internal(String),
}

pub struct Coordinator<S: RunStore> {
    machine: RunMachine,
    plan: Arc<ResolvedPlan>,
    store: S,
    clock: Arc<dyn Clock>,
    executor: Arc<dyn TaskExecutor>,
    observer: Arc<dyn RunObserver>,
    control: mpsc::UnboundedReceiver<Control>,
    control_open: bool,
    next_request: u64,
}

impl<S: RunStore> Coordinator<S> {
    /// Persist a new run for `plan` (which passed preflight) and mark it started. Nothing has
    /// run yet when this returns; call [`Coordinator::run`] to execute it.
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        plan: Arc<ResolvedPlan>,
        run_id: RunId,
        mut store: S,
        clock: Arc<dyn Clock>,
        executor: Arc<dyn TaskExecutor>,
        observer: Arc<dyn RunObserver>,
        control: mpsc::UnboundedReceiver<Control>,
    ) -> Result<Self, (StorageError, S)> {
        let mut machine = RunMachine::from_plan(&plan, run_id, clock.now_ms());
        if let Err(e) = store.create_run(&machine, &plan) {
            return Err((e, store));
        }
        machine
            .apply(RunEvent::Start, clock.now_ms())
            .expect("a new run starts");
        let mut c = Coordinator {
            machine,
            plan,
            store,
            clock,
            executor,
            observer,
            control,
            control_open: true,
            next_request: 1,
        };
        if let Err(e) = c.store.save_run(&c.machine) {
            c.abort(Abort::Storage(e));
        } else {
            c.observer.changed(&c.machine);
        }
        Ok(c)
    }

    pub fn machine(&self) -> &RunMachine {
        &self.machine
    }

    /// Execute the run to its end and hand back the store.
    pub async fn run(mut self) -> (RunStatus, S) {
        let status = loop {
            let step = match self.machine.next() {
                Next::Done { status } => break status,
                Next::Dispatch { seq } => self.dispatch(seq).await,
                Next::AwaitConfirmation { seq } => self.await_confirmation(seq).await,
                // Only a queued run waits with no active task, and `create` started it.
                Next::Wait => Err(Abort::Internal("运行没有可执行的任务".into())),
            };
            if let Err(abort) = step {
                break self.abort(abort);
            }
        };
        (status, self.store)
    }

    /// Apply an event, persist it, then tell the UI.
    fn step(&mut self, event: RunEvent) -> Result<(), Abort> {
        self.machine
            .apply(event, self.clock.now_ms())
            .map_err(|e| Abort::Internal(e.to_string()))?;
        self.store.save_run(&self.machine).map_err(Abort::Storage)?;
        self.observer.changed(&self.machine);
        Ok(())
    }

    /// End the run interrupted in memory and try once more to record that.
    fn abort(&mut self, abort: Abort) -> RunStatus {
        let reason = match abort {
            Abort::Storage(e) => {
                self.observer.storage_failed(&e);
                e.to_string()
            }
            Abort::Internal(message) => format!("内部错误：{message}"),
        };
        if !self.machine.run().status.is_terminal() {
            let _ = self
                .machine
                .apply(RunEvent::Interrupted { reason }, self.clock.now_ms());
            let _ = self.store.save_run(&self.machine);
        }
        self.observer.changed(&self.machine);
        self.machine.run().status
    }

    async fn recv_control(
        control: &mut mpsc::UnboundedReceiver<Control>,
        open: bool,
    ) -> Option<Control> {
        if open {
            control.recv().await
        } else {
            std::future::pending().await
        }
    }

    async fn await_confirmation(&mut self, seq: u32) -> Result<(), Abort> {
        self.step(RunEvent::TaskStarted { seq })?;
        loop {
            match Self::recv_control(&mut self.control, self.control_open).await {
                Some(Control::Confirm { seq: s, proceed }) if s == seq => {
                    return self.step(RunEvent::ConfirmationAnswered { seq, proceed });
                }
                Some(Control::Cancel { reason }) => {
                    return self.step(RunEvent::CancelRequested { reason });
                }
                // A late answer to something else: ignore it.
                Some(_) => {}
                None => self.control_open = false,
            }
        }
    }

    async fn dispatch(&mut self, seq: u32) -> Result<(), Abort> {
        // The intent is on disk before anything runs (ARC-14).
        self.step(RunEvent::TaskStarted { seq })?;

        let plan = Arc::clone(&self.plan);
        let task = &plan.tasks[seq as usize];
        debug_assert_ne!(task.kind, StepKind::Confirm);
        let cancel = CancellationToken::new();
        let (cred_tx, mut cred_rx) = mpsc::unbounded_channel();
        let ctx = TaskContext {
            seq,
            cancel: cancel.clone(),
            observer: Arc::clone(&self.observer),
            credentials: cred_tx,
        };
        let executor = Arc::clone(&self.executor);
        let mut running = executor.execute(task, ctx);
        let mut pending: Option<(u64, oneshot::Sender<Option<Credential>>)> = None;
        let mut failed: Option<Abort> = None;

        let outcome = loop {
            tokio::select! {
                outcome = &mut running => break outcome,
                Some((prompt, reply)) = cred_rx.recv() => {
                    let id = self.next_request;
                    self.next_request += 1;
                    if failed.is_some() || cancel.is_cancelled() {
                        let _ = reply.send(None);
                        continue;
                    }
                    if let Err(abort) = self.step(RunEvent::CredentialRequested { seq }) {
                        failed = Some(abort);
                        cancel.cancel();
                        let _ = reply.send(None);
                        continue;
                    }
                    pending = Some((id, reply));
                    self.observer.credential_requested(&CredentialRequest { id, seq, prompt });
                }
                control = Self::recv_control(&mut self.control, self.control_open) => match control {
                    Some(Control::Cancel { reason }) => {
                        cancel.cancel();
                        if let Some((_, reply)) = pending.take() {
                            let _ = reply.send(None);
                        }
                        if failed.is_none() && let Err(abort) = self.step(RunEvent::CancelRequested { reason }) {
                            failed = Some(abort);
                        }
                    }
                    Some(Control::Credential { request, value }) => {
                        let Some((id, _)) = &pending else { continue };
                        if *id != request {
                            continue;
                        }
                        let (_, reply) = pending.take().expect("checked");
                        let event = match &value {
                            Some(_) => RunEvent::CredentialProvided { seq },
                            None => {
                                cancel.cancel();
                                RunEvent::CancelRequested { reason: "取消输入凭据".into() }
                            }
                        };
                        if let Err(abort) = self.step(event) {
                            failed = Some(abort);
                            cancel.cancel();
                            let _ = reply.send(None);
                        } else {
                            let _ = reply.send(value);
                        }
                    }
                    Some(Control::Confirm { .. }) => {}
                    None => self.control_open = false,
                },
            }
        };
        drop(running);
        if let Some(abort) = failed {
            return Err(abort);
        }
        // A cancel during a credential prompt already finished the task (RunMachine::cancel).
        if self.machine.tasks()[seq as usize].status.is_terminal() {
            return Ok(());
        }
        let status = self.machine.tasks()[seq as usize].status;
        debug_assert!(matches!(
            status,
            TaskStatus::Running | TaskStatus::WaitingCredential
        ));
        let event = match outcome {
            TaskOutcome::Succeeded { exit_code } => RunEvent::TaskSucceeded { seq, exit_code },
            TaskOutcome::Failed {
                exit_code,
                exit_signal,
                reason,
            } => RunEvent::TaskFailed {
                seq,
                exit_code,
                exit_signal,
                reason,
            },
            TaskOutcome::Cancelled => RunEvent::TaskCancelled { seq },
            TaskOutcome::Unknown { reason } => RunEvent::TaskOutcomeUnknown { seq, reason },
        };
        self.step(event)
    }
}

/// The system clock.
#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64)
    }
}
