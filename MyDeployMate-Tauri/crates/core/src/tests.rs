//! Validation tests. AC-02 is the phase 1 exit criterion.

use pretty_assertions::assert_eq;
use serde_json::json;

use crate::fixtures::demo_workspace;
use crate::validate::{
    IssueCode, ObjectKind, check_host, check_workspace_json, validate_workspace,
};
use crate::*;

fn codes_at(ws: &Workspace, path: &str) -> Vec<IssueCode> {
    validate_workspace(ws)
        .issues
        .into_iter()
        .filter(|i| i.path == path)
        .map(|i| i.code)
        .collect()
}

#[test]
fn demo_fixture_is_valid() {
    let demo = demo_workspace();
    let report = validate_workspace(&demo.workspace);
    assert!(report.is_ok(), "{:#?}", report.issues);
    assert_eq!(demo.workspace.projects.len(), 4);
    assert_eq!(demo.workspace.servers.len(), 5);
}

#[test]
fn new_workspace_has_preset_environments() {
    let ws = defaults::new_workspace();
    let names: Vec<_> = ws
        .environments
        .iter()
        .map(|e| (e.name.as_str(), e.protected))
        .collect();
    assert_eq!(
        names,
        [
            ("development", false),
            ("staging", false),
            ("production", true)
        ]
    );
    assert!(validate_workspace(&ws).is_ok());
}

#[test]
fn workspace_round_trips_through_json() {
    let ws = demo_workspace().workspace;
    let value = serde_json::to_value(&ws).unwrap();
    assert_eq!(value["servers"][0]["auth"]["method"], "keyFile");
    assert_eq!(
        value["projects"][0]["pipelines"][0]["steps"][0]["spec"]["type"],
        "local"
    );
    let back = check_workspace_json(value).unwrap();
    assert_eq!(back, ws);
}

// ---- AC-02: invalid port or address blocks saving and points at the field ----

#[test]
fn ac02_port_zero_and_65536_are_rejected_at_the_field() {
    for port in [0, 65_536] {
        let mut ws = demo_workspace().workspace;
        ws.servers[1].port = port;
        let report = validate_workspace(&ws);
        assert_eq!(report.issues.len(), 1, "{:#?}", report.issues);
        let issue = &report.issues[0];
        assert_eq!(issue.path, "servers[1].port");
        assert_eq!(issue.field, "port");
        assert_eq!(issue.code, IssueCode::OutOfRange);
        let object = issue.object.as_ref().unwrap();
        assert_eq!(object.kind, ObjectKind::Server);
        assert_eq!(object.id, ws.servers[1].id.to_string());
    }
}

#[test]
fn ac02_non_integer_port_is_rejected_at_the_field() {
    for bad in [json!(22.5), json!("22"), json!("abc"), json!(-1)] {
        let mut value = serde_json::to_value(demo_workspace().workspace).unwrap();
        value["servers"][2]["port"] = bad.clone();
        let report = check_workspace_json(value).unwrap_err();
        assert_eq!(report.issues.len(), 1);
        assert_eq!(report.issues[0].path, "servers[2].port", "{bad}");
        assert_eq!(report.issues[0].field, "port");
        assert_eq!(report.issues[0].code, IssueCode::InvalidType);
        let expected = if bad.is_string() {
            Message::MustBeNumber
        } else {
            Message::MustBeUnsigned
        };
        assert_eq!(report.issues[0].message, expected, "{bad}");
    }
}

#[test]
fn ac02_address_with_scheme_path_or_port_is_rejected() {
    for host in [
        "ssh://192.0.2.21",
        "192.0.2.21/opt",
        "example.com:22",
        "[2001:db8::1]",
        "deploy@host",
        "",
    ] {
        let mut ws = demo_workspace().workspace;
        ws.servers[0].host = host.into();
        assert!(
            !codes_at(&ws, "servers[0].host").is_empty(),
            "{host:?} should be rejected"
        );
    }
}

#[test]
fn ac02_ipv6_with_separate_port_saves() {
    let mut ws = demo_workspace().workspace;
    ws.servers[0].host = "2001:db8::23".into();
    ws.servers[0].port = 2222;
    let value = serde_json::to_value(&ws).unwrap();
    assert!(check_workspace_json(value).is_ok());
}

