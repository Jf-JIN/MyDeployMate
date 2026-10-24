//! Log pipeline (SRS OBS-01, NFR-02; architecture ARC-07, ARC-27, ARC-33; plan task 4.4).
//!
//! Sits between the coordinator and the UI. Output is redacted per task and stream (a secret
//! split across chunks is still caught), numbered, written to the run's log file and pushed to
//! the UI in batches by [`LogPipeline::flush_every`]. State changes go out at once, after the
//! output that came before them.
//!
//! Past the 50 MiB body limit, one marker entry says so and output is still read and shown, but
//! not saved: the UI keeps only its newest lines. Each task's last [`LOG_TAIL_LIMIT`] bytes are
//! saved when the task ends, so the end of the output is on record. If the log file cannot be
//! written, the run is cancelled: nothing more is dispatched without a record of its output
//! (ARC-33).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mdm_core::log::{Admit, LOG_BODY_LIMIT, LOG_TAIL_LIMIT, LogBudget, LogTail, size_text};
use mdm_core::redact::{Redactor, StreamRedactor};
use mdm_core::{
    Clock, LogEntry, LogSink, LogSource, Message, RunMachine, StorageError, UploadProgress,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::coordinator::{Control, OperatorRequest, RunObserver};
use crate::local::{OutputChunk, Stream};

/// How often batched output is pushed (ARC-07: 50–100 ms).
pub const FLUSH_PERIOD: Duration = Duration::from_millis(80);

/// The UI side of a run.
pub trait RunView: Send + Sync {
    fn state(&self, machine: &RunMachine);
    fn logs(&self, entries: Vec<LogEntry>);
    fn requested(&self, request: &OperatorRequest);
    fn storage_failed(&self, error: &StorageError);
    fn progress(&self, seq: u32, progress: UploadProgress);
}

/// Where an entry goes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Dest {
    Both,
    /// Past the body limit: shown only.
    Ui,
    /// A saved tail: the UI showed it already.
    Disk,
}

pub struct LogPipeline {
    inner: Mutex<Inner>,
    view: Arc<dyn RunView>,
    clock: Arc<dyn Clock>,
    control: mpsc::UnboundedSender<Control>,
}

struct Inner {
    redactor: Redactor,
    streams: HashMap<(u32, LogSource), StreamRedactor>,
    pending: Vec<(LogEntry, Dest)>,
    next: u64,
    budget: LogBudget,
    /// Output past the body limit, per task.
    tails: HashMap<u32, LogTail>,
    tail_limit: usize,
    /// `None` once a write failed.
    sink: Option<Box<dyn LogSink>>,
}

impl LogPipeline {
    /// `secrets` are the real values of the run's sensitive variables.
    pub fn new(
        secrets: impl IntoIterator<Item = String>,
        sink: Box<dyn LogSink>,
        budget: LogBudget,
        view: Arc<dyn RunView>,
        clock: Arc<dyn Clock>,
        control: mpsc::UnboundedSender<Control>,
    ) -> Self {
        LogPipeline {
            inner: Mutex::new(Inner {
                redactor: Redactor::new(secrets),
                streams: HashMap::new(),
                pending: Vec::new(),
                next: 0,
                budget,
                tails: HashMap::new(),
                tail_limit: LOG_TAIL_LIMIT,
                sink: Some(sink),
            }),
            view,
            clock,
            control,
        }
    }

