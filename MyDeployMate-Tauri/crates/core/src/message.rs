//! User-facing messages as a code plus parameters (architecture ARC-38, ADR-12).
//!
//! The backend never builds sentences for the operator. It sends a [`Message`]; the UI renders
//! it in the current language from `backend.<code>` in `src/i18n/zh-CN.ts`, rendering nested
//! messages and labels first. Saved reasons keep the JSON form, so history follows the language
//! too. Parameters are raw values: names, numbers, paths and untranslated technical detail.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "code", content = "params")]
#[ts(export)]
pub enum Message {
    /// Text that is not a message code: user content, or a reason saved before v0.3 (ARC-38).
    #[serde(rename = "text")]
    Text { text: String },
    /// A message followed by untranslated technical detail, e.g. an IO error.
    #[serde(rename = "withDetail")]
    WithDetail {
        message: Box<Message>,
        detail: String,
    },
    /// A message followed by a second message that explains it.
    #[serde(rename = "withNote")]
    WithNote {
        message: Box<Message>,
        note: Box<Message>,
    },
    #[serde(rename = "internal")]
    Internal { detail: String },
    #[serde(rename = "validate.mustBeUnsigned")]
    MustBeUnsigned,
    #[serde(rename = "validate.mustBeNumber")]
    MustBeNumber,
    #[serde(rename = "validate.mustBeText")]
    MustBeText,
    #[serde(rename = "validate.mustBeSwitch")]
    MustBeSwitch,
    #[serde(rename = "validate.badFormat")]
    BadFormat { detail: String },
    #[serde(rename = "validate.schemaVersion")]
    SchemaVersion { found: u32, supported: u32 },
    #[serde(rename = "validate.required")]
    Required { label: Label },
    #[serde(rename = "validate.tooLong")]
    TooLong { label: Label, max: u32 },
    #[serde(rename = "validate.tooLongNow")]
    TooLongNow { label: Label, max: u32, len: u32 },
    #[serde(rename = "validate.outOfRange")]
    OutOfRange {
        label: Label,
        min: u32,
        max: u32,
        value: u32,
    },
    #[serde(rename = "validate.duplicateId")]
    DuplicateId { id: String },
    #[serde(rename = "validate.duplicateName")]
    DuplicateName { label: Label, value: String },
    #[serde(rename = "validate.noWhitespace")]
    NoWhitespace { label: Label },
    #[serde(rename = "validate.envVarChars")]
    EnvVarChars,
    #[serde(rename = "validate.identifier")]
    Identifier,
    #[serde(rename = "validate.reservedName")]
    ReservedName { name: String },
    #[serde(rename = "validate.sensitiveLiteral")]
    SensitiveLiteral,
    #[serde(rename = "validate.unknownEnvironment")]
    UnknownEnvironment,
    #[serde(rename = "validate.rootAbsolute")]
    RootAbsolute,
    #[serde(rename = "validate.unknownDefaultEnvironment")]
    UnknownDefaultEnvironment,
    #[serde(rename = "validate.noPipelines")]
    NoPipelines,
    #[serde(rename = "validate.skipServerOrder")]
    SkipServerOrder,
    #[serde(rename = "validate.skipServerKind")]
    SkipServerKind,
    #[serde(rename = "validate.unknownTargetEnvironment")]
    UnknownTargetEnvironment,
    #[serde(rename = "validate.duplicateTargetEnvironment")]
    DuplicateTargetEnvironment,
    #[serde(rename = "validate.unknownServer")]
    UnknownServer,
    #[serde(rename = "validate.wrongEnvironment")]
    WrongEnvironment,
    #[serde(rename = "validate.duplicateServer")]
    DuplicateServer,
    #[serde(rename = "validate.unknownOverrideEnvironment")]
    UnknownOverrideEnvironment,
    #[serde(rename = "validate.unknownVariable")]
    UnknownVariable { name: String },
    #[serde(rename = "validate.duplicateOverride")]
    DuplicateOverride,
    #[serde(rename = "validate.wslDistro")]
    WslDistro,
    #[serde(rename = "validate.wslCwdAbsolute")]
    WslCwdAbsolute,
    #[serde(rename = "validate.wslCwdRequired")]
    WslCwdRequired,
    #[serde(rename = "validate.relativeCwd")]
    RelativeCwd,
    #[serde(rename = "validate.cwdRequired")]
    CwdRequired,
    #[serde(rename = "validate.relativeSource")]
    RelativeSource,
    #[serde(rename = "validate.argNul")]
    ArgNul,
    #[serde(rename = "validate.remoteAbsolute")]
    RemoteAbsolute { label: Label },
    #[serde(rename = "host.whitespace")]
    HostWhitespace,
    #[serde(rename = "host.scheme")]
    HostScheme,
    #[serde(rename = "host.path")]
    HostPath,
    #[serde(rename = "host.user")]
    HostUser,
    #[serde(rename = "host.brackets")]
    HostBrackets,
    #[serde(rename = "host.port")]
    HostPort,
    #[serde(rename = "host.ipv4")]
    HostIpv4,
    #[serde(rename = "host.tooLong")]
    HostTooLong,
    #[serde(rename = "host.chars")]
    HostChars,
    #[serde(rename = "plan.projectNotFound")]
    ProjectNotFound,
    #[serde(rename = "plan.pipelineNotFound")]
    PipelineNotFound,
    #[serde(rename = "plan.environmentNotFound")]
    EnvironmentNotFound,
    #[serde(rename = "plan.notBound")]
    NotBound,
    #[serde(rename = "plan.versionRequired")]
    VersionRequired,
    #[serde(rename = "plan.versionControl")]
    VersionControl,
    #[serde(rename = "plan.noTargets")]
    NoTargets,
    #[serde(rename = "plan.noLocalSteps")]
    NoLocalSteps,
    #[serde(rename = "plan.noSteps")]
    NoSteps,
    #[serde(rename = "plan.passwordEnvMissing")]
    PasswordEnvUnset { server: String, name: String },
    /// A problem in one step field, e.g. "步骤 1 · 命令：变量 artifact 未定义" (IXD flow 3).
    #[serde(rename = "plan.stepIssue")]
    StepIssue {
        step: u32,
        label: Label,
        reason: Box<Message>,
    },
    /// As `StepIssue`, for a value that may differ by server.
    #[serde(rename = "plan.stepServerIssue")]
    StepServerIssue {
        step: u32,
        label: Label,
        server: String,
        reason: Box<Message>,
    },
    #[serde(rename = "plan.pathControl")]
    PathControl,
    #[serde(rename = "plan.remoteAbsolute")]
    MustBeRemoteAbsolute,
    #[serde(rename = "plan.relativePath")]
    RelativePath,
    #[serde(rename = "plan.uploadFileTarget")]
    UploadFileTarget,
    #[serde(rename = "resolve.via")]
    Via { reason: Box<Message>, chain: String },
    #[serde(rename = "resolve.undefined")]
    Undefined { name: String },
    #[serde(rename = "resolve.cycle")]
    Cycle { chain: String },
    #[serde(rename = "resolve.tooDeep")]
    TooDeep { max: u32, chain: String },
    #[serde(rename = "resolve.serverContext")]
    ServerContext { expression: String },
    #[serde(rename = "resolve.projectRoot")]
    ProjectRootMissing { expression: String },
    #[serde(rename = "resolve.missingEnvVar")]
    MissingEnvVar { name: String },
    #[serde(rename = "resolve.missingSecret")]
    MissingSecret { name: String },
    #[serde(rename = "template.unterminated")]
    Unterminated,
    #[serde(rename = "template.invalid")]
    InvalidExpression { expression: String, escape: String },
    #[serde(rename = "template.unknownBuiltin")]
    UnknownBuiltin { expression: String },
    #[serde(rename = "builtin.projectName")]
    BuiltinProjectName,
    #[serde(rename = "builtin.projectRoot")]
    BuiltinProjectRoot,
    #[serde(rename = "builtin.env")]
    BuiltinEnv,
    #[serde(rename = "builtin.serverName")]
    BuiltinServerName,
    #[serde(rename = "builtin.serverHost")]
    BuiltinServerHost,
    #[serde(rename = "builtin.serverPort")]
    BuiltinServerPort,
    #[serde(rename = "builtin.serverUser")]
    BuiltinServerUser,
    #[serde(rename = "connect.keyFileUnreadable")]
    KeyFileUnreadable { path: String },
    #[serde(rename = "connect.keyFormatUnsupported")]
    KeyFormatUnsupported { path: String },
    #[serde(rename = "connect.passwordEnvMissing")]
    PasswordEnvMissing { name: String },
    #[serde(rename = "connect.agentUnavailable")]
    AgentUnavailable,
    #[serde(rename = "connect.agentNoIdentities")]
    AgentNoIdentities,
    #[serde(rename = "connect.resolveFailed")]
    ResolveFailed,
    #[serde(rename = "connect.connectionRefused")]
    ConnectionRefused,
    #[serde(rename = "connect.unreachable")]
    Unreachable,
    #[serde(rename = "connect.timeout")]
    ConnectTimeout,
    #[serde(rename = "connect.legacyRsaOnly")]
    LegacyRsaOnly,
    #[serde(rename = "connect.algorithmNegotiation")]
    AlgorithmNegotiation,
    #[serde(rename = "connect.algorithms")]
    Algorithms {
        kind: String,
        local: String,
        server: String,
    },
    #[serde(rename = "connect.hostKeyUnknown")]
    HostKeyUnknown,
    #[serde(rename = "connect.hostKeyChanged")]
    HostKeyChanged,
    #[serde(rename = "connect.noHostKey")]
    NoHostKey,
    #[serde(rename = "connect.keyPassphraseWrong")]
    KeyPassphraseWrong,
    #[serde(rename = "connect.authenticationFailed")]
    AuthenticationFailed,
    #[serde(rename = "connect.agentKeysRejected")]
    AgentKeysRejected,
    #[serde(rename = "connect.probeFailed")]
    ProbeFailed { code: u32 },
    #[serde(rename = "connect.probeAborted")]
    ProbeAborted,
    #[serde(rename = "connect.cancelled")]
    Cancelled,
    #[serde(rename = "connect.disconnected")]
    Disconnected,
    #[serde(rename = "connect.protocol")]
    Protocol,
    /// What a credential prompt is for: a connection test.
    #[serde(rename = "connect.testContext")]
    ConnectionTest,
    /// What a credential prompt is for: a task of the run.
    #[serde(rename = "run.taskContext")]
    TaskContext { n: u32, step: String },
    #[serde(rename = "run.userCancelled")]
    UserCancelled,
    #[serde(rename = "run.quitCancelled")]
    QuitCancelled,
    #[serde(rename = "run.confirmDeclined")]
    ConfirmDeclined,
    #[serde(rename = "run.trustDeclined")]
    TrustDeclined,
    #[serde(rename = "run.secretDeclined")]
    SecretDeclined,
    #[serde(rename = "run.notStarted")]
    NotStarted,
    #[serde(rename = "run.outcomeUnknown")]
    OutcomeUnknown { n: u32 },
    #[serde(rename = "run.afterFailure")]
    AfterFailure { n: u32, step: String },
    #[serde(rename = "run.afterServerFailure")]
    AfterServerFailure {
        n: u32,
        step: String,
        server: String,
    },
    /// The skip-server policy stopped this server after task `n` failed (IXD-30).
    #[serde(rename = "run.serverSkipped")]
    ServerSkipped { server: String, n: u32 },
    #[serde(rename = "run.allServersSkipped")]
    AllServersSkipped,
    /// Set by history recovery when the app quit mid-run.
    #[serde(rename = "run.appExited")]
    AppExited,
    #[serde(rename = "run.serverMissing")]
    ServerMissing,
    #[serde(rename = "run.connectFailed")]
    ConnectFailed {
        server: String,
        reason: Box<Message>,
    },
    #[serde(rename = "run.logStorage")]
    LogStorage,
    #[serde(rename = "run.storageFailed")]
    RunStorageFailed { reason: Box<Message> },
    #[serde(rename = "exec.noConnection")]
    NoConnection,
    #[serde(rename = "exec.sudoUnsupported")]
    SudoUnsupported,
    #[serde(rename = "exec.noServer")]
    NoServer,
    #[serde(rename = "exec.startFailed")]
    StartFailed { detail: String },
    #[serde(rename = "exec.noProcessGroup")]
    NoProcessGroup,
    #[serde(rename = "exec.shellMissing")]
    ShellMissing { program: String },
    #[serde(rename = "exec.uploadUnsupported")]
    UploadUnsupported,
    #[serde(rename = "exec.exitCode")]
    ExitCode {
        #[ts(type = "number")]
        code: i64,
    },
    #[serde(rename = "exec.signal")]
    Signal { signal: String },
    #[serde(rename = "exec.refused")]
    Refused,
    #[serde(rename = "exec.disconnected")]
    CommandDisconnected,
    #[serde(rename = "exec.cancelUnconfirmed")]
    CancelUnconfirmed { why: Box<Message> },
    #[serde(rename = "exec.remoteTimeout")]
    RemoteTimeout { limit: u32 },
    #[serde(rename = "exec.remoteTimeoutUnconfirmed")]
    RemoteTimeoutUnconfirmed { limit: u32, why: Box<Message> },
    #[serde(rename = "exec.noProcessGroupId")]
    NoProcessGroupId,
    #[serde(rename = "exec.connectionLost")]
    ConnectionLost,
    #[serde(rename = "exec.checkTimeout")]
    CheckTimeout,
    #[serde(rename = "exec.stillRunning")]
    StillRunning,
    #[serde(rename = "exec.checkFailed")]
    CheckFailed,
    #[serde(rename = "exec.leftovers")]
    Leftovers,
    #[serde(rename = "exec.noExitCode")]
    NoExitCode,
    #[serde(rename = "exec.localTimeout")]
    LocalTimeout { limit: u32 },
    #[serde(rename = "exec.wslTimeout")]
    WslTimeout { limit: u32 },
    #[serde(rename = "exec.wslCancelled")]
    WslCancelled,
    #[serde(rename = "exec.cwdMissing")]
    CwdMissing { path: String },
    #[serde(rename = "exec.spawnFailed")]
    SpawnFailed { program: String, detail: String },
    #[serde(rename = "exec.waitFailed")]
    WaitFailed { detail: String },
    #[serde(rename = "upload.sourceMissing")]
    SourceMissing { path: String },
    #[serde(rename = "upload.sourceNotFile")]
    SourceNotFile { path: String },
    #[serde(rename = "upload.sourceNotDir")]
    SourceNotDir { path: String },
    #[serde(rename = "upload.sourceLink")]
    SourceLink { path: String },
    #[serde(rename = "upload.sourceSpecial")]
    SourceSpecial { path: String },
    #[serde(rename = "upload.sourceRead")]
    SourceRead { path: String, detail: String },
    #[serde(rename = "upload.sftpUnavailable")]
    SftpUnavailable { detail: String },
    #[serde(rename = "upload.targetIsDir")]
    TargetIsDir { path: String },
    #[serde(rename = "upload.targetNotDir")]
    TargetNotDir { path: String },
    #[serde(rename = "upload.targetExists")]
    TargetExists { path: String },
    #[serde(rename = "upload.noPosixRename")]
    NoPosixRename,
    #[serde(rename = "upload.noSpace")]
    NoSpace {
        path: String,
        need: String,
        avail: String,
    },
    #[serde(rename = "upload.permissionDenied")]
    PermissionDenied { path: String },
    #[serde(rename = "upload.remoteFailed")]
    RemoteFailed { path: String, detail: String },
    #[serde(rename = "upload.contentChanged")]
    ContentChanged { path: String },
    #[serde(rename = "upload.sizeMismatch")]
    SizeMismatch { path: String },
    #[serde(rename = "upload.commitUnknown")]
    CommitUnknown { path: String },
    #[serde(rename = "upload.tempLeft")]
    TempLeft { path: String, detail: String },
    #[serde(rename = "log.storageFailed")]
    LogStorageFailed { reason: Box<Message> },
    #[serde(rename = "log.limitReached")]
    LimitReached { limit: String, tail: String },
    #[serde(rename = "log.tailSaved")]
    TailSaved { size: String },
    #[serde(rename = "log.tailPartial")]
    TailPartial { total: String, kept: String },
    #[serde(rename = "store.io")]
    Io { path: String, detail: String },
    #[serde(rename = "store.locked")]
    Locked { path: String },
    #[serde(rename = "store.revisionConflict")]
    RevisionConflict {
        #[ts(type = "number")]
        found: i64,
        #[ts(type = "number")]
        expected: i64,
    },
    #[serde(rename = "store.externalChange")]
    ExternalChange,
    #[serde(rename = "store.corrupt")]
    Corrupt { path: String, reason: Box<Message> },
    #[serde(rename = "store.missingField")]
    MissingField { name: String },
    #[serde(rename = "store.noMigration")]
    NoMigration,
    #[serde(rename = "store.unsupportedVersion")]
    UnsupportedVersion { found: u32, supported: u32 },
    #[serde(rename = "store.migration")]
    Migration { from: u32, reason: Box<Message> },
    #[serde(rename = "store.database")]
    Database { detail: String },
    #[serde(rename = "store.historyVersion")]
    HistoryVersion { found: u32, supported: u32 },
    #[serde(rename = "store.runActive")]
    RunActive,
    #[serde(rename = "store.trustConflict")]
    TrustConflict { endpoint: String },
    #[serde(rename = "store.vault")]
    Vault { detail: String },
    #[serde(rename = "store.invalid")]
    Invalid { n: u32 },
    #[serde(rename = "app.runStateBroken")]
    RunStateBroken,
    #[serde(rename = "app.workspaceStateBroken")]
    WorkspaceStateBroken,
    #[serde(rename = "app.runActive")]
    AnotherRunActive,
    #[serde(rename = "app.stalePreview")]
    StalePreview,
    #[serde(rename = "app.preflightIssues")]
    PreflightIssues { n: u32 },
    #[serde(rename = "app.noTasks")]
    NoTasks,
    #[serde(rename = "app.confirmEnvironment")]
    ConfirmEnvironment { name: String },
    #[serde(rename = "app.runNotFound")]
    RunNotFound,
    #[serde(rename = "app.runUnavailable")]
    RunUnavailable,
    #[serde(rename = "app.runEnded")]
    RunEnded,
    #[serde(rename = "app.runAlreadyEnded")]
    RunAlreadyEnded,
    #[serde(rename = "app.serverInUseDelete")]
    ServerInUseDelete { server: String },
    #[serde(rename = "app.serverInUseMove")]
    ServerInUseMove { server: String },
    #[serde(rename = "app.serverInUseEdit")]
    ServerInUseEdit { server: String },
    #[serde(rename = "app.openDataDir")]
    OpenDataDir { detail: String },
    #[serde(rename = "app.keyFileMissing")]
    KeyFileMissing { path: String },
    #[serde(rename = "app.keyFileUnreadable")]
    KeyFileReadFailed { path: String, detail: String },
    #[serde(rename = "app.projectMissing")]
    ProjectMissing,
    #[serde(rename = "app.requestExpired")]
    RequestExpired,
    #[serde(rename = "app.savedCredentialUnavailable")]
    SavedCredentialUnavailable { reason: Box<Message> },
    #[serde(rename = "app.valueRequired")]
    ValueRequired,
    #[serde(rename = "app.serverNotFound")]
    ServerNotFound,
    #[serde(rename = "app.testRunning")]
    TestRunning,
    #[serde(rename = "app.trustStoreUnavailable")]
    TrustStoreUnavailable { reason: Box<Message> },
}

