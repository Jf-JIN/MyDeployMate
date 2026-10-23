//! Log pipeline (SRS OBS-01, NFR-02; architecture ARC-07, ARC-27, ARC-33; plan task 4.4).
//!
//! Sits between the coordinator and the UI. Output is redacted per task and stream (a secret
//! split across chunks is still caught), numbered, written to the run's log file and pushed to
//! the UI in batches by [`LogPipeline::flush_every`]. State changes go out at once, after the
//! output that came before them.
//!
//! Past the 50 MiB body limit, output is still read but neither kept nor shown, and one marker
//! entry says so. If the log file cannot be written, the run is cancelled: nothing more is
//! dispatched without a record of its output (ARC-33).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mdm_core::log::{Admit, LogBudget};
use mdm_core::redact::{Redactor, StreamRedactor};
use mdm_core::{Clock, LogEntry, LogSink, LogSource, RunMachine, StorageError};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::coordinator::{Control, CredentialRequest, RunObserver};
use crate::local::{OutputChunk, Stream};

/// How often batched output is pushed (ARC-07: 50–100 ms).
pub const FLUSH_PERIOD: Duration = Duration::from_millis(80);

/// The UI side of a run.
pub trait RunView: Send + Sync {
    fn state(&self, machine: &RunMachine);
    fn logs(&self, entries: Vec<LogEntry>);
    fn credential_requested(&self, request: &CredentialRequest);
    fn storage_failed(&self, error: &StorageError);
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
    pending: Vec<LogEntry>,
    next: u64,
    budget: LogBudget,
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
        let mut batch = std::mem::take(&mut inner.pending);
        if let Some(sink) = &mut inner.sink
            && let Err(e) = sink.append(&batch)
        {
            inner.sink = None;
            let at_ms = self.clock.now_ms();
            let task = batch.last().map_or(0, |e| e.task);
            let n = inner.next;
            inner.next += 1;
            batch.push(LogEntry {
                n,
                task,
                source: LogSource::System,
                text: format!("{e}；之后的输出不再保存，运行已停止\n"),
                at_ms,
                truncated: false,
            });
            self.view.storage_failed(&e);
            let _ = self.control.send(Control::Cancel {
                reason: "日志无法写入磁盘".into(),
            });
        }
        self.view.logs(batch);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn push(&self, inner: &mut Inner, task: u32, source: LogSource, text: String) {
        if text.is_empty() {
            return;
        }
        let admit = inner.budget.admit(source, text.len());
        if admit == Admit::Drop {
            return;
        }
        let at_ms = self.clock.now_ms();
        let mut add = |text: String, source, truncated| {
            let n = inner.next;
            inner.next += 1;
            inner.pending.push(LogEntry {
                n,
                task,
                source,
                text,
                at_ms,
                truncated,
            });
        };
        add(text, source, false);
        if admit == Admit::KeepAndMark {
            add(
                "\n日志已达到 50 MiB 上限，之后的命令输出不再保存和显示\n".into(),
                LogSource::System,
                true,
            );
        }
    }
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
            self.push(&mut inner, key.0, key.1, rest);
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
        self.push(&mut inner, seq, source, text);
    }

    fn note(&self, seq: u32, text: &str) {
        let mut inner = self.lock();
        let text = inner.redactor.redact(text);
        self.push(&mut inner, seq, LogSource::System, format!("{text}\n"));
    }

    fn credential_requested(&self, request: &CredentialRequest) {
        self.flush();
        self.view.credential_requested(request);
    }

    fn storage_failed(&self, error: &StorageError) {
        self.flush();
        self.view.storage_failed(error);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicI64, Ordering};

    use mdm_core::fixtures::demo_workspace;
    use mdm_core::log::LOG_BODY_LIMIT;
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
        fn credential_requested(&self, _request: &CredentialRequest) {}
        fn storage_failed(&self, _error: &StorageError) {
            *self.failed.lock().unwrap() = true;
        }
    }

    #[derive(Clone, Default)]
    struct Disk(Arc<Mutex<(Vec<LogEntry>, bool)>>);

    impl LogSink for Disk {
        fn append(&mut self, entries: &[LogEntry]) -> Result<(), StorageError> {
            let mut d = self.0.lock().unwrap();
            if d.1 {
                return Err(StorageError("磁盘已满".into()));
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
    fn body_limit_marks_once_and_drops_the_rest() {
        let s = setup(&[], 10);
        s.pipeline.output(0, chunk(Stream::Stdout, "12345\n"));
        s.pipeline.output(0, chunk(Stream::Stdout, "67890\n"));
        s.pipeline.output(0, chunk(Stream::Stdout, "dropped\n"));
        s.pipeline.note(0, "命令结束时仍有进程在运行");
        s.pipeline.flush();
        let ui = s.view.logs.lock().unwrap().clone();
        assert_eq!(ui.len(), 4);
        assert!(ui[2].truncated && ui[2].source == LogSource::System);
        assert_eq!(ui[3].text, "命令结束时仍有进程在运行\n");
        assert!(!text_of(&ui).contains("dropped"));
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
        assert!(ui[1].text.contains("磁盘已满"));
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
