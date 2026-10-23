//! Open and verify one SSH connection (ARC-19–21, SRV-01–03).

use std::borrow::Cow;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mdm_core::{
    AuthMethod, ConnectError, ConnectErrorKind, ConnectStage, HostEndpoint, HostKey, Message,
    Server, TrustCheck, check_host_key,
};
use russh::client::{self, Handler};
use russh::keys::{Algorithm, HashAlg, PublicKeyOrCertificate};
use russh::{Disconnect, Preferred};
use tokio_util::sync::CancellationToken;

use super::auth::{SecretSource, authenticate, prepare};
use super::error::{ssh_error, tcp_error};
use super::session::SshSession;

/// Everything needed to reach and log in to one server. Holds no secrets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SshTarget {
    pub endpoint: HostEndpoint,
    pub user: String,
    pub auth: AuthMethod,
    pub allow_legacy_rsa: bool,
    /// Bounds name lookup, TCP connect, the handshake, and each authentication round trip.
    pub connect_timeout: Duration,
}

impl SshTarget {
    /// From a validated server (validation guarantees the port fits).
    pub fn from_server(server: &Server) -> Self {
        Self {
            endpoint: HostEndpoint::new(&server.host, u16::try_from(server.port).unwrap_or(22)),
            user: server.user.clone(),
            auth: server.auth.clone(),
            allow_legacy_rsa: server.allow_legacy_rsa,
            connect_timeout: Duration::from_secs(server.connect_timeout_sec.max(1).into()),
        }
    }
}

/// Connect, verify the host key against `known` (the trust store's keys for this endpoint),
/// and log in. Secrets are sent only after the key is verified. Cancelling `cancel` ends the
/// attempt with [`ConnectErrorKind::Cancelled`] at any point.
pub async fn connect(
    target: &SshTarget,
    known: &[HostKey],
    secrets: &dyn SecretSource,
    cancel: &CancellationToken,
) -> Result<SshSession, ConnectError> {
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(ConnectError::new(ConnectErrorKind::Cancelled)),
        r = connect_inner(target, known, secrets) => r,
    }
}

async fn connect_inner(
    target: &SshTarget,
    known: &[HostKey],
    secrets: &dyn SecretSource,
) -> Result<SshSession, ConnectError> {
    let prepared = prepare(&target.auth).await?;
    let mut handle = handshake(
        &target.endpoint,
        target.allow_legacy_rsa,
        target.connect_timeout,
        known,
    )
    .await?;
    let timeout = target.connect_timeout;
    if let Err(e) = authenticate(
        &mut handle,
        &target.user,
        prepared,
        target.allow_legacy_rsa,
        secrets,
        timeout,
    )
    .await
    {
        let _ = handle.disconnect(Disconnect::ByApplication, "", "").await;
        return Err(e);
    }
    Ok(SshSession::new(handle, target.endpoint.clone()))
}

/// Read the key a server presents, without logging in: for an explicit trust update, which
/// must show the new key next to the recorded one (IXD-19).
pub async fn fetch_host_key(
    endpoint: &HostEndpoint,
    timeout: Duration,
    cancel: &CancellationToken,
) -> Result<HostKey, ConnectError> {
    // Offer ssh-rsa too, last: the order still picks a modern key when the server has one.
    let attempt = handshake(endpoint, true, timeout, &[]);
    let result = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(ConnectError::new(ConnectErrorKind::Cancelled)),
        r = attempt => r,
    };
    match result {
        Err(ConnectError {
            kind: ConnectErrorKind::HostKeyUnknown { presented, .. },
            ..
        }) => Ok(presented),
        Err(e) => Err(e),
        Ok(handle) => {
            // Unreachable with no known keys; close anyway.
            let _ = handle.disconnect(Disconnect::ByApplication, "", "").await;
            Err(ConnectError::at(
                ConnectStage::HostKey,
                ConnectErrorKind::Protocol,
            ))
        }
    }
}

