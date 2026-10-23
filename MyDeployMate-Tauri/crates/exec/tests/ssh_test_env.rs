//! SSH connection layer against the Docker targets in `test-env` (architecture ARC-37).
//!
//! Skipped unless `MDM_TEST_ENV_KEYS` points at `test-env/keys` with both containers running;
//! `test-env/run-ssh-tests.sh` sets everything up. The agent test also needs `SSH_AUTH_SOCK`
//! with `id_ed25519` loaded, and the password-variable test needs `MDM_TEST_PASSWORD`.

#![allow(clippy::result_large_err)]

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use mdm_core::{
    AgentKind, AuthMethod, ConnectError, ConnectErrorKind, ConnectStage, CredentialPurpose,
    HostEndpoint, HostKey, Message, PasswordSource,
};
use mdm_exec::ssh::{SecretFuture, SecretPrompt, SecretSource, SshTarget, connect, fetch_host_key};
use secrecy::SecretString;
use tokio_util::sync::CancellationToken;

const MODERN: u16 = 2222;
const LEGACY: u16 = 2223;

fn keys() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("MDM_TEST_ENV_KEYS")?);
    Some(dir)
}

macro_rules! require_test_env {
    () => {
        match keys() {
            Some(k) => k,
            None => {
                eprintln!("skipped: MDM_TEST_ENV_KEYS not set");
                return;
            }
        }
    };
}

fn key_target(port: u16, user: &str, path: PathBuf) -> SshTarget {
    target(
        port,
        user,
        AuthMethod::KeyFile {
            path: path.to_string_lossy().into_owned(),
        },
    )
}

fn target(port: u16, user: &str, auth: AuthMethod) -> SshTarget {
    SshTarget {
        endpoint: HostEndpoint::new("127.0.0.1", port),
        user: user.into(),
        auth,
        allow_legacy_rsa: false,
        connect_timeout: Duration::from_secs(5),
    }
}

/// Answers prompts from a list and records what was asked.
#[derive(Default)]
struct Answers {
    values: Mutex<Vec<Option<&'static str>>>,
    asked: Mutex<Vec<SecretPrompt>>,
    accepted: Mutex<Vec<CredentialPurpose>>,
}

impl Answers {
    fn new(values: &[Option<&'static str>]) -> Self {
        Self {
            values: Mutex::new(values.iter().rev().cloned().collect()),
            ..Default::default()
        }
    }

    fn asked(&self) -> Vec<SecretPrompt> {
        self.asked.lock().unwrap().clone()
    }
}

impl SecretSource for Answers {
    fn secret<'a>(&'a self, prompt: &'a SecretPrompt) -> SecretFuture<'a> {
        self.asked.lock().unwrap().push(prompt.clone());
        let next = self.values.lock().unwrap().pop().flatten();
        Box::pin(async move { next.map(|v| SecretString::from(v.to_owned())) })
    }

