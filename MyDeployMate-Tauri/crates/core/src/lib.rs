//! MyDeployMate domain core: configuration model, IDs, validation, variable resolution
//! and plan building with preflight.
//!
//! Pure logic only: no file system, process, network or async runtime (architecture ARC-04).

pub mod defaults;
#[cfg(any(test, feature = "fixtures"))]
pub mod fixtures;
pub mod ids;
pub mod model;
pub mod plan;
pub mod redact;
pub mod resolve;
pub mod run;
pub mod template;
pub mod validate;

#[cfg(test)]
mod plan_tests;
#[cfg(test)]
mod tests;

pub use ids::*;
pub use model::*;
pub use plan::{PlanRequest, ResolvedPlan, RunScope, build_plan};
pub use resolve::{ResolvedText, Resolver, ValueSource};
pub use validate::{ValidationReport, check_workspace_json, parse_workspace, validate_workspace};
