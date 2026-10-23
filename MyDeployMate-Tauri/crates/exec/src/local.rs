//! Local command executor (SRS STEP-02, RUN-05; architecture ARC-17, ARC-18).
//!
//! stdout and stderr are read while the process runs, decoded incrementally and handed to the
//! caller as they arrive. The process runs in a Windows Job Object, so a timeout or cancel
//! terminates the whole process tree, not only the shell.
//!
//! The step ends when the shell exits. Processes it left behind are terminated with the job:
//! state never carries over to the next step. If they still hold the output pipes, the
//! executor waits [`LEFTOVER_GRACE`] for them before terminating.

use std::path::Path;
use std::time::Duration;

use mdm_core::{Message, OutputEncoding, Shell};
use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::time::{Instant, sleep_until};
use tokio_util::sync::CancellationToken;

use crate::decode::OutputDecoder;
use crate::shell::Invocation;

/// How long output pipes may stay open after the shell exits.
pub const LEFTOVER_GRACE: Duration = Duration::from_secs(1);
const POLL: Duration = Duration::from_millis(20);
const READ_CHUNK: usize = 8 * 1024;

/// One local task, ready to start.
#[derive(Clone, Debug)]
pub struct LocalCommand {
    pub shell: Shell,
    pub invocation: Invocation,
    pub encoding: OutputEncoding,
    /// `None` means no limit.
    pub timeout: Option<Duration>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

/// Output as it arrives. `bytes` are the raw bytes read; `text` is what could be decoded so far
/// (an incomplete trailing character comes with a later chunk).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputChunk {
    pub stream: Stream,
    pub text: String,
    pub bytes: Vec<u8>,
    /// Some bytes could not be decoded and show as U+FFFD.
    pub lossy: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalEnd {
    /// The shell exited on its own; see `exit_code`.
    Exited,
    TimedOut,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalOutcome {
    pub end: LocalEnd,
    /// `None` when the process was killed without an exit code.
    pub exit_code: Option<i32>,
    pub elapsed: Duration,
    /// After a timeout or cancel: whether the whole process tree is known to have stopped.
    /// Terminating `wsl.exe` does not stop the Linux processes it started, so this is false
    /// for WSL; the task must then be recorded as outcome unknown.
    pub tree_stopped: bool,
    /// Processes were still running when the shell exited and were terminated.
    pub leftovers_terminated: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum LocalError {
    #[error("working directory does not exist: {0}")]
    MissingCwd(String),
    #[error("cannot start {program}: {source}")]
    Spawn {
        program: String,
        source: std::io::Error,
    },
    #[error("waiting for the process failed: {0}")]
    Wait(std::io::Error),
}

impl LocalError {
    pub fn message(&self) -> Message {
        match self {
            LocalError::MissingCwd(path) => Message::CwdMissing { path: path.clone() },
            LocalError::Spawn { program, source } => Message::SpawnFailed {
                program: program.clone(),
                detail: source.to_string(),
            },
            LocalError::Wait(e) => Message::WaitFailed {
                detail: e.to_string(),
            },
        }
    }
}

/// Run `cmd` to the end, passing output to `on_output`. A cancel or timeout terminates the
/// process tree and still returns an outcome.
pub async fn run_local(
    cmd: &LocalCommand,
    cancel: &CancellationToken,
    mut on_output: impl FnMut(OutputChunk),
) -> Result<LocalOutcome, LocalError> {
    if let Some(dir) = cmd.invocation.cwd()
        && !dir.is_dir()
    {
        return Err(LocalError::MissingCwd(dir.display().to_string()));
    }

    let started = Instant::now();
    let mut child = spawn(&cmd.invocation)?;
    let mut stdout = child.stdout().take();
    let mut stderr = child.stderr().take();
    let wsl = cmd.shell == Shell::Wsl;
    let mut out_decoder = OutputDecoder::new(cmd.encoding, wsl);
    let mut err_decoder = OutputDecoder::new(cmd.encoding, wsl);
    let mut out_buf = vec![0u8; READ_CHUNK];
    let mut err_buf = vec![0u8; READ_CHUNK];

    let deadline = cmd.timeout.map(|t| started + t);
    let mut end = LocalEnd::Exited;
    let mut killed = false;
    let mut status = None;
    let mut grace_until = None;
    let mut leftovers_terminated = false;

    loop {
        if status.is_some() && stdout.is_none() && stderr.is_none() {
            break;
        }
        tokio::select! {
            biased;
            _ = cancel.cancelled(), if !killed && status.is_none() => {
                end = LocalEnd::Cancelled;
                killed = kill(child.as_mut());
            }
            _ = sleep_until(deadline.unwrap_or_else(Instant::now)),
                if deadline.is_some() && !killed && status.is_none() => {
                end = LocalEnd::TimedOut;
                killed = kill(child.as_mut());
            }
            n = read_some(&mut stdout, &mut out_buf) => {
                forward(n, &mut stdout, &out_buf, &mut out_decoder, Stream::Stdout, &mut on_output);
            }
            n = read_some(&mut stderr, &mut err_buf) => {
                forward(n, &mut stderr, &err_buf, &mut err_decoder, Stream::Stderr, &mut on_output);
            }
            _ = sleep_until(grace_until.unwrap_or_else(Instant::now)),
                if grace_until.is_some() && !killed => {
                leftovers_terminated = true;
                killed = kill(child.as_mut());
            }
            _ = tokio::time::sleep(POLL), if status.is_none() => {
                // The job's own `wait` would also wait for every process left behind, so poll
                // the shell process itself.
                if let Some(s) = child.inner_mut().try_wait().map_err(LocalError::Wait)? {
                    status = Some(s);
                    grace_until = Some(Instant::now() + LEFTOVER_GRACE);
                }
            }
        }
    }

    // Terminate anything the shell left in the job, even if it closed its output.
    if !killed {
        let _ = child.start_kill();
    }
    let elapsed = started.elapsed();
    let status = status.expect("loop ends after exit");
    Ok(LocalOutcome {
        end,
        // A terminated job reports the code we gave it, which is not the command's.
        exit_code: if end == LocalEnd::Exited {
            status.code()
        } else {
            None
        },
        elapsed,
        tree_stopped: end == LocalEnd::Exited || !wsl,
        leftovers_terminated,
    })
}

fn spawn(inv: &Invocation) -> Result<Box<dyn ChildWrapper>, LocalError> {
    let mut command = CommandWrap::with_new(&inv.program, |c| {
        c.args(inv.os_args())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        #[cfg(windows)]
        if let Some(raw) = &inv.raw_args {
            c.raw_arg(raw);
        }
        if let Some(dir) = inv.cwd() {
            c.current_dir(dir);
        }
    });
    #[cfg(windows)]
    {
        use process_wrap::tokio::{CreationFlags, JobObject};
        use windows::Win32::System::Threading::CREATE_NO_WINDOW;
        command
            .wrap(CreationFlags(CREATE_NO_WINDOW))
            .wrap(JobObject);
    }
    #[cfg(unix)]
    command.wrap(process_wrap::tokio::ProcessGroup::leader());
    command.wrap(KillOnDrop);
    command.spawn().map_err(|source| LocalError::Spawn {
        program: display_program(&inv.program),
        source,
    })
}

fn display_program(program: &Path) -> String {
    program.display().to_string()
}

/// Terminate the process tree. Returns true so the caller can record that it happened.
fn kill(child: &mut dyn ChildWrapper) -> bool {
    let _ = child.start_kill();
    true
}

/// Read from an open pipe; never resolves for a closed one.
async fn read_some<R: AsyncRead + Unpin>(
    pipe: &mut Option<R>,
    buf: &mut [u8],
) -> std::io::Result<usize> {
    match pipe {
        Some(p) => p.read(buf).await,
        None => std::future::pending().await,
    }
}

fn forward<R>(
    n: std::io::Result<usize>,
    pipe: &mut Option<R>,
    buf: &[u8],
    decoder: &mut OutputDecoder,
    stream: Stream,
    on_output: &mut impl FnMut(OutputChunk),
) {
    let (bytes, decoded) = match n {
        Ok(n) if n > 0 => (buf[..n].to_vec(), decoder.push(&buf[..n])),
        // End of stream or a broken pipe: flush what the decoder holds.
        _ => {
            *pipe = None;
            (Vec::new(), decoder.finish())
        }
    };
    if !bytes.is_empty() || !decoded.text.is_empty() {
        on_output(OutputChunk {
            stream,
            text: decoded.text,
            bytes,
            lossy: decoded.lossy,
        });
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::path::PathBuf;

    use pretty_assertions::assert_eq;

    use super::*;

    /// The shell adapters are Windows-only; on Unix the same runner is driven through `sh`.
    fn sh(script: &str, timeout: Option<Duration>) -> LocalCommand {
        LocalCommand {
            shell: Shell::Cmd,
            invocation: Invocation {
                program: PathBuf::from("/bin/sh"),
                args: vec!["-c".into(), script.into()],
                raw_args: None,
                cwd: None,
            },
            encoding: OutputEncoding::Auto,
            timeout,
        }
    }

    async fn run(cmd: &LocalCommand, cancel: &CancellationToken) -> (LocalOutcome, String, String) {
        let (mut out, mut err) = (String::new(), String::new());
        let outcome = run_local(cmd, cancel, |c| match c.stream {
            Stream::Stdout => out.push_str(&c.text),
            Stream::Stderr => err.push_str(&c.text),
        })
        .await
        .unwrap();
        (outcome, out, err)
    }

    #[tokio::test]
    async fn captures_both_streams_and_exit_code() {
        let cmd = sh(
            "echo 中文; echo oops >&2; printf 'no newline'; exit 3",
            None,
        );
        let (outcome, out, err) = run(&cmd, &CancellationToken::new()).await;
        assert_eq!(outcome.end, LocalEnd::Exited);
        assert_eq!(outcome.exit_code, Some(3));
        assert_eq!(out, "中文\nno newline");
        assert_eq!(err, "oops\n");
        assert!(!outcome.leftovers_terminated);
    }

    #[tokio::test]
    async fn timeout_kills_process_tree() {
        let marker = std::env::temp_dir().join(format!("mdm-exec-timeout-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let script = format!("(sleep 2; touch {m}) & sleep 30", m = marker.display());
        let cmd = sh(&script, Some(Duration::from_millis(300)));
        let (outcome, _, _) = run(&cmd, &CancellationToken::new()).await;
        assert_eq!(outcome.end, LocalEnd::TimedOut);
        assert!(outcome.elapsed < Duration::from_secs(5));
        tokio::time::sleep(Duration::from_millis(2500)).await;
        assert!(!marker.exists(), "background child survived the timeout");
    }

    #[tokio::test]
    async fn cancel_stops_the_run() {
        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            trigger.cancel();
        });
        let (outcome, out, _) = run(&sh("echo start; sleep 30", None), &cancel).await;
        assert_eq!(outcome.end, LocalEnd::Cancelled);
        assert!(outcome.tree_stopped);
        assert_eq!(out, "start\n");
    }

    #[tokio::test]
    async fn leftover_processes_holding_output_are_terminated() {
        let (outcome, _, _) =
            run(&sh("sleep 30 & echo bye", None), &CancellationToken::new()).await;
        assert_eq!(outcome.end, LocalEnd::Exited);
        assert_eq!(outcome.exit_code, Some(0));
        assert!(outcome.leftovers_terminated);
        assert!(outcome.elapsed < LEFTOVER_GRACE + Duration::from_secs(2));
    }

    #[tokio::test]
    async fn missing_cwd_and_program_are_errors() {
        let mut cmd = sh("true", None);
        cmd.invocation.cwd = Some(PathBuf::from("/definitely/missing"));
        let err = run_local(&cmd, &CancellationToken::new(), |_| {})
            .await
            .unwrap_err();
        assert!(matches!(err, LocalError::MissingCwd(_)));

        let mut cmd = sh("true", None);
        cmd.invocation.program = PathBuf::from("/definitely/missing-shell");
        let err = run_local(&cmd, &CancellationToken::new(), |_| {})
            .await
            .unwrap_err();
        assert!(matches!(err, LocalError::Spawn { .. }));
    }
}