    fn accepted(&self, purpose: &CredentialPurpose) {
        self.accepted.lock().unwrap().push(purpose.clone());
    }
}

async fn try_connect(
    target: &SshTarget,
    known: &[HostKey],
    secrets: &Answers,
) -> Result<mdm_exec::ssh::SshSession, ConnectError> {
    connect(target, known, secrets, &CancellationToken::new()).await
}

/// The host key of a test server, learned the way the app learns it: from the first-trust error.
async fn host_key(port: u16, keys: &std::path::Path) -> HostKey {
    let mut t = key_target(port, "deploy", keys.join("id_rsa"));
    t.allow_legacy_rsa = true;
    match try_connect(&t, &[], &Answers::default()).await {
        Err(ConnectError {
            kind:
                ConnectErrorKind::HostKeyUnknown {
                    presented,
                    endpoint,
                },
            stage: ConnectStage::HostKey,
            ..
        }) => {
            assert_eq!(endpoint, t.endpoint);
            presented
        }
        other => panic!("expected first-trust error, got {:?}", other.err()),
    }
}

#[tokio::test]
async fn first_trust_then_key_login_and_exit_status() {
    let keys = require_test_env!();
    let key = host_key(MODERN, &keys).await;
    assert!(key.fingerprint.starts_with("SHA256:"), "{key:?}");
    assert_eq!(key.algorithm, "ssh-ed25519");

    let t = key_target(MODERN, "deploy", keys.join("id_ed25519"));
    let session = try_connect(&t, &[key], &Answers::default()).await.unwrap();
    let out = session
        .exec_collect("echo out; echo err >&2; exit 3", 1024)
        .await
        .unwrap();
    assert_eq!(out.stdout, b"out\n");
    assert_eq!(out.stderr, b"err\n");
    assert_eq!(out.exit_status, Some(3));
    let system = session.probe(Duration::from_secs(5)).await.unwrap();
    assert!(system.starts_with("Linux "), "{system}");
    let killed = session.exec_collect("kill -9 $$", 1024).await.unwrap();
    assert_eq!(killed.exit_status, None);
    assert_eq!(killed.exit_signal.as_deref(), Some("KILL"));
    session.close().await;
}

#[tokio::test]
async fn changed_key_is_blocked_before_login() {
    let keys = require_test_env!();
    let real = host_key(MODERN, &keys).await;
    let recorded = HostKey {
        algorithm: "ssh-ed25519".into(),
        fingerprint: "SHA256:recorded-earlier".into(),
    };
    // A password prompt would mean a secret was about to be sent: there must be none.
    let answers = Answers::new(&[Some("deploy-pass")]);
    let t = target(
        MODERN,
        "deploy",
        AuthMethod::Password {
            source: PasswordSource::Prompt,
        },
    );
    let err = try_connect(&t, std::slice::from_ref(&recorded), &answers)
        .await
        .err()
        .unwrap();
    assert_eq!(
        err.kind,
        ConnectErrorKind::HostKeyChanged {
            endpoint: t.endpoint.clone(),
            known: vec![recorded],
            presented: real,
        }
    );
    assert!(answers.asked().is_empty());
}

#[tokio::test]
async fn encrypted_key_retries_passphrase() {
    let keys = require_test_env!();
    let known = [host_key(MODERN, &keys).await];
    let t = key_target(MODERN, "deploy", keys.join("id_ed25519_enc"));
    let answers = Answers::new(&[Some("wrong"), Some("test-passphrase")]);
    try_connect(&t, &known, &answers).await.unwrap();
    let asked = answers.asked();
    assert_eq!(asked.len(), 2);
    assert_eq!(asked[0].purpose, CredentialPurpose::KeyPassphrase);
    assert_eq!(
        (asked[0].attempt, asked[0].rejected),
        (1, false),
        "{asked:?}"
    );
    assert_eq!((asked[1].attempt, asked[1].rejected), (2, true));
    assert!(
        asked[1]
            .key_path
            .as_deref()
            .unwrap()
            .ends_with("id_ed25519_enc")
    );
    assert_eq!(
        *answers.accepted.lock().unwrap(),
        vec![CredentialPurpose::KeyPassphrase]
    );

    let wrong = Answers::new(&[Some("a"), Some("b"), Some("c"), Some("test-passphrase")]);
    let err = try_connect(&t, &known, &wrong).await.err().unwrap();
    assert_eq!(err.kind, ConnectErrorKind::KeyPassphraseWrong);
    assert_eq!(wrong.asked().len(), 3);
}

#[tokio::test]
async fn putty_keys_v2_and_encrypted_v3() {
    let keys = require_test_env!();
    let known = [host_key(MODERN, &keys).await];
    let v2 = key_target(MODERN, "deploy", keys.join("id_rsa_v2.ppk"));
    try_connect(&v2, &known, &Answers::default()).await.unwrap();
    let v3 = key_target(MODERN, "deploy", keys.join("id_ed25519_v3_enc.ppk"));
    let answers = Answers::new(&[Some("test-passphrase")]);
    try_connect(&v3, &known, &answers).await.unwrap();
    assert_eq!(answers.asked().len(), 1);
}

#[tokio::test]
async fn password_prompt_retries_then_fails_after_three() {
    let keys = require_test_env!();
    let known = [host_key(MODERN, &keys).await];
    let t = target(
        MODERN,
        "deploy",
        AuthMethod::Password {
            source: PasswordSource::Prompt,
        },
    );
    let ok = Answers::new(&[Some("nope"), Some("deploy-pass")]);
    try_connect(&t, &known, &ok).await.unwrap();
    assert_eq!(ok.asked().len(), 2);
    assert_eq!(ok.asked()[0].purpose, CredentialPurpose::SshPassword);
    assert_eq!(
        *ok.accepted.lock().unwrap(),
        vec![CredentialPurpose::SshPassword]
    );

    let bad = Answers::new(&[Some("a"), Some("b"), Some("c")]);
    let err = try_connect(&t, &known, &bad).await.err().unwrap();
    assert_eq!(err.kind, ConnectErrorKind::AuthenticationFailed);
    assert_eq!(err.stage, ConnectStage::Authenticate);
    assert_eq!(bad.asked().len(), 3);
}

#[tokio::test]
async fn cancelling_the_prompt_cancels_the_connection() {
    let keys = require_test_env!();
    let known = [host_key(MODERN, &keys).await];
    let t = target(
        MODERN,
        "deploy",
        AuthMethod::Password {
            source: PasswordSource::Prompt,
        },
    );
    let err = try_connect(&t, &known, &Answers::new(&[None]))
        .await
        .err()
        .unwrap();
    assert_eq!(err.kind, ConnectErrorKind::Cancelled);
}

#[tokio::test]
async fn password_from_environment_variable() {
    let _keys = require_test_env!();
    let known = [host_key(MODERN, &_keys).await];
    if std::env::var_os("MDM_TEST_PASSWORD").is_none() {
        eprintln!("skipped: MDM_TEST_PASSWORD not set");
        return;
    }
    let t = target(
        MODERN,
        "deploy",
        AuthMethod::Password {
            source: PasswordSource::EnvVar {
                name: "MDM_TEST_PASSWORD".into(),
            },
        },
    );
    try_connect(&t, &known, &Answers::default()).await.unwrap();
}

#[tokio::test]
async fn openssh_agent() {
    let keys = require_test_env!();
    if std::env::var_os("SSH_AUTH_SOCK").is_none() {
        eprintln!("skipped: SSH_AUTH_SOCK not set");
        return;
    }
    let known = [host_key(MODERN, &keys).await];
    let t = target(
        MODERN,
        "nopass",
        AuthMethod::Agent {
            agent: AgentKind::Openssh,
        },
    );
    try_connect(&t, &known, &Answers::default()).await.unwrap();
}

#[tokio::test]
async fn legacy_rsa_needs_the_server_toggle() {
    let keys = require_test_env!();
    let mut t = key_target(LEGACY, "deploy", keys.join("id_rsa"));
    let err = try_connect(&t, &[], &Answers::default())
        .await
        .err()
        .unwrap();
    assert_eq!(
        err.kind,
        ConnectErrorKind::AlgorithmNegotiation {
            legacy_rsa_only: true
        },
        "{:?}",
        err.message
    );
    assert_eq!(err.stage, ConnectStage::Handshake);

    let known = [host_key(LEGACY, &keys).await];
    assert_eq!(known[0].algorithm, "ssh-rsa");
    t.allow_legacy_rsa = true;
    let session = try_connect(&t, &known, &Answers::default()).await.unwrap();
    let out = session.exec_collect("id -un", 64).await.unwrap();
    assert_eq!(out.stdout, b"deploy\n");
}

#[tokio::test]
async fn network_failures_are_classified() {
    let keys = require_test_env!();
    let auth = AuthMethod::KeyFile {
        path: keys.join("id_ed25519").to_string_lossy().into_owned(),
    };

    // Nothing listens on port 1.
    let refused = target(1, "deploy", auth.clone());
    let err = try_connect(&refused, &[], &Answers::default())
        .await
        .err()
        .unwrap();
    assert_eq!(err.kind, ConnectErrorKind::ConnectionRefused);
    assert_eq!(err.stage, ConnectStage::Connect);

    let mut unresolvable = target(22, "deploy", auth.clone());
    unresolvable.endpoint = HostEndpoint::new("no-such-host.invalid", 22);
    let err = try_connect(&unresolvable, &[], &Answers::default())
        .await
        .err()
        .unwrap();
    assert_eq!(err.kind, ConnectErrorKind::ResolveFailed);

    // Accepts TCP but never speaks SSH.
    let silent = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut slow = target(silent.local_addr().unwrap().port(), "deploy", auth);
    slow.connect_timeout = Duration::from_millis(500);
    let err = try_connect(&slow, &[], &Answers::default())
        .await
        .err()
        .unwrap();
    assert_eq!(err.kind, ConnectErrorKind::Timeout);
    assert_eq!(err.stage, ConnectStage::Handshake);
    drop(silent);
}

#[tokio::test]
async fn prepare_errors_come_before_the_network() {
    let _keys = require_test_env!();
    let dir = tempfile::tempdir().unwrap();
    let junk = dir.path().join("junk.key");
    std::fs::write(&junk, "not a key").unwrap();
    // Port 1 refuses connections: these errors must come first.
    let err = try_connect(
        &key_target(1, "deploy", junk.clone()),
        &[],
        &Answers::default(),
    )
    .await
    .err()
    .unwrap();
    assert_eq!(
        err.kind,
        ConnectErrorKind::KeyFormatUnsupported {
            path: junk.to_string_lossy().into_owned()
        }
    );
    let missing = dir.path().join("missing");
    let err = try_connect(&key_target(1, "deploy", missing), &[], &Answers::default())
        .await
        .err()
        .unwrap();
    assert!(matches!(
        err.kind,
        ConnectErrorKind::KeyFileUnreadable { .. }
    ));
    let env = target(
        1,
        "deploy",
        AuthMethod::Password {
            source: PasswordSource::EnvVar {
                name: "MDM_TEST_SURELY_UNSET".into(),
            },
        },
    );
    let err = try_connect(&env, &[], &Answers::default())
        .await
        .err()
        .unwrap();
    assert_eq!(err.stage, ConnectStage::Prepare);
    assert!(
        matches!(err.message, Message::PasswordEnvMissing { .. }),
        "{:?}",
        err.message
    );
}

#[tokio::test]
async fn cancel_token_stops_a_pending_prompt() {
    let keys = require_test_env!();
    let known = [host_key(MODERN, &keys).await];

    struct Never;
    impl SecretSource for Never {
        fn secret<'a>(&'a self, _: &'a SecretPrompt) -> SecretFuture<'a> {
            Box::pin(std::future::pending())
        }
    }
    let t = target(
        MODERN,
        "deploy",
        AuthMethod::Password {
            source: PasswordSource::Prompt,
        },
    );
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        trigger.cancel();
    });
    let err = connect(&t, &known, &Never, &cancel).await.err().unwrap();
    assert_eq!(err.kind, ConnectErrorKind::Cancelled);
}

