//! Login methods (SRS SRV-02, architecture ARC-21): private key file (OpenSSH, PEM, PuTTY
//! `.ppk`), SSH agent, and password. Choosing a key file never falls back to the agent or to
//! other keys; an unavailable agent is an error, not a reason to try something else.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use mdm_core::{
    AgentKind, ConnectError, ConnectErrorKind, ConnectStage, CredentialPurpose, KeyFormat, Message,
    PasswordSource, detect_key_format,
};
use russh::client::{AuthResult, Handle, Handler};
use russh::keys::agent::AgentIdentity;
use russh::keys::agent::client::{AgentClient, AgentStream};
use russh::keys::{Algorithm, HashAlg, PrivateKey, PrivateKeyWithHashAlg};
use russh::{MethodKind, MethodSet};
use secrecy::{ExposeSecret, SecretString};

use super::error::ssh_error;

/// Wrong passwords or passphrases allowed per prompt (IXD-32).
pub const MAX_ATTEMPTS: u8 = 3;

/// One request for a secret the operator types (D04).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecretPrompt {
    pub purpose: CredentialPurpose,
    /// 1 for the first request, up to [`MAX_ATTEMPTS`].
    pub attempt: u8,
    /// The previous value was rejected (wrong password or passphrase).
    pub rejected: bool,
    /// The key file being unlocked, for [`CredentialPurpose::KeyPassphrase`].
    pub key_path: Option<String>,
}

pub type SecretFuture<'a> = Pin<Box<dyn Future<Output = Option<SecretString>> + Send + 'a>>;

/// Where passwords and passphrases come from: the D04 dialog, a saved credential, or a test.
pub trait SecretSource: Send + Sync {
    /// `None` cancels: the connection attempt ends with [`ConnectErrorKind::Cancelled`].
    fn secret<'a>(&'a self, prompt: &'a SecretPrompt) -> SecretFuture<'a>;

    /// The last value given for `purpose` worked (lets a source remember it, SEC-03).
    fn accepted(&self, _purpose: &CredentialPurpose) {}
}

/// What to log in with, prepared before the network is touched so that a missing file,
/// variable or agent fails fast (SRV-02).
pub(crate) enum Prepared {
    Key {
        path: String,
        text: String,
        encrypted: bool,
    },
    Agent(BoxedAgent, Vec<AgentIdentity>),
    Password(Option<SecretString>),
}

pub(crate) async fn prepare(auth: &mdm_core::AuthMethod) -> Result<Prepared, ConnectError> {
    use mdm_core::AuthMethod;
    match auth {
        AuthMethod::KeyFile { path } => {
            let text = tokio::fs::read_to_string(path).await.map_err(|e| {
                ConnectError::new(ConnectErrorKind::KeyFileUnreadable { path: path.clone() })
                    .with_detail(e.to_string())
            })?;
            let encrypted = match detect_key_format(text.as_bytes()) {
                KeyFormat::OpenSsh { encrypted }
                | KeyFormat::Pem { encrypted }
                | KeyFormat::Putty { encrypted, .. } => encrypted,
                KeyFormat::Unknown => {
                    return Err(ConnectError::new(ConnectErrorKind::KeyFormatUnsupported {
                        path: path.clone(),
                    }));
                }
            };
            let prepared = Prepared::Key {
                path: path.clone(),
                text,
                encrypted,
            };
            if !encrypted {
                // Parse now so an unsupported key type fails before connecting.
                if let Prepared::Key { path, text, .. } = &prepared {
                    parse_key(path, text, None)?;
                }
            }
            Ok(prepared)
        }
        AuthMethod::Agent { agent } => {
            let mut client = connect_agent(*agent).await.map_err(|e| {
                ConnectError::new(ConnectErrorKind::AgentUnavailable).with_detail(e)
            })?;
            let ids = client.request_identities().await.map_err(|e| {
                ConnectError::new(ConnectErrorKind::AgentUnavailable).with_detail(e.to_string())
            })?;
            let ids: Vec<_> = ids
                .into_iter()
                .filter(|i| matches!(i, AgentIdentity::PublicKey { .. }))
                .collect();
            if ids.is_empty() {
                return Err(ConnectError::new(ConnectErrorKind::AgentNoIdentities));
            }
            Ok(Prepared::Agent(client, ids))
        }
        AuthMethod::Password {
            source: PasswordSource::EnvVar { name },
        } => match std::env::var(name) {
            Ok(v) if !v.is_empty() => Ok(Prepared::Password(Some(SecretString::from(v)))),
            _ => Err(ConnectError::new(ConnectErrorKind::PasswordEnvMissing {
                name: name.clone(),
            })),
        },
        AuthMethod::Password {
            source: PasswordSource::Prompt,
        } => Ok(Prepared::Password(None)),
    }
}

