//! Field validation for a workspace draft (SRV-01, PRJ-01, PRJ-02, STEP-01–06, DATA-02/03, VAR-01).
//!
//! Every problem becomes an [`Issue`] that names the object and field, so the UI can jump to it.
//! Path syntax that depends on variables (`${...}`) is checked later by preflight, after resolution.

use std::collections::{HashMap, HashSet};
use std::net::{Ipv4Addr, Ipv6Addr};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

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
    /// User-facing message in Chinese.
    pub message: String,
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
                message: format!("字段格式不正确：{}", err.inner()),
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
            format!(
                "配置版本 {} 与程序支持的版本 {} 不一致",
                ws.schema_version, SCHEMA_VERSION
            ),
        );
    }
    v.check_ids(ws);
    v.check_environments(ws);
    v.check_servers(ws);
    v.check_variables("globalVariables", &ws.global_variables);
    v.check_projects(ws);
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
        message: String,
    ) {
        self.issues.push(Issue {
            path,
            object,
            field: last_segment(field),
            code,
            message,
        });
    }

    fn at(&mut self, at: &At, field: &str, code: IssueCode, message: impl Into<String>) {
        let path = format!("{}.{}", at.path, field);
        self.push(path, at.object.clone(), field, code, message.into());
    }

    /// Required name: trimmed length 1..=max.
    fn name(&mut self, at: &At, field: &str, label: &str, value: &str, max: usize) {
        let len = value.trim().chars().count();
        if len == 0 {
            self.at(at, field, IssueCode::Required, format!("{label}不能为空"));
        } else if len > max {
            self.at(
                at,
                field,
                IssueCode::TooLong,
                format!("{label}最多 {max} 个字符，当前 {len} 个"),
            );
        }
    }

    fn max_len(&mut self, at: &At, field: &str, label: &str, value: &str, max: usize) {
        let len = value.chars().count();
        if len > max {
            self.at(
                at,
                field,
                IssueCode::TooLong,
                format!("{label}最多 {max} 个字符，当前 {len} 个"),
            );
        }
    }

    fn range(&mut self, at: &At, field: &str, label: &str, value: u32, min: u32, max: u32) {
        if !(min..=max).contains(&value) {
            self.at(
                at,
                field,
                IssueCode::OutOfRange,
                format!("{label}必须在 {min}–{max} 之间，当前为 {value}"),
            );
        }
    }

    /// DATA-02: IDs are unique across every object kind.
    fn check_ids(&mut self, ws: &Workspace) {
        let mut seen: HashSet<String> = HashSet::new();
        let mut check = |v: &mut Self, at: At| {
            let id = at.object.as_ref().map(|o| o.id.clone()).unwrap_or_default();
            if !seen.insert(id.clone()) {
                v.at(&at, "id", IssueCode::DuplicateId, format!("ID {id} 重复"));
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
            self.name(&at, "name", "环境名称", &e.name, ENV_NAME_MAX);
            unique(self, &mut names, &at, "name", "环境名称", e.name.trim());
        }
    }

    /// SRV-01, SRV-02, SEC-01.
    fn check_servers(&mut self, ws: &Workspace) {
        let envs: HashSet<_> = ws.environments.iter().map(|e| e.id).collect();
        let mut names: HashSet<String> = HashSet::new();
        for (i, s) in ws.servers.iter().enumerate() {
            let at = At::new(format!("servers[{i}]"), ObjectKind::Server, s.id);
            self.name(&at, "name", "服务器名称", &s.name, NAME_MAX);
            unique(self, &mut names, &at, "name", "服务器名称", s.name.trim());
            if let Err(msg) = check_host(&s.host) {
                let code = if s.host.trim().is_empty() {
                    IssueCode::Required
                } else {
                    IssueCode::InvalidFormat
                };
                self.at(&at, "host", code, msg);
            }
            self.range(&at, "port", "端口", s.port, 1, 65_535);
            self.login_name(&at, "user", "登录用户", &s.user);
            if !envs.contains(&s.environment_id) {
                self.at(
                    &at,
                    "environmentId",
                    IssueCode::UnknownReference,
                    "所属环境不存在",
                );
            }
            self.range(
                &at,
                "connectTimeoutSec",
                "连接超时（秒）",
                s.connect_timeout_sec,
                1,
                CONNECT_TIMEOUT_MAX_SEC,
            );
            for (t, tag) in s.tags.iter().enumerate() {
                let field = format!("tags[{t}]");
                self.name(&at, &field, "标签", tag, TAG_MAX);
            }
            self.max_len(&at, "notes", "备注", &s.notes, NOTES_MAX);
            match &s.auth {
                AuthMethod::KeyFile { path } if path.trim().is_empty() => {
                    self.at(
                        &at,
                        "auth.path",
                        IssueCode::Required,
                        "私钥文件路径不能为空",
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
                    self.login_name(&at, "privilege.runAs", "提权目标用户", run_as);
                }
                PrivilegePolicy::None => {}
            }
        }
    }

    fn login_name(&mut self, at: &At, field: &str, label: &str, value: &str) {
        if value.is_empty() {
            self.at(at, field, IssueCode::Required, format!("{label}不能为空"));
        } else if value.chars().any(char::is_whitespace) {
            self.at(
                at,
                field,
                IssueCode::InvalidFormat,
                format!("{label}不能包含空白字符"),
            );
        } else if value.chars().count() > NAME_MAX {
            self.at(
                at,
                field,
                IssueCode::TooLong,
                format!("{label}最多 {NAME_MAX} 个字符"),
            );
        }
    }

    fn env_var_name(&mut self, at: &At, field: &str, name: &str) {
        if name.trim().is_empty() {
            self.at(at, field, IssueCode::Required, "环境变量名不能为空");
        } else if name.contains('=') || name.contains('\0') || name.chars().any(char::is_whitespace)
        {
            self.at(
                at,
                field,
                IssueCode::InvalidFormat,
                "环境变量名不能包含 =、空白或 NUL 字符",
            );
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
                self.at(
                    &at,
                    "name",
                    code,
                    "变量名须以字母或下划线开头，只含字母、数字和下划线",
                );
            } else if RESERVED_VARIABLE_NAMES.contains(&var.name.as_str()) {
                self.at(
                    &at,
                    "name",
                    IssueCode::Reserved,
                    format!("{} 是内建命名空间，不能用作变量名", var.name),
                );
            }
            unique(self, &mut names, &at, "name", "变量名", &var.name);
            self.variable_value(&at, "value", &var.value, var.sensitive);
        }
    }

    fn variable_value(&mut self, at: &At, field: &str, value: &VariableValue, sensitive: bool) {
        match value {
            VariableValue::Literal { .. } if sensitive => {
                self.at(
                    at,
                    field,
                    IssueCode::NotAllowed,
                    "敏感变量不能以明文保存，请使用凭据管理器或环境变量",
                );
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
            self.name(&at, "name", "项目名称", &p.name, NAME_MAX);
            unique(self, &mut names, &at, "name", "项目名称", p.name.trim());
            self.max_len(&at, "description", "说明", &p.description, NOTES_MAX);
            if let Some(root) = p.root.as_deref().filter(|r| !r.trim().is_empty())
                && !is_windows_absolute(root)
            {
                self.at(
                    &at,
                    "root",
                    IssueCode::InvalidFormat,
                    "根目录必须是本地绝对路径，例如 D:\\workspace\\app",
                );
            }
            if !envs.contains(&p.default_environment_id) {
                self.at(
                    &at,
                    "defaultEnvironmentId",
                    IssueCode::UnknownReference,
                    "默认环境不存在",
                );
            }
            self.check_targets(&at, p, &envs, &server_env);
            self.check_variables(&format!("{}.variables", at.path), &p.variables);
            self.check_overrides(&at, p, &ws.global_variables, &envs);
            if p.pipelines.is_empty() {
                self.at(
                    &at,
                    "pipelines",
                    IssueCode::Required,
                    "项目至少需要一个流程",
                );
            }
            let mut pipeline_names: HashSet<String> = HashSet::new();
            let has_root = p.root.as_deref().is_some_and(|r| !r.trim().is_empty());
            for (j, pl) in p.pipelines.iter().enumerate() {
                let pat = At::new(
                    format!("{}.pipelines[{j}]", at.path),
                    ObjectKind::Pipeline,
                    pl.id,
                );
                self.name(&pat, "name", "流程名称", &pl.name, NAME_MAX);
                unique(
                    self,
                    &mut pipeline_names,
                    &pat,
                    "name",
                    "流程名称",
                    pl.name.trim(),
                );
                if pl.failure_policy == PipelineFailurePolicy::SkipServer
                    && pl.execution_order != ExecutionOrder::ServerFirst
                {
                    self.at(
                        &pat,
                        "failurePolicy",
                        IssueCode::NotAllowed,
                        "“跳过该服务器”只能用于服务器优先的流程",
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
                    "目标绑定的环境不存在",
                );
            }
            if !seen_envs.insert(targets.environment_id) {
                self.at(
                    at,
                    &format!("{base}.environmentId"),
                    IssueCode::Duplicate,
                    "同一环境的目标绑定重复",
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
                        "绑定的服务器不存在",
                    ),
                    Some(env) if *env != targets.environment_id => self.at(
                        at,
                        &field,
                        IssueCode::WrongEnvironment,
                        "只能绑定该环境下的服务器",
                    ),
                    Some(_) => {}
                }
                if !seen.insert(*id) {
                    self.at(at, &field, IssueCode::Duplicate, "同一服务器重复绑定");
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
                    "覆盖值所属环境不存在",
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
                    format!("变量 {} 不存在", ov.name),
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
                    "同一环境对同一变量的覆盖重复",
                );
            }
        }
    }

    /// STEP-01–06. Disabled steps keep only their common fields checked (PRJ-02).
    fn check_step(&mut self, at: &At, step: &Step, pl: &Pipeline, has_root: bool) {
        self.name(at, "name", "步骤名称", &step.name, NAME_MAX);
        let kind = step.spec.kind();
        if step.failure_policy == StepFailurePolicy::SkipServer {
            if matches!(kind, StepKind::Local | StepKind::Confirm) {
                self.at(
                    at,
                    "failurePolicy",
                    IssueCode::NotAllowed,
                    "本地命令和人工确认不能选择“跳过该服务器”",
                );
            } else if pl.execution_order != ExecutionOrder::ServerFirst {
                self.at(
                    at,
                    "failurePolicy",
                    IssueCode::NotAllowed,
                    "“跳过该服务器”只能用于服务器优先的流程",
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
                "超时（秒）",
                step.timeout_sec,
                1,
                TIMEOUT_MAX_SEC,
            );
        }
        match &step.spec {
            StepSpec::Local(l) => {
                if l.command.trim().is_empty() {
                    self.at(at, "spec.command", IssueCode::Required, "命令不能为空");
                }
                let cwd = l.cwd.as_deref().filter(|c| !c.trim().is_empty());
                if l.shell == Shell::Wsl {
                    if l.wsl_distro.as_deref().is_none_or(|d| d.trim().is_empty()) {
                        self.at(
                            at,
                            "spec.wslDistro",
                            IssueCode::Required,
                            "WSL 步骤必须选择发行版",
                        );
                    }
                    // The project root is a Windows path; WSL never guesses a mapping (STEP-02).
                    match cwd {
                        Some(c) if !has_template(c) && !c.starts_with('/') => self.at(
                            at,
                            "spec.cwd",
                            IssueCode::InvalidFormat,
                            "WSL 工作目录必须是 Linux 绝对路径，例如 /home/user/app",
                        ),
                        None => self.at(
                            at,
                            "spec.cwd",
                            IssueCode::Required,
                            "WSL 步骤必须填写 Linux 工作目录",
                        ),
                        _ => {}
                    }
                } else {
                    match cwd {
                        Some(c) if !has_template(c) && !is_windows_absolute(c) && !has_root => self
                            .at(
                                at,
                                "spec.cwd",
                                IssueCode::InvalidFormat,
                                "相对工作目录需要项目根目录，请填写绝对路径或设置项目根目录",
                            ),
                        None if !has_root => self.at(
                            at,
                            "spec.cwd",
                            IssueCode::Required,
                            "未设置项目根目录时必须填写工作目录",
                        ),
                        _ => {}
                    }
                }
            }
            StepSpec::Upload(u) => {
                if u.source.trim().is_empty() {
                    self.at(at, "spec.source", IssueCode::Required, "本地源路径不能为空");
                } else if !has_template(&u.source) && !is_windows_absolute(&u.source) && !has_root {
                    self.at(
                        at,
                        "spec.source",
                        IssueCode::InvalidFormat,
                        "相对源路径需要项目根目录",
                    );
                }
                self.remote_absolute(at, "spec.target", "远端目标", &u.target, true);
            }
            StepSpec::Remote(r) => {
                match &r.command {
                    RemoteCommand::Script { script } => {
                        if script.trim().is_empty() {
                            self.at(
                                at,
                                "spec.command.script",
                                IssueCode::Required,
                                "命令不能为空",
                            );
                        }
                    }
                    RemoteCommand::Program { executable, args } => {
                        self.remote_absolute(
                            at,
                            "spec.command.executable",
                            "可执行文件",
                            executable,
                            true,
                        );
                        for (a, arg) in args.iter().enumerate() {
                            if arg.contains('\0') {
                                self.at(
                                    at,
                                    &format!("spec.command.args[{a}]"),
                                    IssueCode::InvalidFormat,
                                    "参数不能包含 NUL 字符",
                                );
                            }
                        }
                    }
                }
                if let Some(cwd) = r.cwd.as_deref().filter(|c| !c.trim().is_empty()) {
                    self.remote_absolute(at, "spec.cwd", "远端目录", cwd, false);
                }
                match &r.privilege {
                    StepPrivilege::SudoNonInteractive { run_as }
                    | StepPrivilege::SudoPassword { run_as } => {
                        self.login_name(at, "spec.privilege.runAs", "提权目标用户", run_as);
                    }
                    StepPrivilege::Inherit | StepPrivilege::None => {}
                }
            }
            StepSpec::Confirm(c) => {
                self.name(at, "spec.prompt", "确认提示", &c.prompt, CONFIRM_PROMPT_MAX);
            }
        }
    }

    fn remote_absolute(&mut self, at: &At, field: &str, label: &str, value: &str, required: bool) {
        if value.trim().is_empty() {
            if required {
                self.at(at, field, IssueCode::Required, format!("{label}不能为空"));
            }
        } else if !has_template(value) && !value.starts_with('/') {
            self.at(
                at,
                field,
                IssueCode::InvalidFormat,
                format!("{label}必须是远端绝对路径（以 / 开头）"),
            );
        }
    }
}

fn unique(
    v: &mut Validator,
    seen: &mut HashSet<String>,
    at: &At,
    field: &str,
    label: &str,
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
            format!("{label}“{value}”已存在"),
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
pub fn check_host(host: &str) -> Result<(), String> {
    let h = host.trim();
    if h.is_empty() {
        return Err("地址不能为空".into());
    }
    if h != host {
        return Err("地址前后不能有空白".into());
    }
    if h.contains("://") {
        return Err("地址不能包含协议（如 ssh://），请只填写主机名或 IP".into());
    }
    if h.contains('/') || h.contains('\\') {
        return Err("地址不能包含路径".into());
    }
    if h.contains('@') {
        return Err("地址不能包含用户名，登录用户请单独填写".into());
    }
    if h.starts_with('[') || h.ends_with(']') {
        return Err("IPv6 地址不需要方括号，端口请单独填写".into());
    }
    if h.parse::<Ipv6Addr>().is_ok() || h.parse::<Ipv4Addr>().is_ok() {
        return Ok(());
    }
    if h.contains(':') {
        return Err("地址不能包含端口，端口请单独填写".into());
    }
    if h.split('.')
        .all(|label| label.chars().all(|c| c.is_ascii_digit()))
    {
        return Err("IPv4 地址格式不正确".into());
    }
    if h.len() > 253 {
        return Err("主机名最长 253 个字符".into());
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
        Err("主机名只能包含字母、数字、连字符和点".into())
    }
}
