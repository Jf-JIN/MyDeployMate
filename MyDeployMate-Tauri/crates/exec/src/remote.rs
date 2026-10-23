//! Remote commands over SSH (SRS STEP-04, RUN-05; architecture ARC-22, ARC-23).
//!
//! Every command runs on its own channel, without a PTY, wrapped so that it leads a new process
//! group and reports the group ID on stderr first:
//!
//! ```sh
//! exec setsid -w sh -c 'echo "__MDM_PGID__ $$" >&2; …; sh -c "$1"; exit $?' mdm '<command>' '<cwd>'
//! ```
//!
//! The command's shell is not exec'd, so a command killed by a signal reports `128 + N` as
//! shells do (`setsid -w` alone would report the bare signal number, which reads like an
//! ordinary exit code).
//! The marker line is removed from the output. Success needs an `exit-status`: a signal alone
//! is a failure, and a channel that closes without either leaves the result unknown.
//!
//! Closing the channel or sending a signal request does not stop the remote process (phase 0),
//! so cancel and timeout kill the process group from a second channel and then check whether
//! anything in the group is still alive. Only a confirmed stop counts as stopped.

use std::time::Duration;

use mdm_core::OutputEncoding;
use mdm_core::plan::ResolvedCommand;
use russh::ChannelMsg;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::decode::OutputDecoder;
use crate::local::{OutputChunk, Stream};
use crate::ssh::SshSession;
use crate::ssh::session::signal_name_text;

const PGID_MARKER: &[u8] = b"__MDM_PGID__ ";
/// Stderr lines before the marker (login scripts may print some) are passed through; past this
/// many bytes without it the marker is given up on.
const PGID_SCAN_LIMIT: usize = 16 * 1024;
/// TERM, then up to this many one-second checks before KILL (ARC-23 grace period).
const TERM_GRACE_CHECKS: u32 = 5;
/// Upper bound for the whole stop-and-check command.
const STOP_TIMEOUT: Duration = Duration::from_secs(20);

/// Quote for a POSIX shell: `it's` becomes `'it'\''s'`.
pub fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// The command line sent to the server for `command`, run in `cwd` when given.
pub fn wrap_command(command: &ResolvedCommand, cwd: Option<&str>) -> String {
    let inner = match command {
        ResolvedCommand::Script { script } => script.expose().to_owned(),
        ResolvedCommand::Program { executable, args } => {
            let mut words = vec!["exec".to_owned(), sh_quote(executable.expose())];
            words.extend(args.iter().map(|a| sh_quote(a.expose())));
            words.join(" ")
        }
    };
    format!(
        "exec setsid -w sh -c {} mdm {} {}",
        sh_quote(
            "echo \"__MDM_PGID__ $$\" >&2; \
             if [ -n \"$2\" ]; then cd -- \"$2\" || exit 1; fi; \
             sh -c \"$1\"; exit $?"
        ),
        sh_quote(&inner),
        sh_quote(cwd.unwrap_or_default()),
    )
}

/// How the remote command ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteEnd {
    Exited(u32),
    /// Killed by a signal; no exit status.
    Signalled(String),
    /// The channel closed without an exit status or signal (connection lost).
    Lost,
    /// The server refused to start the command.
    Refused,
    TimedOut(Stop),
    Cancelled(Stop),
}

/// Whether stopping the remote process group was confirmed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stop {
    Confirmed,
    /// Why it could not be confirmed.
    Unconfirmed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteOutcome {
    pub end: RemoteEnd,
    /// The process group the command ran in, when the marker arrived.
    pub pgid: Option<u32>,
}

