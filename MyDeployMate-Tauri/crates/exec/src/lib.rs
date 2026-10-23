//! MyDeployMate executors and run coordinator (architecture ARC-05–07, ARC-14–23).
//!
//! Phase 4: the run coordinator and the local command executor; SSH (phase 5) and SFTP / sudo
//! (phase 6) follow. Depends only on `mdm-core`; storage is reached through core ports.

pub mod coordinator;
#[cfg(test)]
mod coordinator_tests;
pub mod decode;
pub mod executor;
pub mod local;
pub mod logs;
pub mod shell;

pub use coordinator::{
    Control, Coordinator, Credential, CredentialRequest, RunObserver, SystemClock, TaskContext,
    TaskExecutor, TaskFuture, TaskOutcome,
};
pub use decode::OutputDecoder;
pub use executor::AppExecutor;
pub use local::{LocalCommand, LocalEnd, LocalError, LocalOutcome, OutputChunk, Stream, run_local};
pub use logs::{LogPipeline, RunView};
pub use shell::{Invocation, find_program, program_name, resolve_shell};
