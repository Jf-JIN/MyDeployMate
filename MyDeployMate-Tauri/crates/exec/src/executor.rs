//! The task executor the app uses: local commands, remote commands over SSH (phase 5) and
//! SFTP uploads (phase 6). sudo arrives with 6.2.

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mdm_core::log::size_text;
use mdm_core::{
    ArtifactManifest, Message, PlannedTask, PrivilegePolicy, ServerId, Shell, StepId, TaskAction,
    TaskDetail, TaskId, UploadDetail, UploadProgress, compare_manifests,
};
use tokio::sync::Mutex;

use crate::coordinator::{TaskContext, TaskExecutor, TaskFuture, TaskOutcome};
use crate::local::{LocalCommand, LocalEnd, LocalOutcome, run_local};
use crate::remote::{RemoteEnd, Stop, run_remote, wrap_command};
use crate::shell::{Invocation, program_name, resolve_shell};
use crate::ssh::SshSession;
use crate::upload::{Sftp, UploadEnd, UploadJob, scan, upload};

/// Upload progress goes to the UI at most this often, plus at every file boundary.
const PROGRESS_PERIOD: Duration = Duration::from_millis(200);

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
    /// The manifest each upload step recorded at its first target (ARC-28).
    manifests: Mutex<HashMap<StepId, ArtifactManifest>>,
}

impl AppExecutor {
    pub fn new(remote: Arc<dyn RemoteConnector>) -> Self {
        Self {
            remote: Some(remote),
            ..Self::default()
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

impl AppExecutor {
    /// Upload one source to one server (STEP-03/04). The source is checked when the task
    /// starts, so an earlier build step may create it. The step's first target records the
    /// manifest; later targets must find the same files and content (ARC-28).
    async fn upload(&self, task: &PlannedTask, ctx: &TaskContext) -> TaskOutcome {
        let TaskAction::Upload {
            source,
            source_kind,
            target,
            conflict,
        } = &task.action
        else {
            unreachable!("only upload tasks")
        };
        let Some(server) = &task.server else {
            return failed(Message::NoServer);
        };
        ctx.progress(UploadProgress::default());
        let path = PathBuf::from(source.expose());
        let kind = *source_kind;
        let scanned = {
            let path = path.clone();
            tokio::task::spawn_blocking(move || scan(&path, kind)).await
        };
        let manifest = match scanned {
            Ok(Ok(m)) => m,
            Ok(Err(reason)) => return failed(reason),
            Err(e) => {
                return failed(Message::Internal {
                    detail: e.to_string(),
                });
            }
        };
        if ctx.cancel.is_cancelled() {
            return TaskOutcome::Cancelled;
        }
        let mut detail = UploadDetail::default();
        {
            let mut manifests = self.manifests.lock().await;
            match manifests.get(&task.step_id) {
                None => {
                    manifests.insert(task.step_id, manifest.clone());
                    ctx.note(Message::ManifestRecorded {
                        files: count(manifest.files.len()),
                        size: size_text(manifest.total_bytes()),
                    });
                    detail.manifest = Some(manifest.clone());
                }
                Some(recorded) => {
                    let changes = compare_manifests(recorded, &manifest);
                    if !changes.is_empty() {
                        let n = count(changes.len());
                        detail.changes = changes;
                        ctx.detail(TaskDetail::Upload(detail));
                        return failed(Message::ArtifactChanged { n });
                    }
                    ctx.note(Message::ManifestVerified);
                }
            }
        }
        // The manifest is on record before anything is transferred.
        ctx.detail(TaskDetail::Upload(detail.clone()));

        let session = match self.session(server.id, ctx).await {
            Ok(s) => s,
            Err(outcome) => return outcome,
        };
        if ctx.cancel.is_cancelled() {
            return TaskOutcome::Cancelled;
        }
        let sftp = match Sftp::open(&session).await {
            Ok(s) => s,
            Err(reason) => return failed(reason),
        };
        let token = TaskId::new().to_string();
        let job = UploadJob {
            source: &path,
            manifest: &manifest,
            target: target.expose(),
            conflict: *conflict,
            token: &token,
        };
        let mut last: Option<(Instant, u32)> = None;
        let report = upload(&sftp, &job, &ctx.cancel, |p| {
            let due = match last {
                None => true,
                Some((at, files)) => files != p.files_done || at.elapsed() >= PROGRESS_PERIOD,
            };
            if due || p.bytes_done == p.bytes_total {
                last = Some((Instant::now(), p.files_done));
                ctx.progress(p);
            }
        })
        .await;
        sftp.close();

        for left in &report.temp_left {
            ctx.note(left.clone());
        }
        detail.committed = report.committed;
        detail.stopped_at = report.stopped_at;
        detail.unprocessed = report.unprocessed;
        detail.temp_left = report.temp_left;
        let committed = count(detail.committed.len());
        ctx.detail(TaskDetail::Upload(detail));
        match report.end {
            UploadEnd::Completed => {
                ctx.note(Message::UploadDone {
                    files: committed,
                    size: size_text(manifest.total_bytes()),
                });
                TaskOutcome::Succeeded { exit_code: None }
            }
            UploadEnd::Failed(reason) => failed(reason),
            UploadEnd::Unknown(reason) => TaskOutcome::Unknown { reason },
            UploadEnd::Cancelled => TaskOutcome::Cancelled,
        }
    }
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
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
                TaskAction::Upload { .. } => self.upload(task, &ctx).await,
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
