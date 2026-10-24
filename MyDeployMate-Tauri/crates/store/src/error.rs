use std::path::PathBuf;

use mdm_core::{Message, ValidationReport};

/// Display is for logs; the operator sees [`StoreError::message`] (ARC-38).
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("cannot access {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("data directory {0} is used by another MyDeployMate process")]
    Locked(PathBuf),
    #[error("configuration was saved as revision {found}, the draft is based on {expected}")]
    RevisionConflict { expected: u64, found: u64 },
    #[error("workspace.json was changed outside the app")]
    ExternalChange,
    #[error("{path} is not a valid configuration file: {reason:?}")]
    Corrupt { path: PathBuf, reason: Message },
    #[error("configuration version {found} is newer than supported version {supported}")]
    UnsupportedVersion { found: u32, supported: u32 },
    #[error("configuration migration from version {from} failed: {reason:?}")]
    Migration { from: u32, reason: Message },
    #[error("run history database error: {0}")]
    Database(String),
    #[error("run history version {found} is newer than supported version {supported}")]
    HistoryVersion { found: u32, supported: u32 },
    #[error("another run has not finished")]
    RunActive,
    #[error("the run has not finished")]
    RunUnfinished,
    #[error("a server key is already recorded for {0}")]
    TrustConflict(String),
    #[error("Windows Credential Manager error: {0}")]
    Vault(String),
    #[error("configuration has {} validation issues", .0.issues.len())]
    Invalid(ValidationReport),
}

impl StoreError {
    pub fn message(&self) -> Message {
        match self {
            StoreError::Io { path, source } => Message::Io {
                path: path.display().to_string(),
                detail: source.to_string(),
            },
            StoreError::Locked(path) => Message::Locked {
                path: path.display().to_string(),
            },
            StoreError::RevisionConflict { expected, found } => Message::RevisionConflict {
                found: *found as i64,
                expected: *expected as i64,
            },
            StoreError::ExternalChange => Message::ExternalChange,
            StoreError::Corrupt { path, reason } => Message::Corrupt {
                path: path.display().to_string(),
                reason: Box::new(reason.clone()),
            },
            StoreError::UnsupportedVersion { found, supported } => Message::UnsupportedVersion {
                found: *found,
                supported: *supported,
            },
            StoreError::Migration { from, reason } => Message::Migration {
                from: *from,
                reason: Box::new(reason.clone()),
            },
            StoreError::Database(detail) => Message::Database {
                detail: detail.clone(),
            },
            StoreError::HistoryVersion { found, supported } => Message::HistoryVersion {
                found: *found,
                supported: *supported,
            },
            StoreError::RunActive => Message::RunActive,
            StoreError::RunUnfinished => Message::RunUnfinished,
            StoreError::TrustConflict(endpoint) => Message::TrustConflict {
                endpoint: endpoint.clone(),
            },
            StoreError::Vault(detail) => Message::Vault {
                detail: detail.clone(),
            },
            StoreError::Invalid(report) => Message::Invalid {
                n: report.issues.len() as u32,
            },
        }
    }
}

pub(crate) fn io(path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> StoreError {
    let path = path.into();
    move |source| StoreError::Io { path, source }
}
