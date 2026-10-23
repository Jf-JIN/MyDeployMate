//! Run state machine. Phase 2 exit: the state transitions of AC-15, AC-16, AC-19, AC-22, AC-27.

use pretty_assertions::assert_eq;

use crate::fixtures::demo_workspace;
use crate::machine::{Next, RunEvent, RunMachine, TransitionError, summarize};
use crate::plan::{PlanRequest, RunScope, build_plan};
use crate::resolve::NoValues;
use crate::run::{RunStatus, TaskStatus};
use crate::*;

use TaskStatus::{Cancelled, Failed, Queued, Skipped, Succeeded, Unknown};

/// account-api on staging: build, upload A/B, restart A/B, check A/B (7 tasks), or the
/// server-first order of the same pipeline. `confirm_after_upload` swaps in AC-28's shape.
fn machine(
    order: ExecutionOrder,
    policy: PipelineFailurePolicy,
    confirm_after_upload: bool,
) -> RunMachine {
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
    RunMachine::from_plan(&plan, RunId::new(), 1_000)
}

/// Run tasks in order, succeeding each one unless `fail` names it.
fn drive(m: &mut RunMachine, fail: &[u32]) {
    let mut now = 2_000;
    m.apply(RunEvent::Start, now).unwrap();
    loop {
        now += 10;
        match m.next() {
            Next::Dispatch { seq } => {
                m.apply(RunEvent::TaskStarted { seq }, now).unwrap();
                let event = if fail.contains(&seq) {
                    RunEvent::TaskFailed {
                        seq,
                        exit_code: Some(1),
                        exit_signal: None,
                        reason: Message::ExitCode { code: 1 },
                    }
                } else {
                    RunEvent::TaskSucceeded {
                        seq,
                        exit_code: Some(0),
                    }
                };
                m.apply(event, now + 5).unwrap();
            }
            Next::AwaitConfirmation { seq } => {
                m.apply(RunEvent::TaskStarted { seq }, now).unwrap();
                m.apply(
                    RunEvent::ConfirmationAnswered { seq, proceed: true },
                    now + 5,
                )
                .unwrap();
            }
            Next::Done { .. } => break,
            Next::Wait => panic!("nothing to wait for in a synchronous drive"),
        }
    }
}

fn statuses(m: &RunMachine) -> Vec<TaskStatus> {
    m.tasks().iter().map(|t| t.status).collect()
}

#[test]
fn all_tasks_succeed() {
    let mut m = machine(
        ExecutionOrder::StepFirst,
        PipelineFailurePolicy::Stop,
        false,
    );
    assert_eq!(m.next(), Next::Wait);
    drive(&mut m, &[]);
    assert_eq!(statuses(&m), [Succeeded; 7]);
    assert_eq!(m.run().status, RunStatus::Succeeded);
    assert!(m.run().finished_at_ms.is_some());
    assert!(
        m.tasks()
            .iter()
            .all(|t| t.started_at_ms.is_some() && t.exit_code == Some(0))
    );
}

#[test]
fn ac15_stop_skips_the_remaining_four() {
    let mut m = machine(
        ExecutionOrder::StepFirst,
        PipelineFailurePolicy::Stop,
        false,
    );
    drive(&mut m, &[2]); // upload B
    assert_eq!(
        statuses(&m),
        [
            Succeeded, Succeeded, Failed, Skipped, Skipped, Skipped, Skipped
        ]
    );
    assert_eq!(m.run().status, RunStatus::Failed);
    assert_eq!(
        m.tasks()[3].reason.as_ref(),
        Some(&Message::AfterServerFailure {
            n: 3,
            step: "上传发布文件".into(),
            server: "staging-api-02".into(),
        })
    );
    assert_eq!(
        m.tasks()[2].reason.as_ref(),
        Some(&Message::ExitCode { code: 1 })
    );
    // Restart A never started.
    assert_eq!(m.tasks()[3].started_at_ms, None);
}

#[test]
fn ac16_continue_runs_everything_and_fails() {
    let mut m = machine(
        ExecutionOrder::StepFirst,
        PipelineFailurePolicy::Continue,
        false,
    );
    drive(&mut m, &[2]);
    assert_eq!(
        statuses(&m),
        [
            Succeeded, Succeeded, Failed, Succeeded, Succeeded, Succeeded, Succeeded
        ]
    );
    // B's restart still ran.
    assert_eq!(m.tasks()[4].server_name.as_deref(), Some("staging-api-02"));
    assert_eq!(m.run().status, RunStatus::Failed);
    assert_eq!(m.tasks()[2].exit_code, Some(1));
}