#[test]
fn host_forms() {
    for ok in [
        "192.0.2.1",
        "2001:db8::1",
        "::1",
        "fe80::1%eth0",
        "api.example.com",
        "localhost",
        "a-b.c1",
    ] {
        if ok.contains('%') {
            continue; // zone IDs are not part of P0
        }
        assert!(check_host(ok).is_ok(), "{ok}");
    }
    for bad in [
        "256.1.1.1",
        "1.2.3",
        "-bad.example",
        "bad_.example",
        "a..b",
        " host",
        "http://x",
    ] {
        assert!(check_host(bad).is_err(), "{bad}");
    }
}

// ---- SRV-01 other fields ----

#[test]
fn server_name_user_tags_and_notes() {
    let mut ws = demo_workspace().workspace;
    ws.servers[0].name = "   ".into();
    ws.servers[1].name = ws.servers[2].name.clone();
    ws.servers[3].user = "de ploy".into();
    ws.servers[4].notes = "x".repeat(2001);
    ws.servers[0].connect_timeout_sec = 121;
    assert_eq!(codes_at(&ws, "servers[0].name"), [IssueCode::Required]);
    assert_eq!(codes_at(&ws, "servers[2].name"), [IssueCode::Duplicate]);
    assert_eq!(codes_at(&ws, "servers[3].user"), [IssueCode::InvalidFormat]);
    assert_eq!(codes_at(&ws, "servers[4].notes"), [IssueCode::TooLong]);
    assert_eq!(
        codes_at(&ws, "servers[0].connectTimeoutSec"),
        [IssueCode::OutOfRange]
    );
}

#[test]
fn server_name_limit_counts_characters_not_bytes() {
    let mut ws = demo_workspace().workspace;
    ws.servers[0].name = "服".repeat(64);
    assert!(codes_at(&ws, "servers[0].name").is_empty());
    ws.servers[0].name = "服".repeat(65);
    assert_eq!(codes_at(&ws, "servers[0].name"), [IssueCode::TooLong]);
}

#[test]
fn server_must_belong_to_an_existing_environment() {
    let mut ws = demo_workspace().workspace;
    ws.servers[0].environment_id = EnvironmentId::new();
    assert_eq!(
        codes_at(&ws, "servers[0].environmentId"),
        [IssueCode::UnknownReference]
    );
}

// ---- DATA-02 / DATA-03 ----

#[test]
fn environment_names_are_required_short_and_unique() {
    let mut ws = demo_workspace().workspace;
    ws.environments[0].name = "x".repeat(33);
    ws.environments[2].name = " staging ".into();
    assert_eq!(codes_at(&ws, "environments[0].name"), [IssueCode::TooLong]);
    assert_eq!(
        codes_at(&ws, "environments[2].name"),
        [IssueCode::Duplicate]
    );
}

#[test]
fn duplicate_ids_are_rejected() {
    let mut ws = demo_workspace().workspace;
    ws.servers[1].id = ws.servers[0].id;
    assert!(codes_at(&ws, "servers[1].id").contains(&IssueCode::DuplicateId));
}

#[test]
fn targets_must_exist_and_match_the_environment() {
    let demo = demo_workspace();
    let mut ws = demo.workspace;
    let prod_server = ws.servers[2].id;
    ws.projects[0].targets[0].server_ids.push(prod_server);
    ws.projects[0].targets[0].server_ids.push(ServerId::new());
    assert_eq!(
        codes_at(&ws, "projects[0].targets[0].serverIds[2]"),
        [IssueCode::WrongEnvironment]
    );
    assert_eq!(
        codes_at(&ws, "projects[0].targets[0].serverIds[3]"),
        [IssueCode::UnknownReference]
    );
}

// ---- PRJ-01 / PRJ-02 ----

#[test]
fn project_fields() {
    let mut ws = demo_workspace().workspace;
    ws.projects[0].root = Some("relative/path".into());
    ws.projects[1].name = ws.projects[0].name.clone();
    ws.projects[2].pipelines.clear();
    assert_eq!(
        codes_at(&ws, "projects[0].root"),
        [IssueCode::InvalidFormat]
    );
    assert_eq!(codes_at(&ws, "projects[1].name"), [IssueCode::Duplicate]);
    assert_eq!(
        codes_at(&ws, "projects[2].pipelines"),
        [IssueCode::Required]
    );
}

