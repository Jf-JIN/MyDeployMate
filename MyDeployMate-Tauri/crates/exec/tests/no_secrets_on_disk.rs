//! Nothing secret reaches the data directory or an export (SRS SEC-03, SEC-04, VAR-03, NFR-02;
//! plan task 8.3).
//!
//! One run goes through the real store: workspace file, history database, run log, credential
//! index and import backup. A sensitive variable and a password typed in D04 are used the way
//! a careless command or a failing executor would: printed (split across chunks), put in an
//! upload path, a note and a failure reason, and the password is printed after it was typed. Afterwards every byte under the data directory and
//! both exports is searched for the secrets, as UTF-8 and as UTF-16LE.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mdm_core::fixtures::demo_workspace;
use mdm_core::log::LogBudget;
use mdm_core::*;
use mdm_exec::*;
use mdm_store::*;
use secrecy::{ExposeSecret, SecretString};
use tokio::sync::mpsc;

const TOKEN: &str = "tok-7Hq9-sensitive-value";
const PASSWORD: &str = "pw-D04-typed-Zx81";

struct Values<'a>(&'a SecretStore);

impl ValueSource for Values<'_> {
    fn env_var(&self, _name: &str) -> Option<String> {
        None
    }

    fn secret(&self, variable: VariableId, environment: Option<EnvironmentId>) -> Option<String> {
        let value = self.0.variable(variable, environment).ok()??;
        Some(value.expose_secret().to_owned())
    }
}

/// Prints and reports the secrets; the first remote-side task asks for a password and
/// remembers it once "accepted", as the app does (ARC-26).
struct LeakyExecutor {
    secrets: Arc<Mutex<SecretStore>>,
    last: u32,
}

fn chunk(text: &str) -> OutputChunk {
    OutputChunk {
        stream: Stream::Stdout,
        text: text.into(),
        bytes: Vec::new(),
        lossy: false,
    }
}

impl TaskExecutor for LeakyExecutor {
    fn execute<'a>(&'a self, task: &'a PlannedTask, ctx: TaskContext) -> TaskFuture<'a> {
        Box::pin(async move {
            let (head, tail) = TOKEN.split_at(7);
            ctx.output(chunk(&format!("token={head}")));
            ctx.output(chunk(&format!("{tail}\n")));
            ctx.note(Message::SourceMissing {
                path: format!("/srv/{TOKEN}/app"),
            });
            if task.kind == StepKind::Upload {
                ctx.detail(TaskDetail::Upload(UploadDetail {
                    committed: vec![format!("/srv/{TOKEN}/app")],
                    ..Default::default()
                }));
            }
            if task.seq == 1 {
                let server = task.server.as_ref().expect("upload has a server");
                let prompt = CredentialPrompt {
                    purpose: CredentialPurpose::SshPassword,
                    server_name: server.name.clone(),
                    address: format!("{}:22", server.host),
                    user: "deploy".into(),
                    key_path: None,
                    context: Message::TaskContext {
                        n: task.seq + 1,
                        step: task.step_name.clone(),
                    },
                    attempt: 1,
                    max_attempts: 3,
                    rejected: false,
                    saved_rejected: false,
                    remember_default: false,
                };
                let Some(OperatorAnswer::Secret { value, remember }) =
                    ctx.ask(OperatorPrompt::Credential(prompt)).await
                else {
                    return TaskOutcome::Cancelled;
                };
                ctx.output(chunk(&format!("login with {}\n", value.expose_secret())));
                assert!(remember);
                let credential = CredentialRef {
                    server_id: server.id,
                    user: "deploy".into(),
                    purpose: CredentialPurpose::SshPassword,
                };
                self.secrets
                    .lock()
                    .unwrap()
                    .remember(&credential, &value, 1)
                    .unwrap();
            }
            if task.seq == self.last {
                return TaskOutcome::Failed {
                    exit_code: Some(1),
                    exit_signal: None,
                    reason: Message::ExitCode { code: 1 }
                        .with_detail(format!("curl: (22) {TOKEN} rejected")),
                };
            }
            TaskOutcome::Succeeded { exit_code: Some(0) }
        })
    }
}

/// Answers D04 with the password and remember ticked.
struct View(mpsc::UnboundedSender<u64>);

impl RunView for View {
    fn state(&self, _machine: &RunMachine) {}
    fn logs(&self, _entries: Vec<LogEntry>) {}
    fn requested(&self, request: &OperatorRequest) {
        let _ = self.0.send(request.id);
    }
    fn storage_failed(&self, error: &StorageError) {
        panic!("storage failed: {error:?}");
    }
    fn progress(&self, _seq: u32, _progress: UploadProgress) {}
}

struct TickClock(AtomicI64);

