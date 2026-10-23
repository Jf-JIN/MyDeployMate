//! Template syntax for step fields and variable values (SRS VAR-02).
//!
//! `${name}` is a user variable, `${project.root}` / `${server.host}` / `${run.env}` are built-ins,
//! `${env.NAME}` is a client process environment variable, and `$${` keeps a literal `${` for the
//! shell. A lone `$` is plain text.

use crate::message::Message;
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

/// A built-in expression offered by the variable menu (IXD-11).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BuiltinVariable {
    /// As inserted, e.g. `${server.host}`.
    pub expression: String,
    pub label: Message,
    /// Only meaningful on a target server: not offered for local commands.
    pub needs_server: bool,
}

/// Built-ins for the variable menu, in a stable order.
pub fn builtin_variables() -> Vec<BuiltinVariable> {
    let item = |ns: &str, key: &str, label, needs_server| BuiltinVariable {
        expression: format!("${{{ns}.{key}}}"),
        label,
        needs_server,
    };
    vec![
        item(NS_PROJECT, "name", Message::BuiltinProjectName, false),
        item(NS_PROJECT, "root", Message::BuiltinProjectRoot, false),
        item(NS_RUN, "env", Message::BuiltinEnv, false),
        item(NS_SERVER, "name", Message::BuiltinServerName, true),
        item(NS_SERVER, "host", Message::BuiltinServerHost, true),
        item(NS_SERVER, "port", Message::BuiltinServerPort, true),
        item(NS_SERVER, "user", Message::BuiltinServerUser, true),
    ]
}

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
    pub fn message(&self) -> Message {
        match self {
            SyntaxError::Unterminated => Message::Unterminated,
            SyntaxError::InvalidExpression(e) => Message::InvalidExpression {
                expression: format!("${{{e}}}"),
                escape: "$${".into(),
            },
            SyntaxError::UnknownBuiltin(e) => Message::UnknownBuiltin {
                expression: format!("${{{e}}}"),
            },
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

    #[test]
    fn builtin_menu_items_parse() {
        for item in builtin_variables() {
            let parts = parse(&item.expression).unwrap();
            assert!(matches!(
                parts.as_slice(),
                [Part::Expr(Expr::Builtin { .. })]
            ));
            assert_eq!(item.needs_server, item.expression.starts_with("${server."));
        }
    }
}
