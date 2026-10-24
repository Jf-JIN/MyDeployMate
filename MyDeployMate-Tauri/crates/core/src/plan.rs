//! PlanBuilder and preflight (SRS RUN-01–03, architecture ARC-12, plan tasks 2.3 and 2.4).
//!
//! [`build_plan`] turns a workspace, a pipeline, an environment and the run options into the
//! ordered task list the preview shows and the scheduler runs. It has no side effects: it does
//! not connect, read key files, ask for passwords or check that upload sources exist yet.
//! Every problem that blocks the start becomes an [`Issue`] pointing at the step field.

use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::ids::{EnvironmentId, PipelineId, ProjectId, ServerId, StepId};
use crate::message::{Label, Message};
use crate::model::*;
use crate::resolve::{
    Context, ResolveError, ResolveErrorKind, ResolvedText, Resolver, VERSION_VARIABLE, ValueSource,
};
use crate::validate::{Issue, IssueCode, ObjectKind, ObjectRef, is_windows_absolute};

/// RUN-02: version length after trimming.
pub const VERSION_MAX: usize = 128;

/// What the operator chose in the preview dialog (D03).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlanRequest {
    pub project_id: ProjectId,
    pub pipeline_id: PipelineId,
    pub environment_id: EnvironmentId,
    pub scope: RunScope,
    /// Subset of the servers bound to the environment; `None` means all of them.
    /// Execution always follows the binding order, not the order given here.
    pub server_ids: Option<Vec<ServerId>>,
    /// Run override of `${version}`; `None` uses the resolved `version` variable.
    pub version: Option<String>,
}

/// RUN-03: the full pipeline, or only its local commands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RunScope {
    #[default]
    Full,
    LocalOnly,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PlanError {
    #[error("project not found")]
    ProjectNotFound,
    #[error("pipeline not found")]
    PipelineNotFound,
    #[error("environment not found")]
    EnvironmentNotFound,
}