#[test]
fn ac27_skip_server_skips_only_the_failed_server() {
    let mut m = machine(
        ExecutionOrder::ServerFirst,
        PipelineFailurePolicy::SkipServer,
        false,
    );
    // Order: build, upload A, restart A, check A, upload B, restart B, check B.
    drive(&mut m, &[1]);
    assert_eq!(
        statuses(&m),
        [
            Succeeded, Failed, Skipped, Skipped, Succeeded, Succeeded, Succeeded
        ]
    );
    assert_eq!(
        m.tasks()[2].reason.as_ref(),
        Some(&Message::ServerSkipped {
            server: "staging-api-01".into(),
            n: 2,
        })
    );
    assert_eq!(m.skipped_servers().len(), 1);
    assert_eq!(m.run().status, RunStatus::Failed);
}

#[test]
fn skip_server_with_every_server_skipped_skips_local_and_confirm() {
    // Build, upload A, upload B, confirm, restart A, restart B.
    let mut m = machine(
        ExecutionOrder::ServerFirst,
        PipelineFailurePolicy::SkipServer,
        true,
    );
    drive(&mut m, &[1, 2]);
    assert_eq!(
        statuses(&m),
        [Succeeded, Failed, Failed, Skipped, Skipped, Skipped]
    );
    assert_eq!(
        m.tasks()[3].reason.as_ref(),
        Some(&Message::AllServersSkipped)
    );
    assert_eq!(m.skipped_servers().len(), 2);
    assert_eq!(m.run().status, RunStatus::Failed);
}

#[test]
fn ac19_confirm_waits_then_proceeds_or_cancels() {
    let mut m = machine(
        ExecutionOrder::ServerFirst,
        PipelineFailurePolicy::Stop,
        true,
    );
    m.apply(RunEvent::Start, 0).unwrap();
    for seq in 0..3 {
        assert_eq!(m.next(), Next::Dispatch { seq });
        m.apply(RunEvent::TaskStarted { seq }, 1).unwrap();
        m.apply(
            RunEvent::TaskSucceeded {
                seq,
                exit_code: Some(0),
            },
            2,
        )
        .unwrap();
    }
    assert_eq!(m.next(), Next::AwaitConfirmation { seq: 3 });
    m.apply(RunEvent::TaskStarted { seq: 3 }, 3).unwrap();
    assert_eq!(m.run().status, RunStatus::WaitingConfirmation);
    assert_eq!(m.tasks()[3].status, TaskStatus::WaitingConfirmation);
    // Nothing else is scheduled while waiting.
    assert_eq!(m.next(), Next::Wait);
    assert_eq!(
        m.apply(RunEvent::TaskStarted { seq: 4 }, 4),
        Err(TransitionError::OutOfOrder { seq: 4 })
    );

    let mut proceed = m.clone();
    proceed
        .apply(
            RunEvent::ConfirmationAnswered {
                seq: 3,
                proceed: true,
            },
            5,
        )
        .unwrap();
    assert_eq!(proceed.run().status, RunStatus::Running);
    assert_eq!(proceed.next(), Next::Dispatch { seq: 4 });

    m.apply(
        RunEvent::ConfirmationAnswered {
            seq: 3,
            proceed: false,
        },
        5,
    )
    .unwrap();
    assert_eq!(
        statuses(&m),
        [Succeeded, Succeeded, Succeeded, Cancelled, Skipped, Skipped]
    );
    assert_eq!(m.run().status, RunStatus::Cancelled);
    assert_eq!(
        m.next(),
        Next::Done {
            status: RunStatus::Cancelled
        }
    );
}

