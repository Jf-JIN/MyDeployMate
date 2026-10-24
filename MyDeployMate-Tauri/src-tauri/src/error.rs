//! Error type returned by every Tauri command (architecture ARC-36).

use serde::Serialize;
use ts_rs::TS;

use mdm_core::{Message, ValidationReport};
use mdm_store::StoreError;

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppError {
    pub kind: AppErrorKind,
    /// What went wrong, as a code the UI renders (ARC-38); free of secrets.
    pub message: Message,
    /// Field-level problems when `kind` is `validation`.
    pub validation: Option<ValidationReport>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AppErrorKind {
    Validation,
    RevisionConflict,
    ExternalChange,
    UnsupportedVersion,
    /// Another process holds the data directory.
    Locked,
    StorageUnavailable,
    /// Another run has not finished yet (ARC-08).
    RunActive,
    /// A server key differs from the recorded one; only an explicit trust update may replace
    /// it (SRV-03).
    HostKeyChanged,
    /// Reaching a server failed; the message names the stage and the reason.
    Connection,
    /// The requested record does not exist, e.g. a run that was deleted.
    NotFound,
    Internal,
}

impl AppError {
    pub fn new(kind: AppErrorKind, message: Message) -> Self {
        Self {
            kind,
            message,
            validation: None,
        }
    }

    pub fn validation(report: ValidationReport) -> Self {
        Self {
            kind: AppErrorKind::Validation,
            message: Message::Invalid {
                n: report.issues.len() as u32,
            },
            validation: Some(report),
        }
    }
}

impl From<StoreError> for AppError {
    fn from(err: StoreError) -> Self {
        let message = err.message();
        let kind = match err {
            StoreError::Invalid(report) => return Self::validation(report),
            StoreError::RevisionConflict { .. } => AppErrorKind::RevisionConflict,
            StoreError::ExternalChange => AppErrorKind::ExternalChange,
            StoreError::UnsupportedVersion { .. }
            | StoreError::Migration { .. }
            | StoreError::HistoryVersion { .. } => AppErrorKind::UnsupportedVersion,
            StoreError::Locked(_) => AppErrorKind::Locked,
            StoreError::Io { .. }
            | StoreError::Corrupt { .. }
            | StoreError::Database(_)
            | StoreError::Vault(_) => AppErrorKind::StorageUnavailable,
            StoreError::RunActive | StoreError::RunUnfinished => AppErrorKind::RunActive,
            StoreError::TrustConflict(_) => AppErrorKind::HostKeyChanged,
        };
        Self::new(kind, message)
    }
}
