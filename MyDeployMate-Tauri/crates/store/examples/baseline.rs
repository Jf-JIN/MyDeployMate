//! NFR-01 baseline data and backend timings (plan task 8.1).
//!
//! Fills an empty data directory with 100 servers, 50 projects of 50 steps, 1000 finished runs
//! over the last 80 days and a 10000-line log on the newest run, then times what the app does
//! with it on startup and navigation. Start the app on that directory with `MDM_DATA_DIR`.
//!
//! ```text
//! cargo run --release -p mdm-store --example baseline -- <empty dir> [runs] [log lines]
//! ```
// A command-line tool: its report goes to the terminal.
#![allow(clippy::print_stdout)]

use std::path::PathBuf;
use std::time::Instant;

use mdm_core::fixtures::{DemoWorkspace, baseline_workspace};
use mdm_core::resolve::NoValues;
use mdm_core::*;
use mdm_store::*;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn timed<T>(label: &str, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let value = f();
    println!(
        "{label:<44} {:>9.1} ms",
        start.elapsed().as_secs_f64() * 1000.0
    );
    value
}

fn request(demo: &DemoWorkspace, project: usize, environment: EnvironmentId) -> PlanRequest {
    let p = &demo.workspace.projects[project];
    PlanRequest {
        project_id: p.id,
        pipeline_id: p.pipelines[0].id,
        environment_id: environment,
        scope: RunScope::Full,
        server_ids: None,
        version: None,
    }
}

/// Drive a run of `plan` to its end: every task succeeds, except one in `fail` (by task index).
fn finished_run(plan: &ResolvedPlan, start_ms: i64, fail: Option<u32>) -> RunMachine {
    let mut at = start_ms;
    let mut tick = || {
        at += 900;
        at
    };
    let mut m = RunMachine::from_plan(plan, RunId::new(), tick());
    m.apply(RunEvent::Start, tick()).unwrap();
    loop {
        let (seq, outcome) = match m.next() {
            Next::Done { .. } => break,
            Next::AwaitConfirmation { seq } => {
                (seq, RunEvent::ConfirmationAnswered { seq, proceed: true })
            }
            Next::Dispatch { seq } if Some(seq) == fail => (
                seq,
                RunEvent::TaskFailed {
                    seq,
                    exit_code: Some(1),
                    exit_signal: None,
                    reason: Message::ExitCode { code: 1 },
                },
            ),
            Next::Dispatch { seq } => (
                seq,
                RunEvent::TaskSucceeded {
                    seq,
                    exit_code: Some(0),
                },
            ),
            Next::Wait => unreachable!("no task runs between two events here"),
        };
        m.apply(RunEvent::TaskStarted { seq }, tick()).unwrap();
        m.apply(outcome, tick()).unwrap();
    }
    m
}

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(root) = args.next().map(PathBuf::from) else {
        println!("usage: baseline <empty dir> [runs = 1000] [log lines = 10000]");
        return;
    };
    let runs: usize = args.next().map_or(1000, |a| a.parse().expect("runs"));
    let lines: usize = args
        .next()
        .map_or(10_000, |a| a.parse().expect("log lines"));
    let dir = DataDir::at(&root);
    assert!(
        !dir.workspace_file().exists(),
        "{} already holds a workspace; pick an empty directory",
        root.display()
    );
    dir.ensure().unwrap();

    println!("== seeding {}", root.display());
    let demo = baseline_workspace();
    let mut store = WorkspaceStore::new(dir.clone());
    let mut ws = demo.workspace.clone();
    ws.revision = store.load().unwrap().0.revision;
    let ws = timed("save workspace", || store.save(ws).unwrap());

    let mut history = HistoryStore::open(&dir.history_db()).unwrap();
    let now = now_ms();
    let plans: Vec<ResolvedPlan> = (0..ws.projects.len())
        .map(|p| build_plan(&ws, &request(&demo, p, demo.staging), &NoValues).unwrap())
        .collect();
    let mut newest = None;
    timed(&format!("record {runs} finished runs"), || {
        for i in 0..runs {
            let plan = &plans[i % plans.len()];
            let start = now - 80 * DAY_MS + (i as i64) * (80 * DAY_MS / runs.max(1) as i64);
            let fail = (i % 7 == 3).then_some(plan.tasks.len() as u32 / 2);
            let m = finished_run(plan, start, fail);
            history.create_run(&m, plan).unwrap();
            history.save_run(&m).unwrap();
            newest = Some((m.run().id, plan.tasks.len() as u32));
        }
    });
    if let Some((run, tasks)) = newest {
        timed(&format!("write a {lines}-line log"), || {
            let mut log = RunLogFile::open(&dir, run).unwrap();
            let entries: Vec<LogEntry> = (0..lines)
                .map(|n| LogEntry {
                    n: n as u64,
                    task: (n as u32) % tasks,
                    source: if n % 50 == 49 {
                        LogSource::Stderr
                    } else {
                        LogSource::Stdout
                    },
                    text: format!("[{n:05}] compiling module {} ... ok\n", n % 997),
                    message: None,
                    at_ms: now,
                    truncated: false,
                    live: false,
                })
                .collect();
            for chunk in entries.chunks(500) {
                log.append(chunk).unwrap();
            }
        });
    }
    drop(history);

    println!("== timings (what startup and navigation call)");
    let mut store = WorkspaceStore::new(dir.clone());
    let (ws, _) = timed("load workspace.json", || store.load().unwrap());
    let report = timed("validate workspace", || validate_workspace(&ws));
    assert!(report.is_ok());
    let json = timed("serialize workspace for IPC", || {
        serde_json::to_vec(&ws).unwrap()
    });
    let plan = timed("preview: build plan (50 steps x 4 servers)", || {
        build_plan(&ws, &request(&demo, 0, demo.production), &NoValues).unwrap()
    });
    assert!(plan.can_start());
    let history = timed("open history.sqlite", || {
        HistoryStore::open(&dir.history_db()).unwrap()
    });
    let listed = timed("list runs (no filter)", || {
        history.list_runs(&RunFilter::default()).unwrap()
    });
    let project = ws.projects[3].id;
    timed("list runs (one project)", || {
        history
            .list_runs(&RunFilter {
                project_id: Some(project),
                ..Default::default()
            })
            .unwrap()
    });
    timed("history filter choices", || history.run_choices().unwrap());
    timed("latest run per pipeline", || history.latest_runs().unwrap());
    if let Some(first) = listed.first() {
        let id = first.run.id;
        timed("open run: records and plan", || {
            (
                history.load_run(id).unwrap(),
                history.load_plan(id).unwrap(),
            )
        });
        let log = timed("open run: read log", || read_run_log(&dir, id).unwrap());
        println!("  log entries read: {}", log.entries.len());
    }
    timed("export configuration", || export_json(&ws, true));
    timed("count runs older than 90 days", || {
        history.count_expired(now - 90 * DAY_MS).unwrap()
    });

    let size = |p: PathBuf| std::fs::metadata(p).map_or(0, |m| m.len());
    println!("== sizes");
    println!("workspace JSON for IPC   {:>10} bytes", json.len());
    println!(
        "workspace.json           {:>10} bytes",
        size(dir.workspace_file())
    );
    println!(
        "history.sqlite           {:>10} bytes",
        size(dir.history_db())
    );
    println!("runs listed              {:>10}", listed.len());
    println!("plan tasks               {:>10}", plan.tasks.len());
    println!();
    println!(
        "Start the app on this data with MDM_DATA_DIR={}",
        root.display()
    );
}