/// Name lookup, TCP connect and SSH handshake, ending with the host key check.
async fn handshake(
    endpoint: &HostEndpoint,
    allow_legacy_rsa: bool,
    timeout: Duration,
    known: &[HostKey],
) -> Result<client::Handle<Verifier>, ConnectError> {
    let timed_out = |stage| ConnectError::at(stage, ConnectErrorKind::Timeout);

    let addrs: Vec<SocketAddr> = tokio::time::timeout(
        timeout,
        tokio::net::lookup_host((endpoint.host.as_str(), endpoint.port)),
    )
    .await
    .map_err(|_| timed_out(ConnectStage::Resolve))?
    .map_err(|e| ConnectError::new(ConnectErrorKind::ResolveFailed).with_detail(e.to_string()))?
    .collect();
    if addrs.is_empty() {
        return Err(ConnectError::new(ConnectErrorKind::ResolveFailed));
    }

    let socket = tokio::time::timeout(timeout, async {
        let mut last = None;
        for addr in &addrs {
            match tokio::net::TcpStream::connect(addr).await {
                Ok(s) => return Ok(s),
                Err(e) => last = Some(e),
            }
        }
        Err(last.expect("at least one address"))
    })
    .await
    .map_err(|_| timed_out(ConnectStage::Connect))?
    .map_err(|e| tcp_error(&e))?;
    let _ = socket.set_nodelay(true);

    let verdict = Arc::new(Mutex::new(None));
    let handler = Verifier {
        known: known.to_vec(),
        verdict: verdict.clone(),
    };
    let config = Arc::new(client::Config {
        preferred: preferred(allow_legacy_rsa),
        ..Default::default()
    });
    let handshake = tokio::time::timeout(timeout, client::connect_stream(config, socket, handler))
        .await
        .map_err(|_| timed_out(ConnectStage::Handshake))?;
    let presented = verdict.lock().expect("verdict lock").take();
    match (handshake, presented) {
        (Ok(handle), Some((_, TrustCheck::Trusted))) => Ok(handle),
        (_, Some((presented, TrustCheck::Unknown))) => {
            Err(ConnectError::new(ConnectErrorKind::HostKeyUnknown {
                endpoint: endpoint.clone(),
                presented,
            }))
        }
        (_, Some((presented, TrustCheck::Changed { known }))) => {
            Err(ConnectError::new(ConnectErrorKind::HostKeyChanged {
                endpoint: endpoint.clone(),
                known,
                presented,
            }))
        }
        (Err(e), _) => Err(ssh_error(ConnectStage::Handshake, &e)),
        (Ok(_), None) => Err(
            ConnectError::at(ConnectStage::HostKey, ConnectErrorKind::Protocol)
                .with_note(Message::NoHostKey),
        ),
    }
}

/// Default algorithms without plain `ssh-rsa` (SHA-1) unless the server allows it (ARC-21).
fn preferred(allow_legacy_rsa: bool) -> Preferred {
    let key: Vec<Algorithm> = Preferred::DEFAULT
        .key
        .iter()
        .filter(|a| allow_legacy_rsa || !matches!(a, Algorithm::Rsa { hash: None }))
        .cloned()
        .collect();
    Preferred {
        key: Cow::Owned(key),
        ..Preferred::DEFAULT
    }
}

/// Checks the server key during the handshake and records what it saw, so the caller can
/// show D02 with the exact key.
pub(crate) struct Verifier {
    known: Vec<HostKey>,
    verdict: Arc<Mutex<Option<(HostKey, TrustCheck)>>>,
}

impl Handler for Verifier {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        // Host certificates are not requested (`host_key_certificates` stays empty).
        let PublicKeyOrCertificate::PublicKey { key, .. } = key else {
            return Ok(false);
        };
        let presented = HostKey {
            algorithm: key.algorithm().to_string(),
            fingerprint: key.fingerprint(HashAlg::Sha256).to_string(),
        };
        let check = check_host_key(&self.known, &presented);
        let trusted = check == TrustCheck::Trusted;
        *self.verdict.lock().expect("verdict lock") = Some((presented, check));
        Ok(trusted)
    }
}
