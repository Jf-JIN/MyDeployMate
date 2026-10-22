//! Template syntax for step fields and variable values (SRS VAR-02).
//!
//! `${name}` is a user variable, `${project.root}` / `${server.host}` / `${run.env}` are built-ins,
//! `${env.NAME}` is a client process environment variable, and `$${` keeps a literal `${` for the
//! shell. A lone `$` is plain text.

use crate::validate::is_identifier;

/// Built-in namespaces; user variables cannot use these names (VAR-01).
pub const NS_PROJECT: &str = "project";
pub const NS_SERVER: &str = "server";
pub const NS_RUN: &str = "run";
pub const NS_ENV: &str = "env";

/// Keys each built-in namespace offers. `env` takes any environment variable name instead.
pub const PROJECT_KEYS: [&str; 2] = ["root", "name"];
pub const SERVER_KEYS: [&str; 4] = ["host", "name", "port", "user"];
pub const RUN_KEYS: [&str; 1] = ["env"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Part<'a> {
    Text(&'a str),
    /// `$${` written by the user; stands for a literal `${`.
    EscapedOpen,
    Expr(Expr<'a>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr<'a> {
    /// `${name}`
    User(&'a str),
    /// `${project.root}`, `${server.host}`, `${run.env}`
    Builtin { namespace: &'a str, key: &'a str },
    /// `${env.NAME}`
    EnvVar(&'a str),
}

impl Expr<'_> {
    /// The expression as written, e.g. `${server.host}`.
    pub fn source(&self) -> String {
        match self {
            Expr::User(name) => format!("${{{name}}}"),
            Expr::Builtin { namespace, key } => format!("${{{namespace}.{key}}}"),
            Expr::EnvVar(name) => format!("${{env.{name}}}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyntaxError {
    /// `${` without a closing `}`.
    Unterminated,
    /// Something between the braces that is not a valid reference, e.g. `${}` or `${a b}`.
    InvalidExpression(String),
    /// `${project.version}` and the like.
    UnknownBuiltin(String),
}

impl SyntaxError {
    pub fn message(&self) -> String {
        match self {
            SyntaxError::Unterminated => "表达式缺少右花括号 }".into(),
            SyntaxError::InvalidExpression(e) => {
                format!(
                    "表达式 ${{{e}}} 无效：变量名只能含字母、数字和下划线，需要保留给 Shell 时写成 $${{"
                )
            }
            SyntaxError::UnknownBuiltin(e) => format!("内建变量 ${{{e}}} 不存在"),
        }
    }
}

/// Split a template into text, escapes and expressions.
pub fn parse(input: &str) -> Result<Vec<Part<'_>>, SyntaxError> {
    let mut parts = Vec::new();
    let mut rest = input;
    while let Some(pos) = rest.find('$') {
        let after = &rest[pos + 1..];
        if let Some(escaped) = after.strip_prefix("${") {
            push_text(&mut parts, &rest[..pos]);
            parts.push(Part::EscapedOpen);
            rest = escaped;
        } else if let Some(body) = after.strip_prefix('{') {
            push_text(&mut parts, &rest[..pos]);
            let end = body.find('}').ok_or(SyntaxError::Unterminated)?;
            parts.push(Part::Expr(parse_expr(&body[..end])?));
            rest = &body[end + 1..];
        } else {
            push_text(&mut parts, &rest[..=pos]);
            rest = after;
        }
    }
    push_text(&mut parts, rest);
    Ok(parts)
}

fn push_text<'a>(parts: &mut Vec<Part<'a>>, text: &'a str) {
    if !text.is_empty() {
        parts.push(Part::Text(text));
    }
}

fn parse_expr(body: &str) -> Result<Expr<'_>, SyntaxError> {
    let invalid = || SyntaxError::InvalidExpression(body.to_owned());
    let Some((namespace, key)) = body.split_once('.') else {
        return if is_identifier(body) {
            Ok(Expr::User(body))
        } else {
            Err(invalid())
        };
    };
    match namespace {
        NS_ENV => {
            let ok = !key.is_empty()
                && !key
                    .chars()
                    .any(|c| c == '=' || c == '{' || c == '$' || c.is_whitespace() || c == '\0');
            if ok {
                Ok(Expr::EnvVar(key))
            } else {
                Err(invalid())
            }
        }
        NS_PROJECT | NS_SERVER | NS_RUN => {
            let keys: &[&str] = match namespace {
                NS_PROJECT => &PROJECT_KEYS,
                NS_SERVER => &SERVER_KEYS,
                _ => &RUN_KEYS,
            };
            if keys.contains(&key) {
                Ok(Expr::Builtin { namespace, key })
            } else if is_identifier(key) {
                Err(SyntaxError::UnknownBuiltin(body.to_owned()))
            } else {
                Err(invalid())
            }
        }
        _ => Err(invalid()),
    }
}

/// Whether the text contains any reference (ignoring `$${` escapes).
pub fn has_expressions(input: &str) -> bool {
    match parse(input) {
        Ok(parts) => parts.iter().any(|p| matches!(p, Part::Expr(_))),
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_references_and_escapes() {
        let parts =
            parse("cd ${deploy_dir} && echo $${HOME} $5 ${server.host}:${env.Path}").unwrap();
        assert_eq!(
            parts,
            vec![
                Part::Text("cd "),
                Part::Expr(Expr::User("deploy_dir")),
                Part::Text(" && echo "),
                Part::EscapedOpen,
                Part::Text("HOME} $"),
                Part::Text("5 "),
                Part::Expr(Expr::Builtin {
                    namespace: "server",
                    key: "host"
                }),
                Part::Text(":"),
                Part::Expr(Expr::EnvVar("Path")),
            ]
        );
    }

    #[test]
    fn rejects_bad_expressions() {
        assert_eq!(parse("${a"), Err(SyntaxError::Unterminated));
        assert_eq!(parse("${}"), Err(SyntaxError::InvalidExpression("".into())));
        assert_eq!(
            parse("${a b}"),
            Err(SyntaxError::InvalidExpression("a b".into()))
        );
        assert_eq!(
            parse("${project.version}"),
            Err(SyntaxError::UnknownBuiltin("project.version".into()))
        );
        assert_eq!(
            parse("${foo.bar}"),
            Err(SyntaxError::InvalidExpression("foo.bar".into()))
        );
        assert!(parse("${env.ProgramFiles(x86)}").is_ok());
    }

    #[test]
    fn escape_is_not_an_expression() {
        assert!(!has_expressions("echo $${PATH}"));
        assert!(has_expressions("echo ${PATH}"));
    }
}
