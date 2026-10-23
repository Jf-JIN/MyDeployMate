//! Passwords and passphrases typed in D04, and remembered ones (SRS SEC-02, SEC-03; IXD-31,
//! IXD-32; architecture ARC-26).
//!
//! A connection asks [`PromptingSecrets`] for a secret. A remembered value is tried first
//! without a dialog; otherwise, or when the remembered value was rejected, the operator is
//! asked through an [`Operator`]: the connection test page answers with `submit_secret`, the
//! run page through the run's control channel. A value is remembered only after the server or
//! key accepted it, and only when the operator ticked "remember".

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use mdm_core::{
    Clock, CredentialPrompt, CredentialPurpose, CredentialRef, EnvironmentId, Message,
    OperatorPrompt, ServerId, VariableId,
};
use mdm_exec::ssh::{MAX_ATTEMPTS, SecretFuture, SecretPrompt, SecretSource};
use mdm_exec::{OperatorAnswer, SystemClock};
use mdm_store::{SavedCredential, SecretStore};
use secrecy::SecretString;
use tauri::{AppHandle, Manager, State};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::error::{AppError, AppErrorKind};
use crate::state::AppState;

pub type AskFuture<'a> = Pin<Box<dyn Future<Output = Option<OperatorAnswer>> + Send + 'a>>;

/// Shows D02 or D04 and waits. `None`: declined, cancelled, or nobody to ask.
pub trait Operator: Send + Sync {
    fn ask(&self, prompt: OperatorPrompt) -> AskFuture<'_>;
}

/// Open prompts of connection tests, answered by `submit_secret` and `answer_host_trust`.
#[derive(Default)]
pub struct PromptBroker {
    pending: Mutex<HashMap<u64, oneshot::Sender<Option<OperatorAnswer>>>>,
    next: AtomicU64,
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl PromptBroker {
    /// Send `prompt` with `send` under a new request ID and wait for the answer.
    pub async fn ask(
        &self,
        prompt: OperatorPrompt,
        cancel: &CancellationToken,
        send: impl FnOnce(u64, OperatorPrompt) -> bool,
    ) -> Option<OperatorAnswer> {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let (tx, rx) = oneshot::channel();
        lock(&self.pending).insert(id, tx);
        let answer = if send(id, prompt) {
            tokio::select! {
                _ = cancel.cancelled() => None,
                r = rx => r.ok().flatten(),
            }
        } else {
            None
        };
        lock(&self.pending).remove(&id);
        answer
    }

    pub fn answer(&self, request_id: u64, answer: Option<OperatorAnswer>) -> Result<(), AppError> {
        match lock(&self.pending).remove(&request_id) {
            Some(tx) => {
                let _ = tx.send(answer);
                Ok(())
            }
            None => Err(AppError::new(
                AppErrorKind::Validation,
                Message::RequestExpired,
            )),
        }
    }
}

/// The D04 answer of a connection test. `secret: None` cancels that test (IXD-32).
#[tauri::command]
pub fn submit_secret(
    state: State<'_, AppState>,
    request_id: u64,
    secret: Option<String>,
    remember: bool,
) -> Result<(), AppError> {
    state
        .broker
        .answer(request_id, secret_answer(secret, remember))
}

pub fn secret_answer(secret: Option<String>, remember: bool) -> Option<OperatorAnswer> {
    secret.map(|s| OperatorAnswer::Secret {
        value: SecretString::from(s),
        remember,
    })
}

/// Who the secrets are for.
pub struct SecretContext {
    pub server_id: ServerId,
    pub server_name: String,
    pub address: String,
    pub user: String,
    pub context: Message,
}

/// The value last handed out, kept until the connection says whether it worked.
struct Given {
    purpose: CredentialPurpose,
    value: SecretString,
    remember: bool,
    from_saved: bool,
}

/// D04 plus remembered credentials, for one connection attempt.
pub struct PromptingSecrets<'a> {
    app: &'a AppHandle,
    who: &'a SecretContext,
    operator: &'a dyn Operator,
    given: Mutex<Option<Given>>,
}

impl<'a> PromptingSecrets<'a> {
    pub fn new(app: &'a AppHandle, who: &'a SecretContext, operator: &'a dyn Operator) -> Self {
        Self {
            app,
            who,
            operator,
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

        let answer = self
            .operator
            .ask(OperatorPrompt::Credential(CredentialPrompt {
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
            }))
            .await;
        let Some(OperatorAnswer::Secret { value, remember }) = answer else {
            return None;
        };
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

impl SecretSource for PromptingSecrets<'_> {
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
        Err(reason) => Err(AppError::new(
            AppErrorKind::StorageUnavailable,
            Message::SavedCredentialUnavailable {
                reason: Box::new(reason.clone()),
            },
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
        return Err(AppError::new(
            AppErrorKind::Validation,
            Message::ValueRequired,
        ));
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
