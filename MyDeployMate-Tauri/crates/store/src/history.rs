//! Run history in `history.sqlite` (SRS RUN-07, architecture ARC-14, ARC-32, ARC-34).
//!
//! The coordinator is the only writer. Every state change is one transaction; only task rows
//! that changed since the last write are updated. Database migrations use `PRAGMA user_version`
//! and run inside a transaction.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use mdm_core::machine::TaskMeta;
use mdm_core::{
    ResolvedPlan, Run, RunEvent, RunId, RunMachine, RunStatus, RunStore, StorageError, Task, TaskId,
};
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::StoreError;

/// Schema versions in order; entry `i` upgrades from version `i` to `i + 1`.
const MIGRATIONS: &[&str] = &[r#"
CREATE TABLE runs (
    id               TEXT PRIMARY KEY,
    project_id       TEXT NOT NULL,
    pipeline_id      TEXT NOT NULL,
    environment_id   TEXT NOT NULL,
    project_name     TEXT NOT NULL,
    pipeline_name    TEXT NOT NULL,
    environment_name TEXT NOT NULL,
    version          TEXT NOT NULL,
    execution_order  TEXT NOT NULL,
    local_only       INTEGER NOT NULL,
    status           TEXT NOT NULL,
    finished         INTEGER NOT NULL,
    cancel_reason    TEXT,
    created_at_ms    INTEGER NOT NULL,
    finished_at_ms   INTEGER,
    -- Display-safe plan as previewed: sensitive values are masked (ARC-10).
    plan_json        TEXT NOT NULL
);
CREATE INDEX runs_by_created ON runs (created_at_ms);
CREATE INDEX runs_unfinished ON runs (finished) WHERE finished = 0;
CREATE TABLE tasks (
    id             TEXT PRIMARY KEY,
    run_id         TEXT NOT NULL REFERENCES runs (id) ON DELETE CASCADE,
    seq            INTEGER NOT NULL,
    step_id        TEXT NOT NULL,
    step_name      TEXT NOT NULL,
    kind           TEXT NOT NULL,
    server_id      TEXT,
    server_name    TEXT,
    failure_policy TEXT NOT NULL,
    status         TEXT NOT NULL,
    exit_code      INTEGER,
    exit_signal    TEXT,
    reason         TEXT,
    started_at_ms  INTEGER,
    finished_at_ms INTEGER,
    UNIQUE (run_id, seq)
);
"#];

/// Why a run was found unfinished at startup (RUN-07).
pub const INTERRUPTED_REASON: &str = "应用在运行过程中退出，运行未自动继续";

/// A run as stored, with its tasks in plan order.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredRun {
    pub run: Run,
    pub tasks: Vec<Task>,
    pub cancel_reason: Option<String>,
}

pub struct HistoryStore {
    conn: Connection,
    /// Task rows as last written, to update only what changed.
    written: HashMap<TaskId, Task>,
}