type BoxedAgent = AgentClient<Box<dyn AgentStream + Send + Unpin>>;

#[cfg(windows)]
async fn connect_agent(kind: AgentKind) -> Result<BoxedAgent, String> {
    match kind {
        AgentKind::Openssh => AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent")
            .await
            .map(AgentClient::dynamic)
            .map_err(|e| e.to_string()),
        AgentKind::Pageant => AgentClient::connect_pageant()
            .await
            .map(AgentClient::dynamic)
            .map_err(|e| e.to_string()),
    }
}

/// Development and tests on Linux: the OpenSSH agent is `SSH_AUTH_SOCK`; Pageant is Windows only.
#[cfg(unix)]
async fn connect_agent(kind: AgentKind) -> Result<BoxedAgent, String> {
    match kind {
        AgentKind::Openssh => AgentClient::connect_env()
            .await
            .map(AgentClient::dynamic)
            .map_err(|e| e.to_string()),
        AgentKind::Pageant => Err("Pageant is only available on Windows".into()),
    }
}

fn parse_key(path: &str, text: &str, passphrase: Option<&str>) -> Result<PrivateKey, ConnectError> {
    let result = if text
        .trim_start_matches('\u{feff}')
        .trim_start()
        .starts_with("PuTTY-User-Key-File-")
    {
        PrivateKey::from_ppk(text, passphrase.map(str::to_owned)).map_err(|e| e.to_string())
    } else {
        russh::keys::decode_secret_key(text, passphrase).map_err(|e| e.to_string())
    };
    result.map_err(|detail| {
        let kind = if passphrase.is_some() {
            ConnectErrorKind::KeyPassphraseWrong
        } else {
            ConnectErrorKind::KeyFormatUnsupported {
                path: path.to_owned(),
            }
        };
        ConnectError::new(kind).with_detail(detail)
    })
}

/// Log in on an established, verified connection.
pub(crate) async fn authenticate<H: Handler<Error = russh::Error>>(
    handle: &mut Handle<H>,
    user: &str,
    prepared: Prepared,
    allow_legacy_rsa: bool,
    secrets: &dyn SecretSource,
    timeout: Duration,
) -> Result<(), ConnectError> {
    match prepared {
        Prepared::Password(Some(password)) => {
            let res = net(
                timeout,
                handle.authenticate_password(user, password.expose_secret()),
            )
            .await?;
            accepted_or_failed(res)
        }
        Prepared::Password(None) => {
            let purpose = CredentialPurpose::SshPassword;
            for attempt in 1..=MAX_ATTEMPTS {
                let prompt = SecretPrompt {
                    purpose: purpose.clone(),
                    attempt,
                    rejected: attempt > 1,
                    key_path: None,
                };
                let password = ask(secrets, &prompt).await?;
                match net(
                    timeout,
                    handle.authenticate_password(user, password.expose_secret()),
                )
                .await?
                {
                    AuthResult::Success => {
                        secrets.accepted(&purpose);
                        return Ok(());
                    }
                    AuthResult::Failure {
                        remaining_methods, ..
                    } if !allows(&remaining_methods, MethodKind::Password) => break,
                    AuthResult::Failure { .. } => {}
                }
            }
            Err(failed())
        }
        Prepared::Key {
            path,
            text,
            encrypted: false,
        } => {
            let key = parse_key(&path, &text, None)?;
            publickey(handle, user, key, allow_legacy_rsa, timeout).await
        }
        Prepared::Key {
            path,
            text,
            encrypted: true,
        } => {
            let purpose = CredentialPurpose::KeyPassphrase;
            let mut last = None;
            for attempt in 1..=MAX_ATTEMPTS {
                let prompt = SecretPrompt {
                    purpose: purpose.clone(),
                    attempt,
                    rejected: attempt > 1,
                    key_path: Some(path.clone()),
                };
                let passphrase = ask(secrets, &prompt).await?;
                match parse_key(&path, &text, Some(passphrase.expose_secret())) {
                    Ok(key) => {
                        // The passphrase is right whether or not the server takes the key.
                        secrets.accepted(&purpose);
                        return publickey(handle, user, key, allow_legacy_rsa, timeout).await;
                    }
                    Err(e) => last = Some(e),
                }
            }
            Err(last.expect("at least one attempt"))
        }
        Prepared::Agent(mut agent, ids) => {
            for id in ids {
                let AgentIdentity::PublicKey { key, .. } = &id else {
                    continue;
                };
                let hash = rsa_hash(handle, key.algorithm(), allow_legacy_rsa).await?;
                let res = tokio::time::timeout(
                    timeout,
                    handle.authenticate_publickey_with(user, key.clone(), hash, &mut agent),
                )
                .await
                .map_err(|_| {
                    ConnectError::at(ConnectStage::Authenticate, ConnectErrorKind::Timeout)
                })?
                .map_err(|e| {
                    ConnectError::at(
                        ConnectStage::Authenticate,
                        ConnectErrorKind::AgentUnavailable,
                    )
                    .with_detail(e.to_string())
                })?;
                if res.success() {
                    return Ok(());
                }
            }
            Err(failed().with_note(Message::AgentKeysRejected))
        }
    }
}

