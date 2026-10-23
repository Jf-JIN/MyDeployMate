//! MyDeployMate executors and run coordinator (architecture ARC-05–07, ARC-14–23).
//!
//! Phase 4 adds the local command executor; the coordinator follows, then SSH (phase 5) and
//! SFTP / sudo (phase 6). Depends only on `mdm-core`; storage is reached through core ports.

pub mod decode;
pub mod local;
pub mod shell;

pub use decode::OutputDecoder;
pub use local::{LocalCommand, LocalEnd, LocalError, LocalOutcome, OutputChunk, Stream, run_local};
pub use shell::{Invocation, find_program, program_name, resolve_shell};
