//! The task executor the app uses: local commands now; upload and remote commands arrive with
//! SSH in phases 5 and 6.

use std::time::Duration;

use mdm_core::{PlannedTask, Shell, TaskAction};

use crate::coordinator::{TaskContext, TaskExecutor, TaskFuture, TaskOutcome};
use crate::local::{LocalCommand, LocalEnd, LocalOutcome, run_local};
use crate::shell::{Invocation, program_name, resolve_shell};

#[derive(Debug, Default)]
pub struct AppExecutor;

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
                TaskAction::Upload { .. } | TaskAction::Remote { .. } => {
                    failed("当前版本尚不支持上传和远程命令，请选择“仅本地命令”运行".into())
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
