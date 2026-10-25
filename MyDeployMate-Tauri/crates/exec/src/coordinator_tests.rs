//! Coordinator with a scripted executor and an in-memory store (plan task 4.1). The demo
//! pipeline is account-api on staging: build, upload A/B, restart A/B, check A/B.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mdm_core::fixtures::demo_workspace;
use mdm_core::resolve::NoValues;
use mdm_core::*;
use pretty_assertions::assert_eq;
use secrecy::ExposeSecret;
use tokio::sync::mpsc;

use crate::coordinator::*;
use crate::local::{OutputChunk, Stream};

use TaskStatus::{Cancelled, Failed, Skipped, Succeeded, Unknown};

/// What the fake executor does for one task.
#[derive(Clone, Debug)]
enum Script {
    Succeed,
    Fail,
    /// Wait for cancel, then report this outcome.
    UntilCancelled(TaskOutcome),
    /// Ask for a credential; succeed with it, or report cancelled without one.
    AskCredential,
    /// Record a detail and succeed at once, before the coordinator reads the detail.
    Detail,
    /// Read operator input until stdin closes, then succeed.
    ReadStdin,
    /// Refuse input (as an upload does), then wait for cancel.
    RefuseStdin,
}

#[derive(Default)]
struct Shared {
    /// Last persisted state per run.
    saved: Option<(Run, Vec<Task>)>,
    saves: usize,
    /// Fail every save from this count on.
    fail_from: Option<usize>,
    /// Tasks the executor ran, with the status persisted at that moment.
    executed: Vec<(u32, TaskStatus)>,
    credentials_seen: Vec<Option<String>>,
    stdin_seen: Vec<String>,
}

#[derive(Clone, Default)]
struct MemStore(Arc<Mutex<Shared>>);

impl RunStore for MemStore {
    fn create_run(&mut self, m: &RunMachine, _plan: &ResolvedPlan) -> Result<(), StorageError> {
        self.save_run(m)
    }

    fn save_run(&mut self, m: &RunMachine) -> Result<(), StorageError> {
        let mut s = self.0.lock().unwrap();
        s.saves += 1;
        if s.fail_from.is_some_and(|n| s.saves >= n) {
            return Err(StorageError(Message::text("disk full")));
        }
        s.saved = Some((m.run().clone(), m.tasks().to_vec()));
        Ok(())
    }
}

struct FakeExecutor {
    scripts: HashMap<u32, Script>,
    shared: Arc<Mutex<Shared>>,
}

impl TaskExecutor for FakeExecutor {
    fn execute<'a>(&'a self, task: &'a PlannedTask, ctx: TaskContext) -> TaskFuture<'a> {
        Box::pin(async move {
            {
                let mut s = self.shared.lock().unwrap();
                let persisted = s.saved.as_ref().unwrap().1[task.seq as usize].status;
                s.executed.push((task.seq, persisted));
            }
            ctx.output(OutputChunk {
                stream: Stream::Stdout,
                text: format!("task {}\n", task.seq),
                bytes: Vec::new(),
                lossy: false,
            });
            match self
                .scripts
                .get(&task.seq)
                .cloned()
                .unwrap_or(Script::Succeed)
            {
                Script::Succeed => TaskOutcome::Succeeded { exit_code: Some(0) },
                Script::Fail => TaskOutcome::Failed {
                    exit_code: Some(1),
                    exit_signal: None,
                    reason: Message::ExitCode { code: 1 },
                },
                Script::Detail => {
                    ctx.detail(TaskDetail::Upload(UploadDetail {
                        committed: vec![format!("/srv/{}", task.seq)],
                        ..Default::default()
                    }));
                    TaskOutcome::Succeeded { exit_code: None }
                }
                Script::ReadStdin => {
                    let mut stdin = ctx.take_stdin();
                    while let Some(StdinInput::Line { text, .. }) = stdin.recv().await {
                        let text = text.expose_secret().to_owned();
                        self.shared.lock().unwrap().stdin_seen.push(text);
                    }
                    TaskOutcome::Succeeded { exit_code: Some(0) }
                }
                Script::RefuseStdin => {
                    drop(ctx.take_stdin());
                    ctx.cancel.cancelled().await;
                    TaskOutcome::Cancelled
                }
                Script::UntilCancelled(outcome) => {
                    ctx.cancel.cancelled().await;
                    outcome
                }
                Script::AskCredential => {
                    let answer = ctx.ask(OperatorPrompt::Credential(password_prompt())).await;
                    let seen = match answer {
                        Some(OperatorAnswer::Secret { value, .. }) => {
                            Some(value.expose_secret().to_owned())
                        }
                        _ => None,
                    };
                    self.shared
                        .lock()
                        .unwrap()
                        .credentials_seen
                        .push(seen.clone());
                    match seen {
                        Some(_) => TaskOutcome::Succeeded { exit_code: Some(0) },
                        None => TaskOutcome::Cancelled,
                    }
                }
            }
        })
    }
}

