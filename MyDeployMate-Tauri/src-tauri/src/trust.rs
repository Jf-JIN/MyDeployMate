//! Host trust records in the configuration center (IXD-19, IXD-36; architecture ARC-20).
//!
//! An update never trusts blindly: the page first reads the key the server presents now with
//! `fetch_host_key`, shows it next to the recorded one, and only then calls
//! `replace_trusted_host` with the key the operator compared.

use std::time::Duration;

use mdm_core::{Clock, HostEndpoint, HostKey, Message, TrustedHost};
use mdm_exec::SystemClock;
use mdm_exec::ssh::fetch_host_key as read_host_key;
use mdm_store::TrustStore;
use tauri::{AppHandle, Manager, State};
use tokio_util::sync::CancellationToken;

use crate::error::{AppError, AppErrorKind};
use crate::state::AppState;

fn with_store<T>(
    state: &AppState,
    f: impl FnOnce(&mut TrustStore) -> Result<T, AppError>,
) -> Result<T, AppError> {
    let mut trust = state.trust.lock().unwrap_or_else(|e| e.into_inner());
    match trust.as_mut() {
        Ok(store) => f(store),
        Err(reason) => Err(AppError::new(
            AppErrorKind::StorageUnavailable,
            Message::TrustStoreUnavailable {
                reason: Box::new(reason.clone()),
            },
        )),
    }
}

pub fn known_keys(state: &AppState, endpoint: &HostEndpoint) -> Result<Vec<HostKey>, AppError> {
    with_store(state, |s| Ok(s.keys_for(endpoint)))
}

pub fn trust_new(state: &AppState, endpoint: &HostEndpoint, key: &HostKey) -> Result<(), AppError> {
    with_store(state, |s| {
        Ok(s.trust_new(endpoint, key, SystemClock.now_ms())?)
    })
}

#[tauri::command]
pub fn trusted_hosts(state: State<'_, AppState>) -> Result<Vec<TrustedHost>, AppError> {
    with_store(&state, |s| Ok(s.list().to_vec()))
}

/// Forget every recorded key of `endpoint`; the next connection asks for first trust again.
#[tauri::command]
pub fn remove_trusted_host(
    state: State<'_, AppState>,
    endpoint: HostEndpoint,
) -> Result<(), AppError> {
    with_store(&state, |s| {
        s.remove(&endpoint)?;
        Ok(())
    })
}

/// The key `endpoint` presents now, read without logging in.
#[tauri::command]
pub async fn fetch_host_key(endpoint: HostEndpoint, timeout_sec: u32) -> Result<HostKey, AppError> {
    let timeout = Duration::from_secs(timeout_sec.clamp(1, 120).into());
    read_host_key(&endpoint, timeout, &CancellationToken::new())
        .await
        .map_err(|e| AppError::new(AppErrorKind::Connection, e.message))
}

/// Explicit update after the operator compared old and new keys (IXD-19).
#[tauri::command]
pub fn replace_trusted_host(
    app: AppHandle,
    endpoint: HostEndpoint,
    key: HostKey,
) -> Result<(), AppError> {
    let state = app.state::<AppState>();
    with_store(&state, |s| {
        Ok(s.replace(&endpoint, &key, SystemClock.now_ms())?)
    })
}