    /// Push batched output every [`FLUSH_PERIOD`] until `stop`, then once more.
    pub async fn flush_every(self: Arc<Self>, stop: CancellationToken) {
        let mut tick = tokio::time::interval(FLUSH_PERIOD);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = stop.cancelled() => break,
                _ = tick.tick() => self.flush(),
            }
        }
        self.flush();
    }

    /// Write pending entries to the log file and hand them to the UI.
    pub fn flush(&self) {
        let mut inner = self.lock();
        self.flush_locked(&mut inner);
    }

    fn flush_locked(&self, inner: &mut Inner) {
        if inner.pending.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut inner.pending);
        if let Some(sink) = &mut inner.sink {
            let saved: Vec<LogEntry> = pending
                .iter()
                .filter(|(_, dest)| *dest != Dest::Ui)
                .map(|(e, _)| e.clone())
                .collect();
            if !saved.is_empty()
                && let Err(e) = sink.append(&saved)
            {
                inner.sink = None;
                let at_ms = self.clock.now_ms();
                let task = pending.last().map_or(0, |(e, _)| e.task);
                let mut batch = shown(pending);
                let n = inner.next;
                inner.next += 1;
                batch.push(LogEntry {
                    n,
                    task,
                    source: LogSource::System,
                    text: "\n".into(),
                    message: Some(Message::LogStorageFailed {
                        reason: Box::new(e.message()),
                    }),
                    at_ms,
                    truncated: false,
                    live: false,
                });
                self.view.storage_failed(&e);
                let _ = self.control.send(Control::Cancel {
                    reason: Message::LogStorage,
                });
                self.view.logs(batch);
                return;
            }
        }
        let batch = shown(pending);
        if !batch.is_empty() {
            self.view.logs(batch);
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn push(
        &self,
        inner: &mut Inner,
        task: u32,
        source: LogSource,
        text: String,
        message: Option<Message>,
    ) {
        if text.is_empty() {
            return;
        }
        let admit = inner.budget.admit(source, text.len());
        let tail_limit = inner.tail_limit;
        if admit == Admit::Drop {
            inner
                .tails
                .entry(task)
                .or_insert_with(|| LogTail::new(tail_limit))
                .push(source, &text);
        }
        let at_ms = self.clock.now_ms();
        let mut add = |text: String, message, source, truncated, dest| {
            let n = inner.next;
            inner.next += 1;
            inner.pending.push((
                LogEntry {
                    n,
                    task,
                    source,
                    text,
                    message,
                    at_ms,
                    truncated,
                    live: dest == Dest::Ui,
                },
                dest,
            ));
        };
        match admit {
            Admit::Keep => add(text, message, source, false, Dest::Both),
            Admit::KeepAndMark => {
                add(text, message, source, false, Dest::Both);
                let limit = Message::LimitReached {
                    limit: size_text(LOG_BODY_LIMIT),
                    tail: size_text(tail_limit as u64),
                };
                add(
                    "\n".into(),
                    Some(limit),
                    LogSource::System,
                    true,
                    Dest::Both,
                );
            }
            Admit::Drop => add(text, message, source, false, Dest::Ui),
        }
    }

    /// Save what was kept of `task`'s output past the limit, after it ended.
    fn save_tail(&self, inner: &mut Inner, task: u32) {
        let Some(tail) = inner.tails.remove(&task) else {
            return;
        };
        let saved = tail.finish();
        let at_ms = self.clock.now_ms();
        for (source, text) in saved.chunks {
            let n = inner.next;
            inner.next += 1;
            inner.pending.push((
                LogEntry {
                    n,
                    task,
                    source,
                    text,
                    message: None,
                    at_ms,
                    truncated: false,
                    live: false,
                },
                Dest::Disk,
            ));
        }
        let message = if saved.kept == saved.total {
            Message::TailSaved {
                size: size_text(saved.total),
            }
        } else {
            Message::TailPartial {
                total: size_text(saved.total),
                kept: size_text(saved.kept),
            }
        };
        let n = inner.next;
        inner.next += 1;
        inner.pending.push((
            LogEntry {
                n,
                task,
                source: LogSource::System,
                text: "\n".into(),
                message: Some(message),
                at_ms,
                truncated: false,
                live: false,
            },
            Dest::Both,
        ));
    }
}

/// The entries the UI gets from a batch.
fn shown(pending: Vec<(LogEntry, Dest)>) -> Vec<LogEntry> {
    pending
        .into_iter()
        .filter(|(_, dest)| *dest != Dest::Disk)
        .map(|(e, _)| e)
        .collect()
}

