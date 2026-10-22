//! Variable resolution (SRS VAR-01–03, plan task 2.1).
//!
//! Priority from low to high: global, project, environment override, run override (only
//! `version` in P0). Literal values may reference other variables up to [`MAX_DEPTH`] levels;
//! values read from the process environment or Credential Manager are used as-is.
//! Every resolved piece remembers whether it came from a sensitive variable, so the safe display
//! text masks it (task 2.2) while the real value stays out of `Debug` and serialization.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{EnvironmentId, ProjectId, VariableId};
use crate::model::*;
use crate::template::{self, Expr, Part, SyntaxError};

/// VAR-02: user values may nest at most this many levels.
pub const MAX_DEPTH: usize = 16;
/// Shown in place of sensitive values.
pub const MASK: &str = "******";
/// The variable a run may override (VAR-01).
pub const VERSION_VARIABLE: &str = "version";

/// Values that live outside the workspace. The app reads the process environment and
/// Windows Credential Manager; tests use a map.
pub trait ValueSource {
    fn env_var(&self, name: &str) -> Option<String>;
    /// Sensitive variable value. `environment` is set for an environment override (ARC-10).
    fn secret(&self, variable: VariableId, environment: Option<EnvironmentId>) -> Option<String>;
}

/// A source with no environment variables and no secrets.
pub struct NoValues;

impl ValueSource for NoValues {
    fn env_var(&self, _: &str) -> Option<String> {
        None
    }
    fn secret(&self, _: VariableId, _: Option<EnvironmentId>) -> Option<String> {
        None
    }
}

/// A field after resolution. Only `display` crosses IPC or reaches history; the real value
/// is read with [`ResolvedText::expose`] by the executor.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ResolvedText {
    /// Safe to show and store: sensitive parts are replaced by [`MASK`].
    pub display: String,
    /// Some part came from a sensitive variable.
    pub sensitive: bool,
    #[serde(skip)]
    #[ts(skip)]
    value: String,
    /// Real values of the sensitive parts, for the log redactor (ARC-27).
    #[serde(skip)]
    #[ts(skip)]
    secrets: Vec<String>,
}

impl ResolvedText {
    /// Plain text with no sensitive parts.
    pub fn plain(text: impl Into<String>) -> Self {
        let text = text.into();
        ResolvedText {
            display: text.clone(),
            sensitive: false,
            value: text,
            secrets: Vec::new(),
        }
    }

    pub fn expose(&self) -> &str {
        &self.value
    }

    /// Real values of the sensitive parts. Register these with the log redactor.
    pub fn secrets(&self) -> &[String] {
        &self.secrets
    }

    fn from_pieces(pieces: Vec<Piece>) -> Self {
        let mut out = ResolvedText::plain(String::new());
        let mut masked = false;
        for p in pieces {
            out.value.push_str(&p.text);
            if p.sensitive {
                out.sensitive = true;
                if !masked {
                    out.display.push_str(MASK);
                }
                if !p.text.is_empty() {
                    out.secrets.push(p.text);
                }
            } else {
                out.display.push_str(&p.text);
            }
            masked = p.sensitive;
        }
        out
    }

    /// Apply the same text transformation (e.g. joining a base path) to the value and the display.
    pub fn map(&self, f: impl Fn(&str) -> String) -> Self {
        ResolvedText {
            display: f(&self.display),
            sensitive: self.sensitive,
            value: f(&self.value),
            secrets: self.secrets.clone(),
        }
    }
}

impl fmt::Debug for ResolvedText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ResolvedText").field(&self.display).finish()
    }
}

