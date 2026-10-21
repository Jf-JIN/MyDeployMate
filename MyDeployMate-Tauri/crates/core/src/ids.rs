//! Immutable object identifiers (DATA-02). All IDs are UUID v7 so they sort by creation time.

use std::fmt;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

macro_rules! id_type {
    ($($(#[$doc:meta])* $name:ident),+ $(,)?) => {$(
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
        #[ts(export)]
        pub struct $name(pub Uuid);

        impl $name {
            /// A fresh time-ordered ID.
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    )+};
}

id_type! {
    /// Environment, e.g. staging or production.
    EnvironmentId,
    /// Target server.
    ServerId,
    /// Project.
    ProjectId,
    /// Pipeline (流程) inside a project.
    PipelineId,
    /// Step inside a pipeline.
    StepId,
    /// User variable (global or project).
    VariableId,
    /// One run of a pipeline.
    RunId,
    /// One task inside a run.
    TaskId,
}