#[tokio::test]
async fn fetch_host_key_reads_the_key_without_logging_in() {
    let keys = require_test_env!();
    let expected = host_key(MODERN, &keys).await;
    let fetched = fetch_host_key(
        &HostEndpoint::new("127.0.0.1", MODERN),
        Duration::from_secs(5),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(fetched, expected);
    let legacy = fetch_host_key(
        &HostEndpoint::new("127.0.0.1", LEGACY),
        Duration::from_secs(5),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(legacy.algorithm, "ssh-rsa");
}

// ---- Remote commands (ARC-22, ARC-23) -------------------------------------------------------

mod remote {
    use super::*;
    use mdm_core::ResolvedText;
    use mdm_core::plan::ResolvedCommand;
    use mdm_exec::OutputChunk;
    use mdm_exec::remote::{RemoteEnd, RemoteOutcome, Stop, run_remote, wrap_command};

    async fn session(keys: &std::path::Path) -> mdm_exec::ssh::SshSession {
        let key = host_key(MODERN, keys).await;
        let t = key_target(MODERN, "deploy", keys.join("id_ed25519"));
        try_connect(&t, &[key], &Answers::default()).await.unwrap()
    }

    fn script(s: &str) -> ResolvedCommand {
        ResolvedCommand::Script {
            script: ResolvedText::plain(s),
        }
    }

    async fn run(
        session: &mdm_exec::ssh::SshSession,
        cmd: &ResolvedCommand,
        cwd: Option<&str>,
        timeout: Option<Duration>,
        cancel: &CancellationToken,
    ) -> (RemoteOutcome, String, String) {
        let mut out = String::new();
        let mut err = String::new();
        let outcome = run_remote(
            session,
            &wrap_command(cmd, cwd),
            timeout,
            cancel,
            |c: OutputChunk| match c.stream {
                mdm_exec::Stream::Stdout => out.push_str(&c.text),
                mdm_exec::Stream::Stderr => err.push_str(&c.text),
            },
        )
        .await
        .unwrap();
        (outcome, out, err)
    }

    #[tokio::test]
    async fn exit_codes_cwd_and_quoting() {
        let keys = require_test_env!();
        let s = session(&keys).await;
        let none = CancellationToken::new();

        let (o, out, err) = run(
            &s,
            &script("echo \"it's $(pwd)\"; echo warn >&2; exit 7"),
            Some("/tmp"),
            None,
            &none,
        )
        .await;
        assert_eq!(o.end, RemoteEnd::Exited(7));
        assert!(o.pgid.is_some());
        assert_eq!(out, "it's /tmp\n");
        assert_eq!(err, "warn\n", "the process group marker is not shown");

        // `-w` passes the exit status through even when setsid has to fork.
        let (o, _, _) = run(&s, &script("true"), None, None, &none).await;
        assert_eq!(o.end, RemoteEnd::Exited(0));

        let program = ResolvedCommand::Program {
            executable: ResolvedText::plain("printf"),
            args: vec![ResolvedText::plain("[%s]"), ResolvedText::plain("a b'c")],
        };
        let (o, out, _) = run(&s, &program, None, None, &none).await;
        assert_eq!(o.end, RemoteEnd::Exited(0));
        assert_eq!(out, "[a b'c]");

        let (o, _, err) = run(&s, &script("true"), Some("/no/such/dir"), None, &none).await;
        assert_eq!(o.end, RemoteEnd::Exited(1));
        assert!(err.contains("/no/such/dir"), "{err}");
    }

    #[tokio::test]
    async fn killed_by_a_signal_is_not_success() {
        let keys = require_test_env!();
        let s = session(&keys).await;
        let (o, _, _) = run(
            &s,
            &script("kill -KILL $$"),
            None,
            None,
            &CancellationToken::new(),
        )
        .await;
        // Reported as a shell does: 128 + SIGKILL.
        assert_eq!(o.end, RemoteEnd::Exited(137));
    }

    #[tokio::test]
    async fn cancel_kills_the_whole_process_group() {
        let keys = require_test_env!();
        let s = session(&keys).await;
        let cancel = CancellationToken::new();
        let marker = format!("mdm-cancel-{}", std::process::id());
        // A background child in the same group must stop too.
        let cmd = script(&format!(
            "sleep 300 & echo started; exec sleep 301 # {marker}"
        ));
        let stopper = {
            let cancel = cancel.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(1)).await;
                cancel.cancel();
            })
        };
        let (o, out, _) = run(&s, &cmd, None, None, &cancel).await;
        stopper.await.unwrap();
        assert_eq!(out, "started\n");
        assert_eq!(o.end, RemoteEnd::Cancelled(Stop::Confirmed));
        let left = s
            .exec_collect("ps -eo args | grep -c '[s]leep 30[01]' || true", 1024)
            .await
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&left.stdout).trim(), "0");
    }

    #[tokio::test]
    async fn timeout_stops_the_command() {
        let keys = require_test_env!();
        let s = session(&keys).await;
        let (o, _, _) = run(
            &s,
            &script("sleep 302"),
            None,
            Some(Duration::from_secs(1)),
            &CancellationToken::new(),
        )
        .await;
        assert_eq!(o.end, RemoteEnd::TimedOut(Stop::Confirmed));
    }

    #[tokio::test]
    async fn a_term_ignoring_command_is_killed() {
        let keys = require_test_env!();
        let s = session(&keys).await;
        let cancel = CancellationToken::new();
        let c = cancel.clone();
        let stopper = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            c.cancel();
        });
        let (o, _, _) = run(
            &s,
            &script("trap '' TERM; while :; do sleep 1; done"),
            None,
            None,
            &cancel,
        )
        .await;
        stopper.await.unwrap();
        assert_eq!(o.end, RemoteEnd::Cancelled(Stop::Confirmed));
    }
}