/// Where the effective value of a variable comes from (P04 shows this).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum VariableOrigin {
    Global,
    Project,
    EnvironmentOverride,
    Run,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ResolveError {
    pub kind: ResolveErrorKind,
    /// Variables the reference went through, outermost first.
    pub chain: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum ResolveErrorKind {
    Syntax {
        message: String,
    },
    Undefined {
        name: String,
    },
    /// `chain` ends with the variable that closes the loop.
    Cycle,
    TooDeep,
    /// `${server.*}` used where there is no target server (local or confirm task).
    ServerContext {
        expression: String,
    },
    ProjectRootMissing,
    MissingEnvVar {
        name: String,
    },
    /// A sensitive variable has no value in Credential Manager on this computer ("需填写").
    MissingSecret {
        name: String,
    },
}

impl ResolveError {
    fn new(kind: ResolveErrorKind, chain: &[String]) -> Self {
        ResolveError {
            kind,
            chain: chain.to_vec(),
        }
    }

    /// User-facing message in Chinese, with the reference chain when there is one.
    pub fn message(&self) -> String {
        let chain = self.chain.join(" → ");
        let via = |s: String| {
            if self.chain.is_empty() {
                s
            } else {
                format!("{s}（引用链：{chain}）")
            }
        };
        match &self.kind {
            ResolveErrorKind::Syntax { message } => via(message.clone()),
            ResolveErrorKind::Undefined { name } => via(format!("变量 {name} 未定义")),
            ResolveErrorKind::Cycle => format!("变量循环引用：{chain}"),
            ResolveErrorKind::TooDeep => {
                format!("变量引用超过 {MAX_DEPTH} 层：{chain}")
            }
            ResolveErrorKind::ServerContext { expression } => via(format!(
                "本地命令和人工确认没有目标服务器，不能引用 {expression}"
            )),
            ResolveErrorKind::ProjectRootMissing => {
                via("项目未设置根目录，不能引用 ${project.root}".into())
            }
            ResolveErrorKind::MissingEnvVar { name } => via(format!("本机环境变量 {name} 未设置")),
            ResolveErrorKind::MissingSecret { name } => {
                via(format!("敏感变量 {name} 在本机没有保存值，请先填写"))
            }
        }
    }
}

/// What a template may refer to besides user variables.
#[derive(Clone, Copy, Debug)]
pub struct Context<'a> {
    /// The target server of a remote or upload task; `None` for local and confirm tasks.
    pub server: Option<&'a Server>,
}

impl Context<'_> {
    pub const LOCAL: Context<'static> = Context { server: None };
}

#[derive(Clone, Debug)]
struct Binding<'a> {
    id: Option<VariableId>,
    value: BindingValue<'a>,
    sensitive: bool,
    origin: VariableOrigin,
    /// Set for environment overrides, which key their secret by environment too.
    override_env: Option<EnvironmentId>,
}

#[derive(Clone, Debug)]
enum BindingValue<'a> {
    Config(&'a VariableValue),
    /// Run override: user input taken literally, never as a template.
    RunLiteral(String),
}

struct Piece {
    text: String,
    sensitive: bool,
}

/// Effective variable for one project and environment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EffectiveVariable {
    pub name: String,
    pub origin: VariableOrigin,
    pub sensitive: bool,
    /// Resolved in a local context; `None` when `error` is set.
    pub value: Option<ResolvedText>,
    /// Why it cannot be resolved here, e.g. it depends on `${server.host}`.
    pub error: Option<ResolveError>,
}

pub struct Resolver<'a> {
    project: &'a Project,
    environment: &'a Environment,
    bindings: HashMap<&'a str, Binding<'a>>,
    source: &'a dyn ValueSource,
}

