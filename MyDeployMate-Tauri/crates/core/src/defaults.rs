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

/// A new, unprotected environment (DATA-03).
pub fn new_environment(name: impl Into<String>) -> Environment {
    Environment {
        id: EnvironmentId::new(),
        name: name.into(),
        protected: false,
    }
}

/// A new non-sensitive variable with an empty literal value (VAR-01).
pub fn new_variable() -> Variable {
    Variable {
        id: VariableId::new(),
        name: String::new(),
        value: VariableValue::Literal {
            value: String::new(),
        },
        sensitive: false,
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

/// The name a new step of `kind` starts with (IXD-10).
pub fn step_kind_label(kind: StepKind) -> &'static str {
    match kind {
        StepKind::Local => "本地命令",
        StepKind::Upload => "上传文件",
        StepKind::Remote => "远程命令",
        StepKind::Confirm => "人工确认",
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

/// Name of the pipeline created with a new project (IXD-07).
pub const FIRST_PIPELINE_NAME: &str = "部署";

/// An empty pipeline with RUN-03/RUN-04 defaults: step first, stop on failure.
pub fn new_pipeline(name: impl Into<String>) -> Pipeline {
    Pipeline {
        id: PipelineId::new(),
        name: name.into(),
        execution_order: ExecutionOrder::default(),
        failure_policy: PipelineFailurePolicy::default(),
        steps: Vec::new(),
    }
}

/// A project with one empty pipeline and no targets (IXD-07, PRJ-01).
pub fn new_project(default_environment_id: EnvironmentId) -> Project {
    Project {
        id: ProjectId::new(),
        name: String::new(),
        description: String::new(),
        root: None,
        default_environment_id,
        targets: Vec::new(),
        variables: Vec::new(),
        overrides: Vec::new(),
        pipelines: vec![new_pipeline(FIRST_PIPELINE_NAME)],
    }
}
