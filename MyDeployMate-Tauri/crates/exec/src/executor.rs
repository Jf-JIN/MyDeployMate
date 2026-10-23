//! The task executor the app uses: local commands, and remote commands over SSH (phase 5).
//! Uploads and sudo arrive in phase 6.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use mdm_core::{Message, PlannedTask, PrivilegePolicy, ServerId, Shell, TaskAction};
use tokio::sync::Mutex;

use crate::coordinator::{TaskContext, TaskExecutor, TaskFuture, TaskOutcome};
use crate::local::{LocalCommand, LocalEnd, LocalOutcome, run_local};
use crate::remote::{RemoteEnd, Stop, run_remote, wrap_command};
use crate::shell::{Invocation, program_name, resolve_shell};
use crate::ssh::SshSession;

pub type ConnectFuture<'a> =
    Pin<Box<dyn Future<Output = Result<SshSession, TaskOutcome>> + Send + 'a>>;

/// Opens the connection for a remote task: verifies the host (D02) and logs in (D04) through
/// `ctx`, using the server configuration frozen at the start of the run. A failure is the
/// task's outcome; nothing has run on the server yet.
pub trait RemoteConnector: Send + Sync {
    fn connect<'a>(&'a self, server: ServerId, ctx: &'a TaskContext) -> ConnectFuture<'a>;
}

/// Runs the tasks of one run. Connections are opened on first use and kept for the run's
/// later tasks on the same server; a connection found closed is opened again before the next
/// task starts, never while a command runs (ARC-23).
#[derive(Default)]
pub struct AppExecutor {
    remote: Option<Arc<dyn RemoteConnector>>,
    sessions: Mutex<HashMap<ServerId, Arc<SshSession>>>,
}

impl AppExecutor {
    pub fn new(remote: Arc<dyn RemoteConnector>) -> Self {
        Self {
            remote: Some(remote),
            sessions: Mutex::default(),
        }
    }

    /// Disconnect every session; call when the run has ended.
    pub async fn close(&self) {
        let sessions: Vec<_> = self.sessions.lock().await.drain().map(|(_, s)| s).collect();
        for s in sessions {
            s.close().await;
        }
    }

    async fn session(
        &self,
        server: ServerId,
        ctx: &TaskContext,
    ) -> Result<Arc<SshSession>, TaskOutcome> {
        let Some(remote) = &self.remote else {
            return Err(failed(Message::NoConnection));
        };
        let mut sessions = self.sessions.lock().await;
        if let Some(s) = sessions.get(&server)
            && !s.is_closed()
        {
            return Ok(Arc::clone(s));
        }
        sessions.remove(&server);
        let session = Arc::new(remote.connect(server, ctx).await?);
        sessions.insert(server, Arc::clone(&session));
        Ok(session)
    }

    async fn remote(&self, task: &PlannedTask, ctx: &TaskContext) -> TaskOutcome {
        let TaskAction::Remote {
            command,
            cwd,
            privilege,
            ..
        } = &task.action
        else {
            unreachable!("only remote tasks")
        };
        if *privilege != PrivilegePolicy::None {
            return failed(Message::SudoUnsupported);
        }
        let Some(server) = &task.server else {
            return failed(Message::NoServer);
        };
        let session = match self.session(server.id, ctx).await {
            Ok(s) => s,
            Err(outcome) => return outcome,
        };
        if ctx.cancel.is_cancelled() {
            return TaskOutcome::Cancelled;
        }
        let wrapped = wrap_command(command, cwd.as_ref().map(|c| c.expose()));
        let timeout = task.timeout_sec.map(|s| Duration::from_secs(s.into()));
        let outcome = match run_remote(&session, &wrapped, timeout, &ctx.cancel, |chunk| {
            ctx.output(chunk)
        })
        .await
        {
            Ok(o) => o,
            // The channel could not be opened, so nothing was started.
            Err(e) => {
                return failed(Message::StartFailed {
                    detail: e.to_string(),
                });
            }
        };
        if outcome.pgid.is_none() && matches!(outcome.end, RemoteEnd::Exited(_)) {
            ctx.note(Message::NoProcessGroup);
        }
        remote_outcome(task, outcome.end)
    }
}

