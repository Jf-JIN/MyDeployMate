//! Passwords and passphrases typed in D04, and remembered ones (SRS SEC-02, SEC-03; IXD-31,
//! IXD-32; architecture ARC-26).
//!
//! A connection asks [`PromptingSecrets`] for a secret. A remembered value is tried first
//! without a dialog; otherwise, or when the remembered value was rejected, the page receives a
//! [`CredentialPrompt`] and answers with `submit_secret`. A value is remembered only after the
//! server or key accepted it, and only when the operator ticked "remember".

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use mdm_core::{Clock, CredentialPurpose, CredentialRef, EnvironmentId, ServerId, VariableId};
use mdm_exec::SystemClock;
use mdm_exec::ssh::{MAX_ATTEMPTS, SecretFuture, SecretPrompt, SecretSource};
use mdm_store::{SavedCredential, SecretStore};
use secrecy::SecretString;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::error::{AppError, AppErrorKind};
use crate::state::AppState;

/// What D04 shows. Never contains a secret.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CredentialPrompt {
    #[ts(type = "number")]
    pub request_id: u64,
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

type Answer = Option<(SecretString, bool)>;

/// Open D04 prompts, answered by `submit_secret`.
#[derive(Default)]
pub struct CredentialBroker {
    pending: Mutex<HashMap<u64, oneshot::Sender<Answer>>>,
    next: AtomicU64,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl CredentialBroker {
    fn open(&self) -> (u64, oneshot::Receiver<Answer>) {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let (tx, rx) = oneshot::channel();
        lock(&self.pending).insert(id, tx);
        (id, rx)
    }

    fn close(&self, id: u64) {
        lock(&self.pending).remove(&id);
    }
}

/// The D04 answer. `secret: None` cancels the prompt: a connection test stops, a run is
/// cancelled (IXD-32).
#[tauri::command]
pub fn submit_secret(
    state: State<'_, AppState>,
    request_id: u64,
    secret: Option<String>,
    remember: bool,
) -> Result<(), AppError> {
    let sender = lock(&state.broker.pending).remove(&request_id);
    match sender {
        Some(tx) => {
            let _ = tx.send(secret.map(|s| (SecretString::from(s), remember)));
            Ok(())
        }
        None => Err(AppError::new(AppErrorKind::Validation, "该凭据请求已失效")),
    }
}

/// Who the secrets are for.
pub struct SecretContext {
    pub server_id: ServerId,
    pub server_name: String,
    pub address: String,
    pub user: String,
    pub context: String,
}

/// The value last handed out, kept until the connection says whether it worked.
struct Given {
    purpose: CredentialPurpose,
    value: SecretString,
    remember: bool,
    from_saved: bool,
}

/// D04 plus remembered credentials, for one connection attempt.
pub struct PromptingSecrets<'a, F: Fn(CredentialPrompt) -> bool + Send + Sync> {
    app: &'a AppHandle,
    who: SecretContext,
    send: F,
    cancel: CancellationToken,
    given: Mutex<Option<Given>>,
}

impl<'a, F: Fn(CredentialPrompt) -> bool + Send + Sync> PromptingSecrets<'a, F> {
    pub fn new(app: &'a AppHandle, who: SecretContext, cancel: CancellationToken, send: F) -> Self {
        Self {
            app,
            who,
            send,
            cancel,
            given: Mutex::new(None),
        }
    }

    fn reference(&self, purpose: &CredentialPurpose) -> CredentialRef {
        CredentialRef {
            server_id: self.who.server_id,
            user: self.who.user.clone(),
            purpose: purpose.clone(),
        }
    }

    fn saved(&self, purpose: &CredentialPurpose) -> Option<SecretString> {
        let state = self.app.state::<AppState>();
        let secrets = lock(&state.secrets);
        secrets
            .as_ref()
            .ok()
            .and_then(|s| s.credential(&self.reference(purpose)).ok().flatten())
    }