#[test]
fn ac22_unknown_outcome_stops_and_interrupts_even_with_continue() {
    let mut m = machine(
        ExecutionOrder::StepFirst,
        PipelineFailurePolicy::Continue,
        false,
    );
    m.apply(RunEvent::Start, 0).unwrap();
    for seq in 0..3 {
        m.apply(RunEvent::TaskStarted { seq }, 1).unwrap();
        m.apply(
            RunEvent::TaskSucceeded {
                seq,
                exit_code: Some(0),
            },
            2,
        )
        .unwrap();
    }
    m.apply(RunEvent::TaskStarted { seq: 3 }, 3).unwrap();
    m.apply(
        RunEvent::CancelRequested {
            reason: Message::UserCancelled,
        },
        4,
    )
    .unwrap();
    assert_eq!(m.run().status, RunStatus::Cancelling);
    assert_eq!(m.next(), Next::Wait);
    // Remaining tasks are skipped as soon as cancel is requested.
    assert_eq!(statuses(&m)[4..], [Skipped, Skipped, Skipped]);
    m.apply(
        RunEvent::TaskOutcomeUnknown {
            seq: 3,
            reason: Message::text("无法确认远程命令是否结束"),
        },
        5,
    )
    .unwrap();
    assert_eq!(m.tasks()[3].status, Unknown);
    assert_eq!(m.run().status, RunStatus::Interrupted);
    assert_eq!(m.cancel_reason(), Some(&Message::UserCancelled));
}

#[test]
fn unknown_outcome_without_cancel_skips_the_rest() {
    let mut m = machine(
        ExecutionOrder::ServerFirst,
        PipelineFailurePolicy::SkipServer,
        false,
    );
    m.apply(RunEvent::Start, 0).unwrap();
    m.apply(RunEvent::TaskStarted { seq: 0 }, 1).unwrap();
    m.apply(
        RunEvent::TaskSucceeded {
            seq: 0,
            exit_code: Some(0),
        },
        2,
    )
    .unwrap();
    m.apply(RunEvent::TaskStarted { seq: 1 }, 3).unwrap();
    m.apply(
        RunEvent::TaskOutcomeUnknown {
            seq: 1,
            reason: Message::text("超时后无法确认结果"),
        },
        4,
    )
    .unwrap();
    assert_eq!(
        statuses(&m),
        [
            Succeeded, Unknown, Skipped, Skipped, Skipped, Skipped, Skipped
        ]
    );
    assert_eq!(
        m.tasks()[6].reason.as_ref(),
        Some(&Message::OutcomeUnknown { n: 2 })
    );
    assert_eq!(m.run().status, RunStatus::Interrupted);
}

#[test]
fn cancel_while_running_then_task_stops() {
    let mut m = machine(
        ExecutionOrder::StepFirst,
        PipelineFailurePolicy::Stop,
        false,
    );
    m.apply(RunEvent::Start, 0).unwrap();
    m.apply(RunEvent::TaskStarted { seq: 0 }, 1).unwrap();
    m.apply(
        RunEvent::CancelRequested {
            reason: Message::UserCancelled,
        },
        2,
    )
    .unwrap();
    assert_eq!(
        m.apply(RunEvent::TaskStarted { seq: 1 }, 3),
        Err(TransitionError::Cancelling)
    );
    m.apply(RunEvent::TaskCancelled { seq: 0 }, 4).unwrap();
    assert_eq!(statuses(&m)[0], Cancelled);
    assert_eq!(m.run().status, RunStatus::Cancelled);
}

#[test]
fn cancel_after_an_earlier_failure_ends_failed() {
    let mut m = machine(
        ExecutionOrder::StepFirst,
        PipelineFailurePolicy::Continue,
        false,
    );
    m.apply(RunEvent::Start, 0).unwrap();
    m.apply(RunEvent::TaskStarted { seq: 0 }, 1).unwrap();
    m.apply(
        RunEvent::TaskFailed {
            seq: 0,
            exit_code: Some(2),
            exit_signal: None,
            reason: Message::ExitCode { code: 2 },
        },
        2,
    )
    .unwrap();
    m.apply(RunEvent::TaskStarted { seq: 1 }, 3).unwrap();
    m.apply(
        RunEvent::CancelRequested {
            reason: Message::UserCancelled,
        },
        4,
    )
    .unwrap();
    // The upload finished before it could be stopped.
    m.apply(
        RunEvent::TaskSucceeded {
            seq: 1,
            exit_code: None,
        },
        5,
    )
    .unwrap();
    assert_eq!(m.run().status, RunStatus::Failed);
    assert_eq!(m.cancel_reason(), Some(&Message::UserCancelled));
}

