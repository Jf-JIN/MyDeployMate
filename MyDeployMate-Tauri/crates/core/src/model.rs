//! Workspace configuration objects (SRS DATA-01, architecture ARC-09).
//!
//! These types are the single source of truth for the saved `workspace.json` and for the
//! TypeScript bindings the frontend uses. Field rules live in [`crate::validate`], not here,
//! so a draft that breaks the rules can still be represented and reported field by field.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{EnvironmentId, PipelineId, ProjectId, ServerId, StepId, VariableId};

/// Current workspace schema version. Bump together with a migration in `mdm-store`.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Workspace {
    pub schema_version: u32,
    /// Increases by one on every successful save; used to detect stale drafts.
    #[ts(type = "number")]
    pub revision: u64,
    pub environments: Vec<Environment>,
    pub servers: Vec<Server>,
    pub projects: Vec<Project>,
    pub global_variables: Vec<Variable>,
    #[serde(default)]
    pub preferences: Preferences,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Environment {
    pub id: EnvironmentId,
    pub name: String,
    /// Protected environments require typing the name before a run (RUN-02).
    pub protected: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Server {
    pub id: ServerId,
    pub name: String,
    /// DNS name, IPv4 or IPv6 address, without scheme, path or port.
    pub host: String,
    /// Kept wider than `u16` so out-of-range input reaches validation instead of failing to parse.
    pub port: u32,
    pub user: String,
    pub environment_id: EnvironmentId,
    pub auth: AuthMethod,
    pub privilege: PrivilegePolicy,
    /// Allow `ssh-rsa` (SHA-1) host keys and user auth (SRV-01).
    pub allow_legacy_rsa: bool,
    pub connect_timeout_sec: u32,
    pub tags: Vec<String>,
    pub notes: String,
}

/// How to log in (SRV-02). Never holds a password or key body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "method",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum AuthMethod {
    /// OpenSSH or PuTTY `.ppk` private key file on this computer.
    KeyFile {
        path: String,
    },
    Agent {
        agent: AgentKind,
    },
    Password {
        source: PasswordSource,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AgentKind {
    /// Windows OpenSSH Authentication Agent (named pipe).
    Openssh,
    Pageant,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum PasswordSource {
    /// Ask at run time (optionally remembered in Windows Credential Manager).
    Prompt,
    /// Read from a client process environment variable.
    EnvVar { name: String },
}

/// Server-level default privilege (SEC-01). Default is no elevation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum PrivilegePolicy {
    None,
    SudoNonInteractive { run_as: String },
    SudoPassword { run_as: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Preferences {
    /// Check GitHub Releases for updates at startup (NFR-04).
    #[serde(default = "default_true")]
    pub check_updates: bool,
    /// Finished runs that started more days ago than this are removed with their logs (NFR-02).
    #[serde(default = "default_retention_days")]
    pub history_retention_days: u32,
}

impl Default for Preferences {
    fn default() -> Self {
        Preferences {
            check_updates: true,
            history_retention_days: crate::history::DEFAULT_RETENTION_DAYS,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_retention_days() -> u32 {
    crate::history::DEFAULT_RETENTION_DAYS
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub description: String,
    /// Optional absolute local path; required when steps use relative local paths (PRJ-01).
    pub root: Option<String>,
    pub default_environment_id: EnvironmentId,
    /// Explicit target servers per environment, in execution order (DATA-03).
    pub targets: Vec<EnvironmentTargets>,
    pub variables: Vec<Variable>,
    /// Per-environment overrides of project or global variables (VAR-01).
    pub overrides: Vec<VariableOverride>,
    pub pipelines: Vec<Pipeline>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EnvironmentTargets {
    pub environment_id: EnvironmentId,
    pub server_ids: Vec<ServerId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Variable {
    pub id: VariableId,
    pub name: String,
    pub value: VariableValue,
    pub sensitive: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct VariableOverride {
    pub environment_id: EnvironmentId,
    pub name: String,
    pub value: VariableValue,
}

/// Where a variable's value comes from. Sensitive variables never use `Literal` (ARC-10).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum VariableValue {
    Literal {
        value: String,
    },
    /// Client process environment variable.
    EnvRef {
        name: String,
    },
    /// Stored in Windows Credential Manager, keyed by the variable ID (and environment for overrides).
    SecretRef,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Pipeline {
    pub id: PipelineId,
    pub name: String,
    pub execution_order: ExecutionOrder,
    pub failure_policy: PipelineFailurePolicy,
    pub steps: Vec<Step>,
}

/// RUN-03.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ExecutionOrder {
    #[default]
    StepFirst,
    ServerFirst,
}

/// Pipeline default failure policy (RUN-04).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PipelineFailurePolicy {
    #[default]
    Stop,
    Continue,
    /// Only valid with [`ExecutionOrder::ServerFirst`].
    SkipServer,
}

/// Step failure policy; `Inherit` uses the pipeline's (STEP-01).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum StepFailurePolicy {
    #[default]
    Inherit,
    Stop,
    Continue,
    SkipServer,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Step {
    pub id: StepId,
    pub name: String,
    pub enabled: bool,
    /// Ignored for confirm steps, which have no execution timeout.
    pub timeout_sec: u32,
    pub failure_policy: StepFailurePolicy,
    pub spec: StepSpec,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export)]
pub enum StepSpec {
    Local(LocalSpec),
    Upload(UploadSpec),
    Remote(RemoteSpec),
    Confirm(ConfirmSpec),
}

impl StepSpec {
    pub fn kind(&self) -> StepKind {
        match self {
            StepSpec::Local(_) => StepKind::Local,
            StepSpec::Upload(_) => StepKind::Upload,
            StepSpec::Remote(_) => StepKind::Remote,
            StepSpec::Confirm(_) => StepKind::Confirm,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum StepKind {
    Local,
    Upload,
    Remote,
    Confirm,
}

/// STEP-02.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LocalSpec {
    pub command: String,
    /// Empty means the project root.
    pub cwd: Option<String>,
    pub shell: Shell,
    /// Required when `shell` is WSL.
    pub wsl_distro: Option<String>,
    pub output_encoding: OutputEncoding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Shell {
    /// Windows PowerShell 5.1 (`powershell.exe`).
    Powershell,
    /// PowerShell 7 (`pwsh.exe`).
    Pwsh,
    Cmd,
    Wsl,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum OutputEncoding {
    /// UTF-8, falling back to the ANSI code page (GBK on Chinese Windows).
    #[default]
    Auto,
    Utf8,
    Gbk,
}

/// STEP-03.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UploadSpec {
    pub source: String,
    pub source_kind: SourceKind,
    /// Absolute remote path. For directories, the contents go here.
    pub target: String,
    pub conflict: ConflictPolicy,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SourceKind {
    #[default]
    File,
    Directory,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ConflictPolicy {
    #[default]
    Error,
    Overwrite,
}

/// STEP-05.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RemoteSpec {
    pub command: RemoteCommand,
    /// Empty means the login user's default directory; otherwise an absolute remote path.
    pub cwd: Option<String>,
    pub privilege: StepPrivilege,
}

/// Script text run by `sh -c`, or an exact program with arguments (ARC-11).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum RemoteCommand {
    Script {
        script: String,
    },
    Program {
        executable: String,
        args: Vec<String>,
    },
}

/// Step-level privilege; `Inherit` uses the server default (SEC-01).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum StepPrivilege {
    Inherit,
    None,
    SudoNonInteractive { run_as: String },
    SudoPassword { run_as: String },
}

/// STEP-06.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConfirmSpec {
    pub prompt: String,
}

/// Purpose of a remembered credential in Windows Credential Manager (SEC-03).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(
    tag = "purpose",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum CredentialPurpose {
    SshPassword,
    KeyPassphrase,
    SudoPassword { run_as: String },
}

/// Reference to one remembered credential. The secret itself never appears in this crate.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CredentialRef {
    pub server_id: ServerId,
    pub user: String,
    pub purpose: CredentialPurpose,
}