#[test]
fn relative_local_paths_need_a_project_root() {
    let mut ws = demo_workspace().workspace;
    ws.projects[3].root = None;
    // `${project.root}` cwd is resolved later; a missing cwd needs the root.
    if let StepSpec::Local(l) = &mut ws.projects[3].pipelines[0].steps[0].spec {
        l.cwd = None;
    }
    if let StepSpec::Upload(u) = &mut ws.projects[3].pipelines[0].steps[1].spec {
        u.source = "dist/gateway".into();
    }
    assert_eq!(
        codes_at(&ws, "projects[3].pipelines[0].steps[0].spec.cwd"),
        [IssueCode::Required]
    );
    assert_eq!(
        codes_at(&ws, "projects[3].pipelines[0].steps[1].spec.source"),
        [IssueCode::InvalidFormat]
    );
}

// ---- STEP-01–06 ----

#[test]
fn step_timeout_range_and_confirm_has_none() {
    let mut ws = demo_workspace().workspace;
    let steps = &mut ws.projects[0].pipelines[0].steps;
    steps[0].timeout_sec = 0;
    steps[1].timeout_sec = 86_401;
    steps[4].enabled = true;
    steps[4].timeout_sec = 0;
    assert_eq!(
        codes_at(&ws, "projects[0].pipelines[0].steps[0].timeoutSec"),
        [IssueCode::OutOfRange]
    );
    assert_eq!(
        codes_at(&ws, "projects[0].pipelines[0].steps[1].timeoutSec"),
        [IssueCode::OutOfRange]
    );
    assert!(codes_at(&ws, "projects[0].pipelines[0].steps[4].timeoutSec").is_empty());
}

#[test]
fn disabled_steps_skip_execution_fields_but_not_name() {
    let mut ws = demo_workspace().workspace;
    let step = &mut ws.projects[0].pipelines[0].steps[0];
    step.enabled = false;
    step.name = String::new();
    step.spec = defaults::new_step(StepKind::Local, "x").spec; // empty command
    let report = validate_workspace(&ws);
    let paths: Vec<_> = report.issues.iter().map(|i| i.path.as_str()).collect();
    assert_eq!(paths, ["projects[0].pipelines[0].steps[0].name"]);
}

#[test]
fn skip_server_needs_server_first_and_a_remote_step() {
    let mut ws = demo_workspace().workspace;
    let pl = &mut ws.projects[0].pipelines[0];
    pl.steps[0].failure_policy = StepFailurePolicy::SkipServer;
    pl.steps[1].failure_policy = StepFailurePolicy::SkipServer;
    assert_eq!(
        codes_at(&ws, "projects[0].pipelines[0].steps[0].failurePolicy"),
        [IssueCode::NotAllowed]
    );
    assert_eq!(
        codes_at(&ws, "projects[0].pipelines[0].steps[1].failurePolicy"),
        [IssueCode::NotAllowed]
    );
    ws.projects[0].pipelines[0].execution_order = ExecutionOrder::ServerFirst;
    assert_eq!(
        codes_at(&ws, "projects[0].pipelines[0].steps[0].failurePolicy"),
        [IssueCode::NotAllowed]
    );
    assert!(codes_at(&ws, "projects[0].pipelines[0].steps[1].failurePolicy").is_empty());
}

#[test]
fn wsl_needs_distro_and_linux_cwd() {
    let mut ws = demo_workspace().workspace;
    if let StepSpec::Local(l) = &mut ws.projects[0].pipelines[0].steps[0].spec {
        l.shell = Shell::Wsl;
        l.cwd = Some(r"D:\workspace".into());
    }
    assert_eq!(
        codes_at(&ws, "projects[0].pipelines[0].steps[0].spec.wslDistro"),
        [IssueCode::Required]
    );
    assert_eq!(
        codes_at(&ws, "projects[0].pipelines[0].steps[0].spec.cwd"),
        [IssueCode::InvalidFormat]
    );
    // The project root is a Windows path, so WSL needs its own directory even with a root.
    if let StepSpec::Local(l) = &mut ws.projects[0].pipelines[0].steps[0].spec {
        l.cwd = None;
    }
    assert_eq!(
        codes_at(&ws, "projects[0].pipelines[0].steps[0].spec.cwd"),
        [IssueCode::Required]
    );
}

