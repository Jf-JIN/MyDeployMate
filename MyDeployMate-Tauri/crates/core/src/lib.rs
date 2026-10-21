//! MyDeployMate domain core: configuration model, IDs, validation and variable
//! resolution.
//!
//! Pure logic only: no file system, process, network or async runtime (architecture ARC-04).

pub mod defaults;
#[cfg(any(test, feature = "fixtures"))]
pub mod fixtures;
pub mod ids;
pub mod model;
pub mod redact;
pub mod resolve;
pub mod run;
pub mod template;
pub mod validate;

#[cfg(test)]
mod tests;

pub use ids::*;
pub use model::*;
pub use resolve::{ResolvedText, Resolver, ValueSource};
pub use validate::{ValidationReport, check_workspace_json, parse_workspace, validate_workspace};
