//! MyDeployMate persistence (architecture ARC-08, ARC-31, ARC-34; SRS DATA-04, NFR-02/03).
//!
//! Owns the data directory `%APPDATA%\MyDeployMate`: the process lock, `workspace.json` with
//! atomic replace and backups, schema migrations, run history in SQLite and run log files,
//! the host trust store, and secrets in Windows Credential Manager.

mod atomic;
mod error;
mod history;
mod lock;
mod logs;
pub mod migrate;
mod paths;
mod secrets;
mod trust;
mod workspace;

pub use error::StoreError;
pub use history::{HistoryStore, SharedHistory, StoredRun};
pub use lock::InstanceLock;
pub use logs::{RunLog, RunLogFile, read_run_log, remove_run_log};
pub use paths::DataDir;
pub use secrets::{
    MemoryVault, SavedCredential, SecretStore, SecretVault, credential_account, platform_vault,
    variable_account,
};
pub use trust::TrustStore;
pub use workspace::{LoadOutcome, WorkspaceStore};