// ---- SFTP upload (ARC-28–30, AT-10) ---------------------------------------------------------

mod upload {
    use super::*;
    use std::path::Path;

    use mdm_core::{ConflictPolicy, Message, SourceKind};
    use mdm_exec::ssh::SshSession;
    use mdm_exec::upload::{Sftp, UploadEnd, UploadJob, UploadReport, scan, upload};

    async fn session(keys: &Path) -> SshSession {
        let key = host_key(MODERN, keys).await;
        let t = key_target(MODERN, "deploy", keys.join("id_ed25519"));
        try_connect(&t, &[key], &Answers::default()).await.unwrap()
    }

    /// A fresh remote directory for one test, removed first in case an earlier run left it.
    async fn remote_dir(s: &SshSession, name: &str) -> String {
        let dir = format!("/home/deploy/mdm-upload-{name}");
        sh(s, &format!("chmod -R u+w {dir} 2>/dev/null; rm -rf {dir}")).await;
        dir
    }

    async fn sh(s: &SshSession, cmd: &str) -> String {
        let out = s.exec_collect(cmd, 64 * 1024).await.unwrap();
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    async fn put(
        sftp: &Sftp,
        source: &Path,
        kind: SourceKind,
        target: &str,
        conflict: ConflictPolicy,
    ) -> UploadReport {
        let manifest = scan(source, kind).unwrap();
        let job = UploadJob {
            source,
            manifest: &manifest,
            target,
            conflict,
            token: "t1",
        };
        upload(sftp, &job, &CancellationToken::new(), |_| {}).await
    }

    #[tokio::test]
    async fn file_conflict_policies() {
        let keys = require_test_env!();
        let s = session(&keys).await;
        let sftp = Sftp::open(&s).await.unwrap();
        assert!(sftp.supports_posix_rename());
        let dir = remote_dir(&s, "file").await;
        let local = tempfile::tempdir().unwrap();
        let src = local.path().join("app.jar");
        std::fs::write(&src, "v1").unwrap();

        // Missing parent directories are created.
        let target = format!("{dir}/a/b/app.jar");
        let r = put(
            &sftp,
            &src,
            SourceKind::File,
            &target,
            ConflictPolicy::Error,
        )
        .await;
        assert_eq!(r.end, UploadEnd::Completed, "{r:?}");
        assert_eq!(r.committed, vec![target.clone()]);
        assert_eq!(sh(&s, &format!("cat {target}")).await, "v1");

        // Policy "error" leaves an existing file alone (AC-13).
        std::fs::write(&src, "v2").unwrap();
        let r = put(
            &sftp,
            &src,
            SourceKind::File,
            &target,
            ConflictPolicy::Error,
        )
        .await;
        assert_eq!(
            r.end,
            UploadEnd::Failed(Message::TargetExists {
                path: target.clone()
            })
        );
        assert_eq!(sh(&s, &format!("cat {target}")).await, "v1");

        // Overwrite without posix-rename fails and keeps the original (AC-13).
        let plain = Sftp::open(&s).await.unwrap().without_posix_rename();
        let r = put(
            &plain,
            &src,
            SourceKind::File,
            &target,
            ConflictPolicy::Overwrite,
        )
        .await;
        assert_eq!(r.end, UploadEnd::Failed(Message::NoPosixRename));
        assert_eq!(sh(&s, &format!("cat {target}")).await, "v1");

        let r = put(
            &sftp,
            &src,
            SourceKind::File,
            &target,
            ConflictPolicy::Overwrite,
        )
        .await;
        assert_eq!(r.end, UploadEnd::Completed, "{r:?}");
        assert_eq!(sh(&s, &format!("cat {target}")).await, "v2");

        // A directory as the target of a file upload.
        let r = put(
            &sftp,
            &src,
            SourceKind::File,
            &dir,
            ConflictPolicy::Overwrite,
        )
        .await;
        assert_eq!(
            r.end,
            UploadEnd::Failed(Message::TargetIsDir { path: dir.clone() })
        );
        assert_eq!(sh(&s, &format!("find {dir} -name '*.mdm-*'")).await, "");
    }

    #[tokio::test]
    async fn directory_keeps_hidden_files_and_extra_remote_files() {
        let keys = require_test_env!();
        let s = session(&keys).await;
        let sftp = Sftp::open(&s).await.unwrap();
        let dir = remote_dir(&s, "dir").await;
        sh(
            &s,
            &format!("mkdir -p {dir} && echo keep > {dir}/extra.txt"),
        )
        .await;
        let local = tempfile::tempdir().unwrap();
        let root = local.path();
        std::fs::create_dir_all(root.join("css/empty")).unwrap();
        std::fs::write(root.join(".env"), "secret=1").unwrap();
        std::fs::write(root.join("css/site.css"), "body{}").unwrap();
        // Larger than the in-flight window, so writes overlap.
        let big: Vec<u8> = (0..2_000_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(root.join("big.bin"), &big).unwrap();

        let manifest = scan(root, SourceKind::Directory).unwrap();
        let job = UploadJob {
            source: root,
            manifest: &manifest,
            target: &dir,
            conflict: ConflictPolicy::Error,
            token: "t2",
        };
        let mut last = None;
        let r = upload(&sftp, &job, &CancellationToken::new(), |p| last = Some(p)).await;
        assert_eq!(r.end, UploadEnd::Completed, "{r:?}");
        assert_eq!(r.committed.len(), 3);
        let last = last.unwrap();
        assert_eq!((last.files_done, last.files_total), (3, 3));
        assert_eq!(last.bytes_done, last.bytes_total);

        let listing = sh(&s, &format!("cd {dir} && find . | sort")).await;
        assert_eq!(
            listing,
            ".\n./.env\n./big.bin\n./css\n./css/empty\n./css/site.css\n./extra.txt\n"
        );
        let sum = sh(&s, &format!("sha256sum {dir}/big.bin | cut -d' ' -f1")).await;
        let want = manifest.files.iter().find(|f| f.path == "big.bin").unwrap();
        assert_eq!(sum.trim(), want.sha256);
    }

    #[tokio::test]
    async fn stops_at_the_first_failure_and_lists_the_rest() {
        let keys = require_test_env!();
        let s = session(&keys).await;
        let sftp = Sftp::open(&s).await.unwrap();
        let dir = remote_dir(&s, "partial").await;
        sh(
            &s,
            &format!("mkdir -p {dir}/locked && chmod 555 {dir}/locked"),
        )
        .await;
        let local = tempfile::tempdir().unwrap();
        let root = local.path();
        std::fs::create_dir(root.join("locked")).unwrap();
        for f in ["a", "locked/x", "z"] {
            std::fs::write(root.join(f), f).unwrap();
        }
        let r = put(
            &sftp,
            root,
            SourceKind::Directory,
            &dir,
            ConflictPolicy::Error,
        )
        .await;
        assert_eq!(
            r.end,
            UploadEnd::Failed(Message::PermissionDenied {
                path: format!("{dir}/locked/x")
            })
        );
        assert_eq!(r.committed, vec![format!("{dir}/a")]);
        assert_eq!(r.stopped_at, Some(format!("{dir}/locked/x")));
        assert_eq!(r.unprocessed, vec![format!("{dir}/z")]);
        assert!(r.temp_left.is_empty());
    }

    #[tokio::test]
    async fn no_sudo_for_protected_directories() {
        let keys = require_test_env!();
        let s = session(&keys).await;
        let sftp = Sftp::open(&s).await.unwrap();
        let local = tempfile::tempdir().unwrap();
        let src = local.path().join("f");
        std::fs::write(&src, "x").unwrap();
        let r = put(
            &sftp,
            &src,
            SourceKind::File,
            "/opt/protected/sub/f",
            ConflictPolicy::Error,
        )
        .await;
        assert_eq!(
            r.end,
            UploadEnd::Failed(Message::PermissionDenied {
                path: "/opt/protected/sub".into()
            })
        );
        assert!(r.committed.is_empty());
    }

    #[tokio::test]
    async fn content_changed_after_scan_is_not_committed() {
        let keys = require_test_env!();
        let s = session(&keys).await;
        let sftp = Sftp::open(&s).await.unwrap();
        let dir = remote_dir(&s, "changed").await;
        let local = tempfile::tempdir().unwrap();
        let src = local.path().join("app.jar");
        std::fs::write(&src, "built").unwrap();
        let manifest = scan(&src, SourceKind::File).unwrap();
        std::fs::write(&src, "rebuilt").unwrap();
        let target = format!("{dir}/app.jar");
        let job = UploadJob {
            source: &src,
            manifest: &manifest,
            target: &target,
            conflict: ConflictPolicy::Error,
            token: "t3",
        };
        let r = upload(&sftp, &job, &CancellationToken::new(), |_| {}).await;
        assert!(
            matches!(r.end, UploadEnd::Failed(Message::ContentChanged { .. })),
            "{r:?}"
        );
        assert_eq!(sh(&s, &format!("ls -A {dir}")).await, "");
    }

    #[tokio::test]
    async fn cancel_before_start_uploads_nothing() {
        let keys = require_test_env!();
        let s = session(&keys).await;
        let sftp = Sftp::open(&s).await.unwrap();
        let dir = remote_dir(&s, "cancel").await;
        let local = tempfile::tempdir().unwrap();
        let src = local.path().join("f");
        std::fs::write(&src, "x").unwrap();
        let manifest = scan(&src, SourceKind::File).unwrap();
        let target = format!("{dir}/f");
        let job = UploadJob {
            source: &src,
            manifest: &manifest,
            target: &target,
            conflict: ConflictPolicy::Error,
            token: "t4",
        };
        let cancel = CancellationToken::new();
        cancel.cancel();
        let r = upload(&sftp, &job, &cancel, |_| {}).await;
        assert_eq!(r.end, UploadEnd::Cancelled);
        assert_eq!(r.unprocessed, vec![target]);
        assert_eq!(sh(&s, &format!("ls -A {dir}")).await, "");
    }
}

mod upload_cancel {
    use super::*;

    use mdm_core::{ConflictPolicy, SourceKind};
    use mdm_exec::upload::{Sftp, UploadEnd, UploadJob, scan, upload};

    #[tokio::test]
    async fn cancel_mid_file_removes_the_temporary_file() {
        let keys = require_test_env!();
        let key = host_key(MODERN, &keys).await;
        let t = key_target(MODERN, "deploy", keys.join("id_ed25519"));
        let s = try_connect(&t, &[key], &Answers::default()).await.unwrap();
        let sftp = Sftp::open(&s).await.unwrap();
        let dir = "/home/deploy/mdm-upload-cancel-mid";
        s.exec_collect(&format!("rm -rf {dir}"), 1024)
            .await
            .unwrap();
        let local = tempfile::tempdir().unwrap();
        let src = local.path().join("big.bin");
        std::fs::write(&src, vec![7u8; 8_000_000]).unwrap();
        let manifest = scan(&src, SourceKind::File).unwrap();
        let target = format!("{dir}/big.bin");
        let job = UploadJob {
            source: &src,
            manifest: &manifest,
            target: &target,
            conflict: ConflictPolicy::Error,
            token: "t5",
        };
        let cancel = CancellationToken::new();
        let r = upload(&sftp, &job, &cancel, |p| {
            if p.bytes_done > 0 {
                cancel.cancel();
            }
        })
        .await;
        assert_eq!(r.end, UploadEnd::Cancelled);
        assert!(r.committed.is_empty() && r.temp_left.is_empty(), "{r:?}");
        assert_eq!(r.unprocessed, vec![target]);
        let out = s.exec_collect(&format!("ls -A {dir}"), 1024).await.unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), "");
        assert_eq!(out.exit_status, Some(0));
    }
}