#[test]
fn credential_wait_and_cancelled_prompt() {
    let mut m = machine(
        ExecutionOrder::StepFirst,
        PipelineFailurePolicy::Stop,
        false,
    );
    m.apply(RunEvent::Start, 0).unwrap();
    m.apply(RunEvent::TaskStarted { seq: 0 }, 1).unwrap();
    m.apply(
        RunEvent::TaskSucceeded {
            seq: 0,
            exit_code: Some(0),
        },
        2,
    )
    .unwrap();
    m.apply(RunEvent::TaskStarted { seq: 1 }, 3).unwrap();
    m.apply(RunEvent::CredentialRequested { seq: 1 }, 4)
        .unwrap();
    assert_eq!(m.run().status, RunStatus::WaitingCredential);
    m.apply(RunEvent::CredentialProvided { seq: 1 }, 5).unwrap();
    assert_eq!(m.run().status, RunStatus::Running);
    m.apply(RunEvent::CredentialRequested { seq: 1 }, 6)
        .unwrap();
    // Cancelling the prompt cancels the whole run (SEC-02).
    m.apply(
        RunEvent::CancelRequested {
            reason: Message::SecretDeclined,
        },
        7,
    )
    .unwrap();
    assert_eq!(statuses(&m)[..3], [Succeeded, Cancelled, Skipped]);
    assert_eq!(m.run().status, RunStatus::Cancelled);
}

#[test]
fn cancel_before_start_and_crash_recovery() {
    let mut m = machine(
        ExecutionOrder::StepFirst,
        PipelineFailurePolicy::Stop,
        false,
    );
    m.apply(
        RunEvent::CancelRequested {
            reason: Message::UserCancelled,
        },
        1,
    )
    .unwrap();
    assert_eq!(statuses(&m), [Skipped; 7]);
    assert_eq!(m.run().status, RunStatus::Cancelled);
    assert_eq!(
        m.apply(RunEvent::Start, 2),
        Err(TransitionError::RunFinished)
    );

    let mut m = machine(
        ExecutionOrder::StepFirst,
        PipelineFailurePolicy::Stop,
        false,
    );
    m.apply(RunEvent::Start, 0).unwrap();
    m.apply(RunEvent::TaskStarted { seq: 0 }, 1).unwrap();
    m.apply(
        RunEvent::Interrupted {
            reason: Message::text("应用异常退出，无法确认任务结果"),
        },
        9,
    )
    .unwrap();
    assert_eq!(statuses(&m)[..2], [Unknown, Skipped]);
    assert_eq!(m.run().status, RunStatus::Interrupted);
}

#[test]
fn invalid_events_are_rejected() {
    let mut m = machine(
        ExecutionOrder::StepFirst,
        PipelineFailurePolicy::Stop,
        false,
    );
    assert_eq!(
        m.apply(RunEvent::TaskStarted { seq: 0 }, 0),
        Err(TransitionError::NotStarted)
    );
    m.apply(RunEvent::Start, 0).unwrap();
    assert_eq!(
        m.apply(RunEvent::Start, 0),
        Err(TransitionError::AlreadyStarted)
    );
    assert_eq!(
        m.apply(
            RunEvent::TaskSucceeded {
                seq: 0,
                exit_code: None
            },
            0
        ),
        Err(TransitionError::WrongTaskState {
            seq: 0,
            status: Queued
        })
    );
    assert_eq!(
        m.apply(RunEvent::TaskStarted { seq: 99 }, 0),
        Err(TransitionError::UnknownTask(99))
    );
    assert_eq!(
        m.apply(
            RunEvent::ConfirmationAnswered {
                seq: 0,
                proceed: true
            },
            0
        ),
        Err(TransitionError::WrongTaskState {
            seq: 0,
            status: Queued
        })
    );
}

#[test]
fn summary_priority() {
    let m = machine(
        ExecutionOrder::StepFirst,
        PipelineFailurePolicy::Stop,
        false,
    );
    let with = |statuses: &[TaskStatus]| {
        let mut tasks = m.tasks().to_vec();
        for (t, s) in tasks.iter_mut().zip(statuses) {
            t.status = *s;
        }
        tasks.truncate(statuses.len());
        tasks
    };
    assert_eq!(
        summarize(&with(&[Succeeded, Failed, Unknown]), true),
        RunStatus::Interrupted
    );
    assert_eq!(
        summarize(&with(&[Failed, Cancelled]), true),
        RunStatus::Failed
    );
    assert_eq!(
        summarize(&with(&[Succeeded, Skipped]), true),
        RunStatus::Cancelled
    );
    assert_eq!(
        summarize(&with(&[Succeeded, Succeeded]), false),
        RunStatus::Succeeded
    );
}