impl Clock for TickClock {
    fn now_ms(&self) -> i64 {
        self.0.fetch_add(1, Ordering::Relaxed)
    }
}

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files_under(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn utf16le(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[tokio::test]
async fn secrets_never_reach_disk_or_exports() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = DataDir::at(tmp.path().join("MyDeployMate"));
    dir.ensure().unwrap();
    let exports = tmp.path().join("exports");
    std::fs::create_dir_all(&exports).unwrap();

    // A sensitive variable used by the remote command; its value lives in the vault.
    let demo = demo_workspace();
    let mut ws = demo.workspace;
    let token = Variable {
        id: VariableId::new(),
        name: "api_token".into(),
        value: VariableValue::SecretRef,
        sensitive: true,
    };
    let token_id = token.id;
    let project = &mut ws.projects[0];
    project.variables.push(token);
    let StepSpec::Remote(spec) = &mut project.pipelines[0].steps[3].spec else {
        panic!("step 4 of the demo is the health check");
    };
    spec.command = RemoteCommand::Script {
        script: "curl --fail -H \"Authorization: Bearer ${api_token}\" http://127.0.0.1/health"
            .into(),
    };
    let request = PlanRequest {
        project_id: project.id,
        pipeline_id: project.pipelines[0].id,
        environment_id: demo.staging,
        scope: RunScope::Full,
        server_ids: None,
        version: None,
    };

    let mut store = WorkspaceStore::new(dir.clone());
    ws.revision = store.load().unwrap().0.revision;
    let ws = store.save(ws).unwrap();
    let vault: Arc<dyn SecretVault> = Arc::new(MemoryVault::default());
    let secrets = SecretStore::open(&dir, vault).unwrap();
    secrets
        .set_variable(token_id, None, &SecretString::from(TOKEN))
        .unwrap();

    let plan = build_plan(&ws, &request, &Values(&secrets)).unwrap();
    assert!(plan.can_start(), "{:#?}", plan.issues);
    assert_eq!(plan.secrets.0, [TOKEN]);
    let last = plan.tasks.len() as u32 - 1;

    // The run, wired as src-tauri does it.
    let run_id = RunId::new();
    let history = SharedHistory::new(HistoryStore::open(&dir.history_db()).unwrap());
    let clock: Arc<dyn Clock> = Arc::new(TickClock(AtomicI64::new(1_000)));
    let (control, control_rx) = mpsc::unbounded_channel();
    let (requests, mut requested) = mpsc::unbounded_channel();
    let pipeline = Arc::new(LogPipeline::new(
        plan.secrets.0.clone(),
        Box::new(RunLogFile::open(&dir, run_id).unwrap()),
        LogBudget::default(),
        Arc::new(View(requests)),
        Arc::clone(&clock),
        control.clone(),
    ));
    let secrets = Arc::new(Mutex::new(secrets));
    let executor = Arc::new(LeakyExecutor {
        secrets: Arc::clone(&secrets),
        last,
    });
    let coordinator = Coordinator::create(
        Arc::new(plan),
        run_id,
        history.clone(),
        clock,
        executor,
        pipeline.clone(),
        control_rx,
    )
    .map_err(|(e, _)| e)
    .unwrap();
    let done = tokio::spawn(coordinator.run());
    let id = tokio::time::timeout(Duration::from_secs(5), requested.recv())
        .await
        .unwrap()
        .unwrap();
    control
        .send(Control::Answer {
            request: id,
            answer: Some(OperatorAnswer::Secret {
                value: SecretString::from(PASSWORD),
                remember: true,
            }),
        })
        .unwrap();
    let (status, _) = tokio::time::timeout(Duration::from_secs(5), done)
        .await
        .unwrap()
        .unwrap();
    pipeline.flush();
    assert_eq!(status, RunStatus::Failed);
    assert_eq!(secrets.lock().unwrap().list().len(), 1);

    // What was recorded is still useful: masked, not dropped.
    let stored = history.lock().load_run(run_id).unwrap().unwrap();
    let failed = stored.tasks.last().unwrap();
    assert_eq!(
        failed.reason,
        Some(Message::ExitCode { code: 1 }.with_detail("curl: (22) ****** rejected"))
    );
    let log = read_run_log(&dir, run_id).unwrap();
    let text: String = log.entries.iter().map(|e| e.text.as_str()).collect();
    assert!(text.contains("token=******"), "{text}");
    assert!(text.contains("login with ******"), "{text}");

    // Exports, with and without local paths, and the backup an import takes.
    let full = export_json(&ws, true);
    let lean = export_json(&ws, false);
    std::fs::write(exports.join("full.json"), &full).unwrap();
    std::fs::write(exports.join("lean.json"), &lean).unwrap();
    let (imported, _) = parse_import(&full).unwrap();
    store.replace(imported, ws.revision).unwrap();
    drop(history);

    let mut files = Vec::new();
    files_under(tmp.path(), &mut files);
    let names: Vec<String> = files
        .iter()
        .map(|f| f.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    for expected in ["workspace.json", "history.sqlite", "credentials.json"] {
        assert!(names.iter().any(|n| n == expected), "{names:?}");
    }
    assert!(names.iter().any(|n| n.ends_with(".jsonl")), "{names:?}");
    assert!(
        names
            .iter()
            .any(|n| n.starts_with("workspace-before-import-")),
        "{names:?}"
    );

    let mut leaks = Vec::new();
    for file in &files {
        let bytes = std::fs::read(file).unwrap();
        for secret in [TOKEN, PASSWORD] {
            for (encoding, needle) in [
                ("UTF-8", secret.as_bytes().to_vec()),
                ("UTF-16LE", utf16le(secret)),
            ] {
                if contains(&bytes, &needle) {
                    leaks.push(format!(
                        "{} contains {secret} as {encoding}",
                        file.display()
                    ));
                }
            }
        }
    }
    assert!(leaks.is_empty(), "{leaks:#?}");
}