impl HistoryStore {
    /// Open or create the database and bring its schema up to date.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let conn = Connection::open(path).map_err(db)?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(db)?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(db)?;
        Self::init(conn)
    }

    /// A database that lives only as long as the store, for tests.
    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::init(Connection::open_in_memory().map_err(db)?)
    }

    fn init(mut conn: Connection) -> Result<Self, StoreError> {
        conn.pragma_update(None, "foreign_keys", true).map_err(db)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(db)?;
        migrate(&mut conn)?;
        Ok(HistoryStore {
            conn,
            written: HashMap::new(),
        })
    }

    /// Mark every unfinished run interrupted (ARC-34): tasks that were dispatched become
    /// outcome unknown, tasks not started are skipped. Nothing is resumed. Returns the runs.
    pub fn interrupt_unfinished(&mut self, now_ms: i64) -> Result<Vec<RunId>, StoreError> {
        let ids: Vec<String> = {
            let mut stmt = self
                .conn
                .prepare("SELECT id FROM runs WHERE finished = 0 ORDER BY created_at_ms")
                .map_err(db)?;
            stmt.query_map([], |r| r.get(0))
                .map_err(db)?
                .collect::<Result<_, _>>()
                .map_err(db)?
        };
        let mut out = Vec::new();
        for id in ids {
            let id: RunId = from_text(&id)?;
            let (stored, meta) = self.load_with_meta(id)?.expect("listed above");
            let mut machine = RunMachine::new(stored.run, stored.tasks, meta);
            machine
                .apply(
                    RunEvent::Interrupted {
                        reason: INTERRUPTED_REASON.into(),
                    },
                    now_ms,
                )
                .map_err(|e| StoreError::Database(e.to_string()))?;
            self.save(&machine)?;
            out.push(id);
        }
        Ok(out)
    }

    pub fn load_run(&self, id: RunId) -> Result<Option<StoredRun>, StoreError> {
        Ok(self.load_with_meta(id)?.map(|(run, _)| run))
    }

    /// The plan the run was started from, as frozen at its start (IXD-25).
    pub fn load_plan(&self, id: RunId) -> Result<Option<ResolvedPlan>, StoreError> {
        let json: Option<String> = self
            .conn
            .query_row(
                "SELECT plan_json FROM runs WHERE id = ?1",
                [to_text(&id)],
                |r| r.get(0),
            )
            .optional()
            .map_err(db)?;
        json.map(|j| serde_json::from_str(&j).map_err(|e| StoreError::Database(e.to_string())))
            .transpose()
    }

    fn load_with_meta(&self, id: RunId) -> Result<Option<(StoredRun, Vec<TaskMeta>)>, StoreError> {
        let id_text = to_text(&id);
        let head = self
            .conn
            .query_row(
                "SELECT project_id, pipeline_id, environment_id, project_name, pipeline_name,
                        environment_name, version, execution_order, local_only, status,
                        cancel_reason, created_at_ms, finished_at_ms
                 FROM runs WHERE id = ?1",
                [&id_text],
                |r| Ok((read_run(id, r), r.get::<_, Option<String>>(10)?)),
            )
            .optional()
            .map_err(db)?;
        let Some((run, cancel_reason)) = head else {
            return Ok(None);
        };
        let run = run?;
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, seq, step_id, step_name, kind, server_id, server_name, failure_policy,
                        status, exit_code, exit_signal, reason, started_at_ms, finished_at_ms
                 FROM tasks WHERE run_id = ?1 ORDER BY seq",
            )
            .map_err(db)?;
        let rows = stmt
            .query_map([&id_text], |r| Ok(read_task(id, r)))
            .map_err(db)?;
        let (mut tasks, mut meta) = (Vec::new(), Vec::new());
        for row in rows {
            let (task, m) = row.map_err(db)??;
            tasks.push(task);
            meta.push(m);
        }
        Ok(Some((
            StoredRun {
                run,
                tasks,
                cancel_reason,
            },
            meta,
        )))
    }

    fn create(&mut self, machine: &RunMachine, plan: &ResolvedPlan) -> Result<(), StoreError> {
        let plan_json =
            serde_json::to_string(plan).map_err(|e| StoreError::Database(e.to_string()))?;
        let tx = self.conn.transaction().map_err(db)?;
        let unfinished: i64 = tx
            .query_row("SELECT COUNT(*) FROM runs WHERE finished = 0", [], |r| {
                r.get(0)
            })
            .map_err(db)?;
        if unfinished > 0 {
            return Err(StoreError::RunActive);
        }
        let r = machine.run();
        tx.execute(
            "INSERT INTO runs (id, project_id, pipeline_id, environment_id, project_name,
                               pipeline_name, environment_name, version, execution_order,
                               local_only, status, finished, cancel_reason, created_at_ms,
                               finished_at_ms, plan_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                to_text(&r.id),
                to_text(&r.project_id),
                to_text(&r.pipeline_id),
                to_text(&r.environment_id),
                r.project_name,
                r.pipeline_name,
                r.environment_name,
                r.version,
                to_text(&r.execution_order),
                r.local_only,
                to_text(&r.status),
                r.status.is_terminal(),
                machine.cancel_reason(),
                r.created_at_ms,
                r.finished_at_ms,
                plan_json,
            ],
        )
        .map_err(db)?;
        for (task, meta) in machine.tasks().iter().zip(machine.meta()) {
            insert_task(&tx, task, meta)?;
        }
        tx.commit().map_err(db)?;
        for task in machine.tasks() {
            self.written.insert(task.id, task.clone());
        }
        Ok(())
    }

    fn save(&mut self, machine: &RunMachine) -> Result<(), StoreError> {
        let r = machine.run();
        let tx = self.conn.transaction().map_err(db)?;
        let updated = tx
            .execute(
                "UPDATE runs SET status = ?2, finished = ?3, cancel_reason = ?4,
                                 finished_at_ms = ?5
                 WHERE id = ?1",
                params![
                    to_text(&r.id),
                    to_text(&r.status),
                    r.status.is_terminal(),
                    machine.cancel_reason(),
                    r.finished_at_ms,
                ],
            )
            .map_err(db)?;
        if updated != 1 {
            return Err(StoreError::Database(format!("运行 {} 不存在", r.id)));
        }
        let changed: Vec<&Task> = machine
            .tasks()
            .iter()
            .filter(|t| self.written.get(&t.id) != Some(*t))
            .collect();
        for t in &changed {
            tx.execute(
                "UPDATE tasks SET status = ?2, exit_code = ?3, exit_signal = ?4, reason = ?5,
                                  started_at_ms = ?6, finished_at_ms = ?7
                 WHERE id = ?1",
                params![
                    to_text(&t.id),
                    to_text(&t.status),
                    t.exit_code,
                    t.exit_signal,
                    t.reason,
                    t.started_at_ms,
                    t.finished_at_ms,
                ],
            )
            .map_err(db)?;
        }
        tx.commit().map_err(db)?;
        for t in changed {
            self.written.insert(t.id, t.clone());
        }
        if r.status.is_terminal() {
            let run_id = r.id;
            self.written.retain(|_, t| t.run_id != run_id);
        }
        Ok(())
    }
}