impl PlanError {
    pub fn message(&self) -> Message {
        match self {
            PlanError::ProjectNotFound => Message::ProjectNotFound,
            PlanError::PipelineNotFound => Message::PipelineNotFound,
            PlanError::EnvironmentNotFound => Message::EnvironmentNotFound,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ResolvedPlan {
    /// Workspace revision the plan was built from (ARC-13).
    #[ts(type = "number")]
    pub revision: u64,
    pub project_id: ProjectId,
    pub pipeline_id: PipelineId,
    pub environment_id: EnvironmentId,
    pub project_name: String,
    pub pipeline_name: String,
    pub environment_name: String,
    /// The operator must type the environment name before starting (RUN-02).
    pub protected: bool,
    pub execution_order: ExecutionOrder,
    pub failure_policy: PipelineFailurePolicy,
    pub scope: RunScope,
    /// Trimmed version, when one is known.
    pub version: Option<String>,
    /// Servers bound to the environment, in execution order, with the operator's selection.
    pub targets: Vec<PlanTarget>,
    pub tasks: Vec<PlannedTask>,
    /// Problems that block the start. Empty means the plan may run.
    pub issues: Vec<Issue>,
    #[serde(skip)]
    #[ts(skip)]
    pub secrets: SecretValues,
}

impl ResolvedPlan {
    pub fn can_start(&self) -> bool {
        self.issues.is_empty() && !self.tasks.is_empty()
    }
}

/// Real values of sensitive variables used by the plan, for the log redactor. Never printed.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct SecretValues(pub Vec<String>);

impl fmt::Debug for SecretValues {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretValues({})", self.0.len())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlanTarget {
    pub server_id: ServerId,
    pub name: String,
    pub host: String,
    pub port: u32,
    pub user: String,
    pub selected: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlannedTask {
    /// Position in the plan, from 0. The UI shows `seq + 1`.
    pub seq: u32,
    pub step_id: StepId,
    /// Position of the step in the pipeline, from 0.
    pub step_index: u32,
    pub step_name: String,
    pub kind: StepKind,
    /// Preview grouping (IXD-23). Step-first: one group per step. Server-first: local and
    /// confirm steps have a group each, and the remote steps between them form one group that
    /// the UI splits further by server.
    pub segment: u32,
    /// `None` for local and confirm tasks.
    pub server: Option<TaskServer>,
    /// `None` for confirm tasks, which have no execution timeout.
    pub timeout_sec: Option<u32>,
    /// Step policy with `Inherit` replaced by the pipeline's. Local and confirm tasks never
    /// skip a server: an inherited "skip this server" acts as "stop" for them.
    pub failure_policy: PipelineFailurePolicy,
    pub action: TaskAction,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TaskServer {
    pub id: ServerId,
    pub name: String,
    pub host: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum TaskAction {
    Local {
        command: ResolvedText,
        /// Absolute working directory; `None` only for WSL, which then needs a configured one.
        cwd: Option<ResolvedText>,
        shell: Shell,
        wsl_distro: Option<String>,
        output_encoding: OutputEncoding,
    },
    Upload {
        /// Absolute local path.
        source: ResolvedText,
        source_kind: SourceKind,
        target: ResolvedText,
        conflict: ConflictPolicy,
    },
    Remote {
        command: ResolvedCommand,
        cwd: Option<ResolvedText>,
        /// Step privilege with `Inherit` replaced by the server default (SEC-01).
        privilege: PrivilegePolicy,
        /// The step inherits the server default, shown next to the privilege (IXD-33). Runs
        /// recorded before this field existed read as `false`.
        #[serde(default)]
        privilege_inherited: bool,
        /// Script mode with sudo: the server must allow a shell (STEP-05, IXD-33).
        shell_elevation: bool,
    },
    Confirm {
        prompt: ResolvedText,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum ResolvedCommand {
    Script {
        script: ResolvedText,
    },
    Program {
        executable: ResolvedText,
        args: Vec<ResolvedText>,
    },
}

fn is_remote_kind(kind: StepKind) -> bool {
    matches!(kind, StepKind::Upload | StepKind::Remote)
}

/// Build the plan and run every preflight check (RUN-01). `source` supplies process environment
/// variables and Credential Manager values; nothing else is read.
pub fn build_plan(
    ws: &Workspace,
    req: &PlanRequest,
    source: &dyn ValueSource,
) -> Result<ResolvedPlan, PlanError> {
    let (pi, project) = ws
        .projects
        .iter()
        .enumerate()
        .find(|(_, p)| p.id == req.project_id)
        .ok_or(PlanError::ProjectNotFound)?;
    let (li, pipeline) = project
        .pipelines
        .iter()
        .enumerate()
        .find(|(_, p)| p.id == req.pipeline_id)
        .ok_or(PlanError::PipelineNotFound)?;
    let environment = ws
        .environments
        .iter()
        .find(|e| e.id == req.environment_id)
        .ok_or(PlanError::EnvironmentNotFound)?;

    let local_only = req.scope == RunScope::LocalOnly;
    let steps: Vec<(usize, &Step)> = pipeline
        .steps
        .iter()
        .enumerate()
        .filter(|(_, s)| s.enabled && (!local_only || s.spec.kind() == StepKind::Local))
        .collect();

    let mut b = Builder {
        issues: Vec::new(),
        step_prefix: format!("projects[{pi}].pipelines[{li}].steps"),
    };

    // Targets: only servers bound to this environment, in binding order (DATA-03, ARC-13).
    let bound: Vec<&Server> = project
        .targets
        .iter()
        .find(|t| t.environment_id == environment.id)
        .map(|t| {
            t.server_ids
                .iter()
                .filter_map(|id| ws.servers.iter().find(|s| s.id == *id))
                .collect()
        })
        .unwrap_or_default();
    if let (false, Some(ids)) = (local_only, &req.server_ids) {
        for (i, id) in ids.iter().enumerate() {
            if !bound.iter().any(|s| s.id == *id) {
                b.plain(
                    format!("serverIds[{i}]"),
                    IssueCode::NotBound,
                    Message::NotBound,
                );
            }
        }
    }
    let is_selected = |s: &Server| {
        !local_only
            && req
                .server_ids
                .as_ref()
                .is_none_or(|ids| ids.contains(&s.id))
    };
    let selected: Vec<&Server> = bound.iter().copied().filter(|s| is_selected(s)).collect();

    b.workspace_issues(ws, pi, li, &steps, &selected);

    // Version (RUN-02, VAR-01).
    let resolver = Resolver::new(ws, project, environment, source);
    let version = match &req.version {
        Some(v) => Some(v.trim().to_owned()),
        None if resolver.is_defined(VERSION_VARIABLE) => {
            match resolver.resolve_variable(VERSION_VARIABLE, Context::LOCAL) {
                Ok(t) => Some(t.expose().trim().to_owned()),
                Err(e) => {
                    b.plain("version".into(), issue_code(&e), e.message());
                    None
                }
            }
        }
        None => None,
    };
    let version = version.filter(|v| b.check_version(v));
    if version.is_none() && !b.issues.iter().any(|i| i.path == "version") {
        b.plain(
            "version".into(),
            IssueCode::Required,
            Message::VersionRequired,
        );
    }
    let resolver = resolver.with_version(version.as_deref());

    // Expansion (RUN-03).
    let has_remote = steps.iter().any(|(_, s)| is_remote_kind(s.spec.kind()));
    if has_remote && selected.is_empty() {
        b.plain("serverIds".into(), IssueCode::NoTargets, Message::NoTargets);
    }
    let mut planned: Vec<(usize, &Step, Option<&Server>, u32)> = Vec::new();
    match pipeline.execution_order {
        ExecutionOrder::StepFirst => {
            for (segment, &(k, step)) in steps.iter().enumerate() {
                let segment = segment as u32;
                if is_remote_kind(step.spec.kind()) {
                    for server in &selected {
                        planned.push((k, step, Some(server), segment));
                    }
                } else {
                    planned.push((k, step, None, segment));
                }
            }
        }
        ExecutionOrder::ServerFirst => {
            let mut segment = 0;
            let mut i = 0;
            while i < steps.len() {
                if !is_remote_kind(steps[i].1.spec.kind()) {
                    planned.push((steps[i].0, steps[i].1, None, segment));
                    i += 1;
                } else {
                    let end = steps[i..]
                        .iter()
                        .position(|(_, s)| !is_remote_kind(s.spec.kind()))
                        .map_or(steps.len(), |n| i + n);
                    for server in &selected {
                        for &(k, step) in &steps[i..end] {
                            planned.push((k, step, Some(server), segment));
                        }
                    }
                    i = end;
                }
                segment += 1;
            }
        }
    }

    let root = project.root.as_deref().filter(|r| !r.trim().is_empty());
    let tasks: Vec<PlannedTask> = planned
        .into_iter()
        .enumerate()
        .map(|(seq, (k, step, server, segment))| {
            let kind = step.spec.kind();
            let mut policy = match step.failure_policy {
                StepFailurePolicy::Inherit => pipeline.failure_policy,
                StepFailurePolicy::Stop => PipelineFailurePolicy::Stop,
                StepFailurePolicy::Continue => PipelineFailurePolicy::Continue,
                StepFailurePolicy::SkipServer => PipelineFailurePolicy::SkipServer,
            };
            if server.is_none() && policy == PipelineFailurePolicy::SkipServer {
                policy = PipelineFailurePolicy::Stop;
            }
            let at = StepAt {
                index: k,
                step,
                server,
            };
            PlannedTask {
                seq: seq as u32,
                step_id: step.id,
                step_index: k as u32,
                step_name: step.name.clone(),
                kind,
                segment,
                server: server.map(|s| TaskServer {
                    id: s.id,
                    name: s.name.clone(),
                    host: s.host.clone(),
                }),
                timeout_sec: (kind != StepKind::Confirm).then_some(step.timeout_sec),
                failure_policy: policy,
                action: b.action(&resolver, &at, root),
            }
        })
        .collect();

    if tasks.is_empty() && !b.issues.iter().any(|i| i.code == IssueCode::NoTargets) {
        let message = if local_only {
            Message::NoLocalSteps
        } else {
            Message::NoSteps
        };
        b.plain("steps".into(), IssueCode::NoTasks, message);
    }

    // Authentication references that can be checked offline (RUN-01).
    if tasks.iter().any(|t| t.server.is_some()) {
        for (i, s) in ws.servers.iter().enumerate() {
            if !selected.iter().any(|t| t.id == s.id) {
                continue;
            }
            if let AuthMethod::Password {
                source: PasswordSource::EnvVar { name },
            } = &s.auth
                && !name.trim().is_empty()
                && source.env_var(name).is_none()
            {
                b.issues.push(Issue {
                    path: format!("servers[{i}].auth.source.name"),
                    object: Some(ObjectRef {
                        kind: ObjectKind::Server,
                        id: s.id.to_string(),
                    }),
                    field: "auth.source.name".into(),
                    code: IssueCode::MissingValue,
                    message: Message::PasswordEnvUnset {
                        server: s.name.clone(),
                        name: name.clone(),
                    },
                });
            }
        }
    }

    let mut secrets = Vec::new();
    for t in &tasks {
        for text in t.action.texts() {
            secrets.extend(text.secrets().iter().cloned());
        }
    }
    secrets.sort();
    secrets.dedup();

    Ok(ResolvedPlan {
        revision: ws.revision,
        project_id: project.id,
        pipeline_id: pipeline.id,
        environment_id: environment.id,
        project_name: project.name.clone(),
        pipeline_name: pipeline.name.clone(),
        environment_name: environment.name.clone(),
        protected: environment.protected,
        execution_order: pipeline.execution_order,
        failure_policy: pipeline.failure_policy,
        scope: req.scope,
        version,
        targets: bound
            .iter()
            .map(|s| PlanTarget {
                server_id: s.id,
                name: s.name.clone(),
                host: s.host.clone(),
                port: s.port,
                user: s.user.clone(),
                selected: is_selected(s),
            })
            .collect(),
        tasks,
        issues: b.issues,
        secrets: SecretValues(secrets),
    })
}

impl TaskAction {
    /// Every resolved field, for collecting secrets.
    pub fn texts(&self) -> Vec<&ResolvedText> {
        match self {
            TaskAction::Local { command, cwd, .. } => {
                std::iter::once(command).chain(cwd.as_ref()).collect()
            }
            TaskAction::Upload { source, target, .. } => vec![source, target],
            TaskAction::Remote { command, cwd, .. } => {
                let mut v: Vec<&ResolvedText> = match command {
                    ResolvedCommand::Script { script } => vec![script],
                    ResolvedCommand::Program { executable, args } => {
                        std::iter::once(executable).chain(args).collect()
                    }
                };
                v.extend(cwd.as_ref());
                v
            }
            TaskAction::Confirm { prompt } => vec![prompt],
        }
    }
}

fn issue_code(e: &ResolveError) -> IssueCode {
    match e.kind {
        ResolveErrorKind::Syntax { .. } => IssueCode::InvalidExpression,
        ResolveErrorKind::Undefined { .. } => IssueCode::UndefinedVariable,
        ResolveErrorKind::Cycle => IssueCode::VariableCycle,
        ResolveErrorKind::TooDeep => IssueCode::VariableTooDeep,
        ResolveErrorKind::ServerContext { .. } => IssueCode::ServerContext,
        ResolveErrorKind::ProjectRootMissing => IssueCode::Required,
        ResolveErrorKind::MissingEnvVar { .. } | ResolveErrorKind::MissingSecret { .. } => {
            IssueCode::MissingValue
        }
    }
}

struct StepAt<'a> {
    index: usize,
    step: &'a Step,
    server: Option<&'a Server>,
}

struct Builder {
    issues: Vec<Issue>,
    step_prefix: String,
}

impl Builder {
    /// A plan-level issue (version, targets) with no owning object.
    fn plain(&mut self, path: String, code: IssueCode, message: Message) {
        let field = path.split('[').next().unwrap_or(&path).to_owned();
        self.issues.push(Issue {
            path,
            object: None,
            field,
            code,
            message,
        });
    }

    /// An issue on a step field, e.g. "步骤 1 · 命令：变量 artifact 未定义" (IXD flow 3).
    fn step_issue(
        &mut self,
        at: &StepAt<'_>,
        field: &str,
        label: &Label,
        code: IssueCode,
        reason: Message,
    ) {
        self.push_step_issue(
            at,
            field,
            code,
            Message::StepIssue {
                step: at.index as u32 + 1,
                label: label.clone(),
                reason: Box::new(reason),
            },
        );
    }

    fn push_step_issue(&mut self, at: &StepAt<'_>, field: &str, code: IssueCode, message: Message) {
        let path = format!("{}[{}].{field}", self.step_prefix, at.index);
        if self
            .issues
            .iter()
            .any(|i| i.path == path && i.message == message)
        {
            return;
        }
        self.issues.push(Issue {
            path,
            object: Some(ObjectRef {
                kind: ObjectKind::Step,
                id: at.step.id.to_string(),
            }),
            field: field.to_owned(),
            code,
            message,
        });
    }

    /// Resolve one field; on failure record the issue and keep the raw text for display.
    fn resolve(
        &mut self,
        resolver: &Resolver<'_>,
        at: &StepAt<'_>,
        field: &str,
        label: Label,
        raw: &str,
    ) -> Option<ResolvedText> {
        match resolver.resolve(raw, Context { server: at.server }) {
            Ok(t) => Some(t),
            Err(e) => {
                self.step_issue(at, field, &label, issue_code(&e), e.message());
                None
            }
        }
    }

    /// A path issue found after resolution; names the server because values may differ by server.
    fn path_issue(&mut self, at: &StepAt<'_>, field: &str, label: &Label, reason: Message) {
        match at.server {
            Some(s) => {
                let message = Message::StepServerIssue {
                    step: at.index as u32 + 1,
                    label: label.clone(),
                    server: s.name.clone(),
                    reason: Box::new(reason),
                };
                self.push_step_issue(at, field, IssueCode::InvalidFormat, message);
            }
            None => self.step_issue(at, field, label, IssueCode::InvalidFormat, reason),
        }
    }

    fn control_chars(
        &mut self,
        at: &StepAt<'_>,
        field: &str,
        label: &Label,
        t: &ResolvedText,
    ) -> bool {
        let bad = t.expose().contains(['\n', '\r', '\0']);
        if bad {
            self.path_issue(at, field, label, Message::PathControl);
        }
        !bad
    }

    fn remote_path(&mut self, at: &StepAt<'_>, field: &str, label: &Label, t: &ResolvedText) {
        if self.control_chars(at, field, label, t) && !t.expose().starts_with('/') {
            self.path_issue(at, field, label, Message::MustBeRemoteAbsolute);
        }
    }

    /// A local Windows path: absolute, or relative to the project root.
    fn local_path(
        &mut self,
        at: &StepAt<'_>,
        field: &str,
        label: &Label,
        t: ResolvedText,
        root: Option<&str>,
    ) -> ResolvedText {
        if !self.control_chars(at, field, label, &t) || is_windows_absolute(t.expose()) {
            return t;
        }
        match root {
            Some(root) => t.map(|rel| join_windows(root, rel)),
            None => {
                self.path_issue(at, field, label, Message::RelativePath);
                t
            }
        }
    }

    fn action(&mut self, r: &Resolver<'_>, at: &StepAt<'_>, root: Option<&str>) -> TaskAction {
        let or_raw =
            |t: Option<ResolvedText>, raw: &str| t.unwrap_or_else(|| ResolvedText::plain(raw));
        match &at.step.spec {
            StepSpec::Local(l) => {
                let command = self.resolve(r, at, "spec.command", Label::Command, &l.command);
                let cwd_raw = l.cwd.as_deref().filter(|c| !c.trim().is_empty());
                let cwd = match (cwd_raw, l.shell) {
                    (Some(raw), Shell::Wsl) => self
                        .resolve(r, at, "spec.cwd", Label::Cwd, raw)
                        .inspect(|t| {
                            if self.control_chars(at, "spec.cwd", &Label::Cwd, t)
                                && !t.expose().starts_with('/')
                            {
                                self.path_issue(
                                    at,
                                    "spec.cwd",
                                    &Label::Cwd,
                                    Message::WslCwdAbsolute,
                                );
                            }
                        })
                        .or_else(|| Some(ResolvedText::plain(raw))),
                    (Some(raw), _) => Some(
                        self.resolve(r, at, "spec.cwd", Label::Cwd, raw)
                            .map(|t| self.local_path(at, "spec.cwd", &Label::Cwd, t, root))
                            .unwrap_or_else(|| ResolvedText::plain(raw)),
                    ),
                    (None, Shell::Wsl) => None,
                    (None, _) => root.map(ResolvedText::plain),
                };
                TaskAction::Local {
                    command: or_raw(command, &l.command),
                    cwd,
                    shell: l.shell,
                    wsl_distro: l.wsl_distro.clone(),
                    output_encoding: l.output_encoding,
                }
            }
            StepSpec::Upload(u) => {
                let source = self
                    .resolve(r, at, "spec.source", Label::Source, &u.source)
                    .map(|t| self.local_path(at, "spec.source", &Label::Source, t, root));
                let target = self.resolve(r, at, "spec.target", Label::Target, &u.target);
                if let Some(t) = &target {
                    self.remote_path(at, "spec.target", &Label::Target, t);
                    if u.source_kind == SourceKind::File && t.expose().ends_with('/') {
                        self.path_issue(
                            at,
                            "spec.target",
                            &Label::Target,
                            Message::UploadFileTarget,
                        );
                    }
                }
                TaskAction::Upload {
                    source: or_raw(source, &u.source),
                    source_kind: u.source_kind,
                    target: or_raw(target, &u.target),
                    conflict: u.conflict,
                }
            }
            StepSpec::Remote(rs) => {
                let command = match &rs.command {
                    RemoteCommand::Script { script } => ResolvedCommand::Script {
                        script: or_raw(
                            self.resolve(r, at, "spec.command.script", Label::Command, script),
                            script,
                        ),
                    },
                    RemoteCommand::Program { executable, args } => {
                        let exe = self.resolve(
                            r,
                            at,
                            "spec.command.executable",
                            Label::Executable,
                            executable,
                        );
                        if let Some(t) = &exe {
                            self.remote_path(at, "spec.command.executable", &Label::Executable, t);
                        }
                        let args = args
                            .iter()
                            .enumerate()
                            .map(|(a, raw)| {
                                let field = format!("spec.command.args[{a}]");
                                let label = Label::Argument { n: a as u32 + 1 };
                                let t = self.resolve(r, at, &field, label.clone(), raw);
                                if t.as_ref().is_some_and(|t| t.expose().contains('\0')) {
                                    self.step_issue(
                                        at,
                                        &field,
                                        &label,
                                        IssueCode::InvalidFormat,
                                        Message::ArgNul,
                                    );
                                }
                                or_raw(t, raw)
                            })
                            .collect();
                        ResolvedCommand::Program {
                            executable: or_raw(exe, executable),
                            args,
                        }
                    }
                };
                let cwd = rs
                    .cwd
                    .as_deref()
                    .filter(|c| !c.trim().is_empty())
                    .map(|raw| {
                        let t = self.resolve(r, at, "spec.cwd", Label::RemoteCwd, raw);
                        if let Some(t) = &t {
                            self.remote_path(at, "spec.cwd", &Label::RemoteCwd, t);
                        }
                        or_raw(t, raw)
                    });
                let privilege = match &rs.privilege {
                    StepPrivilege::Inherit => at
                        .server
                        .map_or(PrivilegePolicy::None, |s| s.privilege.clone()),
                    StepPrivilege::None => PrivilegePolicy::None,
                    StepPrivilege::SudoNonInteractive { run_as } => {
                        PrivilegePolicy::SudoNonInteractive {
                            run_as: run_as.clone(),
                        }
                    }
                    StepPrivilege::SudoPassword { run_as } => PrivilegePolicy::SudoPassword {
                        run_as: run_as.clone(),
                    },
                };
                let shell_elevation = matches!(command, ResolvedCommand::Script { .. })
                    && privilege != PrivilegePolicy::None;
                TaskAction::Remote {
                    command,
                    cwd,
                    privilege,
                    privilege_inherited: matches!(rs.privilege, StepPrivilege::Inherit),
                    shell_elevation,
                }
            }
            StepSpec::Confirm(c) => TaskAction::Confirm {
                prompt: or_raw(
                    self.resolve(r, at, "spec.prompt", Label::Prompt, &c.prompt),
                    &c.prompt,
                ),
            },
        }
    }

    fn check_version(&mut self, v: &str) -> bool {
        let problem = if v.is_empty() {
            Some((IssueCode::Required, Message::VersionRequired))
        } else if v.contains(['\n', '\r', '\0']) {
            Some((IssueCode::InvalidFormat, Message::VersionControl))
        } else if v.chars().count() > VERSION_MAX {
            let label = Label::Version;
            let max = VERSION_MAX as u32;
            Some((IssueCode::TooLong, Message::TooLong { label, max }))
        } else {
            None
        };
        match problem {
            Some((code, message)) => {
                self.plain("version".into(), code, message);
                false
            }
            None => true,
        }
    }

    /// Saved-config issues that affect this run: the project itself (other pipelines and steps
    /// that will not run excluded), the selected servers, environments and global variables.
    fn workspace_issues(
        &mut self,
        ws: &Workspace,
        pi: usize,
        li: usize,
        steps: &[(usize, &Step)],
        selected: &[&Server],
    ) {
        let project = format!("projects[{pi}]");
        let pipelines = format!("{project}.pipelines[");
        let pipeline = format!("{project}.pipelines[{li}]");
        let step_paths: Vec<String> = steps
            .iter()
            .map(|(k, _)| format!("{pipeline}.steps[{k}]"))
            .collect();
        let servers: HashSet<String> = ws
            .servers
            .iter()
            .enumerate()
            .filter(|(_, s)| selected.iter().any(|t| t.id == s.id))
            .map(|(i, _)| format!("servers[{i}]"))
            .collect();
        let under = |path: &str, prefix: &str| {
            path.strip_prefix(prefix)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(['.', '[']))
        };
        for issue in crate::validate::validate_workspace(ws).issues {
            let p = issue.path.as_str();
            let relevant = if under(p, &pipeline) {
                !under(p, &format!("{pipeline}.steps")) || step_paths.iter().any(|s| under(p, s))
            } else if p.starts_with(&pipelines) {
                false
            } else {
                under(p, &project)
                    || p == "schemaVersion"
                    || p.starts_with("environments[")
                    || p.starts_with("globalVariables[")
                    || p.split('.')
                        .next()
                        .is_some_and(|head| servers.contains(head))
            };
            if relevant {
                self.issues.push(issue);
            }
        }
    }
}

/// `D:\app` + `dist\x` → `D:\app\dist\x`. A leading `.\` or `./` on the relative part is dropped.
fn join_windows(root: &str, rel: &str) -> String {
    let rel = rel
        .strip_prefix(".\\")
        .or_else(|| rel.strip_prefix("./"))
        .unwrap_or(rel);
    let root = root.trim_end_matches(['\\', '/']);
    if rel.is_empty() || rel == "." {
        root.to_owned()
    } else {
        format!("{root}\\{rel}")
    }
}

#[cfg(test)]
mod tests {
    use super::join_windows;

    #[test]
    fn joins_relative_paths_to_root() {
        assert_eq!(join_windows(r"D:\app\", r"dist\x"), r"D:\app\dist\x");
        assert_eq!(join_windows(r"D:\app", r".\dist"), r"D:\app\dist");
        assert_eq!(join_windows(r"D:\app", "."), r"D:\app");
    }
}