fn password_prompt() -> CredentialPrompt {
    CredentialPrompt {
        purpose: CredentialPurpose::SshPassword,
        server_name: "staging-api-01".into(),
        address: "192.0.2.21:22".into(),
        user: "deploy".into(),
        key_path: None,
        context: Message::TaskContext {
            n: 2,
            step: "deploy".into(),
        },
        attempt: 1,
        max_attempts: 3,
        rejected: false,
        saved_rejected: false,
        remember_default: false,
    }
}

#[derive(Debug)]
enum Seen {
    Changed(RunStatus),
    Output(u32),
    Credential(Box<OperatorRequest>),
    StorageFailed,
    Note(u32, Message),
    Secret(String),
}

struct Observer(mpsc::UnboundedSender<Seen>);

impl RunObserver for Observer {
    fn changed(&self, m: &RunMachine) {
        let _ = self.0.send(Seen::Changed(m.run().status));
    }
    fn output(&self, seq: u32, _chunk: OutputChunk) {
        let _ = self.0.send(Seen::Output(seq));
    }
    fn note(&self, seq: u32, message: Message) {
        let _ = self.0.send(Seen::Note(seq, message));
    }
    fn secret(&self, secret: &str) {
        let _ = self.0.send(Seen::Secret(secret.to_owned()));
    }
    fn requested(&self, request: &OperatorRequest) {
        let _ = self.0.send(Seen::Credential(Box::new(request.clone())));
    }
    fn storage_failed(&self, _error: &StorageError) {
        let _ = self.0.send(Seen::StorageFailed);
    }
}

struct TickClock(AtomicI64);

impl Clock for TickClock {
    fn now_ms(&self) -> i64 {
        self.0.fetch_add(1, Ordering::Relaxed)
    }
}

fn plan(
    order: ExecutionOrder,
    policy: PipelineFailurePolicy,
    confirm_after_upload: bool,
) -> ResolvedPlan {
    let mut demo = demo_workspace();
    let p = &mut demo.workspace.projects[0];
    let pl = &mut p.pipelines[0];
    pl.execution_order = order;
    pl.failure_policy = policy;
    if confirm_after_upload {
        let mut confirm = pl.steps.remove(4);
        confirm.enabled = true;
        pl.steps.truncate(3);
        pl.steps.insert(2, confirm);
    }
    let req = PlanRequest {
        project_id: p.id,
        pipeline_id: pl.id,
        environment_id: demo.staging,
        scope: RunScope::Full,
        server_ids: None,
        version: None,
    };
    let plan = build_plan(&demo.workspace, &req, &NoValues).unwrap();
    assert!(plan.can_start(), "{:#?}", plan.issues);
    plan
}

struct Harness {
    store: MemStore,
    control: mpsc::UnboundedSender<Control>,
    seen: mpsc::UnboundedReceiver<Seen>,
    done: tokio::task::JoinHandle<RunStatus>,
}

impl Harness {
    fn start(plan: ResolvedPlan, scripts: &[(u32, Script)], fail_from: Option<usize>) -> Self {
        let store = MemStore::default();
        store.0.lock().unwrap().fail_from = fail_from;
        let (control, control_rx) = mpsc::unbounded_channel();
        let (seen_tx, seen) = mpsc::unbounded_channel();
        let executor = FakeExecutor {
            scripts: scripts.iter().cloned().collect(),
            shared: Arc::clone(&store.0),
        };
        let coordinator = Coordinator::create(
            Arc::new(plan),
            RunId::new(),
            store.clone(),
            Arc::new(TickClock(AtomicI64::new(1_000))),
            Arc::new(executor),
            Arc::new(Observer(seen_tx)),
            control_rx,
        )
        .map_err(|(e, _)| e)
        .unwrap();
        let done = tokio::spawn(async move { coordinator.run().await.0 });
        Harness {
            store,
            control,
            seen,
            done,
        }
    }

