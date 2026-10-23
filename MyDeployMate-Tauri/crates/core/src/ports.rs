//! Ports the run coordinator uses for IO (architecture ARC-04, ARC-14, ARC-37). `store`
//! implements [`RunStore`]; `src-tauri` injects it at startup, and tests use in-memory fakes.

use thiserror::Error;

use crate::log::LogEntry;
use crate::machine::RunMachine;
use crate::message::Message;
use crate::plan::ResolvedPlan;

/// Run history could not be written. The coordinator dispatches nothing more (ARC-33).
#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("run history could not be saved: {0:?}")]
pub struct StorageError(pub Message);

impl StorageError {
    pub fn message(&self) -> Message {
        Message::RunStorageFailed {
            reason: Box::new(self.0.clone()),
        }
    }
}

/// Durable run and task records (RUN-07, ARC-32). Every call is one transaction.
pub trait RunStore: Send {
    /// Record a new run with all its tasks and the display-safe plan snapshot. Fails when
    /// another run is still unfinished (single active run, ARC-08).
    fn create_run(&mut self, machine: &RunMachine, plan: &ResolvedPlan)
    -> Result<(), StorageError>;

    /// Record the current state after an event.
    fn save_run(&mut self, machine: &RunMachine) -> Result<(), StorageError>;
}

/// Append-only log of one run (ARC-31). Entries are already redacted.
pub trait LogSink: Send {
    fn append(&mut self, entries: &[LogEntry]) -> Result<(), StorageError>;
}

/// Wall-clock time for records, in Unix milliseconds.
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> i64;
}
