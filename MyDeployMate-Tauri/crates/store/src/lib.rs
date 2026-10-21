//! MyDeployMate persistence (architecture ARC-08, ARC-31, ARC-34; SRS DATA-04, NFR-02/03).
//!
//! Owns the data directory `%APPDATA%\MyDeployMate`: the process lock, `workspace.json` with
//! atomic replace and backups, and schema migrations. Run history and logs arrive in phase 4.

mod error;
mod lock;
pub mod migrate;
mod paths;
mod workspace;

pub use error::StoreError;
pub use lock::InstanceLock;
pub use paths::DataDir;
pub use workspace::{LoadOutcome, WorkspaceStore};
