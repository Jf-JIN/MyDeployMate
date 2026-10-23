//! An authenticated SSH connection. Every command gets its own channel (ARC-19).

use std::time::Duration;

use mdm_core::{ConnectError, ConnectErrorKind, ConnectStage, HostEndpoint};
use russh::client::Handle;
use russh::{ChannelMsg, Disconnect};

use super::connect::Verifier;
use super::error::ssh_error;

/// The connection test's command: reads system facts, changes nothing (SRV-04).
const PROBE: &str = "uname -srm";

pub struct SshSession {
    handle: Handle<Verifier>,
    endpoint: HostEndpoint,
}

/// Collected result of a short command.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    /// Missing when the command was killed by a signal or the channel closed early (ARC-22).
    pub exit_status: Option<u32>,
    pub exit_signal: Option<String>,
}

impl SshSession {
    pub(crate) fn new(handle: Handle<Verifier>, endpoint: HostEndpoint) -> Self {
        Self { handle, endpoint }
    }

    pub fn endpoint(&self) -> &HostEndpoint {
        &self.endpoint
    }

    pub fn is_closed(&self) -> bool {
        self.handle.is_closed()
    }

    /// Start `command` on a new channel without a PTY; the caller reads the channel (ARC-22).
    pub async fn exec_channel(
        &self,
        command: &str,
    ) -> Result<russh::Channel<russh::client::Msg>, russh::Error> {
        let channel = self.handle.channel_open_session().await?;
        channel.exec(true, command).await?;
        Ok(channel)
    }

    /// Run a short, non-interactive command without a PTY and collect at most `limit` bytes
    /// of each stream (the rest is read and dropped).
    pub async fn exec_collect(
        &self,
        command: &str,
        limit: usize,
    ) -> Result<CommandOutput, russh::Error> {
        let mut channel = self.handle.channel_open_session().await?;
        channel.exec(true, command).await?;
        let mut out = CommandOutput::default();
        let keep = |buf: &mut Vec<u8>, data: &[u8]| {
            let room = limit.saturating_sub(buf.len());
            buf.extend_from_slice(&data[..data.len().min(room)]);
        };
        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { data } => keep(&mut out.stdout, &data),
                ChannelMsg::ExtendedData { data, .. } => keep(&mut out.stderr, &data),
                ChannelMsg::ExitStatus { exit_status } => out.exit_status = Some(exit_status),
                ChannelMsg::ExitSignal { signal_name, .. } => {
                    out.exit_signal = Some(signal_name_text(&signal_name))
                }
                _ => {}
            }
        }
        Ok(out)
    }

    /// Run the connection test probe and return the system it reports, e.g.
    /// `Linux 6.8.0 x86_64`.
    pub async fn probe(&self, timeout: Duration) -> Result<String, ConnectError> {
        let out = tokio::time::timeout(timeout, self.exec_collect(PROBE, 4096))
            .await
            .map_err(|_| ConnectError::at(ConnectStage::Command, ConnectErrorKind::Timeout))?
            .map_err(|e| ssh_error(ConnectStage::Command, &e))?;
        if out.exit_status == Some(0) {
            Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
        } else {
            let stderr = String::from_utf8_lossy(&out.stderr);
            Err(ConnectError::new(ConnectErrorKind::CommandFailed {
                exit_status: out.exit_status,
            })
            .with_detail(stderr.lines().next().unwrap_or_default()))
        }
    }

    pub async fn close(&self) {
        let _ = self
            .handle
            .disconnect(Disconnect::ByApplication, "", "")
            .await;
    }
}

pub(crate) fn signal_name_text(sig: &russh::Sig) -> String {
    match sig {
        russh::Sig::Custom(name) => name.clone(),
        other => format!("{other:?}"),
    }
}
