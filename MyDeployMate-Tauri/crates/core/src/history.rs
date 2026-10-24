//! Run history queries for P08 (SRS OBS-02, IXD-34) and the retention rule (NFR-02).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{EnvironmentId, PipelineId, ProjectId};
use crate::run::{Run, RunStatus};

/// Runs older than this many days are removed unless the operator sets another value (NFR-02).
pub const DEFAULT_RETENTION_DAYS: u32 = 90;
pub const MIN_RETENTION_DAYS: u32 = 1;
pub const MAX_RETENTION_DAYS: u32 = 3650;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// Runs that started before this time are past the retention period.
pub fn retention_cutoff_ms(now_ms: i64, days: u32) -> i64 {
    now_ms - i64::from(days) * DAY_MS
}

/// One row of the history list.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RunSummary {
    pub run: Run,
    /// Distinct servers among the run's tasks; 0 for a local-only run.
    pub target_count: u32,
}

/// The result filter: active runs are grouped, since queued, waiting and cancelling all mean
/// "not finished yet".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum StatusFilter {
    Active,
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
}

impl StatusFilter {
    pub fn matches(self, status: RunStatus) -> bool {
        match self {
            StatusFilter::Active => !status.is_terminal(),
            StatusFilter::Succeeded => status == RunStatus::Succeeded,
            StatusFilter::Failed => status == RunStatus::Failed,
            StatusFilter::Cancelled => status == RunStatus::Cancelled,
            StatusFilter::Interrupted => status == RunStatus::Interrupted,
        }
    }
}

/// History filters (IXD-34); a `None` field matches every run. Times are Unix milliseconds of
/// the run start, `since_ms` inclusive and `until_ms` exclusive.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RunFilter {
    pub project_id: Option<ProjectId>,
    pub pipeline_id: Option<PipelineId>,
    pub environment_id: Option<EnvironmentId>,
    pub status: Option<StatusFilter>,
    #[ts(type = "number | null")]
    pub since_ms: Option<i64>,
    #[ts(type = "number | null")]
    pub until_ms: Option<i64>,
}

/// A project, pipeline and environment that history refers to, with the names of its latest
/// run. Filters offer these, so deleted objects stay selectable by their snapshot names.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RunChoice {
    pub project_id: ProjectId,
    pub project_name: String,
    pub pipeline_id: PipelineId,
    pub pipeline_name: String,
    pub environment_id: EnvironmentId,
    pub environment_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cutoff_counts_whole_days_back() {
        assert_eq!(retention_cutoff_ms(10 * DAY_MS, 3), 7 * DAY_MS);
    }

    #[test]
    fn active_filter_covers_every_unfinished_status() {
        use RunStatus::*;
        for s in [
            Queued,
            Running,
            WaitingConfirmation,
            WaitingCredential,
            Cancelling,
        ] {
            assert!(StatusFilter::Active.matches(s));
        }
        for s in [Succeeded, Failed, Cancelled, Interrupted] {
            assert!(!StatusFilter::Active.matches(s));
        }
        assert!(StatusFilter::Failed.matches(Failed));
        assert!(!StatusFilter::Failed.matches(Interrupted));
    }
}