// ---- Upload tasks in a run (STEP-04, ARC-28; AC-11, AT-10) ------------------------------------

mod upload_run {
    use super::*;
    use std::sync::Arc;

    use mdm_core::fixtures::demo_workspace;
    use mdm_core::resolve::NoValues;
    use mdm_core::{
        ArtifactChange, ConflictPolicy, PipelineFailurePolicy, PlanRequest, PlannedTask,
        ResolvedPlan, ResolvedText, Run, RunId, RunMachine, RunScope, RunStatus, RunStore,
        ServerId, SourceKind, StepKind, StorageError, Task, TaskAction, TaskDetail, TaskStatus,
        build_plan,
    };
    use mdm_exec::{
        AppExecutor, ConnectFuture, Coordinator, OperatorRequest, OutputChunk, RemoteConnector,
        RunObserver, SystemClock, TaskContext, TaskExecutor, TaskFuture, TaskOutcome,
    };

    type Saved = Option<(Run, Vec<Task>)>;

    #[derive(Clone, Default)]
    struct MemStore(Arc<std::sync::Mutex<Saved>>);

    impl RunStore for MemStore {
        fn create_run(&mut self, m: &RunMachine, _: &ResolvedPlan) -> Result<(), StorageError> {
            self.save_run(m)
        }
        fn save_run(&mut self, m: &RunMachine) -> Result<(), StorageError> {
            *self.0.lock().unwrap() = Some((m.run().clone(), m.tasks().to_vec()));
            Ok(())
        }
    }

