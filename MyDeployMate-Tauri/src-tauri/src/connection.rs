//! Connection tests (SRS SRV-04, SRV-05; IXD-18) and first trust of a server key from a test
//! (D02, IXD-19; architecture ARC-20).
//!
//! A test logs in to the saved server, runs a side-effect-free probe, and keeps the result in
//! memory together with the connection fields it used. A result whose fields no longer match
//! the saved server is dropped, so editing the address, port, user, authentication or the
//! legacy algorithm switch turns the server back to "untested" (SRV-05).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use mdm_core::{
    Clock, ConnectError, ConnectErrorKind, HostTrustPrompt, Message, OperatorPrompt, Server,
    ServerId, Workspace,
};
use mdm_exec::ssh::{SshSession, SshTarget, connect};
use mdm_exec::{OperatorAnswer, SystemClock};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::credentials::{AskFuture, Operator, PromptingSecrets, SecretContext, lock};
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

/// Pushed to the page while a test runs: show D02 or D04, and answer with
/// `answer_host_trust` or `submit_secret` under `request_id`.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TestEvent {
    #[ts(type = "number")]
    pub request_id: u64,
    pub prompt: OperatorPrompt,
}

#[derive(Default)]
pub struct ConnectionTests {
    /// Latest result per server, with the connection fields it was taken with.
    results: Mutex<HashMap<ServerId, (String, ConnectionTest)>>,
    /// Running tests by the operation ID the page chose.
    running: Mutex<HashMap<String, CancellationToken>>,
}

/// The fields whose change invalidates a test result (SRV-05).
pub(crate) fn signature(server: &Server) -> String {
    serde_json::to_string(&(
        server.host.trim(),
        server.port,
        &server.user,
        &server.auth,
        server.allow_legacy_rsa,
    ))
    .expect("server fields serialize")
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
        .ok_or_else(|| AppError::new(AppErrorKind::NotFound, Message::ServerNotFound))?;

    let cancel = CancellationToken::new();
    {
        let mut running = lock(&state.tests.running);
        if running.contains_key(&operation_id) {
            return Err(AppError::new(
                AppErrorKind::Validation,
                Message::TestRunning,
            ));
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
    let operator = TestOperator {
        app,
        on_event,
        cancel,
    };
    let session = match open_session(app, server, Message::ConnectionTest, &operator, cancel).await
    {
        Ok(s) => s,
        Err(SessionError::Cancelled) => return TestOutcome::Cancelled,
        Err(SessionError::Failed(error)) => return TestOutcome::Failed { error: *error },
    };
    let timeout = SshTarget::from_server(server).connect_timeout;
    let outcome = tokio::select! {
        _ = cancel.cancelled() => TestOutcome::Cancelled,
        r = session.probe(timeout) => match r {
            Ok(system) => TestOutcome::Succeeded { system },
            Err(error) => TestOutcome::Failed { error },
        },
    };
    session.close().await;
    outcome
}

/// Asks through the test page's channel.
struct TestOperator<'a> {
    app: &'a AppHandle,
    on_event: &'a Channel<TestEvent>,
    cancel: &'a CancellationToken,
}

impl Operator for TestOperator<'_> {
    fn ask(&self, prompt: OperatorPrompt) -> AskFuture<'_> {
        Box::pin(async move {
            let broker = &self.app.state::<AppState>().broker;
            broker
                .ask(prompt, self.cancel, |request_id, prompt| {
                    self.on_event.send(TestEvent { request_id, prompt }).is_ok()
                })
                .await
        })
    }
}

pub enum SessionError {
    /// The operator cancelled or declined a prompt, or `cancel` fired.
    Cancelled,
    Failed(Box<ConnectError>),
}

/// Connect and log in to `server`: a first-seen host key is shown in D02 and, once trusted,
/// recorded before connecting again; secrets come from [`PromptingSecrets`]. `context` says
/// what the connection is for in D04.
pub async fn open_session(
    app: &AppHandle,
    server: &Server,
    context: Message,
    operator: &dyn Operator,
    cancel: &CancellationToken,
) -> Result<SshSession, SessionError> {
    let target = SshTarget::from_server(server);
    let state = app.state::<AppState>();
    let who = SecretContext {
        server_id: server.id,
        server_name: server.name.clone(),
        address: target.endpoint.to_string(),
        user: server.user.clone(),
        context,
    };
    let secrets = PromptingSecrets::new(app, &who, operator);
    // At most two rounds: the second follows a first trust the operator just confirmed.
    for _ in 0..2 {
        let known = crate::trust::known_keys(&state, &target.endpoint).map_err(storage_error)?;
        match connect(&target, &known, &secrets, cancel).await {
            Ok(s) => return Ok(s),
            Err(ConnectError {
                kind: ConnectErrorKind::Cancelled,
                ..
            }) => return Err(SessionError::Cancelled),
            Err(ConnectError {
                kind:
                    ConnectErrorKind::HostKeyUnknown {
                        endpoint,
                        presented,
                    },
                ..
            }) => {
                let prompt = OperatorPrompt::HostTrust(HostTrustPrompt {
                    server_id: server.id,
                    server_name: server.name.clone(),
                    user: server.user.clone(),
                    endpoint: endpoint.clone(),
                    presented: presented.clone(),
                });
                match operator.ask(prompt).await {
                    Some(OperatorAnswer::Trust) => {
                        crate::trust::trust_new(&state, &endpoint, &presented)
                            .map_err(storage_error)?;
                    }
                    _ => return Err(SessionError::Cancelled),
                }
            }
            Err(error) => return Err(SessionError::Failed(Box::new(error))),
        }
    }
    unreachable!("a trusted key is accepted on the second round")
}

fn storage_error(e: AppError) -> SessionError {
    SessionError::Failed(Box::new(
        ConnectError::at(mdm_core::ConnectStage::HostKey, ConnectErrorKind::Protocol)
            .with_note(e.message),
    ))
}

/// D02 answer of a connection test: `trust` after the operator checked the fingerprint,
/// otherwise cancel the test.
#[tauri::command]
pub fn answer_host_trust(app: AppHandle, request_id: u64, trust: bool) -> Result<(), AppError> {
    let state = app.state::<AppState>();
    state
        .broker
        .answer(request_id, trust.then_some(OperatorAnswer::Trust))
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