/// The name of a field, used inside messages such as "{label}不能为空".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "code", content = "params")]
#[ts(export)]
pub enum Label {
    #[serde(rename = "label.address")]
    Address,
    #[serde(rename = "label.envName")]
    EnvironmentName,
    #[serde(rename = "label.serverName")]
    ServerName,
    #[serde(rename = "label.port")]
    Port,
    #[serde(rename = "label.user")]
    User,
    #[serde(rename = "label.connectTimeout")]
    ConnectTimeout,
    #[serde(rename = "label.tag")]
    Tag,
    #[serde(rename = "label.notes")]
    Notes,
    #[serde(rename = "label.keyPath")]
    KeyPath,
    #[serde(rename = "label.envVarName")]
    EnvVarName,
    #[serde(rename = "label.runAs")]
    RunAs,
    #[serde(rename = "label.variableName")]
    VariableName,
    #[serde(rename = "label.projectName")]
    ProjectName,
    #[serde(rename = "label.description")]
    Description,
    #[serde(rename = "label.pipelineName")]
    PipelineName,
    #[serde(rename = "label.stepName")]
    StepName,
    #[serde(rename = "label.timeout")]
    Timeout,
    #[serde(rename = "label.command")]
    Command,
    #[serde(rename = "label.cwd")]
    Cwd,
    #[serde(rename = "label.source")]
    Source,
    #[serde(rename = "label.target")]
    Target,
    #[serde(rename = "label.executable")]
    Executable,
    #[serde(rename = "label.argument")]
    Argument { n: u32 },
    #[serde(rename = "label.remoteCwd")]
    RemoteCwd,
    #[serde(rename = "label.prompt")]
    Prompt,
    #[serde(rename = "label.version")]
    Version,
}

