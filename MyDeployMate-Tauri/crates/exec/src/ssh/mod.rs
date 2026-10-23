//! SSH connection layer (SRS SRV-02/03, architecture ARC-19–21).
//!
//! [`connect`] verifies the server identity against keys the caller read from the trust store
//! before any secret is sent, then logs in with the server's configured method. An unknown or
//! changed key ends the attempt with [`ConnectErrorKind::HostKeyUnknown`] or
//! [`ConnectErrorKind::HostKeyChanged`]: the caller shows D02, records the trust, and connects
//! again. Passwords and passphrases are asked through [`SecretSource`], at most
//! [`MAX_ATTEMPTS`] times each.
//!
//! [`ConnectErrorKind::HostKeyUnknown`]: mdm_core::ConnectErrorKind::HostKeyUnknown
//! [`ConnectErrorKind::HostKeyChanged`]: mdm_core::ConnectErrorKind::HostKeyChanged

// ConnectError carries the full host keys for D02. Connecting happens once per server and run,
// so its size on the error path does not matter.
#![allow(clippy::result_large_err)]

mod auth;
mod connect;
mod error;
pub(crate) mod session;

pub use auth::{MAX_ATTEMPTS, SecretFuture, SecretPrompt, SecretSource};
pub use connect::{SshTarget, connect, fetch_host_key};
pub use session::{CommandOutput, SshSession};