impl RunStore for HistoryStore {
    fn create_run(
        &mut self,
        machine: &RunMachine,
        plan: &ResolvedPlan,
    ) -> Result<(), StorageError> {
        self.create(machine, plan)
            .map_err(|e| StorageError(e.to_string()))
    }

    fn save_run(&mut self, machine: &RunMachine) -> Result<(), StorageError> {
        self.save(machine).map_err(|e| StorageError(e.to_string()))
    }
}

/// One [`HistoryStore`] shared by the running coordinator and the commands that read history.
#[derive(Clone)]
pub struct SharedHistory(Arc<Mutex<HistoryStore>>);

impl SharedHistory {
    pub fn new(store: HistoryStore) -> Self {
        SharedHistory(Arc::new(Mutex::new(store)))
    }

    pub fn lock(&self) -> MutexGuard<'_, HistoryStore> {
        // A panic while holding the lock leaves SQLite consistent; keep using the store.
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl RunStore for SharedHistory {
    fn create_run(
        &mut self,
        machine: &RunMachine,
        plan: &ResolvedPlan,
    ) -> Result<(), StorageError> {
        self.lock().create_run(machine, plan)
    }

    fn save_run(&mut self, machine: &RunMachine) -> Result<(), StorageError> {
        self.lock().save_run(machine)
    }
}

fn migrate(conn: &mut Connection) -> Result<(), StoreError> {
    let current: u32 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(db)?;
    let supported = MIGRATIONS.len() as u32;
    if current > supported {
        return Err(StoreError::HistoryVersion {
            found: current,
            supported,
        });
    }
    for (version, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction().map_err(db)?;
        tx.execute_batch(sql).map_err(db)?;
        tx.pragma_update(None, "user_version", version as u32 + 1)
            .map_err(db)?;
        tx.commit().map_err(db)?;
    }
    Ok(())
}

fn insert_task(tx: &Transaction<'_>, t: &Task, meta: &TaskMeta) -> Result<(), StoreError> {
    tx.execute(
        "INSERT INTO tasks (id, run_id, seq, step_id, step_name, kind, server_id, server_name,
                            failure_policy, status, exit_code, exit_signal, reason,
                            started_at_ms, finished_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            to_text(&t.id),
            to_text(&t.run_id),
            t.seq,
            to_text(&t.step_id),
            t.step_name,
            to_text(&t.kind),
            t.server_id.as_ref().map(to_text),
            t.server_name,
            to_text(&meta.policy),
            to_text(&t.status),
            t.exit_code,
            t.exit_signal,
            t.reason,
            t.started_at_ms,
            t.finished_at_ms,
        ],
    )
    .map_err(db)?;
    Ok(())
}

