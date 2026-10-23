//! Run and task state machine (SRS RUN-04–06, architecture ARC-15 and ARCH-D04, plan task 2.5).
//!
//! The coordinator owns one [`RunMachine`] per run. It applies each event with
//! [`RunMachine::apply`] (persisting the records after every change, ARC-14) and asks
//! [`RunMachine::next`] what to do. The machine never performs IO and never decides timing.
//!
//! Failure policies:
//! - stop: every task not yet started is skipped, naming the failed task;
//! - continue: scheduling goes on, including later tasks of the failed server;
//! - skip server: the failed server's remaining tasks are skipped, the others go on; once every
//!   server is skipped, the remaining local and confirm tasks are skipped too;
//! - an unknown outcome stops the whole run whatever the policy (RUN-05).
//!
//! Final status (RUN-06): interrupted (unknown outcome or crash) > failed > cancelled > succeeded.

use thiserror::Error;

use crate::ids::{RunId, ServerId, TaskId};
use crate::model::{PipelineFailurePolicy, StepKind};
use crate::plan::{ResolvedPlan, RunScope};
use crate::run::{Run, RunStatus, Task, TaskStatus};

/// Something that happened to the run. `seq` is the task position in the plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunEvent {
    /// The plan and all task records are persisted; scheduling may begin.
    Start,
    /// The coordinator persisted the intent and handed the task to its executor. For a confirm
    /// task this means the confirmation request was shown.
    TaskStarted {
        seq: u32,
    },
    /// The executor needs a password or passphrase from the operator.
    CredentialRequested {
        seq: u32,
    },
    CredentialProvided {
        seq: u32,
    },
    TaskSucceeded {
        seq: u32,
        exit_code: Option<i32>,
    },
    /// Includes timeouts and errors before the command ran.
    TaskFailed {
        seq: u32,
        exit_code: Option<i32>,
        exit_signal: Option<String>,
        reason: String,
    },
    /// The remote result cannot be confirmed (lost channel, cancel not verified).
    TaskOutcomeUnknown {
        seq: u32,
        reason: String,
    },
    /// The running task stopped after a cancel request.
    TaskCancelled {
        seq: u32,
    },
    ConfirmationAnswered {
        seq: u32,
        proceed: bool,
    },
    /// Operator cancel, including cancelling a credential prompt (SEC-02).
    CancelRequested {
        reason: String,
    },
    /// Found unfinished at startup, or the coordinator lost track (RUN-07).
    Interrupted {
        reason: String,
    },
}

