//! MyDeployMate domain core: configuration model, IDs, validation, variable resolution,
//! plan building with preflight, and the run state machine.
//!
//! Pure logic only: no file system, process, network or async runtime (architecture ARC-04).

pub mod defaults;
pub mod edit;
#[cfg(any(test, feature = "fixtures"))]
pub mod fixtures;
pub mod ids;
pub mod keyfile;
pub mod log;
pub mod machine;
pub mod model;
pub mod plan;
pub mod ports;
pub mod redact;
pub mod resolve;
pub mod run;
pub mod template;
pub mod validate;
pub mod wsl;

#[cfg(test)]
mod machine_tests;
#[cfg(test)]
mod plan_tests;
#[cfg(test)]
mod tests;

pub use edit::{Reference, ServerEdit};
pub use ids::*;
pub use keyfile::{KeyFormat, detect_key_format};
pub use log::{LogEntry, LogSource};
pub use machine::{Next, RunEvent, RunMachine, TaskMeta, TransitionError};
pub use model::*;
pub use plan::{PlanRequest, PlannedTask, ResolvedPlan, RunScope, TaskAction, build_plan};
pub use ports::{Clock, LogSink, RunStore, StorageError};
pub use resolve::{ResolvedText, Resolver, ValueSource};
pub use run::{Run, RunStatus, Task, TaskStatus};
pub use validate::{ValidationReport, check_workspace_json, parse_workspace, validate_workspace};