impl RunObserver for LogPipeline {
    fn changed(&self, machine: &RunMachine) {
        let mut inner = self.lock();
        // Release what the redactors held back for tasks that have ended.
        let ended: Vec<(u32, LogSource)> = inner
            .streams
            .keys()
            .filter(|(seq, _)| machine.tasks()[*seq as usize].status.is_terminal())
            .copied()
            .collect();
        for key in ended {
            let mut stream = inner.streams.remove(&key).expect("listed");
            let rest = stream.finish();
            self.push(&mut inner, key.0, key.1, rest, None);
        }
        let mut ended: Vec<u32> = inner
            .tails
            .keys()
            .filter(|seq| machine.tasks()[**seq as usize].status.is_terminal())
            .copied()
            .collect();
        ended.sort_unstable();
        for seq in ended {
            self.save_tail(&mut inner, seq);
        }
        self.flush_locked(&mut inner);
        drop(inner);
        self.view.state(machine);
    }

    fn output(&self, seq: u32, chunk: OutputChunk) {
        let source = match chunk.stream {
            Stream::Stdout => LogSource::Stdout,
            Stream::Stderr => LogSource::Stderr,
        };
        let mut inner = self.lock();
        let redactor = inner.redactor.clone();
        let text = inner
            .streams
            .entry((seq, source))
            .or_insert_with(|| StreamRedactor::new(redactor))
            .push(&chunk.text);
        self.push(&mut inner, seq, source, text, None);
    }

    /// Notes are written by the app, but their parameters (paths, third-party text) may come
    /// from resolved fields, so they are masked like output.
    fn note(&self, seq: u32, message: Message) {
        let mut inner = self.lock();
        let message = inner.redactor.redact_value(message);
        self.push(
            &mut inner,
            seq,
            LogSource::System,
            "\n".into(),
            Some(message),
        );
    }

    fn secret(&self, secret: &str) {
        let mut inner = self.lock();
        inner.redactor.add(secret);
        for stream in inner.streams.values_mut() {
            stream.add(secret);
        }
    }

    fn requested(&self, request: &OperatorRequest) {
        self.flush();
        self.view.requested(request);
    }

    fn storage_failed(&self, error: &StorageError) {
        self.flush();
        self.view.storage_failed(error);
    }

    fn progress(&self, seq: u32, progress: UploadProgress) {
        self.view.progress(seq, progress);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicI64, Ordering};

    use mdm_core::fixtures::demo_workspace;
    use mdm_core::resolve::NoValues;
    use mdm_core::*;
    use pretty_assertions::assert_eq;

    use super::*;

    #[derive(Default)]
    struct View {
        logs: Mutex<Vec<LogEntry>>,
        states: Mutex<Vec<RunStatus>>,
        failed: Mutex<bool>,
    }

    impl RunView for View {
        fn state(&self, m: &RunMachine) {
            self.states.lock().unwrap().push(m.run().status);
        }
        fn logs(&self, entries: Vec<LogEntry>) {
            self.logs.lock().unwrap().extend(entries);
        }
        fn requested(&self, _request: &OperatorRequest) {}
        fn storage_failed(&self, _error: &StorageError) {
            *self.failed.lock().unwrap() = true;
        }

        fn progress(&self, _seq: u32, _progress: UploadProgress) {}
    }

    #[derive(Clone, Default)]
    struct Disk(Arc<Mutex<(Vec<LogEntry>, bool)>>);

    impl LogSink for Disk {
        fn append(&mut self, entries: &[LogEntry]) -> Result<(), StorageError> {
            let mut d = self.0.lock().unwrap();
            if d.1 {
                return Err(StorageError(Message::text("disk full")));
            }
            d.0.extend_from_slice(entries);
            Ok(())
        }
    }

    struct Clock0(AtomicI64);
    impl Clock for Clock0 {
        fn now_ms(&self) -> i64 {
            self.0.fetch_add(1, Ordering::Relaxed)
        }
    }