    #[derive(Default)]
    struct Notes(std::sync::Mutex<Vec<(u32, Message)>>);

    impl RunObserver for Notes {
        fn changed(&self, _: &RunMachine) {}
        fn output(&self, _: u32, _: OutputChunk) {}
        fn note(&self, seq: u32, message: Message) {
            self.0.lock().unwrap().push((seq, message));
        }
        fn requested(&self, _: &OperatorRequest) {}
        fn storage_failed(&self, _: &StorageError) {}
    }

    /// Every server is the modern test container, logged in as deploy with the plain key.
    struct TestEnv {
        keys: PathBuf,
        known: Vec<HostKey>,
    }

    impl RemoteConnector for TestEnv {
        fn connect<'a>(&'a self, _: ServerId, _: &'a TaskContext) -> ConnectFuture<'a> {
            Box::pin(async move {
                let t = key_target(MODERN, "deploy", self.keys.join("id_ed25519"));
                try_connect(&t, &self.known, &Answers::default())
                    .await
                    .map_err(|e| TaskOutcome::Failed {
                        exit_code: None,
                        exit_signal: None,
                        reason: e.message,
                    })
            })
        }
    }

    /// Runs `after_first` once task 0 has ended, e.g. to change the artifact.
    struct Hook {
        inner: AppExecutor,
        after_first: Box<dyn Fn() + Send + Sync>,
    }