fn read_run(id: RunId, r: &Row<'_>) -> Result<Run, StoreError> {
    let text = |i: usize| r.get::<_, String>(i).map_err(db);
    Ok(Run {
        id,
        project_id: from_text(&text(0)?)?,
        pipeline_id: from_text(&text(1)?)?,
        environment_id: from_text(&text(2)?)?,
        project_name: text(3)?,
        pipeline_name: text(4)?,
        environment_name: text(5)?,
        version: text(6)?,
        execution_order: from_text(&text(7)?)?,
        local_only: r.get(8).map_err(db)?,
        status: from_text::<RunStatus>(&text(9)?)?,
        created_at_ms: r.get(11).map_err(db)?,
        finished_at_ms: r.get(12).map_err(db)?,
    })
}

fn read_task(run_id: RunId, r: &Row<'_>) -> Result<(Task, TaskMeta), StoreError> {
    let text = |i: usize| r.get::<_, String>(i).map_err(db);
    let opt = |i: usize| r.get::<_, Option<String>>(i).map_err(db);
    let task = Task {
        id: from_text(&text(0)?)?,
        run_id,
        seq: r.get(1).map_err(db)?,
        step_id: from_text(&text(2)?)?,
        step_name: text(3)?,
        kind: from_text(&text(4)?)?,
        server_id: opt(5)?.map(|s| from_text(&s)).transpose()?,
        server_name: opt(6)?,
        status: from_text(&text(8)?)?,
        exit_code: r.get(9).map_err(db)?,
        exit_signal: opt(10)?,
        reason: opt(11)?,
        started_at_ms: r.get(12).map_err(db)?,
        finished_at_ms: r.get(13).map_err(db)?,
    };
    let meta = TaskMeta {
        policy: from_text(&text(7)?)?,
    };
    Ok((task, meta))
}

/// IDs and enums are stored as their JSON string form, e.g. `waitingConfirmation`.
fn to_text<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(s)) => s,
        other => panic!("not a string value: {other:?}"),
    }
}

fn from_text<T: DeserializeOwned>(text: &str) -> Result<T, StoreError> {
    serde_json::from_value(serde_json::Value::String(text.to_owned()))
        .map_err(|e| StoreError::Database(format!("无法读取 {text:?}: {e}")))
}

fn db(e: rusqlite::Error) -> StoreError {
    StoreError::Database(e.to_string())
}

#[cfg(test)]
mod tests {
    use mdm_core::fixtures::demo_workspace;
    use mdm_core::resolve::NoValues;
    use mdm_core::{Next, PlanRequest, RunScope, TaskStatus, build_plan};
    use pretty_assertions::assert_eq;

    use super::*;

    fn plan() -> ResolvedPlan {
        let demo = demo_workspace();
        let p = &demo.workspace.projects[0];
        let req = PlanRequest {
            project_id: p.id,
            pipeline_id: p.pipelines[0].id,
            environment_id: demo.staging,
            scope: RunScope::Full,
            server_ids: None,
            version: None,
        };
        let plan = build_plan(&demo.workspace, &req, &NoValues).unwrap();
        assert!(plan.can_start(), "{:#?}", plan.issues);
        plan
    }

    fn started(store: &mut HistoryStore) -> RunMachine {
        let plan = plan();
        let mut m = RunMachine::from_plan(&plan, RunId::new(), 1_000);
        store.create_run(&m, &plan).unwrap();
        m.apply(RunEvent::Start, 1_001).unwrap();
        store.save_run(&m).unwrap();
        m
    }

    fn stored(store: &HistoryStore, m: &RunMachine) -> StoredRun {
        store.load_run(m.run().id).unwrap().unwrap()
    }

