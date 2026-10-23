//! Run log entries (SRS OBS-01, NFR-02; architecture ARC-07, ARC-33).
//!
//! Output is kept as the chunks it arrived in, already redacted; the UI joins chunks into lines.
//! stdout and stderr keep their own order, but their interleaving is only the order observed.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Body limit per run (NFR-02). Notes and the truncation marker are always kept.
pub const LOG_BODY_LIMIT: u64 = 50 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LogSource {
    Stdout,
    Stderr,
    /// Written by the app, not the command (e.g. "processes left behind were terminated").
    System,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LogEntry {
    /// Order of arrival in the run, from 0.
    #[ts(type = "number")]
    pub n: u64,
    /// Task position in the plan.
    pub task: u32,
    pub source: LogSource,
    pub text: String,
    #[ts(type = "number")]
    pub at_ms: i64,
    /// The body limit was reached here; later stdout and stderr were not kept.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

/// Counts body bytes against [`LOG_BODY_LIMIT`].
#[derive(Clone, Debug)]
pub struct LogBudget {
    left: u64,
    exhausted: bool,
}

/// What to do with one entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admit {
    Keep,
    /// Keep, then add the truncation marker: this entry used up the budget.
    KeepAndMark,
    Drop,
}

impl Default for LogBudget {
    fn default() -> Self {
        Self::new(LOG_BODY_LIMIT)
    }
}

impl LogBudget {
    pub fn new(limit: u64) -> Self {
        LogBudget {
            left: limit,
            exhausted: false,
        }
    }

    pub fn exhausted(&self) -> bool {
        self.exhausted
    }

    /// Whether to keep an entry of `source` with `len` bytes of text. System notes always stay.
    /// The entry that crosses the limit is still kept whole, so no line is cut in the middle
    /// of a character.
    pub fn admit(&mut self, source: LogSource, len: usize) -> Admit {
        if source == LogSource::System {
            return Admit::Keep;
        }
        if self.exhausted {
            return Admit::Drop;
        }
        let len = len as u64;
        if len < self.left {
            self.left -= len;
            Admit::Keep
        } else {
            self.left = 0;
            self.exhausted = true;
            Admit::KeepAndMark
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_keeps_system_notes_and_marks_once() {
        let mut b = LogBudget::new(10);
        assert_eq!(b.admit(LogSource::Stdout, 6), Admit::Keep);
        assert_eq!(b.admit(LogSource::Stderr, 6), Admit::KeepAndMark);
        assert!(b.exhausted());
        assert_eq!(b.admit(LogSource::Stdout, 1), Admit::Drop);
        assert_eq!(b.admit(LogSource::System, 100), Admit::Keep);
    }

    #[test]
    fn entry_omits_false_truncated_flag() {
        let e = LogEntry {
            n: 0,
            task: 1,
            source: LogSource::Stdout,
            text: "ok\n".into(),
            at_ms: 5,
            truncated: false,
        };
        let json = serde_json::to_string(&e).unwrap();
        assert_eq!(
            json,
            r#"{"n":0,"task":1,"source":"stdout","text":"ok\n","atMs":5}"#
        );
        assert_eq!(serde_json::from_str::<LogEntry>(&json).unwrap(), e);
    }
}
