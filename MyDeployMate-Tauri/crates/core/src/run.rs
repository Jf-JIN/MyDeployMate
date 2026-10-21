//! Run and task records (SRS RUN-06, DATA-01). The state machine that moves between these
//! states arrives in phase 2; this module only fixes the vocabulary and the stored shape.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{EnvironmentId, PipelineId, ProjectId, RunId, ServerId, StepId, TaskId};
use crate::model::{ExecutionOrder, StepKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RunStatus {
    Queued,
    Running,
    WaitingConfirmation,
    WaitingCredential,
    Cancelling,
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
}

impl RunStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            RunStatus::Succeeded
                | RunStatus::Failed
                | RunStatus::Cancelled
                | RunStatus::Interrupted
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum TaskStatus {
    Queued,
    Running,
    WaitingConfirmation,
    WaitingCredential,
    Succeeded,
    Failed,
    Cancelled,
    Skipped,
    /// The remote outcome could not be confirmed (no exit status, lost reply).
    Unknown,
}

impl TaskStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            TaskStatus::Succeeded
                | TaskStatus::Failed
                | TaskStatus::Cancelled
                | TaskStatus::Skipped
                | TaskStatus::Unknown
        )
    }
}

/// Run summary as stored in history. Timestamps are Unix milliseconds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Run {
    pub id: RunId,
    pub project_id: ProjectId,
    pub pipeline_id: PipelineId,
    pub environment_id: EnvironmentId,
    /// Names at the time of the run, so renames and deletes never rewrite history (DATA-02).
    pub project_name: String,
    pub pipeline_name: String,
    pub environment_name: String,
    pub version: String,
    pub execution_order: ExecutionOrder,
    pub local_only: bool,
    pub status: RunStatus,
    #[ts(type = "number")]
    pub created_at_ms: i64,
    #[ts(type = "number | null")]
    pub finished_at_ms: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Task {
    pub id: TaskId,
    pub run_id: RunId,
    /// Position in the frozen plan, from 0.
    pub seq: u32,
    pub step_id: StepId,
    pub step_name: String,
    pub kind: StepKind,
    /// `None` for local and confirm tasks.
    pub server_id: Option<ServerId>,
    pub server_name: Option<String>,
    pub status: TaskStatus,
    pub exit_code: Option<i32>,
    pub exit_signal: Option<String>,
    /// Why the task ended this way, e.g. which upstream failure caused a skip.
    pub reason: Option<String>,
    #[ts(type = "number | null")]
    pub started_at_ms: Option<i64>,
    #[ts(type = "number | null")]
    pub finished_at_ms: Option<i64>,
}
