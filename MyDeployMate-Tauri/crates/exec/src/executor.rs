//! The task executor the app uses: local commands, and remote commands over SSH (phase 5).
//! Uploads and sudo arrive in phase 6.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use mdm_core::{PlannedTask, PrivilegePolicy, ServerId, Shell, TaskAction};
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
            return Err(failed("没有可用的远程连接".into()));
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
            return failed("当前版本尚不支持 sudo 提权，请把该步骤的提权方式改为“不提权”".into());
        }
        let Some(server) = &task.server else {
            return failed("远程任务缺少目标服务器".into());
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
            Err(e) => return failed(format!("无法在服务器上开始命令：{e}")),
        };
        if outcome.pgid.is_none() && matches!(outcome.end, RemoteEnd::Exited(_)) {
            ctx.note("没有收到远端进程组 ID：服务器可能缺少 setsid（util-linux），取消时将无法确认远端进程已停止");
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
                        return failed(format!("本机未找到 {}", program_name(*shell)));
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
                        Err(e) => failed(e.to_string()),
                    }
                }
                TaskAction::Remote { .. } => self.remote(task, &ctx).await,
                TaskAction::Upload { .. } => {
                    failed("当前版本尚不支持上传，请选择“仅本地命令”运行或去掉上传步骤".into())
                }
                TaskAction::Confirm { .. } => unreachable!("confirm tasks are never dispatched"),
            }
        })
    }
}

fn failed(reason: String) -> TaskOutcome {
    TaskOutcome::Failed {
        exit_code: None,
        exit_signal: None,
        reason,
    }
}

const VERIFY_HINT: &str = "请登录服务器核验该命令是否仍在运行或已经生效";

fn remote_outcome(task: &PlannedTask, end: RemoteEnd) -> TaskOutcome {
    match end {
        RemoteEnd::Exited(0) => TaskOutcome::Succeeded { exit_code: Some(0) },
        RemoteEnd::Exited(code) => {
            let code = i32::try_from(code).unwrap_or(i32::MAX);
            TaskOutcome::Failed {
                exit_code: Some(code),
                exit_signal: None,
                reason: format!("退出码 {code}"),
            }
        }
        RemoteEnd::Signalled(sig) => TaskOutcome::Failed {
            exit_code: None,
            reason: format!("远端进程被信号 {sig} 终止"),
            exit_signal: Some(sig),
        },
        RemoteEnd::Refused => failed("服务器拒绝执行命令".into()),
        RemoteEnd::Lost => TaskOutcome::Unknown {
            reason: format!("连接在命令结束前断开，没有收到退出状态。{VERIFY_HINT}"),
        },
        RemoteEnd::Cancelled(Stop::Confirmed) => TaskOutcome::Cancelled,
        RemoteEnd::Cancelled(Stop::Unconfirmed(why)) => TaskOutcome::Unknown {
            reason: format!("已取消，但无法确认远端进程已停止（{why}）。{VERIFY_HINT}"),
        },
        RemoteEnd::TimedOut(stop) => {
            let limit = task.timeout_sec.unwrap_or_default();
            match stop {
                Stop::Confirmed => failed(format!("超过 {limit} 秒超时，远端进程已终止")),
                Stop::Unconfirmed(why) => TaskOutcome::Unknown {
                    reason: format!(
                        "超过 {limit} 秒超时，无法确认远端进程已停止（{why}）。{VERIFY_HINT}"
                    ),
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
        ctx.note("命令结束时仍有它启动的进程在运行，这些进程已被终止");
    }
    let wsl_unknown = "已终止 wsl.exe，但无法确认 WSL 内的进程已停止，请手动检查";
    match outcome.end {
        LocalEnd::Exited => match outcome.exit_code {
            Some(0) => TaskOutcome::Succeeded { exit_code: Some(0) },
            code => TaskOutcome::Failed {
                exit_code: code,
                exit_signal: None,
                reason: match code {
                    Some(c) => format!("退出码 {c}"),
                    None => "进程异常结束，没有退出码".into(),
                },
            },
        },
        LocalEnd::TimedOut => {
            let limit = task.timeout_sec.unwrap_or_default();
            if outcome.tree_stopped {
                failed(format!("超过 {limit} 秒超时，进程已终止"))
            } else {
                debug_assert_eq!(shell, Shell::Wsl);
                TaskOutcome::Unknown {
                    reason: format!("超过 {limit} 秒超时。{wsl_unknown}"),
                }
            }
        }
        LocalEnd::Cancelled if outcome.tree_stopped => TaskOutcome::Cancelled,
        LocalEnd::Cancelled => TaskOutcome::Unknown {
            reason: format!("已取消。{wsl_unknown}"),
        },
    }
}