/// Run `wrapped` (from [`wrap_command`]) and stream its output to `on_output`.
pub async fn run_remote(
    session: &SshSession,
    wrapped: &str,
    timeout: Option<Duration>,
    cancel: &CancellationToken,
    mut on_output: impl FnMut(OutputChunk),
) -> Result<RemoteOutcome, russh::Error> {
    let mut channel = session.exec_channel(wrapped).await?;
    let mut out = StreamDecoder::new(Stream::Stdout);
    let mut err = StreamDecoder::new(Stream::Stderr);
    let mut scan = PgidScan::default();
    let mut exit_status = None;
    let mut exit_signal = None;
    let mut refused = false;
    let deadline = timeout.map(|t| Instant::now() + t);

    let interrupted = loop {
        let msg = tokio::select! {
            biased;
            _ = cancel.cancelled() => break Some(false),
            _ = sleep_until(deadline) => break Some(true),
            msg = channel.wait() => msg,
        };
        match msg {
            None => break None,
            Some(ChannelMsg::Data { data }) => out.push(&data, &mut on_output),
            Some(ChannelMsg::ExtendedData { data, .. }) => {
                let pass = scan.push(&data);
                err.push(&pass, &mut on_output);
            }
            Some(ChannelMsg::ExitStatus { exit_status: s }) => exit_status = Some(s),
            Some(ChannelMsg::ExitSignal { signal_name, .. }) => {
                exit_signal = Some(signal_name_text(&signal_name))
            }
            Some(ChannelMsg::Failure) => refused = true,
            Some(_) => {}
        }
    };
    let rest = scan.finish();
    err.push(&rest, &mut on_output);
    out.finish(&mut on_output);
    err.finish(&mut on_output);
    let pgid = scan.pgid;

    let end = match interrupted {
        Some(timed_out) => {
            let stop = stop_group(session, pgid).await;
            // Whatever is left on the channel no longer matters; closing it does not stop
            // anything remote, which `stop_group` has dealt with.
            let _ = channel.close().await;
            if timed_out {
                RemoteEnd::TimedOut(stop)
            } else {
                RemoteEnd::Cancelled(stop)
            }
        }
        None => match (exit_status, exit_signal) {
            (Some(code), _) => RemoteEnd::Exited(code),
            (None, Some(sig)) => RemoteEnd::Signalled(sig),
            (None, None) if refused => RemoteEnd::Refused,
            (None, None) => RemoteEnd::Lost,
        },
    };
    Ok(RemoteOutcome { end, pgid })
}

async fn sleep_until(deadline: Option<Instant>) {
    match deadline {
        Some(d) => tokio::time::sleep_until(d).await,
        None => std::future::pending().await,
    }
}

/// TERM the group, wait, KILL what is left, and report whether the group is gone.
async fn stop_group(session: &SshSession, pgid: Option<u32>) -> Stop {
    let Some(pgid) = pgid else {
        return Stop::Unconfirmed("没有取得远端进程组 ID".into());
    };
    if session.is_closed() {
        return Stop::Unconfirmed("与服务器的连接已断开".into());
    }
    let script = format!(
        "kill -TERM -- -{pgid} 2>/dev/null; i=0; \
         while kill -0 -- -{pgid} 2>/dev/null && [ $i -lt {TERM_GRACE_CHECKS} ]; do sleep 1; i=$((i+1)); done; \
         if kill -0 -- -{pgid} 2>/dev/null; then kill -KILL -- -{pgid} 2>/dev/null; sleep 1; fi; \
         if kill -0 -- -{pgid} 2>/dev/null; then echo __MDM_ALIVE__; else echo __MDM_GONE__; fi"
    );
    match tokio::time::timeout(STOP_TIMEOUT, session.exec_collect(&script, 4096)).await {
        Err(_) => Stop::Unconfirmed("检查远端进程超时".into()),
        Ok(Err(_)) => Stop::Unconfirmed("与服务器的连接已断开".into()),
        Ok(Ok(out)) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if stdout.contains("__MDM_GONE__") {
                Stop::Confirmed
            } else if stdout.contains("__MDM_ALIVE__") {
                Stop::Unconfirmed("强制终止后进程组仍在运行".into())
            } else {
                Stop::Unconfirmed("无法检查远端进程状态".into())
            }
        }
    }
}

