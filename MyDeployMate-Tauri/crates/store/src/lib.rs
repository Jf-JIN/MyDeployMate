//! MyDeployMate persistence (architecture ARC-08, ARC-31, ARC-34; SRS DATA-04, NFR-02/03).
//!
//! Owns the data directory `%APPDATA%\MyDeployMate`: the process lock, `workspace.json` with
//! atomic replace and backups, schema migrations, and run history in SQLite and run log files.

mod error;
mod history;
mod lock;
mod logs;
pub mod migrate;
mod paths;
mod workspace;

pub use error::StoreError;
pub use history::{HistoryStore, INTERRUPTED_REASON, SharedHistory, StoredRun};
pub use lock::InstanceLock;
pub use logs::{RunLog, RunLogFile, read_run_log};
pub use paths::DataDir;
pub use workspace::{LoadOutcome, WorkspaceStore};
