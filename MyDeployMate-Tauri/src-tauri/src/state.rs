use std::sync::Mutex;

use mdm_store::{DataDir, InstanceLock, StoreError, WorkspaceStore};

use crate::error::{AppError, AppErrorKind};

/// Process-wide state managed by Tauri.
pub struct AppState {
    pub data_dir: DataDir,
    pub store: Mutex<WorkspaceStore>,
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
        Self {
            store: Mutex::new(WorkspaceStore::new(data_dir.clone())),
            data_dir,
            _lock: lock,
            startup_error,
            startup_error_kind: kind,
        }
    }
}