/// Finds the process group marker in stderr and removes its line.
#[derive(Default)]
struct PgidScan {
    /// The current incomplete line, while still looking.
    line: Vec<u8>,
    seen: usize,
    done: bool,
    pgid: Option<u32>,
}

impl PgidScan {
    /// Returns the bytes to show.
    fn push(&mut self, data: &[u8]) -> Vec<u8> {
        if self.done {
            return data.to_vec();
        }
        self.seen += data.len();
        self.line.extend_from_slice(data);
        let mut pass = Vec::new();
        while let Some(pos) = self.line.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.line.drain(..=pos).collect();
            if let Some(pgid) = parse_marker(&line) {
                self.pgid = Some(pgid);
                self.done = true;
                pass.append(&mut self.line);
                return pass;
            }
            pass.extend_from_slice(&line);
        }
        if self.seen > PGID_SCAN_LIMIT {
            self.done = true;
            pass.append(&mut self.line);
        }
        pass
    }

    fn finish(&mut self) -> Vec<u8> {
        self.done = true;
        std::mem::take(&mut self.line)
    }
}

fn parse_marker(line: &[u8]) -> Option<u32> {
    let rest = line.strip_prefix(PGID_MARKER)?;
    std::str::from_utf8(rest).ok()?.trim().parse().ok()
}

/// Remote output is UTF-8; invalid bytes show as U+FFFD.
struct StreamDecoder {
    stream: Stream,
    decoder: OutputDecoder,
}

impl StreamDecoder {
    fn new(stream: Stream) -> Self {
        Self {
            stream,
            decoder: OutputDecoder::new(OutputEncoding::Utf8, false),
        }
    }

    fn push(&mut self, bytes: &[u8], on_output: &mut impl FnMut(OutputChunk)) {
        if bytes.is_empty() {
            return;
        }
        let d = self.decoder.push(bytes);
        on_output(OutputChunk {
            stream: self.stream,
            text: d.text,
            bytes: bytes.to_vec(),
            lossy: d.lossy,
        });
    }

    fn finish(&mut self, on_output: &mut impl FnMut(OutputChunk)) {
        let d = self.decoder.finish();
        if !d.text.is_empty() {
            on_output(OutputChunk {
                stream: self.stream,
                text: d.text,
                bytes: Vec::new(),
                lossy: d.lossy,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdm_core::ResolvedText;

    #[test]
    fn quoting_survives_single_quotes() {
        assert_eq!(sh_quote("it's"), r"'it'\''s'");
        assert_eq!(sh_quote(""), "''");
    }

    #[test]
    fn program_words_are_quoted_one_by_one() {
        let cmd = ResolvedCommand::Program {
            executable: ResolvedText::plain("/opt/app/bin/ctl"),
            args: vec![ResolvedText::plain("restart"), ResolvedText::plain("a b")],
        };
        let wrapped = wrap_command(&cmd, Some("/srv/app"));
        assert!(wrapped.starts_with("exec setsid -w sh -c '"));
        assert!(wrapped.ends_with(
            r#" mdm 'exec '\''/opt/app/bin/ctl'\'' '\''restart'\'' '\''a b'\''' '/srv/app'"#
        ));
    }

    #[test]
    fn marker_line_is_removed_and_other_stderr_kept() {
        let mut scan = PgidScan::default();
        let mut shown = scan.push(b"motd noise\n__MDM_PG");
        shown.extend(scan.push(b"ID__ 4242\nreal error\n"));
        shown.extend(scan.push(b"more\n"));
        shown.extend(scan.finish());
        assert_eq!(scan.pgid, Some(4242));
        assert_eq!(shown, b"motd noise\nreal error\nmore\n");
    }

    #[test]
    fn missing_marker_passes_everything_through() {
        let mut scan = PgidScan::default();
        let mut shown = scan.push(b"sh: 1: setsid: not found\npartial");
        shown.extend(scan.finish());
        assert_eq!(scan.pgid, None);
        assert_eq!(shown, b"sh: 1: setsid: not found\npartial");
    }
}