    impl TaskExecutor for Hook {
        fn execute<'a>(&'a self, task: &'a PlannedTask, ctx: TaskContext) -> TaskFuture<'a> {
            Box::pin(async move {
                let out = self.inner.execute(task, ctx).await;
                if task.seq == 0 {
                    (self.after_first)();
                }
                out
            })
        }
    }

    /// A plan of upload tasks of one step, one per `(server, target)`, built on the demo plan.
    fn plan(source: &std::path::Path, kind: SourceKind, targets: &[&str]) -> ResolvedPlan {
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
        let mut plan = build_plan(&demo.workspace, &req, &NoValues).unwrap();
        let template = plan
            .tasks
            .iter()
            .find(|t| t.kind == StepKind::Upload)
            .unwrap()
            .clone();
        plan.tasks = targets
            .iter()
            .enumerate()
            .map(|(i, target)| {
                let mut t = template.clone();
                t.seq = i as u32;
                t.failure_policy = PipelineFailurePolicy::Stop;
                t.server.as_mut().unwrap().id = ServerId::new();
                t.action = TaskAction::Upload {
                    source: ResolvedText::plain(source.to_string_lossy()),
                    source_kind: kind,
                    target: ResolvedText::plain(*target),
                    conflict: ConflictPolicy::Overwrite,
                };
                t
            })
            .collect();
        plan
    }

