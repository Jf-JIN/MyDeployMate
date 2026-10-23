//! Run log files `logs/<runId>.jsonl`, one JSON entry per line (ARC-31, ARC-32).
//!
//! The file and the database are not one transaction. A crash can leave a half-written last
//! line; reading skips it and reports that the tail is incomplete.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;

use mdm_core::{LogEntry, LogSink, Message, RunId, StorageError};

use crate::error::{StoreError, io};
use crate::paths::DataDir;

pub struct RunLogFile {
    path: PathBuf,
    out: BufWriter<File>,
}

impl RunLogFile {
    /// Create (or continue) the log of `run`.
    pub fn open(dir: &DataDir, run: RunId) -> Result<Self, StoreError> {
        let logs = dir.logs();
        std::fs::create_dir_all(&logs).map_err(io(&logs))?;
        let path = dir.run_log(run);
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(io(&path))?;
        Ok(RunLogFile {
            path,
            out: BufWriter::new(file),
        })
    }
}

impl LogSink for RunLogFile {
    fn append(&mut self, entries: &[LogEntry]) -> Result<(), StorageError> {
        let fail = |e: std::io::Error| {
            StorageError(Message::Io {
                path: self.path.display().to_string(),
                detail: e.to_string(),
            })
        };
        for e in entries {
            serde_json::to_writer(&mut self.out, e).map_err(|e| fail(e.into()))?;
            self.out.write_all(b"\n").map_err(fail)?;
        }
        self.out.flush().map_err(fail)
    }
}

/// A run's log as read back.
#[derive(Debug, Default, PartialEq)]
pub struct RunLog {
    pub entries: Vec<LogEntry>,
    /// Lines that could not be read, normally a last line cut off by a crash.
    pub damaged_lines: usize,
}

/// Read the log of `run`; a missing file is an empty log.
pub fn read_run_log(dir: &DataDir, run: RunId) -> Result<RunLog, StoreError> {
    let path = dir.run_log(run);
    let file = match File::open(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(RunLog::default()),
        Err(e) => return Err(io(&path)(e)),
    };
    let mut log = RunLog::default();
    for line in BufReader::new(file).split(b'\n') {
        let line = line.map_err(io(&path))?;
        if line.is_empty() {
            continue;
        }
        match serde_json::from_slice(&line) {
            Ok(entry) => log.entries.push(entry),
            Err(_) => log.damaged_lines += 1,
        }
    }
    Ok(log)
}

#[cfg(test)]
mod tests {
    use mdm_core::LogSource;
    use pretty_assertions::assert_eq;

    use super::*;

    fn entry(n: u64, text: &str) -> LogEntry {
        LogEntry {
            n,
            task: 0,
            source: LogSource::Stdout,
            text: text.into(),
            message: None,
            at_ms: 1,
            truncated: false,
            live: false,
        }
    }

    #[test]
    fn appends_and_reads_back_with_a_damaged_tail() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = DataDir::at(tmp.path());
        let run = RunId::new();
        assert_eq!(read_run_log(&dir, run).unwrap(), RunLog::default());

        let mut f = RunLogFile::open(&dir, run).unwrap();
        f.append(&[entry(0, "第一行\n"), entry(1, "partial")])
            .unwrap();
        f.append(&[entry(2, "\r\n")]).unwrap();
        drop(f);
        // A crash in the middle of a write.
        let mut raw = OpenOptions::new()
            .append(true)
            .open(dir.run_log(run))
            .unwrap();
        raw.write_all(br#"{"n":3,"task":0,"sou"#).unwrap();

        let log = read_run_log(&dir, run).unwrap();
        assert_eq!(
            log.entries,
            vec![entry(0, "第一行\n"), entry(1, "partial"), entry(2, "\r\n")]
        );
        assert_eq!(log.damaged_lines, 1);
    }
}
