//! Real shells on Windows (runs in the Windows CI job). PowerShell 7 tests are skipped when
//! `pwsh.exe` is not on PATH.
#![cfg(windows)]

use std::path::PathBuf;
use std::time::Duration;

use mdm_core::{OutputEncoding, Shell};
use mdm_exec::{
    Invocation, LocalCommand, LocalEnd, LocalOutcome, Stream, resolve_shell, run_local,
};
use tokio_util::sync::CancellationToken;

fn command(shell: Shell, script: &str, timeout: Option<Duration>) -> Option<LocalCommand> {
    let program = resolve_shell(shell)?;
    let cwd = std::env::temp_dir();
    Some(LocalCommand {
        shell,
        invocation: Invocation::new(shell, program, script, cwd.to_str(), None),
        encoding: OutputEncoding::Auto,
        timeout,
    })
}

async fn run(cmd: &LocalCommand) -> (LocalOutcome, String, String) {
    let (mut out, mut err) = (String::new(), String::new());
    let outcome = run_local(cmd, &CancellationToken::new(), |c| match c.stream {
        Stream::Stdout => out.push_str(&c.text),
        Stream::Stderr => err.push_str(&c.text),
    })
    .await
    .unwrap();
    (outcome, out, err)
}

fn marker(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("mdm-exec-{name}-{}", std::process::id()));
    let _ = std::fs::remove_file(&path);
    path
}

#[test]
fn builtin_shells_resolve() {
    let cmd = resolve_shell(Shell::Cmd).expect("cmd.exe on PATH");
    assert!(cmd.is_absolute());
    assert!(resolve_shell(Shell::Powershell).is_some());
}

#[tokio::test]
async fn cmd_runs_script_verbatim() {
    let cmd = command(
        Shell::Cmd,
        r#"echo "a  b"& echo oops 1>&2& exit /b 3"#,
        None,
    )
    .unwrap();
    let (outcome, out, err) = run(&cmd).await;
    assert_eq!(outcome.end, LocalEnd::Exited);
    assert_eq!(outcome.exit_code, Some(3));
    assert_eq!(out.trim_end(), r#""a  b""#);
    assert_eq!(err.trim_end(), "oops");
}

async fn powershell_case(shell: Shell) {
    let Some(cmd) = command(
        shell,
        r#"[Console]::OutputEncoding = [Text.Encoding]::UTF8; Write-Output "中文 'x' ""y"""; [Console]::Error.WriteLine('err'); exit 4"#,
        None,
    ) else {
        eprintln!("skip: {shell:?} not installed");
        return;
    };
    let (outcome, out, err) = run(&cmd).await;
    assert_eq!(outcome.exit_code, Some(4), "stdout {out:?} stderr {err:?}");
    assert_eq!(out.trim_end(), r#"中文 'x' "y""#);
    assert_eq!(err.trim_end(), "err");
}

#[tokio::test]
async fn windows_powershell_passes_quotes_and_exit_code() {
    powershell_case(Shell::Powershell).await;
}

#[tokio::test]
async fn pwsh_passes_quotes_and_exit_code() {
    powershell_case(Shell::Pwsh).await;
}

#[tokio::test]
async fn timeout_terminates_the_job() {
    let flag = marker("timeout");
    let script = format!(
        r#"start /b cmd /c "ping -n 4 127.0.0.1 >nul & echo x> {f}" & ping -n 30 127.0.0.1 >nul"#,
        f = flag.display()
    );
    let cmd = command(Shell::Cmd, &script, Some(Duration::from_millis(800))).unwrap();
    let (outcome, _, _) = run(&cmd).await;
    assert_eq!(outcome.end, LocalEnd::TimedOut);
    assert_eq!(outcome.exit_code, None);
    assert!(outcome.tree_stopped);
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(!flag.exists(), "background process survived the timeout");
}

#[tokio::test]
async fn leftovers_are_terminated_when_the_shell_exits() {
    let cmd = command(Shell::Cmd, "start /b ping -n 30 127.0.0.1 & echo bye", None).unwrap();
    let (outcome, out, _) = run(&cmd).await;
    assert_eq!(outcome.end, LocalEnd::Exited);
    assert_eq!(outcome.exit_code, Some(0));
    assert!(out.contains("bye"));
    assert!(outcome.leftovers_terminated);
    assert!(outcome.elapsed < Duration::from_secs(10));
}