    async fn run(
        keys: &std::path::Path,
        plan: ResolvedPlan,
        after_first: impl Fn() + Send + Sync + 'static,
    ) -> (RunStatus, Vec<Task>, Vec<(u32, Message)>) {
        let env = TestEnv {
            keys: keys.to_path_buf(),
            known: vec![host_key(MODERN, keys).await],
        };
        let exec = Hook {
            inner: AppExecutor::new(Arc::new(env)),
            after_first: Box::new(after_first),
        };
        let store = MemStore::default();
        let notes = Arc::new(Notes::default());
        let (_control, control_rx) = tokio::sync::mpsc::unbounded_channel();
        let coordinator = Coordinator::create(
            Arc::new(plan),
            RunId::new(),
            store.clone(),
            Arc::new(SystemClock),
            Arc::new(exec),
            notes.clone(),
            control_rx,
        )
        .map_err(|(e, _)| e)
        .unwrap();
        let (status, _) = coordinator.run().await;
        let (_, tasks) = store.0.lock().unwrap().clone().unwrap();
        let notes = notes.0.lock().unwrap().clone();
        (status, tasks, notes)
    }

    fn upload_detail(t: &Task) -> &mdm_core::UploadDetail {
        match t.detail.as_ref().expect("upload detail") {
            TaskDetail::Upload(d) => d,
        }
    }

    #[tokio::test]
    async fn later_target_fails_when_the_artifact_changed() {
        let keys = require_test_env!();
        let local = tempfile::tempdir().unwrap();
        let root = local.path().join("dist");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("app.js"), "v1").unwrap();
        std::fs::write(root.join("old.js"), "x").unwrap();
        let a = "/home/deploy/mdm-run-changed/a";
        let b = "/home/deploy/mdm-run-changed/b";
        let plan = plan(&root, SourceKind::Directory, &[a, b]);
        let changed = root.clone();
        let (status, tasks, notes) = run(&keys, plan, move || {
            std::fs::write(changed.join("app.js"), "v2").unwrap();
            std::fs::remove_file(changed.join("old.js")).unwrap();
            std::fs::write(changed.join("new.js"), "n").unwrap();
        })
        .await;

        assert_eq!(status, RunStatus::Failed, "{tasks:#?}");
        assert_eq!(tasks[0].status, TaskStatus::Succeeded);
        let first = upload_detail(&tasks[0]);
        assert_eq!(first.manifest.as_ref().unwrap().files.len(), 2);
        assert_eq!(
            first.committed,
            vec![format!("{a}/app.js"), format!("{a}/old.js")]
        );

        assert_eq!(tasks[1].status, TaskStatus::Failed);
        assert_eq!(tasks[1].reason, Some(Message::ArtifactChanged { n: 3 }));
        let second = upload_detail(&tasks[1]);
        assert!(second.manifest.is_none() && second.committed.is_empty());
        assert_eq!(
            second.changes,
            vec![
                ArtifactChange::Modified("app.js".into()),
                ArtifactChange::Added("new.js".into()),
                ArtifactChange::Removed("old.js".into()),
            ]
        );
        assert!(
            notes.iter().any(
                |(seq, m)| *seq == 0 && matches!(m, Message::ManifestRecorded { files: 2, .. })
            )
        );
        assert!(
            notes
                .iter()
                .any(|(seq, m)| *seq == 0 && matches!(m, Message::UploadDone { files: 2, .. }))
        );
    }

    #[tokio::test]
    async fn missing_artifact_fails_at_the_upload_and_a_failed_target_stops_the_run() {
        let keys = require_test_env!();
        let local = tempfile::tempdir().unwrap();
        let src = local.path().join("app.jar");
        let a = "/home/deploy/mdm-run-stop/app.jar";
        // Not built (yet): the upload task itself reports it (AC-11).
        let (status, tasks, _) = run(&keys, plan(&src, SourceKind::File, &[a]), || {}).await;
        assert_eq!(status, RunStatus::Failed);
        assert_eq!(
            tasks[0].reason,
            Some(Message::SourceMissing {
                path: src.to_string_lossy().into_owned()
            })
        );

        // A succeeds, B has no permission: B fails, the rest is skipped, A stays (AC-15).
        std::fs::write(&src, "built").unwrap();
        let b = "/opt/protected/app.jar";
        let c = "/home/deploy/mdm-run-stop/c.jar";
        let (status, tasks, _) = run(&keys, plan(&src, SourceKind::File, &[a, b, c]), || {}).await;
        assert_eq!(status, RunStatus::Failed);
        let statuses: Vec<_> = tasks.iter().map(|t| t.status).collect();
        assert_eq!(
            statuses,
            vec![
                TaskStatus::Succeeded,
                TaskStatus::Failed,
                TaskStatus::Skipped
            ]
        );
        assert!(matches!(
            tasks[1].reason,
            Some(Message::PermissionDenied { ref path }) if path == b
        ));
        assert_eq!(upload_detail(&tasks[1]).stopped_at.as_deref(), Some(b));
    }
}