impl TaskExecutor for AppExecutor {
    fn execute<'a>(&'a self, task: &'a PlannedTask, ctx: TaskContext) -> TaskFuture<'a> {
        Box::pin(async move {
            match &task.action {
                TaskAction::Local {
                    command,
                    cwd,
                    shell,
                    wsl_distro,
                    output_encoding,
                } => {
                    let Some(program) = resolve_shell(*shell) else {
                        return failed(Message::ShellMissing {
                            program: program_name(*shell).into(),
                        });
                    };
                    let cmd = LocalCommand {
                        shell: *shell,
                        invocation: Invocation::new(
                            *shell,
                            program,
                            command.expose(),
                            cwd.as_ref().map(|c| c.expose()),
                            wsl_distro.as_deref(),
                        ),
                        encoding: *output_encoding,
                        timeout: task.timeout_sec.map(|s| Duration::from_secs(s.into())),
                    };
                    match run_local(&cmd, &ctx.cancel, |chunk| ctx.output(chunk)).await {
                        Ok(outcome) => local_outcome(&ctx, *shell, task, outcome),
                        Err(e) => failed(e.message()),
                    }
                }
                TaskAction::Remote { .. } => self.remote(task, &ctx).await,
                TaskAction::Upload { .. } => failed(Message::UploadUnsupported),
                TaskAction::Confirm { .. } => unreachable!("confirm tasks are never dispatched"),
            }
        })
    }
}

fn failed(reason: Message) -> TaskOutcome {
    TaskOutcome::Failed {
        exit_code: None,
        exit_signal: None,
        reason,
    }
}

fn remote_outcome(task: &PlannedTask, end: RemoteEnd) -> TaskOutcome {
    match end {
        RemoteEnd::Exited(0) => TaskOutcome::Succeeded { exit_code: Some(0) },
        RemoteEnd::Exited(code) => {
            let code = i32::try_from(code).unwrap_or(i32::MAX);
            TaskOutcome::Failed {
                exit_code: Some(code),
                exit_signal: None,
                reason: Message::ExitCode { code: code.into() },
            }
        }
        RemoteEnd::Signalled(sig) => TaskOutcome::Failed {
            exit_code: None,
            reason: Message::Signal {
                signal: sig.clone(),
            },
            exit_signal: Some(sig),
        },
        RemoteEnd::Refused => failed(Message::Refused),
        RemoteEnd::Lost => TaskOutcome::Unknown {
            reason: Message::CommandDisconnected,
        },
        RemoteEnd::Cancelled(Stop::Confirmed) => TaskOutcome::Cancelled,
        RemoteEnd::Cancelled(Stop::Unconfirmed(why)) => TaskOutcome::Unknown {
            reason: Message::CancelUnconfirmed { why: Box::new(why) },
        },
        RemoteEnd::TimedOut(stop) => {
            let limit = task.timeout_sec.unwrap_or_default();
            match stop {
                Stop::Confirmed => failed(Message::RemoteTimeout { limit }),
                Stop::Unconfirmed(why) => TaskOutcome::Unknown {
                    reason: Message::RemoteTimeoutUnconfirmed {
                        limit,
                        why: Box::new(why),
                    },
                },
            }
        }
    }
}

fn local_outcome(
    ctx: &TaskContext,
    shell: Shell,
    task: &PlannedTask,
    outcome: LocalOutcome,
) -> TaskOutcome {
    if outcome.leftovers_terminated {
        ctx.note(Message::Leftovers);
    }
    match outcome.end {
        LocalEnd::Exited => match outcome.exit_code {
            Some(0) => TaskOutcome::Succeeded { exit_code: Some(0) },
            code => TaskOutcome::Failed {
                exit_code: code,
                exit_signal: None,
                reason: match code {
                    Some(c) => Message::ExitCode { code: c.into() },
                    None => Message::NoExitCode,
                },
            },
        },
        LocalEnd::TimedOut => {
            let limit = task.timeout_sec.unwrap_or_default();
            if outcome.tree_stopped {
                failed(Message::LocalTimeout { limit })
            } else {
                debug_assert_eq!(shell, Shell::Wsl);
                TaskOutcome::Unknown {
                    reason: Message::WslTimeout { limit },
                }
            }
        }
        LocalEnd::Cancelled if outcome.tree_stopped => TaskOutcome::Cancelled,
        LocalEnd::Cancelled => TaskOutcome::Unknown {
            reason: Message::WslCancelled,
        },
    }
}