impl<'a> Resolver<'a> {
    pub fn new(
        workspace: &'a Workspace,
        project: &'a Project,
        environment: &'a Environment,
        source: &'a dyn ValueSource,
    ) -> Self {
        let mut bindings: HashMap<&str, Binding> = HashMap::new();
        let mut add = |var: &'a Variable, origin| {
            bindings.insert(
                var.name.as_str(),
                Binding {
                    id: Some(var.id),
                    value: BindingValue::Config(&var.value),
                    sensitive: var.sensitive,
                    origin,
                    override_env: None,
                },
            );
        };
        for var in &workspace.global_variables {
            add(var, VariableOrigin::Global);
        }
        for var in &project.variables {
            add(var, VariableOrigin::Project);
        }
        for ov in project
            .overrides
            .iter()
            .filter(|o| o.environment_id == environment.id)
        {
            // An override keeps the identity and sensitivity of the variable it overrides.
            if let Some(base) = bindings.get_mut(ov.name.as_str()) {
                base.value = BindingValue::Config(&ov.value);
                base.origin = VariableOrigin::EnvironmentOverride;
                base.override_env = Some(environment.id);
            }
        }
        Resolver {
            project,
            environment,
            bindings,
            source,
        }
    }

    /// Apply the run override of `version` (VAR-01). The value is used literally.
    pub fn with_version(mut self, version: Option<&str>) -> Self {
        if let Some(v) = version {
            let sensitive = self
                .bindings
                .get(VERSION_VARIABLE)
                .is_some_and(|b| b.sensitive);
            self.bindings.insert(
                VERSION_VARIABLE,
                Binding {
                    id: None,
                    value: BindingValue::RunLiteral(v.to_owned()),
                    sensitive,
                    origin: VariableOrigin::Run,
                    override_env: None,
                },
            );
        }
        self
    }

    pub fn is_defined(&self, name: &str) -> bool {
        self.bindings.contains_key(name)
    }

    /// Resolve one field in the given context.
    pub fn resolve(&self, text: &str, ctx: Context<'_>) -> Result<ResolvedText, ResolveError> {
        let mut pieces = Vec::new();
        let mut stack = Vec::new();
        self.expand(text, ctx, &mut stack, false, &mut pieces)?;
        Ok(ResolvedText::from_pieces(pieces))
    }

    /// Resolve a user variable by name, as `${name}` would.
    pub fn resolve_variable(
        &self,
        name: &str,
        ctx: Context<'_>,
    ) -> Result<ResolvedText, ResolveError> {
        let mut pieces = Vec::new();
        self.user(name, ctx, &mut Vec::new(), false, &mut pieces)?;
        Ok(ResolvedText::from_pieces(pieces))
    }

    /// All variables visible to this project and environment, sorted by name.
    pub fn effective_variables(&self) -> Vec<EffectiveVariable> {
        let mut out: Vec<_> = self
            .bindings
            .iter()
            .map(|(name, b)| {
                let (value, error) = match self.resolve_variable(name, Context::LOCAL) {
                    Ok(v) => (Some(v), None),
                    Err(e) => (None, Some(e)),
                };
                EffectiveVariable {
                    name: (*name).to_owned(),
                    origin: b.origin,
                    sensitive: b.sensitive,
                    value,
                    error,
                }
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    fn expand(
        &self,
        text: &str,
        ctx: Context<'_>,
        stack: &mut Vec<String>,
        sensitive: bool,
        out: &mut Vec<Piece>,
    ) -> Result<(), ResolveError> {
        let parts = template::parse(text).map_err(|e: SyntaxError| {
            ResolveError::new(
                ResolveErrorKind::Syntax {
                    message: e.message(),
                },
                stack,
            )
        })?;
        for part in parts {
            match part {
                Part::Text(t) => out.push(Piece {
                    text: t.to_owned(),
                    sensitive,
                }),
                Part::EscapedOpen => out.push(Piece {
                    text: "${".into(),
                    sensitive,
                }),
                Part::Expr(expr) => self.expr(&expr, ctx, stack, sensitive, out)?,
            }
        }
        Ok(())
    }

    fn expr(
        &self,
        expr: &Expr<'_>,
        ctx: Context<'_>,
        stack: &mut Vec<String>,
        sensitive: bool,
        out: &mut Vec<Piece>,
    ) -> Result<(), ResolveError> {
        let text = match *expr {
            Expr::User(name) => return self.user(name, ctx, stack, sensitive, out),
            Expr::EnvVar(name) => self.source.env_var(name).ok_or_else(|| {
                ResolveError::new(
                    ResolveErrorKind::MissingEnvVar {
                        name: name.to_owned(),
                    },
                    stack,
                )
            })?,
            Expr::Builtin { namespace, key } => match (namespace, key) {
                (template::NS_PROJECT, "root") => self
                    .project
                    .root
                    .clone()
                    .filter(|r| !r.trim().is_empty())
                    .ok_or_else(|| {
                        ResolveError::new(ResolveErrorKind::ProjectRootMissing, stack)
                    })?,
                (template::NS_PROJECT, _) => self.project.name.clone(),
                (template::NS_RUN, _) => self.environment.name.clone(),
                (_, key) => {
                    let server = ctx.server.ok_or_else(|| {
                        ResolveError::new(
                            ResolveErrorKind::ServerContext {
                                expression: expr.source(),
                            },
                            stack,
                        )
                    })?;
                    match key {
                        "host" => server.host.clone(),
                        "name" => server.name.clone(),
                        "port" => server.port.to_string(),
                        _ => server.user.clone(),
                    }
                }
            },
        };
        out.push(Piece { text, sensitive });
        Ok(())
    }

    fn user(
        &self,
        name: &str,
        ctx: Context<'_>,
        stack: &mut Vec<String>,
        sensitive: bool,
        out: &mut Vec<Piece>,
    ) -> Result<(), ResolveError> {
        if let Some(pos) = stack.iter().position(|n| n == name) {
            let mut chain = stack[pos..].to_vec();
            chain.push(name.to_owned());
            return Err(ResolveError {
                kind: ResolveErrorKind::Cycle,
                chain,
            });
        }
        if stack.len() >= MAX_DEPTH {
            let mut chain = stack.clone();
            chain.push(name.to_owned());
            return Err(ResolveError {
                kind: ResolveErrorKind::TooDeep,
                chain,
            });
        }
        let Some(binding) = self.bindings.get(name) else {
            return Err(ResolveError::new(
                ResolveErrorKind::Undefined {
                    name: name.to_owned(),
                },
                stack,
            ));
        };
        let sensitive = sensitive || binding.sensitive;
        stack.push(name.to_owned());
        let result = match &binding.value {
            BindingValue::RunLiteral(v) => {
                out.push(Piece {
                    text: v.clone(),
                    sensitive,
                });
                Ok(())
            }
            BindingValue::Config(VariableValue::Literal { value }) => {
                self.expand(value, ctx, stack, sensitive, out)
            }
            BindingValue::Config(VariableValue::EnvRef { name: env }) => {
                match self.source.env_var(env) {
                    Some(text) => {
                        out.push(Piece { text, sensitive });
                        Ok(())
                    }
                    None => Err(ResolveError::new(
                        ResolveErrorKind::MissingEnvVar { name: env.clone() },
                        stack,
                    )),
                }
            }
            BindingValue::Config(VariableValue::SecretRef) => {
                let secret = binding
                    .id
                    .and_then(|id| self.source.secret(id, binding.override_env));
                match secret {
                    Some(text) => {
                        out.push(Piece {
                            text,
                            sensitive: true,
                        });
                        Ok(())
                    }
                    None => Err(ResolveError::new(
                        ResolveErrorKind::MissingSecret {
                            name: name.to_owned(),
                        },
                        stack,
                    )),
                }
            }
        };
        stack.pop();
        result
    }
}

/// One row of the P04 effective-value column (IXD-13): the masked value or why it cannot be
/// resolved, already worded for the user.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct VariableView {
    pub name: String,
    pub origin: VariableOrigin,
    pub sensitive: bool,
    pub display: Option<String>,
    pub error: Option<String>,
}

/// Effective variables of a project in an environment, resolved in a local context. Empty
/// when the project or environment is not in the workspace.
pub fn variable_views(
    workspace: &Workspace,
    project: ProjectId,
    environment: EnvironmentId,
    source: &dyn ValueSource,
) -> Vec<VariableView> {
    let (Some(p), Some(env)) = (
        workspace.projects.iter().find(|p| p.id == project),
        workspace.environments.iter().find(|e| e.id == environment),
    ) else {
        return Vec::new();
    };
    Resolver::new(workspace, p, env, source)
        .effective_variables()
        .into_iter()
        .map(|v| VariableView {
            name: v.name,
            origin: v.origin,
            sensitive: v.sensitive,
            display: v.value.map(|t| t.display),
            error: v.error.map(|e| e.message()),
        })
        .collect()
}