    async fn ask(&self, prompt: &SecretPrompt) -> Option<SecretString> {
        let saved_rejected = prompt.rejected
            && lock(&self.given)
                .as_ref()
                .is_some_and(|g| g.from_saved && g.purpose == prompt.purpose);
        if !prompt.rejected
            && let Some(value) = self.saved(&prompt.purpose)
        {
            *lock(&self.given) = Some(Given {
                purpose: prompt.purpose.clone(),
                value: value.clone(),
                remember: true,
                from_saved: true,
            });
            return Some(value);
        }

        let broker = &self.app.state::<AppState>().broker;
        let (request_id, rx) = broker.open();
        let sent = (self.send)(CredentialPrompt {
            request_id,
            purpose: prompt.purpose.clone(),
            server_name: self.who.server_name.clone(),
            address: self.who.address.clone(),
            user: self.who.user.clone(),
            key_path: prompt.key_path.clone(),
            context: self.who.context.clone(),
            attempt: prompt.attempt,
            max_attempts: MAX_ATTEMPTS,
            rejected: prompt.rejected,
            saved_rejected,
            remember_default: saved_rejected,
        });
        let answer = if sent {
            tokio::select! {
                _ = self.cancel.cancelled() => None,
                r = rx => r.ok().flatten(),
            }
        } else {
            None
        };
        broker.close(request_id);
        let (value, remember) = answer?;
        *lock(&self.given) = Some(Given {
            purpose: prompt.purpose.clone(),
            value: value.clone(),
            remember,
            from_saved: false,
        });
        if saved_rejected && !remember {
            // The operator chose not to keep a remembered value that no longer works.
            self.forget(&prompt.purpose);
        }
        Some(value)
    }

    fn forget(&self, purpose: &CredentialPurpose) {
        let state = self.app.state::<AppState>();
        if let Ok(store) = lock(&state.secrets).as_mut() {
            let _ = store.forget(&self.reference(purpose));
        }
    }
}

impl<F: Fn(CredentialPrompt) -> bool + Send + Sync> SecretSource for PromptingSecrets<'_, F> {
    fn secret<'b>(&'b self, prompt: &'b SecretPrompt) -> SecretFuture<'b> {
        Box::pin(self.ask(prompt))
    }

    fn accepted(&self, purpose: &CredentialPurpose) {
        let given = lock(&self.given).take();
        let Some(given) = given.filter(|g| &g.purpose == purpose) else {
            return;
        };
        if given.from_saved || !given.remember {
            return;
        }
        let state = self.app.state::<AppState>();
        if let Ok(store) = lock(&state.secrets).as_mut() {
            // Remembering is a convenience: a failure here does not fail the connection.
            let _ = store.remember(&self.reference(purpose), &given.value, SystemClock.now_ms());
        }
    }
}

// ---- Configuration center (IXD-36) ----------------------------------------------------------

fn with_store<T>(
    state: &AppState,
    f: impl FnOnce(&mut SecretStore) -> Result<T, AppError>,
) -> Result<T, AppError> {
    let mut secrets = lock(&state.secrets);
    match secrets.as_mut() {
        Ok(store) => f(store),
        Err(message) => Err(AppError::new(
            AppErrorKind::StorageUnavailable,
            format!("已保存凭据不可用：{message}"),
        )),
    }
}

#[tauri::command]
pub fn saved_credentials(state: State<'_, AppState>) -> Result<Vec<SavedCredential>, AppError> {
    with_store(&state, |s| Ok(s.list().to_vec()))
}

#[tauri::command]
pub fn forget_credential(
    state: State<'_, AppState>,
    credential: CredentialRef,
) -> Result<(), AppError> {
    with_store(&state, |s| {
        s.forget(&credential)?;
        Ok(())
    })
}

/// Sensitive variable values (VAR-03, ARC-10): written straight to Credential Manager, never
/// to the workspace draft.
#[tauri::command]
pub fn set_variable_secret(
    state: State<'_, AppState>,
    variable_id: VariableId,
    environment_id: Option<EnvironmentId>,
    value: String,
) -> Result<(), AppError> {
    if value.is_empty() {
        return Err(AppError::new(AppErrorKind::Validation, "值不能为空"));
    }
    with_store(&state, |s| {
        Ok(s.set_variable(variable_id, environment_id, &SecretString::from(value))?)
    })
}

#[tauri::command]
pub fn variable_secret_present(
    state: State<'_, AppState>,
    variable_id: VariableId,
    environment_id: Option<EnvironmentId>,
) -> Result<bool, AppError> {
    with_store(&state, |s| {
        Ok(s.variable(variable_id, environment_id)?.is_some())
    })
}
