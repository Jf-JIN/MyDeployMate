//! An authenticated SSH connection. Every command gets its own channel (ARC-19).

use mdm_core::HostEndpoint;
use russh::client::Handle;
use russh::{ChannelMsg, Disconnect};

use super::connect::Verifier;

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

    pub async fn close(self) {
        let _ = self
            .handle
            .disconnect(Disconnect::ByApplication, "", "")
            .await;
    }
}

fn signal_name_text(sig: &russh::Sig) -> String {
    match sig {
        russh::Sig::Custom(name) => name.clone(),
        other => format!("{other:?}"),
    }
}
