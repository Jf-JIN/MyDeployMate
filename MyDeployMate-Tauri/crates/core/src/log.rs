//! Run log entries (SRS OBS-01, NFR-02; architecture ARC-07, ARC-33).
//!
//! Output is kept as the chunks it arrived in, already redacted; the UI joins chunks into lines.
//! stdout and stderr keep their own order, but their interleaving is only the order observed.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Body limit per run (NFR-02). Notes and the truncation marker are always kept.
pub const LOG_BODY_LIMIT: u64 = 50 * 1024 * 1024;

/// Past the body limit, the newest output kept per task and saved when the task ends (NFR-02).
pub const LOG_TAIL_LIMIT: usize = 1024 * 1024;

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
    /// The body limit was reached here; later stdout and stderr were not kept in full.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
    /// Past the body limit: shown while the run is open but not saved (only its tail is).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub live: bool,
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

/// One task's output past the body limit. Only the newest `limit` bytes stay, so the end of
/// the output (usually where an error is) can still be saved.
#[derive(Clone, Debug)]
pub struct LogTail {
    chunks: VecDeque<(LogSource, String)>,
    kept: usize,
    total: u64,
    limit: usize,
    /// Streams that lost output at the front.
    cut: Vec<LogSource>,
}

/// What a [`LogTail`] saves when its task ends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedTail {
    pub chunks: Vec<(LogSource, String)>,
    /// Bytes seen past the limit.
    pub total: u64,
    /// Bytes in `chunks`.
    pub kept: u64,
}

impl Default for LogTail {
    fn default() -> Self {
        Self::new(LOG_TAIL_LIMIT)
    }
}

impl LogTail {
    pub fn new(limit: usize) -> Self {
        LogTail {
            chunks: VecDeque::new(),
            kept: 0,
            total: 0,
            limit,
            cut: Vec::new(),
        }
    }

    pub fn push(&mut self, source: LogSource, text: &str) {
        if text.is_empty() {
            return;
        }
        self.total += text.len() as u64;
        self.kept += text.len();
        self.chunks.push_back((source, text.to_owned()));
        while self.kept > self.limit {
            let excess = self.kept - self.limit;
            let (source, front) = self.chunks.front_mut().expect("kept > 0");
            if !self.cut.contains(source) {
                self.cut.push(*source);
            }
            if front.len() <= excess {
                self.kept -= front.len();
                self.chunks.pop_front();
            } else {
                let mut cut = excess;
                while !front.is_char_boundary(cut) {
                    cut += 1;
                }
                front.drain(..cut);
                self.kept -= cut;
                break;
            }
        }
    }

    /// The kept output. A stream that lost output before it starts at its next line.
    pub fn finish(mut self) -> SavedTail {
        let mut chunks = Vec::new();
        for (source, mut text) in self.chunks {
            if let Some(i) = self.cut.iter().position(|s| *s == source) {
                match text.find('\n') {
                    Some(at) => {
                        self.cut.remove(i);
                        text.drain(..=at);
                    }
                    // Still inside the first, partial line.
                    None => continue,
                }
            }
            if !text.is_empty() {
                chunks.push((source, text));
            }
        }
        let kept = chunks.iter().map(|(_, t)| t.len() as u64).sum();
        SavedTail {
            chunks,
            total: self.total,
            kept,
        }
    }
}

/// Byte count for people: 512 B, 3.4 KiB, 12.0 MiB.
pub fn size_text(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    let b = bytes as f64;
    if b < KIB {
        format!("{bytes} B")
    } else if b < KIB * KIB {
        format!("{:.1} KiB", b / KIB)
    } else if b < KIB * KIB * KIB {
        format!("{:.1} MiB", b / KIB / KIB)
    } else {
        format!("{:.1} GiB", b / KIB / KIB / KIB)
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
            live: false,
        };
        let json = serde_json::to_string(&e).unwrap();
        assert_eq!(
            json,
            r#"{"n":0,"task":1,"source":"stdout","text":"ok\n","atMs":5}"#
        );
        assert_eq!(serde_json::from_str::<LogEntry>(&json).unwrap(), e);
    }

    #[test]
    fn tail_keeps_the_newest_bytes_from_the_next_line() {
        let mut t = LogTail::new(10);
        t.push(LogSource::Stdout, "aaaa\nbbbb\n");
        t.push(LogSource::Stderr, "E\n");
        t.push(LogSource::Stdout, "cc\ndd");
        let saved = t.finish();
        assert_eq!(saved.total, 17);
        // stdout kept "bbb\n", but that line had lost its start; stderr lost nothing.
        assert_eq!(
            saved.chunks,
            [
                (LogSource::Stderr, "E\n".to_string()),
                (LogSource::Stdout, "cc\ndd".to_string()),
            ]
        );
        assert_eq!(saved.kept, 7);
    }

    #[test]
    fn tail_cuts_a_long_chunk_at_a_char_boundary() {
        let mut t = LogTail::new(4);
        t.push(LogSource::Stdout, "中文\n");
        let saved = t.finish();
        // 7 bytes, the last 4 start inside 中: cut to 文\n, then to the next line (none left).
        assert_eq!(saved.chunks, []);
        assert_eq!(saved.total, 7);

        let mut whole = LogTail::new(100);
        whole.push(LogSource::Stdout, "partial");
        assert_eq!(
            whole.finish().chunks,
            [(LogSource::Stdout, "partial".into())]
        );
    }

    #[test]
    fn sizes_read_well() {
        assert_eq!(size_text(512), "512 B");
        assert_eq!(size_text(3 * 1024 + 400), "3.4 KiB");
        assert_eq!(size_text(LOG_BODY_LIMIT), "50.0 MiB");
    }
}