    /// Wait until the observer reports a state matching `f`.
    async fn until(&mut self, f: impl Fn(&Seen) -> bool) -> Seen {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let s = self.seen.recv().await.expect("observer open");
                if f(&s) {
                    return s;
                }
            }
        })
        .await
        .expect("expected observer event")
    }

    async fn finish(self) -> (RunStatus, Vec<TaskStatus>, Shared) {
        let status = tokio::time::timeout(Duration::from_secs(5), self.done)
            .await
            .expect("run finished")
            .unwrap();
        let shared = std::mem::take(&mut *self.store.0.lock().unwrap());
        let tasks = shared
            .saved
            .as_ref()
            .unwrap()
            .1
            .iter()
            .map(|t| t.status)
            .collect();
        (status, tasks, shared)
    }
}

fn step_first(policy: PipelineFailurePolicy) -> ResolvedPlan {
    plan(ExecutionOrder::StepFirst, policy, false)
}

#[tokio::test]
async fn every_dispatch_is_persisted_first_and_all_succeed() {
    let h = Harness::start(step_first(PipelineFailurePolicy::Stop), &[], None);
    let (status, tasks, shared) = h.finish().await;
    assert_eq!(status, RunStatus::Succeeded);
    assert_eq!(tasks, [Succeeded; 7]);
    assert_eq!(shared.saved.unwrap().0.status, RunStatus::Succeeded);
    // Each executor call saw its own task already recorded as running (ARC-14).
    assert_eq!(
        shared.executed,
        (0..7).map(|s| (s, TaskStatus::Running)).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn ac15_stop_policy() {
    let h = Harness::start(
        step_first(PipelineFailurePolicy::Stop),
        &[(2, Script::Fail)],
        None,
    );
    let (status, tasks, shared) = h.finish().await;
    assert_eq!(status, RunStatus::Failed);
    assert_eq!(
        tasks,
        [
            Succeeded, Succeeded, Failed, Skipped, Skipped, Skipped, Skipped
        ]
    );
    assert_eq!(shared.executed.len(), 3);
}

#[tokio::test]
async fn ac16_continue_policy() {
    let h = Harness::start(
        step_first(PipelineFailurePolicy::Continue),
        &[(2, Script::Fail)],
        None,
    );
    let (status, tasks, shared) = h.finish().await;
    assert_eq!(status, RunStatus::Failed);
    assert_eq!(
        tasks,
        [
            Succeeded, Succeeded, Failed, Succeeded, Succeeded, Succeeded, Succeeded
        ]
    );
    assert_eq!(shared.executed.len(), 7);
}

#[tokio::test]
async fn ac27_skip_server_policy() {
    // Server-first: build, upload A, restart A, check A, upload B, restart B, check B.
    let plan = plan(
        ExecutionOrder::ServerFirst,
        PipelineFailurePolicy::SkipServer,
        false,
    );
    let h = Harness::start(plan, &[(1, Script::Fail)], None);
    let (status, tasks, _) = h.finish().await;
    assert_eq!(status, RunStatus::Failed);
    assert_eq!(
        tasks,
        [
            Succeeded, Failed, Skipped, Skipped, Succeeded, Succeeded, Succeeded
        ]
    );
}

#[tokio::test]
async fn ac19_confirm_waits_then_proceeds() {
    // Build, upload A, upload B, confirm, restart A, restart B.
    let plan = plan(ExecutionOrder::StepFirst, PipelineFailurePolicy::Stop, true);
    let confirm = plan
        .tasks
        .iter()
        .find(|t| t.kind == StepKind::Confirm)
        .unwrap()
        .seq;
    let mut h = Harness::start(plan, &[], None);
    h.until(|s| matches!(s, Seen::Changed(RunStatus::WaitingConfirmation)))
        .await;
    // Nothing after the confirm task has started.
    assert_eq!(h.store.0.lock().unwrap().executed.len(), confirm as usize);
    h.control
        .send(Control::Confirm {
            seq: confirm + 1,
            proceed: true,
        })
        .unwrap(); // wrong task: ignored
    h.control
        .send(Control::Confirm {
            seq: confirm,
            proceed: true,
        })
        .unwrap();
    let (status, tasks, _) = h.finish().await;
    assert_eq!(status, RunStatus::Succeeded);
    assert_eq!(tasks, [Succeeded; 6]);
}

#[tokio::test]
async fn ac19_confirm_cancel_skips_the_rest() {
    let plan = plan(ExecutionOrder::StepFirst, PipelineFailurePolicy::Stop, true);
    let confirm = plan
        .tasks
        .iter()
        .find(|t| t.kind == StepKind::Confirm)
        .unwrap()
        .seq;
    let mut h = Harness::start(plan, &[], None);
    h.until(|s| matches!(s, Seen::Changed(RunStatus::WaitingConfirmation)))
        .await;
    h.control
        .send(Control::Confirm {
            seq: confirm,
            proceed: false,
        })
        .unwrap();
    let (status, tasks, shared) = h.finish().await;
    assert_eq!(status, RunStatus::Cancelled);
    assert_eq!(
        tasks,
        [Succeeded, Succeeded, Succeeded, Cancelled, Skipped, Skipped]
    );
    assert_eq!(shared.executed.len(), 3);
}

#[tokio::test]
async fn cancel_stops_the_running_task_and_skips_the_rest() {
    let mut h = Harness::start(
        step_first(PipelineFailurePolicy::Continue),
        &[(1, Script::UntilCancelled(TaskOutcome::Cancelled))],
        None,
    );
    h.until(|s| matches!(s, Seen::Output(1))).await;
    h.control
        .send(Control::Cancel {
            reason: Message::UserCancelled,
        })
        .unwrap();
    h.until(|s| matches!(s, Seen::Changed(RunStatus::Cancelling)))
        .await;
    let (status, tasks, shared) = h.finish().await;
    assert_eq!(status, RunStatus::Cancelled);
    assert_eq!(
        tasks,
        [
            Succeeded, Cancelled, Skipped, Skipped, Skipped, Skipped, Skipped
        ]
    );
    assert_eq!(shared.executed.len(), 2);
}

#[tokio::test]
async fn unconfirmed_cancel_interrupts_the_run() {
    let mut h = Harness::start(
        step_first(PipelineFailurePolicy::Continue),
        &[(
            0,
            Script::UntilCancelled(TaskOutcome::Unknown {
                reason: Message::ConnectionLost,
            }),
        )],
        None,
    );
    h.until(|s| matches!(s, Seen::Output(0))).await;
    h.control
        .send(Control::Cancel {
            reason: Message::UserCancelled,
        })
        .unwrap();
    let (status, tasks, _) = h.finish().await;
    assert_eq!(status, RunStatus::Interrupted);
    assert_eq!(tasks[0], Unknown);
    assert!(tasks[1..].iter().all(|t| *t == Skipped));
}

#[tokio::test]
async fn credential_wait_then_provided() {
    let mut h = Harness::start(
        step_first(PipelineFailurePolicy::Stop),
        &[(1, Script::AskCredential)],
        None,
    );
    let Seen::Credential(req) = h.until(|s| matches!(s, Seen::Credential(_))).await else {
        unreachable!()
    };
    assert_eq!(req.seq, 1);
    // The wait is recorded before the prompt is shown.
    let saved = h.store.0.lock().unwrap().saved.clone().unwrap();
    assert_eq!(saved.0.status, RunStatus::WaitingCredential);
    assert_eq!(saved.1[1].status, TaskStatus::WaitingCredential);
    h.control
        .send(Control::Answer {
            request: req.id + 1,
            answer: None,
        })
        .unwrap(); // stale request id: ignored
    h.control
        .send(Control::Answer {
            request: req.id,
            answer: Some(OperatorAnswer::Secret {
                value: "s3cret".into(),
                remember: false,
            }),
        })
        .unwrap();
    let (status, _, shared) = h.finish().await;
    assert_eq!(status, RunStatus::Succeeded);
    assert_eq!(shared.credentials_seen, [Some("s3cret".to_owned())]);
}

#[tokio::test]
async fn cancelling_the_credential_prompt_cancels_the_run() {
    let mut h = Harness::start(
        step_first(PipelineFailurePolicy::Continue),
        &[(1, Script::AskCredential)],
        None,
    );
    let Seen::Credential(req) = h.until(|s| matches!(s, Seen::Credential(_))).await else {
        unreachable!()
    };
    h.control
        .send(Control::Answer {
            request: req.id,
            answer: None,
        })
        .unwrap();
    let (status, tasks, shared) = h.finish().await;
    assert_eq!(status, RunStatus::Cancelled);
    assert_eq!(tasks[1], Cancelled);
    assert!(tasks[2..].iter().all(|t| *t == Skipped));
    assert_eq!(shared.credentials_seen, [None]);
}

#[tokio::test]
async fn failed_write_dispatches_nothing_more() {
    // Saves: 1 create, 2 start, 3 task 0 started, 4 task 0 done, 5 task 1 started (fails).
    let mut h = Harness::start(step_first(PipelineFailurePolicy::Stop), &[], Some(5));
    h.until(|s| matches!(s, Seen::StorageFailed)).await;
    let status = h.done.await.unwrap();
    assert_eq!(status, RunStatus::Interrupted);
    let shared = h.store.0.lock().unwrap();
    // Task 1 was never handed to the executor.
    assert_eq!(shared.executed, [(0, TaskStatus::Running)]);
}

#[test]
fn failed_create_runs_nothing() {
    let store = MemStore::default();
    store.0.lock().unwrap().fail_from = Some(1);
    let (_control, control_rx) = mpsc::unbounded_channel();
    let (seen_tx, _seen) = mpsc::unbounded_channel();
    let result = Coordinator::create(
        Arc::new(step_first(PipelineFailurePolicy::Stop)),
        RunId::new(),
        store.clone(),
        Arc::new(TickClock(AtomicI64::new(0))),
        Arc::new(FakeExecutor {
            scripts: HashMap::new(),
            shared: Arc::clone(&store.0),
        }),
        Arc::new(Observer(seen_tx)),
        control_rx,
    );
    assert!(result.is_err());
    assert!(store.0.lock().unwrap().executed.is_empty());
}

#[tokio::test]
async fn details_are_recorded_before_the_outcome() {
    let plan = plan(
        ExecutionOrder::StepFirst,
        PipelineFailurePolicy::Stop,
        false,
    );
    let h = Harness::start(plan, &[(1, Script::Detail)], None);
    let (status, _, shared) = h.finish().await;
    assert_eq!(status, RunStatus::Succeeded);
    let tasks = shared.saved.unwrap().1;
    assert_eq!(
        tasks[1].detail,
        Some(TaskDetail::Upload(UploadDetail {
            committed: vec!["/srv/1".into()],
            ..Default::default()
        }))
    );
    assert!(
        tasks
            .iter()
            .enumerate()
            .all(|(i, t)| i == 1 || t.detail.is_none())
    );
}

fn line(text: &str, hidden: bool) -> StdinInput {
    StdinInput::Line {
        text: text.into(),
        hidden,
    }
}

#[tokio::test]
async fn operator_input_reaches_the_running_task_and_is_echoed() {
    let mut h = Harness::start(
        step_first(PipelineFailurePolicy::Stop),
        &[(1, Script::ReadStdin)],
        None,
    );
    h.until(|s| matches!(s, Seen::Output(1))).await;
    let send = |seq, input| h.control.send(Control::Stdin { seq, input }).unwrap();
    // Input for another task is ignored.
    send(2, line("wrong task", false));
    send(1, line("deploy", false));
    send(1, line("p4ss", true));
    send(1, StdinInput::Close);
    let mut notes = Vec::new();
    let mut secrets = Vec::new();
    while notes.len() < 3 {
        match h
            .until(|s| matches!(s, Seen::Note(..) | Seen::Secret(_)))
            .await
        {
            Seen::Note(seq, m) => notes.push((seq, m)),
            Seen::Secret(s) => secrets.push(s),
            _ => unreachable!(),
        }
    }
    assert_eq!(
        notes,
        [
            (
                1,
                Message::StdinLine {
                    text: "deploy".into()
                }
            ),
            (1, Message::StdinHidden),
            (1, Message::StdinClosed),
        ]
    );
    // A hidden line is masked before it is sent.
    assert_eq!(secrets, ["p4ss"]);
    let (status, _, shared) = h.finish().await;
    assert_eq!(status, RunStatus::Succeeded);
    assert_eq!(shared.stdin_seen, ["deploy", "p4ss"]);
}

#[tokio::test]
async fn input_for_a_task_that_reads_none_is_refused() {
    let mut h = Harness::start(
        step_first(PipelineFailurePolicy::Stop),
        &[(1, Script::RefuseStdin)],
        None,
    );
    h.until(|s| matches!(s, Seen::Output(1))).await;
    h.control
        .send(Control::Stdin {
            seq: 1,
            input: line("ignored", false),
        })
        .unwrap();
    let Seen::Note(1, m) = h.until(|s| matches!(s, Seen::Note(..))).await else {
        unreachable!()
    };
    assert_eq!(m, Message::StdinRefused);
    h.control
        .send(Control::Cancel {
            reason: Message::text("stop"),
        })
        .unwrap();
    let (status, _, _) = h.finish().await;
    assert_eq!(status, RunStatus::Cancelled);
}