#[test]
fn remote_paths_must_be_absolute_unless_templated() {
    let mut ws = demo_workspace().workspace;
    let steps = &mut ws.projects[0].pipelines[0].steps;
    if let StepSpec::Upload(u) = &mut steps[1].spec {
        u.target = "releases/app".into();
    }
    if let StepSpec::Remote(r) = &mut steps[2].spec {
        r.command = RemoteCommand::Program {
            executable: "systemctl".into(),
            args: vec!["restart".into()],
        };
        r.cwd = Some("${deploy_dir}".into());
    }
    assert_eq!(
        codes_at(&ws, "projects[0].pipelines[0].steps[1].spec.target"),
        [IssueCode::InvalidFormat]
    );
    assert_eq!(
        codes_at(
            &ws,
            "projects[0].pipelines[0].steps[2].spec.command.executable"
        ),
        [IssueCode::InvalidFormat]
    );
    assert!(codes_at(&ws, "projects[0].pipelines[0].steps[2].spec.cwd").is_empty());
}

// ---- VAR-01 / ARC-10 ----

#[test]
fn variable_names_reserved_words_and_sensitive_literals() {
    let mut ws = demo_workspace().workspace;
    let vars = &mut ws.projects[0].variables;
    vars[0].name = "1bad".into();
    vars[1].name = "server".into();
    vars[2].name = "port".into();
    vars[3].sensitive = true;
    assert_eq!(
        codes_at(&ws, "projects[0].variables[0].name"),
        [IssueCode::InvalidFormat]
    );
    assert_eq!(
        codes_at(&ws, "projects[0].variables[1].name"),
        [IssueCode::Reserved]
    );
    assert_eq!(
        codes_at(&ws, "projects[0].variables[3].name"),
        [IssueCode::Duplicate]
    );
    assert_eq!(
        codes_at(&ws, "projects[0].variables[3].value"),
        [IssueCode::NotAllowed]
    );
}

#[test]
fn overrides_must_name_an_existing_variable() {
    let demo = demo_workspace();
    let mut ws = demo.workspace;
    ws.projects[0].overrides.push(VariableOverride {
        environment_id: demo.staging,
        name: "missing".into(),
        value: VariableValue::Literal { value: "1".into() },
    });
    ws.projects[0].overrides.push(VariableOverride {
        environment_id: demo.staging,
        name: "app_root".into(),
        value: VariableValue::Literal {
            value: "/srv".into(),
        },
    });
    assert_eq!(
        codes_at(&ws, "projects[0].overrides[1].name"),
        [IssueCode::UnknownReference]
    );
    assert!(codes_at(&ws, "projects[0].overrides[2].name").is_empty());
}

#[test]
fn wrong_schema_version_is_reported() {
    let mut ws = demo_workspace().workspace;
    ws.schema_version = 99;
    assert_eq!(
        codes_at(&ws, "schemaVersion"),
        [IssueCode::UnsupportedVersion]
    );
}

// ---- NFR-02: history retention ----

#[test]
fn retention_days_must_be_in_range() {
    let mut ws = demo_workspace().workspace;
    assert_eq!(ws.preferences.history_retention_days, 90);
    for days in [0, 3651] {
        ws.preferences.history_retention_days = days;
        assert_eq!(
            codes_at(&ws, "preferences.historyRetentionDays"),
            [IssueCode::OutOfRange]
        );
    }
    ws.preferences.history_retention_days = 1;
    assert!(validate_workspace(&ws).issues.is_empty());
}

#[test]
fn missing_retention_reads_as_the_default() {
    let prefs: Preferences = serde_json::from_str(r#"{"checkUpdates":false}"#).unwrap();
    assert_eq!(prefs.history_retention_days, 90);
    assert!(!prefs.check_updates);
}
