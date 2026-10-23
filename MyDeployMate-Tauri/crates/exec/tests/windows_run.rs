//! A local-only run end to end on Windows: coordinator, app executor and real shells (runs in
//! the Windows CI job). The store is in memory; `mdm-store` has its own tests.
#![cfg(windows)]

use std::sync::{Arc, Mutex};

use mdm_core::fixtures::demo_workspace;
use mdm_core::resolve::NoValues;
use mdm_core::*;
use mdm_exec::{AppExecutor, Coordinator, OperatorRequest, OutputChunk, RunObserver, SystemClock};

type Saved = Option<(Run, Vec<Task>)>;

#[derive(Clone, Default)]
struct MemStore(Arc<Mutex<Saved>>);

impl RunStore for MemStore {
    fn create_run(&mut self, m: &RunMachine, _plan: &ResolvedPlan) -> Result<(), StorageError> {
        self.save_run(m)
    }
    fn save_run(&mut self, m: &RunMachine) -> Result<(), StorageError> {
        *self.0.lock().unwrap() = Some((m.run().clone(), m.tasks().to_vec()));
        Ok(())
    }
}

#[derive(Default)]
struct Collect(Mutex<String>);

impl RunObserver for Collect {
    fn changed(&self, _m: &RunMachine) {}
    fn output(&self, _seq: u32, chunk: OutputChunk) {
        self.0.lock().unwrap().push_str(&chunk.text);
    }
    fn note(&self, _seq: u32, _message: Message) {}
    fn requested(&self, _request: &OperatorRequest) {}
    fn storage_failed(&self, _error: &StorageError) {}
}

/// Run account-api's local steps with `command` as the build step, in cmd.
async fn run_build(command: &str) -> (RunStatus, Vec<Task>, String) {
    let mut demo = demo_workspace();
    let p = &mut demo.workspace.projects[0];
    p.root = Some(std::env::temp_dir().display().to_string());
    let StepSpec::Local(spec) = &mut p.pipelines[0].steps[0].spec else {
        panic!("first step is local")
    };
    spec.command = command.into();
    spec.shell = Shell::Cmd;
    let req = PlanRequest {
        project_id: p.id,
        pipeline_id: p.pipelines[0].id,
        environment_id: demo.staging,
        scope: RunScope::LocalOnly,
        server_ids: None,
        version: None,
    };
    let plan = build_plan(&demo.workspace, &req, &NoValues).unwrap();
    assert!(plan.can_start(), "{:#?}", plan.issues);

    let store = MemStore::default();
    let observer = Arc::new(Collect::default());
    let (_control, control_rx) = tokio::sync::mpsc::unbounded_channel();
    let coordinator = Coordinator::create(
        Arc::new(plan),
        RunId::new(),
        store.clone(),
        Arc::new(SystemClock),
        Arc::new(AppExecutor::default()),
        observer.clone(),
        control_rx,
    )
    .map_err(|(e, _)| e)
    .unwrap();
    let (status, _) = coordinator.run().await;
    let (run, tasks) = store.0.lock().unwrap().clone().unwrap();
    assert_eq!(run.status, status);
    let output = observer.0.lock().unwrap().clone();
    (status, tasks, output)
}

#[tokio::test]
async fn local_only_run_succeeds() {
    let (status, tasks, output) = run_build("echo built").await;
    assert_eq!(status, RunStatus::Succeeded, "{tasks:#?}");
    assert!(tasks.iter().all(|t| t.kind == StepKind::Local));
    assert_eq!(tasks[0].exit_code, Some(0));
    assert!(output.contains("built"));
}

#[tokio::test]
async fn failing_command_fails_the_run() {
    let (status, tasks, _) = run_build("exit /b 7").await;
    assert_eq!(status, RunStatus::Failed);
    assert_eq!(tasks[0].status, TaskStatus::Failed);
    assert_eq!(tasks[0].exit_code, Some(7));
    assert_eq!(tasks[0].reason, Some(Message::ExitCode { code: 7 }));
}
