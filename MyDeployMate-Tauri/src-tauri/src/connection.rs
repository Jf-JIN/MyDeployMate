//! Connection tests (SRS SRV-04, SRV-05; IXD-18) and first trust of a server key from a test
//! (D02, IXD-19; architecture ARC-20).
//!
//! A test logs in to the saved server, runs a side-effect-free probe, and keeps the result in
//! memory together with the connection fields it used. A result whose fields no longer match
//! the saved server is dropped, so editing the address, port, user, authentication or the
//! legacy algorithm switch turns the server back to "untested" (SRV-05).

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use mdm_core::{
    Clock, ConnectError, ConnectErrorKind, HostEndpoint, HostKey, Server, ServerId, Workspace,
};
use mdm_exec::SystemClock;
use mdm_exec::ssh::{SshTarget, connect};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::credentials::{CredentialPrompt, PromptingSecrets, SecretContext};
use crate::error::{AppError, AppErrorKind};
use crate::state::AppState;

/// The latest test of one server.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConnectionTest {
    pub server_id: ServerId,
    #[ts(type = "number")]
    pub started_at_ms: i64,
    #[ts(type = "number")]
    pub duration_ms: u64,
    pub outcome: TestOutcome,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum TestOutcome {
    /// Logged in and the probe ran; `system` is what `uname -srm` printed.
    Succeeded {
        system: String,
    },
    Failed {
        error: ConnectError,
    },
    Cancelled,
}

/// Pushed to the page while a test runs.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum TestEvent {
    /// First connection: show D02 and answer with `answer_host_trust`.
    HostTrust {
        #[ts(type = "number")]
        request_id: u64,
        server_id: ServerId,
        server_name: String,
        user: String,
        endpoint: HostEndpoint,
        presented: HostKey,
    },
    /// A password or passphrase is needed: show D04 and answer with `submit_secret`.
    Credential(CredentialPrompt),
}

#[derive(Default)]
pub struct ConnectionTests {
    /// Latest result per server, with the connection fields it was taken with.
    results: Mutex<HashMap<ServerId, (String, ConnectionTest)>>,
    /// Running tests by the operation ID the page chose.
    running: Mutex<HashMap<String, CancellationToken>>,
    /// Open D02 prompts.
    trust_prompts: Mutex<HashMap<u64, oneshot::Sender<bool>>>,
    next_request: AtomicU64,
}

