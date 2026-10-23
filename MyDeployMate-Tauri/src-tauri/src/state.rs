use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use mdm_core::Clock;
use mdm_exec::SystemClock;
use mdm_store::{DataDir, HistoryStore, InstanceLock, SharedHistory, StoreError, WorkspaceStore};

use crate::run::ActiveRun;

use crate::error::{AppError, AppErrorKind};

/// Process-wide state managed by Tauri.
pub struct AppState {
    pub data_dir: DataDir,
    pub store: Mutex<WorkspaceStore>,
    /// Run records; `None` when the database could not be opened (`history_error` says why).
    pub history: Option<SharedHistory>,
    pub history_error: Option<String>,
    /// The single run in progress (ARC-08).
    pub active: Mutex<Option<ActiveRun>>,
    /// Set by "cancel and quit": the app exits once the active run has stopped (IXD-29).
    pub quit_when_idle: AtomicBool,
    /// Held for the life of the process (ARC-08).
    _lock: Option<InstanceLock>,
    /// When the data directory could not be prepared, every workspace command reports this.
    pub startup_error: Option<String>,
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
            Err(err @ StoreError::Locked(_)) => (None, Some(err.to_string()), AppErrorKind::Locked),
            Err(err) => {
                let kind = AppError::from(err);
                (None, Some(kind.message), kind.kind)
            }
        };
        let (history, history_error) = match &lock {
            Some(_) => match open_history(&data_dir) {
                Ok(h) => (Some(SharedHistory::new(h)), None),
                Err(e) => (None, Some(e.to_string())),
            },
            None => (None, startup_error.clone()),
        };
        Self {
            store: Mutex::new(WorkspaceStore::new(data_dir.clone())),
            history,
            history_error,
            active: Mutex::new(None),
            quit_when_idle: AtomicBool::new(false),
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
