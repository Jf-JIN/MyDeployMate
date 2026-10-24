use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, Once};

use mdm_core::{Clock, Message};
use mdm_exec::SystemClock;
use mdm_store::{
    DataDir, HistoryStore, InstanceLock, SecretStore, SharedHistory, StoreError, TrustStore,
    WorkspaceStore, platform_vault,
};

use crate::connection::ConnectionTests;
use crate::credentials::PromptBroker;
use crate::run::ActiveRun;

use crate::error::{AppError, AppErrorKind};

/// Process-wide state managed by Tauri.
pub struct AppState {
    pub data_dir: DataDir,
    pub store: Mutex<WorkspaceStore>,
    /// Run records; `None` when the database could not be opened (`history_error` says why).
    pub history: Option<SharedHistory>,
    pub history_error: Option<Message>,
    /// The single run in progress (ARC-08).
    pub active: Mutex<Option<ActiveRun>>,
    /// Host trust store; why it could not be opened (ARC-20).
    pub trust: Mutex<Result<TrustStore, Message>>,
    /// Remembered credentials and sensitive variable values (SEC-03, ARC-10, ARC-26).
    pub secrets: Mutex<Result<SecretStore, Message>>,
    /// Open D02 and D04 prompts of connection tests.
    pub broker: PromptBroker,
    /// Connection test results and running tests (IXD-18).
    pub tests: ConnectionTests,
    /// Set by "cancel and quit": the app exits once the active run has stopped (IXD-29).
    pub quit_when_idle: AtomicBool,
    /// A validated configuration import waiting for confirmation (IXD-37).
    pub import: crate::exchange::ImportSlot,
    /// The retention cleanup at the first workspace load of this launch (NFR-02).
    pub startup_purge: Once,
    /// Held for the life of the process (ARC-08).
    _lock: Option<InstanceLock>,
    /// When the data directory could not be prepared, every workspace command reports this.
    pub startup_error: Option<Message>,
    pub startup_error_kind: AppErrorKind,
}

impl AppState {
    pub fn init() -> Self {
        let data_dir = DataDir::default_location().unwrap_or_else(|| DataDir::at("MyDeployMate"));
        let lock = data_dir
            .ensure()
            .and_then(|()| InstanceLock::acquire(&data_dir));
        let (lock, startup_error, kind) = match lock {
            Ok(lock) => (Some(lock), None, AppErrorKind::Internal),
            Err(err @ StoreError::Locked(_)) => (None, Some(err.message()), AppErrorKind::Locked),
            Err(err) => {
                let kind = AppError::from(err);
                (None, Some(kind.message), kind.kind)
            }
        };
        let (history, history_error) = match &lock {
            Some(_) => match open_history(&data_dir) {
                Ok(h) => (Some(SharedHistory::new(h)), None),
                Err(e) => (None, Some(e.message())),
            },
            None => (None, startup_error.clone()),
        };
        let trust = match &lock {
            Some(_) => TrustStore::open(&data_dir).map_err(|e| e.message()),
            None => Err(startup_error
                .clone()
                .unwrap_or(Message::WorkspaceStateBroken)),
        };
        let secrets = match &lock {
            Some(_) => SecretStore::open(&data_dir, platform_vault()).map_err(|e| e.message()),
            None => Err(startup_error
                .clone()
                .unwrap_or(Message::WorkspaceStateBroken)),
        };
        Self {
            trust: Mutex::new(trust),
            secrets: Mutex::new(secrets),
            broker: PromptBroker::default(),
            tests: ConnectionTests::default(),
            store: Mutex::new(WorkspaceStore::new(data_dir.clone())),
            history,
            history_error,
            active: Mutex::new(None),
            quit_when_idle: AtomicBool::new(false),
            import: Default::default(),
            startup_purge: Once::new(),
            data_dir,
            _lock: lock,
            startup_error,
            startup_error_kind: kind,
        }
    }
}

/// Open the run database and mark runs left unfinished by a crash or forced exit as
/// interrupted (RUN-07, ARC-34). Only after the data directory lock is held.
fn open_history(data_dir: &DataDir) -> Result<HistoryStore, StoreError> {
    let mut history = HistoryStore::open(&data_dir.history_db())?;
    history.interrupt_unfinished(SystemClock.now_ms())?;
    Ok(history)
}