async fn publickey<H: Handler<Error = russh::Error>>(
    handle: &mut Handle<H>,
    user: &str,
    key: PrivateKey,
    allow_legacy_rsa: bool,
    timeout: Duration,
) -> Result<(), ConnectError> {
    let hash = rsa_hash(handle, key.algorithm(), allow_legacy_rsa).await?;
    let key = PrivateKeyWithHashAlg::new(Arc::new(key), hash);
    let res = net(timeout, handle.authenticate_publickey(user, key)).await?;
    accepted_or_failed(res)
}

/// Signature hash for an RSA user key. SHA-1 (`ssh-rsa`) only with the legacy toggle (SRV-01).
async fn rsa_hash<H: Handler<Error = russh::Error>>(
    handle: &Handle<H>,
    algorithm: Algorithm,
    allow_legacy_rsa: bool,
) -> Result<Option<HashAlg>, ConnectError> {
    if !matches!(algorithm, Algorithm::Rsa { .. }) {
        return Ok(None);
    }
    let best = handle
        .best_supported_rsa_hash()
        .await
        .map_err(|e| ssh_error(ConnectStage::Authenticate, &e))?;
    match best {
        Some(Some(hash)) => Ok(Some(hash)),
        // The server only accepts ssh-rsa signatures.
        Some(None) if allow_legacy_rsa => Ok(None),
        Some(None) => Err(ConnectError::at(
            ConnectStage::Authenticate,
            ConnectErrorKind::AlgorithmNegotiation {
                legacy_rsa_only: true,
            },
        )),
        // The server did not say (no server-sig-algs): old servers expect ssh-rsa.
        None if allow_legacy_rsa => Ok(None),
        None => Ok(Some(HashAlg::Sha256)),
    }
}

async fn ask(
    secrets: &dyn SecretSource,
    prompt: &SecretPrompt,
) -> Result<SecretString, ConnectError> {
    secrets
        .secret(prompt)
        .await
        .ok_or_else(|| ConnectError::at(ConnectStage::Authenticate, ConnectErrorKind::Cancelled))
}

/// One authentication round trip, bounded by the connect timeout. Time spent waiting for
/// the operator is not counted.
async fn net(
    timeout: Duration,
    fut: impl Future<Output = Result<AuthResult, russh::Error>>,
) -> Result<AuthResult, ConnectError> {
    tokio::time::timeout(timeout, fut)
        .await
        .map_err(|_| ConnectError::at(ConnectStage::Authenticate, ConnectErrorKind::Timeout))?
        .map_err(|e| ssh_error(ConnectStage::Authenticate, &e))
}

fn allows(methods: &MethodSet, kind: MethodKind) -> bool {
    methods.contains(&kind)
}

fn accepted_or_failed(res: AuthResult) -> Result<(), ConnectError> {
    if res.success() { Ok(()) } else { Err(failed()) }
}

fn failed() -> ConnectError {
    ConnectError::new(ConnectErrorKind::AuthenticationFailed)
}