    #[test]
    fn records_round_trip_after_every_change() {
        let mut store = HistoryStore::open_in_memory().unwrap();
        let mut m = started(&mut store);
        assert_eq!(stored(&store, &m).run, *m.run());

        let Next::Dispatch { seq } = m.next() else {
            panic!()
        };
        m.apply(RunEvent::TaskStarted { seq }, 1_010).unwrap();
        store.save_run(&m).unwrap();
        m.apply(
            RunEvent::TaskFailed {
                seq,
                exit_code: Some(2),
                exit_signal: None,
                reason: "退出码 2".into(),
            },
            1_020,
        )
        .unwrap();
        store.save_run(&m).unwrap();

        let back = stored(&store, &m);
        assert_eq!(back.run, *m.run());
        assert_eq!(back.tasks, m.tasks());
        assert_eq!(back.run.status, RunStatus::Failed);
        assert_eq!(back.tasks[0].exit_code, Some(2));
        assert!(
            back.tasks[1..]
                .iter()
                .all(|t| t.status == TaskStatus::Skipped)
        );
    }

    #[test]
    fn only_one_unfinished_run_at_a_time() {
        let mut store = HistoryStore::open_in_memory().unwrap();
        let mut first = started(&mut store);
        let plan = plan();
        let second = RunMachine::from_plan(&plan, RunId::new(), 2_000);
        let err = store.create_run(&second, &plan).unwrap_err();
        assert!(err.0.contains("另一个运行"), "{err}");

        first
            .apply(
                RunEvent::CancelRequested {
                    reason: "用户取消".into(),
                },
                2_001,
            )
            .unwrap();
        store.save_run(&first).unwrap();
        assert_eq!(
            stored(&store, &first).cancel_reason.as_deref(),
            Some("用户取消")
        );
        store.create_run(&second, &plan).unwrap();
    }

    #[test]
    fn unfinished_runs_are_interrupted_on_open() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.sqlite");
        let (run_id, running_seq) = {
            let mut store = HistoryStore::open(&path).unwrap();
            let mut m = started(&mut store);
            let Next::Dispatch { seq } = m.next() else {
                panic!()
            };
            m.apply(RunEvent::TaskStarted { seq }, 1_010).unwrap();
            store.save_run(&m).unwrap();
            (m.run().id, seq)
        };

        let mut store = HistoryStore::open(&path).unwrap();
        assert_eq!(store.interrupt_unfinished(5_000).unwrap(), vec![run_id]);
        let back = store.load_run(run_id).unwrap().unwrap();
        assert_eq!(back.run.status, RunStatus::Interrupted);
        assert_eq!(back.run.finished_at_ms, Some(5_000));
        let running = &back.tasks[running_seq as usize];
        assert_eq!(running.status, TaskStatus::Unknown);
        assert_eq!(running.reason.as_deref(), Some(INTERRUPTED_REASON));
        assert!(
            back.tasks
                .iter()
                .filter(|t| t.seq != running_seq)
                .all(|t| t.status == TaskStatus::Skipped)
        );
        assert!(store.interrupt_unfinished(6_000).unwrap().is_empty());
    }

    #[test]
    fn newer_schema_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.sqlite");
        Connection::open(&path)
            .unwrap()
            .pragma_update(None, "user_version", 99)
            .unwrap();
        assert!(matches!(
            HistoryStore::open(&path),
            Err(StoreError::HistoryVersion { found: 99, .. })
        ));
    }

    #[test]
    fn plan_snapshot_has_no_sensitive_values() {
        let mut store = HistoryStore::open_in_memory().unwrap();
        let m = started(&mut store);
        let json: String = store
            .conn
            .query_row(
                "SELECT plan_json FROM runs WHERE id = ?1",
                [to_text(&m.run().id)],
                |r| r.get(0),
            )
            .unwrap();
        let back = store.load_plan(m.run().id).unwrap().unwrap();
        assert_eq!(back.tasks.len(), m.tasks().len());
        assert!(store.load_plan(RunId::new()).unwrap().is_none());
        assert!(!json.contains("\"secrets\""));
    }
}