    struct Setup {
        pipeline: LogPipeline,
        view: Arc<View>,
        disk: Disk,
        control: mpsc::UnboundedReceiver<Control>,
    }

    fn setup(secrets: &[&str], limit: u64) -> Setup {
        let view = Arc::new(View::default());
        let disk = Disk::default();
        let (tx, control) = mpsc::unbounded_channel();
        let pipeline = LogPipeline::new(
            secrets.iter().map(|s| s.to_string()),
            Box::new(disk.clone()),
            LogBudget::new(limit),
            view.clone(),
            Arc::new(Clock0(AtomicI64::new(0))),
            tx,
        );
        Setup {
            pipeline,
            view,
            disk,
            control,
        }
    }

    fn chunk(stream: Stream, text: &str) -> OutputChunk {
        OutputChunk {
            stream,
            text: text.into(),
            bytes: text.as_bytes().to_vec(),
            lossy: false,
        }
    }

    fn text_of(entries: &[LogEntry]) -> String {
        entries.iter().map(|e| e.text.as_str()).collect()
    }

    /// A local-only run of account-api (one task) driven to success.
    fn finished_machine() -> RunMachine {
        let demo = demo_workspace();
        let p = &demo.workspace.projects[0];
        let req = PlanRequest {
            project_id: p.id,
            pipeline_id: p.pipelines[0].id,
            environment_id: demo.staging,
            scope: RunScope::LocalOnly,
            server_ids: None,
            version: None,
        };
        let plan = build_plan(&demo.workspace, &req, &NoValues).unwrap();
        let mut m = RunMachine::from_plan(&plan, RunId::new(), 0);
        m.apply(RunEvent::Start, 1).unwrap();
        m.apply(RunEvent::TaskStarted { seq: 0 }, 2).unwrap();
        m.apply(
            RunEvent::TaskSucceeded {
                seq: 0,
                exit_code: Some(0),
            },
            3,
        )
        .unwrap();
        m
    }

    #[test]
    fn secrets_split_across_chunks_never_reach_disk_or_ui() {
        let s = setup(&["hunter2"], LOG_BODY_LIMIT_TEST);
        s.pipeline.output(0, chunk(Stream::Stdout, "pass=hun"));
        s.pipeline.output(0, chunk(Stream::Stderr, "err\n"));
        s.pipeline.output(0, chunk(Stream::Stdout, "ter2 ok\nhunt"));
        s.pipeline.flush();
        // The possible start of a secret at the end is held back until the task ends.
        assert_eq!(
            text_of(&s.view.logs.lock().unwrap()),
            "pass=err\n****** ok\n"
        );
        s.pipeline.changed(&finished_machine());
        let ui = s.view.logs.lock().unwrap().clone();
        let disk = s.disk.0.lock().unwrap().0.clone();
        assert_eq!(ui, disk);
        assert_eq!(text_of(&ui), "pass=err\n****** ok\nhunt");
        assert!(!text_of(&ui).contains("hunter2"));
        // Numbered in arrival order; sources kept.
        assert_eq!(ui.iter().map(|e| e.n).collect::<Vec<_>>(), [0, 1, 2, 3]);
        assert_eq!(ui[1].source, LogSource::Stderr);
        assert_eq!(*s.view.states.lock().unwrap(), [RunStatus::Succeeded]);
    }

    const LOG_BODY_LIMIT_TEST: u64 = 1024;

    #[test]
    fn past_the_body_limit_output_is_shown_but_not_saved() {
        let s = setup(&[], 10);
        s.pipeline.output(0, chunk(Stream::Stdout, "12345\n"));
        s.pipeline.output(0, chunk(Stream::Stdout, "67890\n"));
        s.pipeline.output(0, chunk(Stream::Stdout, "live\n"));
        s.pipeline.note(0, Message::Leftovers);
        s.pipeline.flush();
        let ui = s.view.logs.lock().unwrap().clone();
        assert_eq!(ui.len(), 5);
        assert!(ui[2].truncated && ui[2].source == LogSource::System);
        assert!(ui[3].live && ui[3].text == "live\n");
        assert_eq!(ui[4].message, Some(Message::Leftovers));
        let disk = s.disk.0.lock().unwrap().0.clone();
        assert_eq!(disk.iter().map(|e| e.n).collect::<Vec<_>>(), [0, 1, 2, 4]);
    }

