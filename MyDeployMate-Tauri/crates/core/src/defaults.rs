//! Defaults for new objects. The UI asks the backend for these instead of hard-coding them.

use crate::ids::*;
use crate::model::*;

pub const DEFAULT_PORT: u32 = 22;
pub const DEFAULT_USER: &str = "deploy";
pub const DEFAULT_CONNECT_TIMEOUT_SEC: u32 = 10;
pub const DEFAULT_STEP_TIMEOUT_SEC: u32 = 300;
pub const DEFAULT_RUN_AS: &str = "root";

/// A fresh workspace with the preset environments (DATA-03): production is protected.
pub fn new_workspace() -> Workspace {
    let env = |name: &str, protected: bool| Environment {
        id: EnvironmentId::new(),
        name: name.into(),
        protected,
    };
    Workspace {
        schema_version: SCHEMA_VERSION,
        revision: 0,
        environments: vec![
            env("development", false),
            env("staging", false),
            env("production", true),
        ],
        servers: Vec::new(),
        projects: Vec::new(),
        global_variables: Vec::new(),
        preferences: Preferences {
            check_updates: true,
        },
    }
}

/// SRV-01 defaults: port 22, user `deploy`, key file auth, no elevation, legacy RSA off.
pub fn new_server(environment_id: EnvironmentId) -> Server {
    Server {
        id: ServerId::new(),
        name: String::new(),
        host: String::new(),
        port: DEFAULT_PORT,
        user: DEFAULT_USER.into(),
        environment_id,
        auth: AuthMethod::KeyFile {
            path: String::new(),
        },
        privilege: PrivilegePolicy::None,
        allow_legacy_rsa: false,
        connect_timeout_sec: DEFAULT_CONNECT_TIMEOUT_SEC,
        tags: Vec::new(),
        notes: String::new(),
    }
}

/// A step of the given kind with STEP-01 defaults.
pub fn new_step(kind: StepKind, name: impl Into<String>) -> Step {
    let spec = match kind {
        StepKind::Local => StepSpec::Local(LocalSpec {
            command: String::new(),
            cwd: None,
            shell: Shell::Powershell,
            wsl_distro: None,
            output_encoding: OutputEncoding::Auto,
        }),
        StepKind::Upload => StepSpec::Upload(UploadSpec {
            source: String::new(),
            source_kind: SourceKind::File,
            target: String::new(),
            conflict: ConflictPolicy::Error,
        }),
        StepKind::Remote => StepSpec::Remote(RemoteSpec {
            command: RemoteCommand::Script {
                script: String::new(),
            },
            cwd: None,
            privilege: StepPrivilege::Inherit,
        }),
        StepKind::Confirm => StepSpec::Confirm(ConfirmSpec {
            prompt: String::new(),
        }),
    };
    Step {
        id: StepId::new(),
        name: name.into(),
        enabled: true,
        timeout_sec: DEFAULT_STEP_TIMEOUT_SEC,
        failure_policy: StepFailurePolicy::Inherit,
        spec,
    }
}
