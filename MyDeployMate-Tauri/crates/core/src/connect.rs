//! Why an SSH connection failed (SRS SRV-04, architecture ARC-21, ARC-36).
//!
//! The connection layer in `mdm-exec` classifies every failure into one of these kinds, so the
//! connection test (IXD-18) and run detail can name the stage and suggest what to do, and never
//! report a refused login as "offline". Messages are safe to show: no secrets, no key bodies.

use serde::Serialize;
use ts_rs::TS;

use crate::message::Message;
use crate::trust::{HostEndpoint, HostKey};

/// The step a connection had reached when it failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ConnectStage {
    /// Reading the key file, password variable or agent, before any network traffic.
    Prepare,
    /// Looking up the host name.
    Resolve,
    /// Opening the TCP connection.
    Connect,
    /// SSH version exchange and key exchange.
    Handshake,
    /// Checking the server key against the trust store.
    HostKey,
    Authenticate,
    /// Running a command after login (connection test probe).
    Command,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum ConnectErrorKind {
    KeyFileUnreadable {
        path: String,
    },
    /// SRV-02: an unrecognised key file must say so, not "authentication failed".
    KeyFormatUnsupported {
        path: String,
    },
    PasswordEnvMissing {
        name: String,
    },
    AgentUnavailable,
    AgentNoIdentities,
    ResolveFailed,
    ConnectionRefused,
    Unreachable,
    Timeout,
    /// No common algorithm. `legacyRsaOnly` when the server only offers `ssh-rsa` and the
    /// server setting "allow legacy algorithms" is off (IXD-18 suggestion).
    AlgorithmNegotiation {
        legacy_rsa_only: bool,
    },
    /// First connection: the operator must check the fingerprint (D02).
    HostKeyUnknown {
        endpoint: HostEndpoint,
        presented: HostKey,
    },
    /// The recorded key differs: blocked until an explicit trust update (D02).
    HostKeyChanged {
        endpoint: HostEndpoint,
        known: Vec<HostKey>,
        presented: HostKey,
    },
    KeyPassphraseWrong,
    AuthenticationFailed,
    /// The operator cancelled a prompt or the whole operation.
    Cancelled,
    /// Logged in, but the side-effect-free probe command of the connection test failed.
    CommandFailed {
        exit_status: Option<u32>,
    },
    /// The server closed the connection, e.g. after its login grace time.
    Disconnected,
    /// Anything else the SSH library reported; `message` has the detail.
    Protocol,
}

impl ConnectErrorKind {
    pub fn stage(&self) -> ConnectStage {
        use ConnectErrorKind::*;
        match self {
            KeyFileUnreadable { .. }
            | KeyFormatUnsupported { .. }
            | PasswordEnvMissing { .. }
            | AgentUnavailable
            | AgentNoIdentities => ConnectStage::Prepare,
            ResolveFailed => ConnectStage::Resolve,
            ConnectionRefused | Unreachable => ConnectStage::Connect,
            AlgorithmNegotiation { .. } => ConnectStage::Handshake,
            HostKeyUnknown { .. } | HostKeyChanged { .. } => ConnectStage::HostKey,
            KeyPassphraseWrong | AuthenticationFailed => ConnectStage::Authenticate,
            CommandFailed { .. } => ConnectStage::Command,
            // Depends on when it happened; the connection layer passes the real stage.
            Timeout | Cancelled | Disconnected | Protocol => ConnectStage::Handshake,
        }
    }

    /// A short, safe explanation with a suggestion where one helps.
    pub fn describe(&self) -> Message {
        use ConnectErrorKind::*;
        match self {
            KeyFileUnreadable { path } => Message::KeyFileUnreadable { path: path.clone() },
            KeyFormatUnsupported { path } => Message::KeyFormatUnsupported { path: path.clone() },
            PasswordEnvMissing { name } => Message::PasswordEnvMissing { name: name.clone() },
            AgentUnavailable => Message::AgentUnavailable,
            AgentNoIdentities => Message::AgentNoIdentities,
            ResolveFailed => Message::ResolveFailed,
            ConnectionRefused => Message::ConnectionRefused,
            Unreachable => Message::Unreachable,
            Timeout => Message::ConnectTimeout,
            AlgorithmNegotiation {
                legacy_rsa_only: true,
            } => Message::LegacyRsaOnly,
            AlgorithmNegotiation { .. } => Message::AlgorithmNegotiation,
            HostKeyUnknown { .. } => Message::HostKeyUnknown,
            HostKeyChanged { .. } => Message::HostKeyChanged,
            KeyPassphraseWrong => Message::KeyPassphraseWrong,
            AuthenticationFailed => Message::AuthenticationFailed,
            CommandFailed {
                exit_status: Some(code),
            } => Message::ProbeFailed { code: *code },
            CommandFailed { .. } => Message::ProbeAborted,
            Cancelled => Message::Cancelled,
            Disconnected => Message::Disconnected,
            Protocol => Message::Protocol,
        }
    }
}

/// A classified connection failure.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConnectError {
    pub stage: ConnectStage,
    #[serde(flatten)]
    pub kind: ConnectErrorKind,
    /// Safe text for the operator: [`ConnectErrorKind::describe`] plus any technical detail.
    pub message: Message,
}

impl ConnectError {
    pub fn new(kind: ConnectErrorKind) -> Self {
        Self::at(kind.stage(), kind)
    }

    pub fn at(stage: ConnectStage, kind: ConnectErrorKind) -> Self {
        let message = kind.describe();
        Self {
            stage,
            kind,
            message,
        }
    }

    /// Append technical detail, e.g. an IO error; it is shown untranslated.
    pub fn with_detail(mut self, detail: impl AsRef<str>) -> Self {
        self.message = self.message.with_detail(detail);
        self
    }

    /// Append an explanation of our own, e.g. the algorithm lists.
    pub fn with_note(mut self, note: Message) -> Self {
        self.message = self.message.with_note(note);
        self
    }
}

impl std::fmt::Display for ConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} failed: {:?}", self.stage, self.message)
    }
}

impl std::error::Error for ConnectError {}