impl Message {
    pub fn text(text: impl Into<String>) -> Self {
        Message::Text { text: text.into() }
    }

    /// Append untranslated technical detail; blank detail is dropped.
    pub fn with_detail(self, detail: impl AsRef<str>) -> Self {
        let detail = detail.as_ref().trim();
        if detail.is_empty() {
            return self;
        }
        Message::WithDetail {
            message: Box::new(self),
            detail: detail.to_owned(),
        }
    }

    pub fn with_note(self, note: Message) -> Self {
        Message::WithNote {
            message: Box::new(self),
            note: Box::new(note),
        }
    }

    /// Stored form: JSON in the TEXT columns that held plain Chinese before v0.3.
    pub fn to_stored(&self) -> String {
        serde_json::to_string(self).expect("messages always serialize")
    }

    /// Read a stored reason; text that is not a message is shown as it was written.
    pub fn from_stored(stored: &str) -> Self {
        serde_json::from_str(stored).unwrap_or_else(|_| Message::text(stored))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_as_code_and_params() {
        let m = Message::Required {
            label: Label::Argument { n: 2 },
        };
        assert_eq!(
            serde_json::to_value(&m).unwrap(),
            serde_json::json!({
                "code": "validate.required",
                "params": { "label": { "code": "label.argument", "params": { "n": 2 } } }
            })
        );
        assert_eq!(
            serde_json::to_value(Message::RunActive).unwrap(),
            serde_json::json!({ "code": "store.runActive" })
        );
    }

    #[test]
    fn reads_stored_reasons_of_both_forms() {
        let m = Message::ServerSkipped {
            server: "api-01".into(),
            n: 3,
        };
        assert_eq!(Message::from_stored(&m.to_stored()), m);
        assert_eq!(
            Message::from_stored("服务器 api-01 已跳过（任务 3 失败）"),
            Message::text("服务器 api-01 已跳过（任务 3 失败）")
        );
        assert_eq!(
            Message::from_stored("{\"code\":1}"),
            Message::text("{\"code\":1}")
        );
    }

    #[test]
    fn drops_blank_detail() {
        assert_eq!(
            Message::ConnectTimeout.with_detail("  "),
            Message::ConnectTimeout
        );
    }
}
