//! Field validation for a workspace draft (SRV-01, PRJ-01, PRJ-02, STEP-01–06, DATA-02/03, VAR-01).
//!
//! Every problem becomes an [`Issue`] that names the object and field, so the UI can jump to it.
//! Path syntax that depends on variables (`${...}`) is checked later by preflight, after resolution.

use std::collections::{HashMap, HashSet};
use std::net::{Ipv4Addr, Ipv6Addr};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::history::{MAX_RETENTION_DAYS, MIN_RETENTION_DAYS};
use crate::message::{Label, Message};
use crate::model::*;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ValidationReport {
    pub issues: Vec<Issue>,
}

impl ValidationReport {
    pub fn is_ok(&self) -> bool {
        self.issues.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Issue {
    /// JSON path of the field, e.g. `servers[2].port`.
    pub path: String,
    /// The object that owns the field, when it has an ID.
    pub object: Option<ObjectRef>,
    /// Field name in camelCase, e.g. `port`.
    pub field: String,
    pub code: IssueCode,
    pub message: Message,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ObjectRef {
    pub kind: ObjectKind,
    pub id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ObjectKind {
    Environment,
    Server,
    Project,
    Pipeline,
    Step,
    Variable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum IssueCode {
    /// The JSON value has the wrong type or shape (e.g. a non-integer port).
    InvalidType,
    UnsupportedVersion,
    Required,
    TooLong,
    OutOfRange,
    InvalidFormat,
    Duplicate,
    DuplicateId,
    UnknownReference,
    WrongEnvironment,
    Reserved,
    NotAllowed,
    // Preflight (RUN-01), reported by `plan::build_plan`.
    /// `${...}` that cannot be parsed.
    InvalidExpression,
    UndefinedVariable,
    VariableCycle,
    VariableTooDeep,
    /// `${server.*}` in a local or confirm step (VAR-02).
    ServerContext,
    /// Environment variable or Credential Manager value missing on this computer.
    MissingValue,
    /// The pipeline has remote steps but no target is selected.
    NoTargets,
    /// A selected server is not bound to the project in this environment.
    NotBound,
    /// Nothing would run.
    NoTasks,
}

pub const ENV_NAME_MAX: usize = 32;
pub const NAME_MAX: usize = 64;
pub const NOTES_MAX: usize = 2000;
pub const TAG_MAX: usize = 32;
pub const TIMEOUT_MAX_SEC: u32 = 86_400;
pub const CONNECT_TIMEOUT_MAX_SEC: u32 = 120;
pub const CONFIRM_PROMPT_MAX: usize = 2000;
/// Namespaces reserved by the variable syntax (VAR-01).
pub const RESERVED_VARIABLE_NAMES: [&str; 4] = ["project", "server", "run", "env"];

/// Parse a draft sent by the UI or read from disk. Type errors are reported with their JSON path.
pub fn parse_workspace(value: serde_json::Value) -> Result<Workspace, ValidationReport> {
    serde_path_to_error::deserialize::<_, Workspace>(value).map_err(|err| {
        let path = err.path().to_string();
        let field = last_segment(&path);
        ValidationReport {
            issues: vec![Issue {
                message: type_error_message(&err.inner().to_string()),
                path,
                object: None,
                field,
                code: IssueCode::InvalidType,
            }],
        }
    })
}

/// Parse and validate in one go: what `validate_config` and `save_config` run on a UI draft.
pub fn check_workspace_json(value: serde_json::Value) -> Result<Workspace, ValidationReport> {
    let ws = parse_workspace(value)?;
    let report = validate_workspace(&ws);
    if report.is_ok() { Ok(ws) } else { Err(report) }
}

/// A field that does not have the expected JSON type, in words the user can act on (AC-02).
/// serde's own text is English and names Rust types, so only unrecognised cases keep it.
fn type_error_message(serde: &str) -> Message {
    let expects_unsigned = serde.contains("expected u");
    if expects_unsigned
        && (serde.contains("floating point") || serde.contains("invalid value: integer"))
    {
        Message::MustBeUnsigned
    } else if expects_unsigned {
        Message::MustBeNumber
    } else if serde.contains("expected a string") {
        Message::MustBeText
    } else if serde.contains("expected a boolean") {
        Message::MustBeSwitch
    } else {
        Message::BadFormat {
            detail: serde.to_owned(),
        }
    }
}

fn last_segment(path: &str) -> String {
    let tail = path.rsplit('.').next().unwrap_or(path);
    tail.split('[').next().unwrap_or(tail).to_owned()
}

pub fn validate_workspace(ws: &Workspace) -> ValidationReport {
    let mut v = Validator::default();
    if ws.schema_version != SCHEMA_VERSION {
        v.push(
            "schemaVersion".into(),
            None,
            "schemaVersion",
            IssueCode::UnsupportedVersion,
            Message::SchemaVersion {
                found: ws.schema_version,
                supported: SCHEMA_VERSION,
            },
        );
    }
    v.check_ids(ws);
    v.check_environments(ws);
    v.check_servers(ws);
    v.check_variables("globalVariables", &ws.global_variables);
    v.check_projects(ws);
    v.check_preferences(&ws.preferences);
    v.finish()
}

#[derive(Default)]
struct Validator {
    issues: Vec<Issue>,
}

/// Where an object lives: its JSON path prefix and its ID.
struct At {
    path: String,
    object: Option<ObjectRef>,
}

impl At {
    fn new(path: String, kind: ObjectKind, id: impl ToString) -> Self {
        At {
            path,
            object: Some(ObjectRef {
                kind,
                id: id.to_string(),
            }),
        }
    }
}

impl Validator {
    fn finish(self) -> ValidationReport {
        ValidationReport {
            issues: self.issues,
        }
    }

    fn push(
        &mut self,
        path: String,
        object: Option<ObjectRef>,
        field: &str,
        code: IssueCode,
        message: Message,
    ) {
        self.issues.push(Issue {
            path,
            object,
            field: last_segment(field),
            code,
            message,
        });
    }

    fn at(&mut self, at: &At, field: &str, code: IssueCode, message: Message) {
        let path = format!("{}.{}", at.path, field);
        self.push(path, at.object.clone(), field, code, message);
    }

    /// Required name: trimmed length 1..=max.
    fn name(&mut self, at: &At, field: &str, label: Label, value: &str, max: usize) {
        let len = value.trim().chars().count();
        if len == 0 {
            self.at(at, field, IssueCode::Required, Message::Required { label });
        } else if len > max {
            self.too_long(at, field, label, len, max);
        }
    }

    fn max_len(&mut self, at: &At, field: &str, label: Label, value: &str, max: usize) {
        let len = value.chars().count();
        if len > max {
            self.too_long(at, field, label, len, max);
        }
    }

    fn too_long(&mut self, at: &At, field: &str, label: Label, len: usize, max: usize) {
        let message = Message::TooLongNow {
            label,
            max: max as u32,
            len: len as u32,
        };
        self.at(at, field, IssueCode::TooLong, message);
    }

    fn range(&mut self, at: &At, field: &str, label: Label, value: u32, min: u32, max: u32) {
        if !(min..=max).contains(&value) {
            self.at(
                at,
                field,
                IssueCode::OutOfRange,
                Message::OutOfRange {
                    label,
                    min,
                    max,
                    value,
                },
            );
        }
    }

    fn check_preferences(&mut self, prefs: &Preferences) {
        let at = At {
            path: "preferences".into(),
            object: None,
        };
        self.range(
            &at,
            "historyRetentionDays",
            Label::RetentionDays,
            prefs.history_retention_days,
            MIN_RETENTION_DAYS,
            MAX_RETENTION_DAYS,
        );
    }

    /// DATA-02: IDs are unique across every object kind.
    fn check_ids(&mut self, ws: &Workspace) {
        let mut seen: HashSet<String> = HashSet::new();
        let mut check = |v: &mut Self, at: At| {
            let id = at.object.as_ref().map(|o| o.id.clone()).unwrap_or_default();
            if !seen.insert(id.clone()) {
                v.at(
                    &at,
                    "id",
                    IssueCode::DuplicateId,
                    Message::DuplicateId { id },
                );
            }
        };
        for (i, e) in ws.environments.iter().enumerate() {
            check(
                self,
                At::new(format!("environments[{i}]"), ObjectKind::Environment, e.id),
            );
        }
        for (i, s) in ws.servers.iter().enumerate() {
            check(
                self,
                At::new(format!("servers[{i}]"), ObjectKind::Server, s.id),
            );
        }
        for (i, var) in ws.global_variables.iter().enumerate() {
            check(
                self,
                At::new(
                    format!("globalVariables[{i}]"),
                    ObjectKind::Variable,
                    var.id,
                ),
            );
        }
        for (i, p) in ws.projects.iter().enumerate() {
            check(
                self,
                At::new(format!("projects[{i}]"), ObjectKind::Project, p.id),
            );
            for (j, var) in p.variables.iter().enumerate() {
                check(
                    self,
                    At::new(
                        format!("projects[{i}].variables[{j}]"),
                        ObjectKind::Variable,
                        var.id,
                    ),
                );
            }
            for (j, pl) in p.pipelines.iter().enumerate() {
                check(
                    self,
                    At::new(
                        format!("projects[{i}].pipelines[{j}]"),
                        ObjectKind::Pipeline,
                        pl.id,
                    ),
                );
                for (k, s) in pl.steps.iter().enumerate() {
                    check(
                        self,
                        At::new(
                            format!("projects[{i}].pipelines[{j}].steps[{k}]"),
                            ObjectKind::Step,
                            s.id,
                        ),
                    );
                }
            }
        }
    }

    /// DATA-03.
    fn check_environments(&mut self, ws: &Workspace) {
        let mut names: HashSet<String> = HashSet::new();
        for (i, e) in ws.environments.iter().enumerate() {
            let at = At::new(format!("environments[{i}]"), ObjectKind::Environment, e.id);
            self.name(&at, "name", Label::EnvironmentName, &e.name, ENV_NAME_MAX);
            unique(
                self,
                &mut names,
                &at,
                "name",
                Label::EnvironmentName,
                e.name.trim(),
            );
        }
    }

    /// SRV-01, SRV-02, SEC-01.
    fn check_servers(&mut self, ws: &Workspace) {
        let envs: HashSet<_> = ws.environments.iter().map(|e| e.id).collect();
        let mut names: HashSet<String> = HashSet::new();
        for (i, s) in ws.servers.iter().enumerate() {
            let at = At::new(format!("servers[{i}]"), ObjectKind::Server, s.id);
            self.name(&at, "name", Label::ServerName, &s.name, NAME_MAX);
            unique(
                self,
                &mut names,
                &at,
                "name",
                Label::ServerName,
                s.name.trim(),
            );
            if let Err(msg) = check_host(&s.host) {
                let code = if s.host.trim().is_empty() {
                    IssueCode::Required
                } else {
                    IssueCode::InvalidFormat
                };
                self.at(&at, "host", code, msg);
            }
            self.range(&at, "port", Label::Port, s.port, 1, 65_535);
            self.login_name(&at, "user", Label::User, &s.user);
            if !envs.contains(&s.environment_id) {
                self.at(
                    &at,
                    "environmentId",
                    IssueCode::UnknownReference,
                    Message::UnknownEnvironment,
                );
            }
            self.range(
                &at,
                "connectTimeoutSec",
                Label::ConnectTimeout,
                s.connect_timeout_sec,
                1,
                CONNECT_TIMEOUT_MAX_SEC,
            );
            for (t, tag) in s.tags.iter().enumerate() {
                let field = format!("tags[{t}]");
                self.name(&at, &field, Label::Tag, tag, TAG_MAX);
            }
            self.max_len(&at, "notes", Label::Notes, &s.notes, NOTES_MAX);
            match &s.auth {
                AuthMethod::KeyFile { path } if path.trim().is_empty() => {
                    self.at(
                        &at,
                        "auth.path",
                        IssueCode::Required,
                        Message::Required {
                            label: Label::KeyPath,
                        },
                    );
                }
                AuthMethod::Password {
                    source: PasswordSource::EnvVar { name },
                } => {
                    self.env_var_name(&at, "auth.source.name", name);
                }
                _ => {}
            }
            match &s.privilege {
                PrivilegePolicy::SudoNonInteractive { run_as }
                | PrivilegePolicy::SudoPassword { run_as } => {
                    self.login_name(&at, "privilege.runAs", Label::RunAs, run_as);
                }
                PrivilegePolicy::None => {}
            }
        }
    }

    fn login_name(&mut self, at: &At, field: &str, label: Label, value: &str) {
        if value.is_empty() {
            self.at(at, field, IssueCode::Required, Message::Required { label });
        } else if value.chars().any(char::is_whitespace) {
            self.at(
                at,
                field,
                IssueCode::InvalidFormat,
                Message::NoWhitespace { label },
            );
        } else if value.chars().count() > NAME_MAX {
            self.at(
                at,
                field,
                IssueCode::TooLong,
                Message::TooLong {
                    label,
                    max: NAME_MAX as u32,
                },
            );
        }
    }

    fn env_var_name(&mut self, at: &At, field: &str, name: &str) {
        if name.trim().is_empty() {
            let label = Label::EnvVarName;
            self.at(at, field, IssueCode::Required, Message::Required { label });
        } else if name.contains('=') || name.contains('\0') || name.chars().any(char::is_whitespace)
        {
            self.at(at, field, IssueCode::InvalidFormat, Message::EnvVarChars);
        }
    }

    /// VAR-01: names, uniqueness in scope, reserved namespaces, sensitive values (ARC-10).
    fn check_variables(&mut self, prefix: &str, vars: &[Variable]) {
        let mut names: HashSet<String> = HashSet::new();
        for (i, var) in vars.iter().enumerate() {
            let at = At::new(format!("{prefix}[{i}]"), ObjectKind::Variable, var.id);
            if !is_identifier(&var.name) {
                let code = if var.name.is_empty() {
                    IssueCode::Required
                } else {
                    IssueCode::InvalidFormat
                };
                self.at(&at, "name", code, Message::Identifier);
            } else if RESERVED_VARIABLE_NAMES.contains(&var.name.as_str()) {
                self.at(
                    &at,
                    "name",
                    IssueCode::Reserved,
                    Message::ReservedName {
                        name: var.name.clone(),
                    },
                );
            }
            unique(
                self,
                &mut names,
                &at,
                "name",
                Label::VariableName,
                &var.name,
            );
            self.variable_value(&at, "value", &var.value, var.sensitive);
        }
    }

    fn variable_value(&mut self, at: &At, field: &str, value: &VariableValue, sensitive: bool) {
        match value {
            VariableValue::Literal { .. } if sensitive => {
                self.at(at, field, IssueCode::NotAllowed, Message::SensitiveLiteral);
            }
            VariableValue::EnvRef { name } => self.env_var_name(at, &format!("{field}.name"), name),
            _ => {}
        }
    }

    /// PRJ-01, PRJ-02, DATA-03, STEP-*.
    fn check_projects(&mut self, ws: &Workspace) {
        let envs: HashSet<_> = ws.environments.iter().map(|e| e.id).collect();
        let server_env: HashMap<_, _> = ws
            .servers
            .iter()
            .map(|s| (s.id, s.environment_id))
            .collect();
        let mut names: HashSet<String> = HashSet::new();
        for (i, p) in ws.projects.iter().enumerate() {
            let at = At::new(format!("projects[{i}]"), ObjectKind::Project, p.id);
            self.name(&at, "name", Label::ProjectName, &p.name, NAME_MAX);
            unique(
                self,
                &mut names,
                &at,
                "name",
                Label::ProjectName,
                p.name.trim(),
            );
            self.max_len(
                &at,
                "description",
                Label::Description,
                &p.description,
                NOTES_MAX,
            );
            if let Some(root) = p.root.as_deref().filter(|r| !r.trim().is_empty())
                && !is_windows_absolute(root)
            {
                self.at(&at, "root", IssueCode::InvalidFormat, Message::RootAbsolute);
            }
            if !envs.contains(&p.default_environment_id) {
                self.at(
                    &at,
                    "defaultEnvironmentId",
                    IssueCode::UnknownReference,
                    Message::UnknownDefaultEnvironment,
                );
            }
            self.check_targets(&at, p, &envs, &server_env);
            self.check_variables(&format!("{}.variables", at.path), &p.variables);
            self.check_overrides(&at, p, &ws.global_variables, &envs);
            if p.pipelines.is_empty() {
                self.at(&at, "pipelines", IssueCode::Required, Message::NoPipelines);
            }
            let mut pipeline_names: HashSet<String> = HashSet::new();
            let has_root = p.root.as_deref().is_some_and(|r| !r.trim().is_empty());
            for (j, pl) in p.pipelines.iter().enumerate() {
                let pat = At::new(
                    format!("{}.pipelines[{j}]", at.path),
                    ObjectKind::Pipeline,
                    pl.id,
                );
                self.name(&pat, "name", Label::PipelineName, &pl.name, NAME_MAX);
                unique(
                    self,
                    &mut pipeline_names,
                    &pat,
                    "name",
                    Label::PipelineName,
                    pl.name.trim(),
                );
                if pl.failure_policy == PipelineFailurePolicy::SkipServer
                    && pl.execution_order != ExecutionOrder::ServerFirst
                {
                    self.at(
                        &pat,
                        "failurePolicy",
                        IssueCode::NotAllowed,
                        Message::SkipServerOrder,
                    );
                }
                for (k, step) in pl.steps.iter().enumerate() {
                    let sat = At::new(
                        format!("{}.steps[{k}]", pat.path),
                        ObjectKind::Step,
                        step.id,
                    );
                    self.check_step(&sat, step, pl, has_root);
                }
            }
        }
    }

    fn check_targets(
        &mut self,
        at: &At,
        p: &Project,
        envs: &HashSet<crate::ids::EnvironmentId>,
        server_env: &HashMap<crate::ids::ServerId, crate::ids::EnvironmentId>,
    ) {
        let mut seen_envs = HashSet::new();
        for (t, targets) in p.targets.iter().enumerate() {
            let base = format!("targets[{t}]");
            if !envs.contains(&targets.environment_id) {
                self.at(
                    at,
                    &format!("{base}.environmentId"),
                    IssueCode::UnknownReference,
                    Message::UnknownTargetEnvironment,
                );
            }
            if !seen_envs.insert(targets.environment_id) {
                self.at(
                    at,
                    &format!("{base}.environmentId"),
                    IssueCode::Duplicate,
                    Message::DuplicateTargetEnvironment,
                );
            }
            let mut seen = HashSet::new();
            for (s, id) in targets.server_ids.iter().enumerate() {
                let field = format!("{base}.serverIds[{s}]");
                match server_env.get(id) {
                    None => self.at(
                        at,
                        &field,
                        IssueCode::UnknownReference,
                        Message::UnknownServer,
                    ),
                    Some(env) if *env != targets.environment_id => self.at(
                        at,
                        &field,
                        IssueCode::WrongEnvironment,
                        Message::WrongEnvironment,
                    ),
                    Some(_) => {}
                }
                if !seen.insert(*id) {
                    self.at(at, &field, IssueCode::Duplicate, Message::DuplicateServer);
                }
            }
        }
    }

    fn check_overrides(
        &mut self,
        at: &At,
        p: &Project,
        globals: &[Variable],
        envs: &HashSet<crate::ids::EnvironmentId>,
    ) {
        let mut seen = HashSet::new();
        for (o, ov) in p.overrides.iter().enumerate() {
            let base = format!("overrides[{o}]");
            if !envs.contains(&ov.environment_id) {
                self.at(
                    at,
                    &format!("{base}.environmentId"),
                    IssueCode::UnknownReference,
                    Message::UnknownOverrideEnvironment,
                );
            }
            let target = p
                .variables
                .iter()
                .chain(globals)
                .find(|v| v.name == ov.name);
            match target {
                None => self.at(
                    at,
                    &format!("{base}.name"),
                    IssueCode::UnknownReference,
                    Message::UnknownVariable {
                        name: ov.name.clone(),
                    },
                ),
                Some(var) => {
                    self.variable_value(at, &format!("{base}.value"), &ov.value, var.sensitive)
                }
            }
            if !seen.insert((ov.environment_id, ov.name.as_str())) {
                self.at(
                    at,
                    &format!("{base}.name"),
                    IssueCode::Duplicate,
                    Message::DuplicateOverride,
                );
            }
        }
    }

    /// STEP-01–06. Disabled steps keep only their common fields checked (PRJ-02).
    fn check_step(&mut self, at: &At, step: &Step, pl: &Pipeline, has_root: bool) {
        self.name(at, "name", Label::StepName, &step.name, NAME_MAX);
        let kind = step.spec.kind();
        if step.failure_policy == StepFailurePolicy::SkipServer {
            if matches!(kind, StepKind::Local | StepKind::Confirm) {
                self.at(
                    at,
                    "failurePolicy",
                    IssueCode::NotAllowed,
                    Message::SkipServerKind,
                );
            } else if pl.execution_order != ExecutionOrder::ServerFirst {
                self.at(
                    at,
                    "failurePolicy",
                    IssueCode::NotAllowed,
                    Message::SkipServerOrder,
                );
            }
        }
        if !step.enabled {
            return;
        }
        if kind != StepKind::Confirm {
            self.range(
                at,
                "timeoutSec",
                Label::Timeout,
                step.timeout_sec,
                1,
                TIMEOUT_MAX_SEC,
            );
        }
        match &step.spec {
            StepSpec::Local(l) => {
                if l.command.trim().is_empty() {
                    let label = Label::Command;
                    self.at(
                        at,
                        "spec.command",
                        IssueCode::Required,
                        Message::Required { label },
                    );
                }
                let cwd = l.cwd.as_deref().filter(|c| !c.trim().is_empty());
                if l.shell == Shell::Wsl {
                    if l.wsl_distro.as_deref().is_none_or(|d| d.trim().is_empty()) {
                        self.at(
                            at,
                            "spec.wslDistro",
                            IssueCode::Required,
                            Message::WslDistro,
                        );
                    }
                    // The project root is a Windows path; WSL never guesses a mapping (STEP-02).
                    match cwd {
                        Some(c) if !has_template(c) && !c.starts_with('/') => self.at(
                            at,
                            "spec.cwd",
                            IssueCode::InvalidFormat,
                            Message::WslCwdAbsolute,
                        ),
                        None => {
                            self.at(at, "spec.cwd", IssueCode::Required, Message::WslCwdRequired)
                        }
                        _ => {}
                    }
                } else {
                    match cwd {
                        Some(c) if !has_template(c) && !is_windows_absolute(c) && !has_root => self
                            .at(
                                at,
                                "spec.cwd",
                                IssueCode::InvalidFormat,
                                Message::RelativeCwd,
                            ),
                        None if !has_root => {
                            self.at(at, "spec.cwd", IssueCode::Required, Message::CwdRequired)
                        }
                        _ => {}
                    }
                }
            }
            StepSpec::Upload(u) => {
                if u.source.trim().is_empty() {
                    let label = Label::Source;
                    self.at(
                        at,
                        "spec.source",
                        IssueCode::Required,
                        Message::Required { label },
                    );
                } else if !has_template(&u.source) && !is_windows_absolute(&u.source) && !has_root {
                    self.at(
                        at,
                        "spec.source",
                        IssueCode::InvalidFormat,
                        Message::RelativeSource,
                    );
                }
                self.remote_absolute(at, "spec.target", Label::Target, &u.target, true);
            }
            StepSpec::Remote(r) => {
                match &r.command {
                    RemoteCommand::Script { script } => {
                        if script.trim().is_empty() {
                            self.at(
                                at,
                                "spec.command.script",
                                IssueCode::Required,
                                Message::Required {
                                    label: Label::Command,
                                },
                            );
                        }
                    }
                    RemoteCommand::Program { executable, args } => {
                        self.remote_absolute(
                            at,
                            "spec.command.executable",
                            Label::Executable,
                            executable,
                            true,
                        );
                        for (a, arg) in args.iter().enumerate() {
                            if arg.contains('\0') {
                                self.at(
                                    at,
                                    &format!("spec.command.args[{a}]"),
                                    IssueCode::InvalidFormat,
                                    Message::ArgNul,
                                );
                            }
                        }
                    }
                }
                if let Some(cwd) = r.cwd.as_deref().filter(|c| !c.trim().is_empty()) {
                    self.remote_absolute(at, "spec.cwd", Label::RemoteCwd, cwd, false);
                }
                match &r.privilege {
                    StepPrivilege::SudoNonInteractive { run_as }
                    | StepPrivilege::SudoPassword { run_as } => {
                        self.login_name(at, "spec.privilege.runAs", Label::RunAs, run_as);
                    }
                    StepPrivilege::Inherit | StepPrivilege::None => {}
                }
            }
            StepSpec::Confirm(c) => {
                self.name(
                    at,
                    "spec.prompt",
                    Label::Prompt,
                    &c.prompt,
                    CONFIRM_PROMPT_MAX,
                );
            }
        }
    }

    fn remote_absolute(&mut self, at: &At, field: &str, label: Label, value: &str, required: bool) {
        if value.trim().is_empty() {
            if required {
                self.at(at, field, IssueCode::Required, Message::Required { label });
            }
        } else if !has_template(value) && !value.starts_with('/') {
            self.at(
                at,
                field,
                IssueCode::InvalidFormat,
                Message::RemoteAbsolute { label },
            );
        }
    }
}

fn unique(
    v: &mut Validator,
    seen: &mut HashSet<String>,
    at: &At,
    field: &str,
    label: Label,
    value: &str,
) {
    if value.is_empty() {
        return;
    }
    if !seen.insert(value.to_owned()) {
        v.at(
            at,
            field,
            IssueCode::Duplicate,
            Message::DuplicateName {
                label,
                value: value.to_owned(),
            },
        );
    }
}

/// `${...}` makes a path depend on variables; its syntax is checked after resolution.
fn has_template(s: &str) -> bool {
    crate::template::has_expressions(s)
}

pub fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `C:\...`, `C:/...` or a UNC path `\\server\share`.
pub fn is_windows_absolute(p: &str) -> bool {
    let b = p.as_bytes();
    (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/'))
        || p.starts_with(r"\\")
}

/// SRV-01: DNS name, IPv4 or IPv6, without scheme, path, user or port.
pub fn check_host(host: &str) -> Result<(), Message> {
    let h = host.trim();
    if h.is_empty() {
        return Err(Message::Required {
            label: Label::Address,
        });
    }
    if h != host {
        return Err(Message::HostWhitespace);
    }
    if h.contains("://") {
        return Err(Message::HostScheme);
    }
    if h.contains('/') || h.contains('\\') {
        return Err(Message::HostPath);
    }
    if h.contains('@') {
        return Err(Message::HostUser);
    }
    if h.starts_with('[') || h.ends_with(']') {
        return Err(Message::HostBrackets);
    }
    if h.parse::<Ipv6Addr>().is_ok() || h.parse::<Ipv4Addr>().is_ok() {
        return Ok(());
    }
    if h.contains(':') {
        return Err(Message::HostPort);
    }
    if h.split('.')
        .all(|label| label.chars().all(|c| c.is_ascii_digit()))
    {
        return Err(Message::HostIpv4);
    }
    if h.len() > 253 {
        return Err(Message::HostTooLong);
    }
    let label_ok = |l: &str| {
        !l.is_empty()
            && l.len() <= 63
            && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            && !l.starts_with('-')
            && !l.ends_with('-')
    };
    if h.split('.').all(label_ok) {
        Ok(())
    } else {
        Err(Message::HostChars)
    }
}
