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
    Clock, Message, Next, OperatorPrompt, PlannedTask, ResolvedPlan, RunEvent, RunId, RunMachine,
    RunStatus, RunStore, StepKind, StorageError, TaskStatus,
};
use secrecy::SecretString;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::local::OutputChunk;

/// Operator input for the active run.
#[derive(Debug)]
pub enum Control {
    Cancel {
        reason: Message,
    },
    /// Answer to the confirm task `seq` (IXD-28).
    Confirm {
        seq: u32,
        proceed: bool,
    },
    /// Answer to an operator request; `None` declines it, which cancels the run (IXD-32).
    Answer {
        request: u64,
        answer: Option<OperatorAnswer>,
    },
}

/// What the operator gave for an [`OperatorPrompt`].
pub enum OperatorAnswer {
    /// D04: the typed secret, and whether to remember it once it is accepted.
    Secret { value: SecretString, remember: bool },
    /// D02: the operator checked the fingerprint and trusts the key.
    Trust,
}

impl fmt::Debug for OperatorAnswer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Secret { remember, .. } => write!(f, "Secret(***, remember: {remember})"),
            Self::Trust => f.write_str("Trust"),
        }
    }
}

/// A task waits for the operator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperatorRequest {
    pub id: u64,
    pub seq: u32,
    pub prompt: OperatorPrompt,
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
        reason: Message,
    },
    /// Stopped after a cancel request, and known to have stopped.
    Cancelled,
    /// Whether the command still runs or took effect cannot be confirmed.
    Unknown {
        reason: Message,
    },
}

/// Receives everything the UI shows about the run. Control updates (`changed`, credential
/// requests, storage failures) must not be dropped; output may be batched (ARC-07).
pub trait RunObserver: Send + Sync {
    /// The run after a state change that is already persisted.
    fn changed(&self, machine: &RunMachine);
    fn output(&self, seq: u32, chunk: OutputChunk);
    /// A line written by the app itself, not by the command (e.g. why a process was killed).
    fn note(&self, seq: u32, message: Message);
    /// A task waits for the operator; the run is already recorded as waiting.
    fn requested(&self, request: &OperatorRequest);
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
    requests: mpsc::UnboundedSender<(OperatorPrompt, oneshot::Sender<Option<OperatorAnswer>>)>,
}

impl TaskContext {
    pub fn output(&self, chunk: OutputChunk) {
        self.observer.output(self.seq, chunk);
    }

    pub fn note(&self, message: Message) {
        self.observer.note(self.seq, message);
    }

    /// Ask the operator and wait (D02, D04). `None` means the prompt was declined or the run
    /// was cancelled meanwhile; the run is then cancelled and the executor should stop.
    pub async fn ask(&self, prompt: OperatorPrompt) -> Option<OperatorAnswer> {
        let (tx, rx) = oneshot::channel();
        self.requests.send((prompt, tx)).ok()?;
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
                Next::Wait => Err(Abort::Internal("the run has no runnable task".into())),
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
                e.message()
            }
            Abort::Internal(detail) => Message::Internal { detail },
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
        let (req_tx, mut req_rx) = mpsc::unbounded_channel();
        let ctx = TaskContext {
            seq,
            cancel: cancel.clone(),
            observer: Arc::clone(&self.observer),
            requests: req_tx,
        };
        let executor = Arc::clone(&self.executor);
        let mut running = executor.execute(task, ctx);
        let mut pending: Option<(u64, bool, oneshot::Sender<Option<OperatorAnswer>>)> = None;
        let mut failed: Option<Abort> = None;

        let outcome = loop {
            tokio::select! {
                outcome = &mut running => break outcome,
                Some((prompt, reply)) = req_rx.recv() => {
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
                    let trust = matches!(prompt, OperatorPrompt::HostTrust(_));
                    pending = Some((id, trust, reply));
                    self.observer.requested(&OperatorRequest { id, seq, prompt });
                }
                control = Self::recv_control(&mut self.control, self.control_open) => match control {
                    Some(Control::Cancel { reason }) => {
                        cancel.cancel();
                        if let Some((_, _, reply)) = pending.take() {
                            let _ = reply.send(None);
                        }
                        if failed.is_none() && let Err(abort) = self.step(RunEvent::CancelRequested { reason }) {
                            failed = Some(abort);
                        }
                    }
                    Some(Control::Answer { request, answer }) => {
                        let Some((id, _, _)) = &pending else { continue };
                        if *id != request {
                            continue;
                        }
                        let (_, trust, reply) = pending.take().expect("checked");
                        let event = match &answer {
                            Some(_) => RunEvent::CredentialProvided { seq },
                            None => {
                                cancel.cancel();
                                let reason = if trust {
                                    Message::TrustDeclined
                                } else {
                                    Message::SecretDeclined
                                };
                                RunEvent::CancelRequested { reason }
                            }
                        };
                        if let Err(abort) = self.step(event) {
                            failed = Some(abort);
                            cancel.cancel();
                            let _ = reply.send(None);
                        } else {
                            let _ = reply.send(answer);
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