    #[test]
    fn the_end_of_a_task_past_the_limit_is_saved_when_it_ends() {
        let s = setup(&[], 4);
        s.pipeline.lock().tail_limit = 8;
        for text in ["ab\n", "cd\n", "x1\n", "x2\n", "x3\n", "x4\n"] {
            s.pipeline.output(0, chunk(Stream::Stdout, text));
        }
        s.pipeline.changed(&finished_machine());
        let disk = s.disk.0.lock().unwrap().0.clone();
        // x2's line lost its start, so the tail begins at x3.
        assert_eq!(
            disk.iter().map(|e| e.text.as_str()).collect::<Vec<_>>()[3..],
            ["x3\n", "x4\n", "\n"]
        );
        let partial = Message::TailPartial {
            total: "12 B".into(),
            kept: "6 B".into(),
        };
        assert_eq!(disk[5].message.as_ref(), Some(&partial));
        assert!(disk.windows(2).all(|w| w[0].n < w[1].n));
        assert!(disk.iter().all(|e| !e.live));
        // The UI showed all of it live and gets only the note again, not the saved copy.
        let ui = s.view.logs.lock().unwrap().clone();
        assert_eq!(text_of(&ui[3..]), "x1\nx2\nx3\nx4\n\n");
        assert_eq!(ui[7].message.as_ref(), Some(&partial));
        assert_eq!(ui[3..7].iter().filter(|e| e.live).count(), 4);
    }

    #[test]
    fn disk_failure_cancels_the_run_and_still_shows_output() {
        let mut s = setup(&[], LOG_BODY_LIMIT_TEST);
        s.disk.0.lock().unwrap().1 = true;
        s.pipeline.output(0, chunk(Stream::Stdout, "line\n"));
        s.pipeline.flush();
        assert!(*s.view.failed.lock().unwrap());
        let ui = s.view.logs.lock().unwrap().clone();
        assert_eq!(ui[0].text, "line\n");
        let Some(Message::LogStorageFailed { reason }) = &ui[1].message else {
            panic!("{:?}", ui[1]);
        };
        assert_eq!(**reason, StorageError(Message::text("disk full")).message());
        assert!(matches!(s.control.try_recv(), Ok(Control::Cancel { .. })));
        // Later output still reaches the UI, without retrying the disk.
        s.pipeline.output(0, chunk(Stream::Stdout, "more\n"));
        s.pipeline.flush();
        assert_eq!(s.view.logs.lock().unwrap().len(), 3);
        assert!(s.control.try_recv().is_err());
    }

    #[tokio::test]
    async fn ten_thousand_lines_flow_through_in_batches() {
        let s = setup(&["s3cret"], LOG_BODY_LIMIT);
        let pipeline = Arc::new(s.pipeline);
        let stop = CancellationToken::new();
        let flusher = tokio::spawn(Arc::clone(&pipeline).flush_every(stop.clone()));
        let started = std::time::Instant::now();
        for i in 0..10_000 {
            pipeline.output(
                0,
                chunk(Stream::Stdout, &format!("line {i} token s3cret\n")),
            );
        }
        let pushed = started.elapsed();
        stop.cancel();
        flusher.await.unwrap();
        let ui = s.view.logs.lock().unwrap();
        assert_eq!(ui.len(), 10_000);
        assert!(!text_of(&ui).contains("s3cret"));
        assert_eq!(s.disk.0.lock().unwrap().0.len(), 10_000);
        // Redaction and batching must keep up with fast output (NFR-01 baseline).
        assert!(pushed < Duration::from_secs(2), "took {pushed:?}");
    }
}
