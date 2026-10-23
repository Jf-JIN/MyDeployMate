//! What a connection asks the operator while it runs: D02 first trust and D04 secrets
//! (IXD-19, IXD-31, IXD-32). Shared by connection tests and runs; never contains a secret.

use serde::Serialize;
use ts_rs::TS;

use crate::{CredentialPurpose, HostEndpoint, HostKey, ServerId};

/// D04: a password or passphrase is needed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CredentialPrompt {
    #[serde(flatten)]
    pub purpose: CredentialPurpose,
    pub server_name: String,
    /// `host:port` as shown elsewhere.
    pub address: String,
    pub user: String,
    /// The key file being unlocked, for a passphrase.
    pub key_path: Option<String>,
    /// What needs it, e.g. "连接测试" or a task name.
    pub context: String,
    pub attempt: u8,
    pub max_attempts: u8,
    /// The previous value was rejected.
    pub rejected: bool,
    /// The rejected value was the remembered one (IXD-31 notice).
    pub saved_rejected: bool,
    /// Initial state of "remember": ticked only to replace a remembered value that failed.
    pub remember_default: bool,
}

/// D02: the server presents a key and nothing is recorded for its endpoint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HostTrustPrompt {
    pub server_id: ServerId,
    pub server_name: String,
    pub user: String,
    pub endpoint: HostEndpoint,
    pub presented: HostKey,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export)]
pub enum OperatorPrompt {
    Credential(CredentialPrompt),
    HostTrust(HostTrustPrompt),
}