/// What the coordinator should do now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Next {
    /// Run the task with its executor, then report `TaskStarted`.
    Dispatch {
        seq: u32,
    },
    /// Show the confirmation request, then report `TaskStarted`.
    AwaitConfirmation {
        seq: u32,
    },
    /// Wait for an event: the run is queued, or a task is active.
    Wait,
    Done {
        status: RunStatus,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum TransitionError {
    #[error("运行已结束")]
    RunFinished,
    #[error("运行尚未开始")]
    NotStarted,
    #[error("运行已开始")]
    AlreadyStarted,
    #[error("任务 {0} 不存在")]
    UnknownTask(u32),
    #[error("任务 {seq} 当前状态为 {status:?}，不能执行该操作")]
    WrongTaskState { seq: u32, status: TaskStatus },
    #[error("任务 {seq} 不是下一个待执行任务")]
    OutOfOrder { seq: u32 },
    #[error("运行正在取消，不能开始新任务")]
    Cancelling,
}

/// Scheduling data for one task that the stored record does not keep.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskMeta {
    pub policy: PipelineFailurePolicy,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RunMachine {
    run: Run,
    tasks: Vec<Task>,
    meta: Vec<TaskMeta>,
    cancel_reason: Option<String>,
    skipped_servers: Vec<ServerId>,
}

impl RunMachine {
    /// Records for a new run from a plan that passed preflight.
    pub fn from_plan(plan: &ResolvedPlan, run_id: RunId, created_at_ms: i64) -> Self {
        let run = Run {
            id: run_id,
            project_id: plan.project_id,
            pipeline_id: plan.pipeline_id,
            environment_id: plan.environment_id,
            project_name: plan.project_name.clone(),
            pipeline_name: plan.pipeline_name.clone(),
            environment_name: plan.environment_name.clone(),
            version: plan.version.clone().unwrap_or_default(),
            execution_order: plan.execution_order,
            local_only: plan.scope == RunScope::LocalOnly,
            status: RunStatus::Queued,
            created_at_ms,
            finished_at_ms: None,
        };
        let tasks = plan
            .tasks
            .iter()
            .map(|t| Task {
                id: TaskId::new(),
                run_id,
                seq: t.seq,
                step_id: t.step_id,
                step_name: t.step_name.clone(),
                kind: t.kind,
                server_id: t.server.as_ref().map(|s| s.id),
                server_name: t.server.as_ref().map(|s| s.name.clone()),
                status: TaskStatus::Queued,
                exit_code: None,
                exit_signal: None,
                reason: None,
                started_at_ms: None,
                finished_at_ms: None,
            })
            .collect();
        let meta = plan
            .tasks
            .iter()
            .map(|t| TaskMeta {
                policy: t.failure_policy,
            })
            .collect();
        Self::new(run, tasks, meta)
    }

    /// Restore from stored records (e.g. to mark an unfinished run interrupted at startup).
    pub fn new(run: Run, tasks: Vec<Task>, meta: Vec<TaskMeta>) -> Self {
        assert_eq!(tasks.len(), meta.len(), "one TaskMeta per task");
        RunMachine {
            run,
            tasks,
            meta,
            cancel_reason: None,
            skipped_servers: Vec::new(),
        }
    }

    pub fn run(&self) -> &Run {
        &self.run
    }

    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    /// Scheduling data per task, in plan order.
    pub fn meta(&self) -> &[TaskMeta] {
        &self.meta
    }

    pub fn cancel_reason(&self) -> Option<&str> {
        self.cancel_reason.as_deref()
    }

    /// Servers skipped by the "skip this server" policy, in the order they were skipped.
    pub fn skipped_servers(&self) -> &[ServerId] {
        &self.skipped_servers
    }

    pub fn next(&self) -> Next {
        if self.run.status.is_terminal() {
            return Next::Done {
                status: self.run.status,
            };
        }
        if self.run.status == RunStatus::Queued || self.active().is_some() {
            return Next::Wait;
        }
        match self.tasks.iter().find(|t| t.status == TaskStatus::Queued) {
            Some(t) if t.kind == StepKind::Confirm => Next::AwaitConfirmation { seq: t.seq },
            Some(t) => Next::Dispatch { seq: t.seq },
            // Unreachable after `apply`, which finishes the run when nothing is left.
            None => Next::Wait,
        }
    }

    pub fn apply(&mut self, event: RunEvent, at_ms: i64) -> Result<(), TransitionError> {
        if self.run.status.is_terminal() {
            return Err(TransitionError::RunFinished);
        }
        match event {
            RunEvent::Start => {
                if self.run.status != RunStatus::Queued {
                    return Err(TransitionError::AlreadyStarted);
                }
                self.run.status = RunStatus::Running;
            }
            RunEvent::Interrupted { reason } => {
                for t in &mut self.tasks {
                    if is_active(t.status) {
                        t.status = TaskStatus::Unknown;
                        t.reason = Some(reason.clone());
                        t.finished_at_ms = Some(at_ms);
                    }
                }
                self.skip_queued(|_| true, &reason, at_ms);
                self.finish(RunStatus::Interrupted, at_ms);
                return Ok(());
            }
            RunEvent::CancelRequested { reason } => self.cancel(reason, at_ms),
            other => {
                if self.run.status == RunStatus::Queued {
                    return Err(TransitionError::NotStarted);
                }
                self.task_event(other, at_ms)?;
            }
        }
        self.settle(at_ms);
        Ok(())
    }

    fn task_event(&mut self, event: RunEvent, at_ms: i64) -> Result<(), TransitionError> {
        match event {
            RunEvent::TaskStarted { seq } => {
                if self.run.status == RunStatus::Cancelling {
                    return Err(TransitionError::Cancelling);
                }
                let expected = match self.next() {
                    Next::Dispatch { seq } | Next::AwaitConfirmation { seq } => Some(seq),
                    _ => None,
                };
                if expected != Some(seq) {
                    self.index(seq)?;
                    return Err(TransitionError::OutOfOrder { seq });
                }
                let t = self.task_mut(seq)?;
                t.status = if t.kind == StepKind::Confirm {
                    TaskStatus::WaitingConfirmation
                } else {
                    TaskStatus::Running
                };
                t.started_at_ms = Some(at_ms);
            }
            RunEvent::CredentialRequested { seq } => {
                self.expect(seq, &[TaskStatus::Running])?.status = TaskStatus::WaitingCredential;
            }
            RunEvent::CredentialProvided { seq } => {
                self.expect(seq, &[TaskStatus::WaitingCredential])?.status = TaskStatus::Running;
            }
            RunEvent::TaskSucceeded { seq, exit_code } => {
                let t = self.expect(seq, &[TaskStatus::Running])?;
                t.status = TaskStatus::Succeeded;
                t.exit_code = exit_code;
                t.finished_at_ms = Some(at_ms);
            }
            RunEvent::TaskFailed {
                seq,
                exit_code,
                exit_signal,
                reason,
            } => {
                let t = self.expect(seq, &[TaskStatus::Running, TaskStatus::WaitingCredential])?;
                t.status = TaskStatus::Failed;
                t.exit_code = exit_code;
                t.exit_signal = exit_signal;
                t.reason = Some(reason);
                t.finished_at_ms = Some(at_ms);
                self.apply_policy(seq, at_ms);
            }
            RunEvent::TaskOutcomeUnknown { seq, reason } => {
                let t = self.expect(seq, &[TaskStatus::Running, TaskStatus::WaitingCredential])?;
                t.status = TaskStatus::Unknown;
                t.reason = Some(reason);
                t.finished_at_ms = Some(at_ms);
                let why = format!("任务 {} 结果未知，运行已停止", seq + 1);
                self.skip_queued(|_| true, &why, at_ms);
            }
            RunEvent::TaskCancelled { seq } => {
                let t = self.expect(seq, &[TaskStatus::Running, TaskStatus::WaitingCredential])?;
                t.status = TaskStatus::Cancelled;
                t.finished_at_ms = Some(at_ms);
                // Stopping a task always stops the run (STEP-01).
                self.cancel("用户取消".into(), at_ms);
            }
            RunEvent::ConfirmationAnswered { seq, proceed } => {
                let t = self.expect(seq, &[TaskStatus::WaitingConfirmation])?;
                t.finished_at_ms = Some(at_ms);
                if proceed {
                    t.status = TaskStatus::Succeeded;
                } else {
                    t.status = TaskStatus::Cancelled;
                    t.reason = Some("人工确认时选择取消".into());
                    self.cancel("人工确认时选择取消".into(), at_ms);
                }
            }
            RunEvent::Start | RunEvent::CancelRequested { .. } | RunEvent::Interrupted { .. } => {
                unreachable!("handled in apply")
            }
        }
        Ok(())
    }

    fn cancel(&mut self, reason: String, at_ms: i64) {
        self.cancel_reason.get_or_insert(reason);
        self.skip_queued(|_| true, "运行已取消，任务未开始", at_ms);
        // Nothing executes while waiting for the operator, so those tasks stop right away.
        for t in &mut self.tasks {
            if matches!(
                t.status,
                TaskStatus::WaitingConfirmation | TaskStatus::WaitingCredential
            ) {
                t.status = TaskStatus::Cancelled;
                t.finished_at_ms = Some(at_ms);
            }
        }
    }

    fn apply_policy(&mut self, seq: u32, at_ms: i64) {
        let i = seq as usize;
        let failed = &self.tasks[i];
        let label = match &failed.server_name {
            Some(server) => format!("任务 {}（{} · {}）", seq + 1, failed.step_name, server),
            None => format!("任务 {}（{}）", seq + 1, failed.step_name),
        };
        match self.meta[i].policy {
            PipelineFailurePolicy::Continue => {}
            PipelineFailurePolicy::Stop => {
                self.skip_queued(|_| true, &format!("{label}失败，后续任务已跳过"), at_ms);
            }
            PipelineFailurePolicy::SkipServer => {
                let Some(server) = failed.server_id else {
                    self.skip_queued(|_| true, &format!("{label}失败，后续任务已跳过"), at_ms);
                    return;
                };
                let name = failed.server_name.clone().unwrap_or_default();
                if !self.skipped_servers.contains(&server) {
                    self.skipped_servers.push(server);
                }
                self.skip_queued(
                    |t| t.server_id == Some(server),
                    &format!("服务器 {name} 已跳过（任务 {} 失败）", seq + 1),
                    at_ms,
                );
                let all_skipped = self
                    .tasks
                    .iter()
                    .filter_map(|t| t.server_id)
                    .all(|s| self.skipped_servers.contains(&s));
                if all_skipped {
                    self.skip_queued(|_| true, "所有服务器均已跳过", at_ms);
                }
            }
        }
    }

    fn skip_queued(&mut self, which: impl Fn(&Task) -> bool, reason: &str, at_ms: i64) {
        for t in &mut self.tasks {
            if t.status == TaskStatus::Queued && which(t) {
                t.status = TaskStatus::Skipped;
                t.reason = Some(reason.to_owned());
                t.finished_at_ms = Some(at_ms);
            }
        }
    }

    /// Recompute the run status after an event, finishing the run when nothing is left.
    fn settle(&mut self, at_ms: i64) {
        if let Some(active) = self.active() {
            self.run.status = if self.cancel_reason.is_some() {
                RunStatus::Cancelling
            } else {
                match active.status {
                    TaskStatus::WaitingConfirmation => RunStatus::WaitingConfirmation,
                    TaskStatus::WaitingCredential => RunStatus::WaitingCredential,
                    _ => RunStatus::Running,
                }
            };
        } else if self.tasks.iter().any(|t| t.status == TaskStatus::Queued) {
            self.run.status = RunStatus::Running;
        } else {
            let status = summarize(&self.tasks, self.cancel_reason.is_some());
            self.finish(status, at_ms);
        }
    }

    fn finish(&mut self, status: RunStatus, at_ms: i64) {
        self.run.status = status;
        self.run.finished_at_ms = Some(at_ms);
    }

    fn active(&self) -> Option<&Task> {
        self.tasks.iter().find(|t| is_active(t.status))
    }

    fn index(&self, seq: u32) -> Result<usize, TransitionError> {
        let i = seq as usize;
        if i < self.tasks.len() {
            Ok(i)
        } else {
            Err(TransitionError::UnknownTask(seq))
        }
    }

    fn task_mut(&mut self, seq: u32) -> Result<&mut Task, TransitionError> {
        let i = self.index(seq)?;
        Ok(&mut self.tasks[i])
    }

    fn expect(&mut self, seq: u32, allowed: &[TaskStatus]) -> Result<&mut Task, TransitionError> {
        let t = self.task_mut(seq)?;
        if allowed.contains(&t.status) {
            Ok(t)
        } else {
            Err(TransitionError::WrongTaskState {
                seq,
                status: t.status,
            })
        }
    }
}

fn is_active(status: TaskStatus) -> bool {
    matches!(
        status,
        TaskStatus::Running | TaskStatus::WaitingConfirmation | TaskStatus::WaitingCredential
    )
}

/// RUN-06 final status of finished tasks.
pub fn summarize(tasks: &[Task], cancelled: bool) -> RunStatus {
    if tasks.iter().any(|t| t.status == TaskStatus::Unknown) {
        RunStatus::Interrupted
    } else if tasks.iter().any(|t| t.status == TaskStatus::Failed) {
        RunStatus::Failed
    } else if cancelled || tasks.iter().any(|t| t.status == TaskStatus::Cancelled) {
        RunStatus::Cancelled
    } else if tasks.iter().all(|t| t.status == TaskStatus::Succeeded) {
        RunStatus::Succeeded
    } else {
        // Skipped tasks without a failure or cancel cannot happen through `RunMachine`.
        RunStatus::Failed
    }
}