/// The fields whose change invalidates a test result (SRV-05).
fn signature(server: &Server) -> String {
    serde_json::to_string(&(
        server.host.trim(),
        server.port,
        &server.user,
        &server.auth,
        server.allow_legacy_rsa,
    ))
    .expect("server fields serialize")
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl ConnectionTests {
    /// Results that still match the saved servers.
    fn current(&self, workspace: &Workspace) -> Vec<ConnectionTest> {
        let results = lock(&self.results);
        workspace
            .servers
            .iter()
            .filter_map(|s| match results.get(&s.id) {
                Some((sig, test)) if *sig == signature(s) => Some(test.clone()),
                _ => None,
            })
            .collect()
    }
}

/// Test results that are still valid for the saved configuration.
#[tauri::command]
pub fn connection_tests(app: AppHandle) -> Result<Vec<ConnectionTest>, AppError> {
    let state = app.state::<AppState>();
    let (workspace, _) = state.store()?.load()?;
    Ok(state.tests.current(&workspace))
}

/// Test the saved server `server_id`. `operation_id` names the test for
/// `cancel_connection_test`; D02 requests go to `on_event`.
#[tauri::command]
pub async fn test_connection(
    app: AppHandle,
    server_id: ServerId,
    operation_id: String,
    on_event: Channel<TestEvent>,
) -> Result<ConnectionTest, AppError> {
    let state = app.state::<AppState>();
    let (workspace, _) = state.store()?.load()?;
    let server = workspace
        .servers
        .iter()
        .find(|s| s.id == server_id)
        .cloned()
        .ok_or_else(|| AppError::new(AppErrorKind::NotFound, "服务器不存在，可能已被删除"))?;

    let cancel = CancellationToken::new();
    {
        let mut running = lock(&state.tests.running);
        if running.contains_key(&operation_id) {
            return Err(AppError::new(AppErrorKind::Validation, "该测试已在进行"));
        }
        running.insert(operation_id.clone(), cancel.clone());
    }
    let started_at_ms = SystemClock.now_ms();
    let started = Instant::now();
    let outcome = run_test(&app, &server, &on_event, &cancel).await;
    lock(&state.tests.running).remove(&operation_id);

    let test = ConnectionTest {
        server_id,
        started_at_ms,
        duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        outcome,
    };
    lock(&state.tests.results).insert(server_id, (signature(&server), test.clone()));
    Ok(test)
}

async fn run_test(
    app: &AppHandle,
    server: &Server,
    on_event: &Channel<TestEvent>,
    cancel: &CancellationToken,
) -> TestOutcome {
    let target = SshTarget::from_server(server);
    let state = app.state::<AppState>();
    let secrets = PromptingSecrets::new(
        app,
        SecretContext {
            server_id: server.id,
            server_name: server.name.clone(),
            address: target.endpoint.to_string(),
            user: server.user.clone(),
            context: "连接测试".into(),
        },
        cancel.clone(),
        |prompt| on_event.send(TestEvent::Credential(prompt)).is_ok(),
    );
    // At most two rounds: the second follows a first trust the operator just confirmed.
    for _ in 0..2 {
        let known = match crate::trust::known_keys(&state, &target.endpoint) {
            Ok(k) => k,
            Err(e) => return failed_storage(e),
        };
        let session = match connect(&target, &known, &secrets, cancel).await {
            Ok(s) => s,
            Err(ConnectError {
                kind: ConnectErrorKind::Cancelled,
                ..
            }) => return TestOutcome::Cancelled,
            Err(ConnectError {
                kind:
                    ConnectErrorKind::HostKeyUnknown {
                        endpoint,
                        presented,
                    },
                ..
            }) => match ask_trust(app, server, &endpoint, &presented, on_event, cancel).await {
                Some(true) => {
                    if let Err(e) = crate::trust::trust_new(&state, &endpoint, &presented) {
                        return failed_storage(e);
                    }
                    continue;
                }
                _ => return TestOutcome::Cancelled,
            },
            Err(error) => return TestOutcome::Failed { error },
        };
        let outcome = tokio::select! {
            _ = cancel.cancelled() => TestOutcome::Cancelled,
            r = session.probe(target.connect_timeout) => match r {
                Ok(system) => TestOutcome::Succeeded { system },
                Err(error) => TestOutcome::Failed { error },
            },
        };
        session.close().await;
        return outcome;
    }
    unreachable!("a trusted key is accepted on the second round")
}

fn failed_storage(e: AppError) -> TestOutcome {
    TestOutcome::Failed {
        error: ConnectError::at(mdm_core::ConnectStage::HostKey, ConnectErrorKind::Protocol)
            .with_detail(e.message),
    }
}

/// Show D02 and wait. `None` when the test was cancelled meanwhile.
async fn ask_trust(
    app: &AppHandle,
    server: &Server,
    endpoint: &HostEndpoint,
    presented: &HostKey,
    on_event: &Channel<TestEvent>,
    cancel: &CancellationToken,
) -> Option<bool> {
    let tests = &app.state::<AppState>().tests;
    let request_id = tests.next_request.fetch_add(1, Ordering::Relaxed) + 1;
    let (tx, rx) = oneshot::channel();
    lock(&tests.trust_prompts).insert(request_id, tx);
    let sent = on_event.send(TestEvent::HostTrust {
        request_id,
        server_id: server.id,
        server_name: server.name.clone(),
        user: server.user.clone(),
        endpoint: endpoint.clone(),
        presented: presented.clone(),
    });
    let answer = if sent.is_err() {
        None
    } else {
        tokio::select! {
            _ = cancel.cancelled() => None,
            r = rx => r.ok(),
        }
    };
    lock(&tests.trust_prompts).remove(&request_id);
    answer
}

/// D02 answer: `trust` after the operator checked the fingerprint, otherwise cancel the test.
#[tauri::command]
pub fn answer_host_trust(app: AppHandle, request_id: u64, trust: bool) -> Result<(), AppError> {
    let state = app.state::<AppState>();
    let sender = lock(&state.tests.trust_prompts).remove(&request_id);
    match sender {
        Some(tx) => {
            let _ = tx.send(trust);
            Ok(())
        }
        None => Err(AppError::new(AppErrorKind::Validation, "该确认已失效")),
    }
}

/// Cancel one running test; a batch cancels its current test this way and starts no more.
#[tauri::command]
pub fn cancel_connection_test(app: AppHandle, operation_id: String) {
    let state = app.state::<AppState>();
    if let Some(cancel) = lock(&state.tests.running).get(&operation_id) {
        cancel.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdm_core::{AuthMethod, EnvironmentId, defaults};

    #[test]
    fn signature_ignores_name_and_tags_but_not_connection_fields() {
        let base = defaults::new_server(EnvironmentId::new());
        let mut renamed = base.clone();
        renamed.name = "renamed".into();
        renamed.tags.push("x".into());
        renamed.notes = "n".into();
        assert_eq!(signature(&base), signature(&renamed));
        for edit in [
            |s: &mut Server| s.host = "other".into(),
            |s: &mut Server| s.port = 2200,
            |s: &mut Server| s.user = "root".into(),
            |s: &mut Server| s.allow_legacy_rsa = !s.allow_legacy_rsa,
            |s: &mut Server| {
                s.auth = AuthMethod::KeyFile {
                    path: "C:/other".into(),
                }
            },
        ] {
            let mut changed = base.clone();
            edit(&mut changed);
            assert_ne!(signature(&base), signature(&changed));
        }
    }
}
