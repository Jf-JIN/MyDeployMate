//! Variable resolution, plan expansion and preflight. Phase 2 exit: AC-08, AC-09, AC-10, AC-14,
//! AC-17 and AC-28 (expansion part).

use std::collections::HashMap;

use pretty_assertions::assert_eq;

use crate::fixtures::{DemoWorkspace, demo_workspace};
use crate::plan::{PlanRequest, ResolvedPlan, RunScope, TaskAction, build_plan};
use crate::resolve::{Context, MASK, NoValues, ResolveErrorKind, Resolver, ValueSource};
use crate::validate::IssueCode;
use crate::*;

#[derive(Default)]
struct MapSource {
    env: HashMap<String, String>,
    secrets: HashMap<(VariableId, Option<EnvironmentId>), String>,
}

impl ValueSource for MapSource {
    fn env_var(&self, name: &str) -> Option<String> {
        self.env.get(name).cloned()
    }
    fn secret(&self, variable: VariableId, environment: Option<EnvironmentId>) -> Option<String> {
        self.secrets.get(&(variable, environment)).cloned()
    }
}

fn var(name: &str, value: &str) -> Variable {
    Variable {
        id: VariableId::new(),
        name: name.into(),
        value: VariableValue::Literal {
            value: value.into(),
        },
        sensitive: false,
    }
}

fn request(demo: &DemoWorkspace, project: usize, env: EnvironmentId) -> PlanRequest {
    let p = &demo.workspace.projects[project];
    PlanRequest {
        project_id: p.id,
        pipeline_id: p.pipelines[0].id,
        environment_id: env,
        scope: RunScope::Full,
        server_ids: None,
        version: None,
    }
}

fn plan(demo: &DemoWorkspace, req: &PlanRequest) -> ResolvedPlan {
    build_plan(&demo.workspace, req, &NoValues).unwrap()
}

/// "step name @ server" for each task, the way AC-14 reads.
fn order(plan: &ResolvedPlan) -> Vec<String> {
    plan.tasks
        .iter()
        .map(|t| match &t.server {
            Some(s) => format!("{} @ {}", t.step_name, s.name),
            None => t.step_name.clone(),
        })
        .collect()
}

fn resolve_local(demo: &DemoWorkspace, project: usize, env: EnvironmentId, text: &str) -> String {
    let ws = &demo.workspace;
    let environment = ws.environments.iter().find(|e| e.id == env).unwrap();
    Resolver::new(ws, &ws.projects[project], environment, &NoValues)
        .resolve(text, Context::LOCAL)
        .unwrap()
        .expose()
        .to_owned()
}

#[test]
fn ac08_override_priority_and_removal() {
    let mut demo = demo_workspace();
    let staging = demo.staging;
    demo.workspace.global_variables.push(var("port", "8080"));
    // The fixture project already has port=8080 and a production override; set the AC values.
    let p = &mut demo.workspace.projects[0];
    p.variables.retain(|v| v.name != "port");
    p.variables.push(var("port", "8081"));
    p.overrides.push(VariableOverride {
        environment_id: staging,
        name: "port".into(),
        value: VariableValue::Literal {
            value: "8082".into(),
        },
    });
    assert!(validate_workspace(&demo.workspace).is_ok());
    assert_eq!(resolve_local(&demo, 0, staging, "${port}"), "8082");
    assert_eq!(resolve_local(&demo, 0, demo.development, "${port}"), "8081");
    // Another project without its own value sees the global one.
    assert_eq!(resolve_local(&demo, 1, staging, "${port}"), "8080");

    // An explicit empty string is an override, not a removal.
    demo.workspace.projects[0]
        .overrides
        .last_mut()
        .unwrap()
        .value = VariableValue::Literal {
        value: String::new(),
    };
    assert_eq!(resolve_local(&demo, 0, staging, "[${port}]"), "[]");

    demo.workspace.projects[0]
        .overrides
        .retain(|o| o.environment_id != staging);
    assert_eq!(resolve_local(&demo, 0, staging, "${port}"), "8081");
}

#[test]
fn effective_variables_show_origin() {
    let demo = demo_workspace();
    let ws = &demo.workspace;
    let prod = ws
        .environments
        .iter()
        .find(|e| e.id == demo.production)
        .unwrap();
    let vars = Resolver::new(ws, &ws.projects[0], prod, &NoValues).effective_variables();
    let port = vars.iter().find(|v| v.name == "port").unwrap();
    assert_eq!(port.origin, resolve::VariableOrigin::EnvironmentOverride);
    assert_eq!(port.value.as_ref().unwrap().display, "8081");
    let root = vars.iter().find(|v| v.name == "app_root").unwrap();
    assert_eq!(root.origin, resolve::VariableOrigin::Global);
}

#[test]
fn ac09_undefined_variable_blocks_start_with_field() {
    let mut demo = demo_workspace();
    if let StepSpec::Local(l) = &mut demo.workspace.projects[0].pipelines[0].steps[0].spec {
        l.command = "cargo build --release && copy ${artifact} dist".into();
    }
    let req = request(&demo, 0, demo.staging);
    let plan = plan(&demo, &req);
    assert!(!plan.can_start());
    assert_eq!(plan.issues.len(), 1, "{:#?}", plan.issues);
    let issue = &plan.issues[0];
    assert_eq!(issue.code, IssueCode::UndefinedVariable);
    assert_eq!(issue.path, "projects[0].pipelines[0].steps[0].spec.command");
    assert_eq!(issue.field, "spec.command");
    assert_eq!(issue.message, "步骤 1 · 命令：变量 artifact 未定义");
    let step_id = demo.workspace.projects[0].pipelines[0].steps[0].id;
    assert_eq!(issue.object.as_ref().unwrap().id, step_id.to_string());
}

#[test]
fn ac09_cycle_reports_reference_chain() {
    let mut demo = demo_workspace();
    let p = &mut demo.workspace.projects[0];
    p.variables.push(var("a", "${b}/x"));
    p.variables.push(var("b", "${c}"));
    p.variables.push(var("c", "${a}"));
    if let StepSpec::Local(l) = &mut p.pipelines[0].steps[0].spec {
        l.command = "build ${a}".into();
    }
    let plan = plan(&demo, &request(&demo, 0, demo.staging));
    assert_eq!(plan.issues.len(), 1, "{:#?}", plan.issues);
    assert_eq!(plan.issues[0].code, IssueCode::VariableCycle);
    assert_eq!(
        plan.issues[0].message,
        "步骤 1 · 命令：变量循环引用：a → b → c → a"
    );
}

#[test]
fn undefined_inside_a_variable_names_the_chain() {
    let mut demo = demo_workspace();
    let p = &mut demo.workspace.projects[0];
    p.variables.push(var("out", "${dist}/app"));
    if let StepSpec::Local(l) = &mut p.pipelines[0].steps[0].spec {
        l.command = "build ${out}".into();
    }
    let plan = plan(&demo, &request(&demo, 0, demo.staging));
    assert_eq!(
        plan.issues[0].message,
        "步骤 1 · 命令：变量 dist 未定义（引用链：out）"
    );
}

#[test]
fn nesting_deeper_than_sixteen_levels_fails() {
    let mut demo = demo_workspace();
    let p = &mut demo.workspace.projects[0];
    for i in 0..16 {
        p.variables
            .push(var(&format!("v{i}"), &format!("${{v{}}}", i + 1)));
    }
    p.variables.push(var("v16", "end"));
    let ws = &demo.workspace;
    let env = &ws.environments[1];
    let r = Resolver::new(ws, &ws.projects[0], env, &NoValues);
    // v1 → … → v16 is 16 levels.
    assert_eq!(r.resolve("${v1}", Context::LOCAL).unwrap().expose(), "end");
    let err = r.resolve("${v0}", Context::LOCAL).unwrap_err();
    assert_eq!(err.kind, ResolveErrorKind::TooDeep);
    assert_eq!(err.chain.len(), 17);
}

#[test]
fn ac10_server_variable_in_local_step_and_escape() {
    let mut demo = demo_workspace();
    let steps = &mut demo.workspace.projects[0].pipelines[0].steps;
    if let StepSpec::Local(l) = &mut steps[0].spec {
        l.command = "ping ${server.host}".into();
    }
    if let StepSpec::Remote(r) = &mut steps[2].spec {
        r.command = RemoteCommand::Script {
            script: "echo $${HOME} ${server.host} $$ ${run.env}".into(),
        };
    }
    let plan = plan(&demo, &request(&demo, 0, demo.staging));
    assert_eq!(plan.issues.len(), 1, "{:#?}", plan.issues);
    assert_eq!(plan.issues[0].code, IssueCode::ServerContext);
    assert_eq!(
        plan.issues[0].path,
        "projects[0].pipelines[0].steps[0].spec.command"
    );

    let restarts: Vec<_> = plan
        .tasks
        .iter()
        .filter(|t| t.step_index == 2)
        .map(|t| match &t.action {
            TaskAction::Remote {
                command: plan::ResolvedCommand::Script { script },
                ..
            } => script.expose().to_owned(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        restarts,
        [
            "echo ${HOME} 192.0.2.21 $$ staging",
            "echo ${HOME} 192.0.2.22 $$ staging"
        ]
    );
}

#[test]
fn server_variable_reached_through_user_variable_is_still_rejected_locally() {
    let mut demo = demo_workspace();
    let p = &mut demo.workspace.projects[0];
    p.variables
        .push(var("url", "http://${server.host}:${port}"));
    if let StepSpec::Local(l) = &mut p.pipelines[0].steps[0].spec {
        l.command = "curl ${url}".into();
    }
    let plan = plan(&demo, &request(&demo, 0, demo.staging));
    assert_eq!(plan.issues[0].code, IssueCode::ServerContext);
    assert!(plan.issues[0].message.contains("引用链：url"));
}

#[test]
fn ac14_step_first_seven_then_ten_tasks() {
    let mut demo = demo_workspace();
    let req = request(&demo, 0, demo.staging);
    let p = plan(&demo, &req);
    assert!(p.can_start(), "{:#?}", p.issues);
    assert_eq!(
        order(&p),
        [
            "构建发布产物",
            "上传发布文件 @ staging-api-01",
            "上传发布文件 @ staging-api-02",
            "切换版本并重启 @ staging-api-01",
            "切换版本并重启 @ staging-api-02",
            "检查服务健康状态 @ staging-api-01",
            "检查服务健康状态 @ staging-api-02",
        ]
    );
    assert_eq!(
        p.tasks.iter().map(|t| t.seq).collect::<Vec<_>>(),
        (0..7).collect::<Vec<_>>()
    );

    // AC-01 + AC-14: a third server changes nothing until it is bound.
    let mut s3 = defaults::new_server(demo.staging);
    s3.name = "staging-api-03".into();
    s3.host = "192.0.2.23".into();
    s3.auth = AuthMethod::Agent {
        agent: AgentKind::Openssh,
    };
    let s3_id = s3.id;
    demo.workspace.servers.push(s3);
    assert_eq!(plan(&demo, &req).tasks.len(), 7);
    demo.workspace.projects[0].targets[0].server_ids.push(s3_id);
    let p = plan(&demo, &req);
    assert_eq!(p.tasks.len(), 10);
    assert_eq!(order(&p)[3], "上传发布文件 @ staging-api-03");
}

#[test]
fn ac14_server_first_order() {
    let mut demo = demo_workspace();
    demo.workspace.projects[0].pipelines[0].execution_order = ExecutionOrder::ServerFirst;
    let p = plan(&demo, &request(&demo, 0, demo.staging));
    assert!(p.can_start(), "{:#?}", p.issues);
    assert_eq!(
        order(&p),
        [
            "构建发布产物",
            "上传发布文件 @ staging-api-01",
            "切换版本并重启 @ staging-api-01",
            "检查服务健康状态 @ staging-api-01",
            "上传发布文件 @ staging-api-02",
            "切换版本并重启 @ staging-api-02",
            "检查服务健康状态 @ staging-api-02",
        ]
    );
    assert_eq!(
        p.tasks.iter().map(|t| t.segment).collect::<Vec<_>>(),
        [0, 1, 1, 1, 1, 1, 1]
    );
}

#[test]
fn ac28_server_first_with_confirm_between_remote_steps() {
    let mut demo = demo_workspace();
    let pl = &mut demo.workspace.projects[0].pipelines[0];
    pl.execution_order = ExecutionOrder::ServerFirst;
    // Build, upload, confirm, restart.
    let mut confirm = pl.steps.remove(4);
    confirm.enabled = true;
    pl.steps.truncate(3);
    pl.steps.insert(2, confirm);
    let p = plan(&demo, &request(&demo, 0, demo.staging));
    assert!(p.can_start(), "{:#?}", p.issues);
    assert_eq!(
        order(&p),
        [
            "构建发布产物",
            "上传发布文件 @ staging-api-01",
            "上传发布文件 @ staging-api-02",
            "确认发布结果",
            "切换版本并重启 @ staging-api-01",
            "切换版本并重启 @ staging-api-02",
        ]
    );
    assert_eq!(
        p.tasks.iter().map(|t| t.segment).collect::<Vec<_>>(),
        [0, 1, 1, 2, 3, 3]
    );
    assert_eq!(p.tasks[3].timeout_sec, None);
}

#[test]
fn ac17_local_only_needs_no_targets() {
    let mut demo = demo_workspace();
    demo.workspace.projects[0].targets.clear();
    let mut req = request(&demo, 0, demo.staging);
    let full = plan(&demo, &req);
    assert!(!full.can_start());
    assert_eq!(full.issues[0].code, IssueCode::NoTargets);
    assert_eq!(
        full.issues[0].message,
        "此流程包含远程操作，请至少选择一台目标服务器。"
    );

    req.scope = RunScope::LocalOnly;
    let local = plan(&demo, &req);
    assert!(local.can_start(), "{:#?}", local.issues);
    assert_eq!(order(&local), ["构建发布产物"]);
    assert!(local.tasks.iter().all(|t| t.server.is_none()));
}

#[test]
fn local_only_skips_confirm_and_ignores_target_selection() {
    let mut demo = demo_workspace();
    demo.workspace.projects[0].pipelines[0].steps[4].enabled = true;
    let mut req = request(&demo, 0, demo.staging);
    req.scope = RunScope::LocalOnly;
    req.server_ids = Some(vec![ServerId::new()]);
    let p = plan(&demo, &req);
    assert!(p.can_start(), "{:#?}", p.issues);
    assert_eq!(order(&p), ["构建发布产物"]);
    assert!(p.targets.iter().all(|t| !t.selected));
}

#[test]
fn ac07_disabled_steps_create_no_tasks() {
    let mut demo = demo_workspace();
    demo.workspace.projects[0].pipelines[0].steps[1].enabled = false;
    let p = plan(&demo, &request(&demo, 0, demo.staging));
    assert_eq!(p.tasks.len(), 5);
    assert!(p.tasks.iter().all(|t| t.step_index != 1));
}

#[test]
fn target_subset_keeps_binding_order_and_rejects_unbound() {
    let demo = demo_workspace();
    let staging = &demo.workspace.projects[0].targets[0].server_ids;
    let mut req = request(&demo, 0, demo.staging);
    req.server_ids = Some(vec![staging[1]]);
    let p = plan(&demo, &req);
    assert_eq!(p.tasks.len(), 4);
    assert_eq!(p.targets.iter().filter(|t| t.selected).count(), 1);

    // A production server is not bound to staging.
    let prod = demo.workspace.projects[0].targets[1].server_ids[0];
    req.server_ids = Some(vec![staging[1], staging[0], prod]);
    let p = plan(&demo, &req);
    assert_eq!(p.issues.len(), 1);
    assert_eq!(p.issues[0].code, IssueCode::NotBound);
    assert_eq!(p.issues[0].path, "serverIds[2]");
    assert_eq!(order(&p)[1], "上传发布文件 @ staging-api-01");
}

#[test]
fn version_comes_from_variable_or_run_override() {
    let demo = demo_workspace();
    let mut req = request(&demo, 0, demo.staging);
    let p = plan(&demo, &req);
    assert_eq!(p.version.as_deref(), Some("v1.8.0"));

    req.version = Some("  v1.9.0-rc1 ".into());
    let p = plan(&demo, &req);
    assert!(p.can_start());
    assert_eq!(p.version.as_deref(), Some("v1.9.0-rc1"));
    let TaskAction::Upload { target, .. } = &p.tasks[1].action else {
        panic!()
    };
    assert_eq!(
        target.expose(),
        "/opt/apps/account-api/releases/v1.9.0-rc1/account-api"
    );

    for bad in ["", "   ", "a\nb", &"x".repeat(129)] {
        req.version = Some(bad.into());
        let p = plan(&demo, &req);
        assert!(!p.can_start(), "{bad:?}");
        assert_eq!(p.issues.len(), 1);
        assert_eq!(p.issues[0].path, "version");
    }
}

#[test]
fn missing_version_variable_requires_input() {
    let mut demo = demo_workspace();
    let p = &mut demo.workspace.projects[3];
    p.variables.retain(|v| v.name != "version");
    let req = request(&demo, 3, demo.production);
    let plan = plan(&demo, &req);
    assert_eq!(plan.version, None);
    assert_eq!(plan.issues.len(), 1, "{:#?}", plan.issues);
    assert_eq!(plan.issues[0].code, IssueCode::Required);
    assert_eq!(plan.issues[0].path, "version");
}

#[test]
fn sensitive_values_are_masked_and_collected() {
    let mut demo = demo_workspace();
    let token = Variable {
        id: VariableId::new(),
        name: "token".into(),
        value: VariableValue::SecretRef,
        sensitive: true,
    };
    let token_id = token.id;
    let p = &mut demo.workspace.projects[0];
    p.variables.push(token);
    // A plain variable that embeds the secret is masked too.
    p.variables
        .push(var("auth_header", "Authorization: Bearer ${token}"));
    if let StepSpec::Remote(r) = &mut p.pipelines[0].steps[3].spec {
        r.command = RemoteCommand::Script {
            script: "curl -H '${auth_header}' http://127.0.0.1:${port}/health".into(),
        };
    }
    let req = request(&demo, 0, demo.staging);

    // Not stored on this computer: "需填写" blocks the run.
    let p = plan(&demo, &req);
    assert_eq!(p.issues.len(), 1, "{:#?}", p.issues);
    assert_eq!(p.issues[0].code, IssueCode::MissingValue);

    let mut source = MapSource::default();
    source.secrets.insert((token_id, None), "tok-123456".into());
    let p = build_plan(&demo.workspace, &req, &source).unwrap();
    assert!(p.can_start(), "{:#?}", p.issues);
    let TaskAction::Remote {
        command: plan::ResolvedCommand::Script { script },
        ..
    } = &p.tasks[5].action
    else {
        panic!()
    };
    assert_eq!(
        script.display,
        format!("curl -H 'Authorization: Bearer {MASK}' http://127.0.0.1:8080/health")
    );
    assert!(script.sensitive);
    assert!(script.expose().contains("tok-123456"));
    assert_eq!(p.secrets.0, ["tok-123456"]);

    // Nothing that crosses IPC or reaches logs carries the secret.
    let json = serde_json::to_string(&p).unwrap();
    assert!(!json.contains("tok-123456"));
    assert!(!format!("{p:?}").contains("tok-123456"));
}

#[test]
fn env_refs_and_password_env_vars_are_checked() {
    let mut demo = demo_workspace();
    let p = &mut demo.workspace.projects[2];
    p.variables.push(Variable {
        id: VariableId::new(),
        name: "pip_index".into(),
        value: VariableValue::EnvRef {
            name: "PIP_INDEX_URL".into(),
        },
        sensitive: false,
    });
    if let StepSpec::Remote(r) = &mut p.pipelines[0].steps[2].spec {
        r.command = RemoteCommand::Script {
            script: "pip install -i ${pip_index} pkg && echo ${env.USERNAME}".into(),
        };
    }
    let req = request(&demo, 2, demo.development);
    let p = plan(&demo, &req);
    let codes: Vec<_> = p.issues.iter().map(|i| (i.code, i.path.as_str())).collect();
    assert_eq!(
        codes,
        [
            (
                IssueCode::MissingValue,
                "projects[2].pipelines[0].steps[2].spec.command.script"
            ),
            (IssueCode::MissingValue, "servers[4].auth.source.name"),
        ]
    );

    let mut source = MapSource::default();
    for (k, v) in [
        ("PIP_INDEX_URL", "https://pypi.example/simple"),
        ("USERNAME", "can"),
        ("SM_SSH_PASSWORD", "x"),
    ] {
        source.env.insert(k.into(), v.into());
    }
    let p = build_plan(&demo.workspace, &req, &source).unwrap();
    assert!(p.can_start(), "{:#?}", p.issues);
}

#[test]
fn paths_are_checked_after_resolution() {
    let mut demo = demo_workspace();
    let p = &mut demo.workspace.projects[0];
    p.variables.push(var("rel_dir", "releases"));
    let steps = &mut p.pipelines[0].steps;
    if let StepSpec::Upload(u) = &mut steps[1].spec {
        u.target = "${rel_dir}/app".into();
    }
    if let StepSpec::Remote(r) = &mut steps[2].spec {
        r.cwd = Some("${deploy_dir}".into());
    }
    let plan = plan(&demo, &request(&demo, 0, demo.staging));
    let issues: Vec<_> = plan.issues.iter().map(|i| i.message.as_str()).collect();
    assert_eq!(
        issues,
        [
            "步骤 2 · 远端目标（staging-api-01）：必须是远端绝对路径（以 / 开头）",
            "步骤 2 · 远端目标（staging-api-02）：必须是远端绝对路径（以 / 开头）",
        ]
    );
}

#[test]
fn file_upload_target_must_name_a_file() {
    let mut demo = demo_workspace();
    if let StepSpec::Upload(u) = &mut demo.workspace.projects[0].pipelines[0].steps[1].spec {
        u.target = "${deploy_dir}/releases/".into();
    }
    let plan = plan(&demo, &request(&demo, 0, demo.staging));
    assert_eq!(plan.issues.len(), 2);
    assert!(plan.issues[0].message.contains("完整文件路径"));
}

#[test]
fn local_paths_join_project_root() {
    let mut demo = demo_workspace();
    let steps = &mut demo.workspace.projects[0].pipelines[0].steps;
    if let StepSpec::Local(l) = &mut steps[0].spec {
        l.cwd = Some(r"crates\app".into());
    }
    if let StepSpec::Upload(u) = &mut steps[1].spec {
        u.source = r"target\release\account-api".into();
    }
    let p = plan(&demo, &request(&demo, 0, demo.staging));
    assert!(p.can_start(), "{:#?}", p.issues);
    let TaskAction::Local { cwd, .. } = &p.tasks[0].action else {
        panic!()
    };
    assert_eq!(
        cwd.as_ref().unwrap().display,
        r"D:\workspace\account-api\crates\app"
    );
    let TaskAction::Upload { source, .. } = &p.tasks[1].action else {
        panic!()
    };
    assert_eq!(
        source.expose(),
        r"D:\workspace\account-api\target\release\account-api"
    );
}

#[test]
fn privilege_and_policy_inheritance() {
    let mut demo = demo_workspace();
    let pl = &mut demo.workspace.projects[0].pipelines[0];
    pl.execution_order = ExecutionOrder::ServerFirst;
    pl.failure_policy = PipelineFailurePolicy::SkipServer;
    if let StepSpec::Remote(r) = &mut pl.steps[2].spec {
        r.command = RemoteCommand::Program {
            executable: "/usr/bin/systemctl".into(),
            args: vec!["restart".into(), "${service_name}".into()],
        };
    }
    let p = plan(&demo, &request(&demo, 0, demo.production));
    assert!(p.can_start(), "{:#?}", p.issues);
    // Local tasks never skip a server.
    assert_eq!(p.tasks[0].failure_policy, PipelineFailurePolicy::Stop);
    assert_eq!(p.tasks[1].failure_policy, PipelineFailurePolicy::SkipServer);
    // prod-api-01 defaults to sudo with password; prod-api-02 to no elevation.
    let privileges: Vec<_> = p
        .tasks
        .iter()
        .filter_map(|t| match &t.action {
            TaskAction::Remote {
                privilege,
                shell_elevation,
                ..
            } => Some((
                t.server.as_ref().unwrap().name.as_str(),
                privilege.clone(),
                *shell_elevation,
            )),
            _ => None,
        })
        .collect();
    let sudo = PrivilegePolicy::SudoPassword {
        run_as: "root".into(),
    };
    assert_eq!(
        privileges,
        [
            ("prod-api-01", sudo.clone(), false),
            ("prod-api-01", sudo, true),
            ("prod-api-02", PrivilegePolicy::None, false),
            ("prod-api-02", PrivilegePolicy::None, false),
        ]
    );
    assert!(p.protected);
}

#[test]
fn skip_server_in_step_first_is_a_preflight_issue() {
    let mut demo = demo_workspace();
    demo.workspace.projects[0].pipelines[0].steps[1].failure_policy = StepFailurePolicy::SkipServer;
    let p = plan(&demo, &request(&demo, 0, demo.staging));
    assert!(!p.can_start());
    assert_eq!(p.issues[0].code, IssueCode::NotAllowed);
    assert_eq!(
        p.issues[0].path,
        "projects[0].pipelines[0].steps[1].failurePolicy"
    );
}

#[test]
fn issues_elsewhere_do_not_block_this_pipeline() {
    let mut demo = demo_workspace();
    // A broken disabled step in this pipeline and a broken other project.
    let steps = &mut demo.workspace.projects[0].pipelines[0].steps;
    steps[4].name = String::new();
    demo.workspace.projects[1].name = String::new();
    assert!(!validate_workspace(&demo.workspace).is_ok());
    let p = plan(&demo, &request(&demo, 0, demo.staging));
    assert!(p.can_start(), "{:#?}", p.issues);

    // The same problem on an enabled step blocks the run.
    demo.workspace.projects[0].pipelines[0].steps[0].name = String::new();
    let p = plan(&demo, &request(&demo, 0, demo.staging));
    assert_eq!(p.issues.len(), 1, "{:#?}", p.issues);
    assert_eq!(p.issues[0].path, "projects[0].pipelines[0].steps[0].name");
}

#[test]
fn empty_pipeline_has_nothing_to_run() {
    let mut demo = demo_workspace();
    for s in &mut demo.workspace.projects[0].pipelines[0].steps {
        s.enabled = false;
    }
    let p = plan(&demo, &request(&demo, 0, demo.staging));
    assert_eq!(p.issues.len(), 1);
    assert_eq!(p.issues[0].code, IssueCode::NoTasks);
}

#[test]
fn unknown_ids_are_errors() {
    let demo = demo_workspace();
    let mut req = request(&demo, 0, demo.staging);
    req.pipeline_id = PipelineId::new();
    assert_eq!(
        build_plan(&demo.workspace, &req, &NoValues),
        Err(plan::PlanError::PipelineNotFound)
    );
}

#[test]
fn variable_views_show_overrides_and_messages() {
    let demo = demo_workspace();
    let project = demo.workspace.projects[0].id;
    let views =
        crate::resolve::variable_views(&demo.workspace, project, demo.production, &NoValues);
    let port = views.iter().find(|v| v.name == "port").unwrap();
    assert_eq!(port.display.as_deref(), Some("8081"));
    assert_eq!(
        port.origin,
        crate::resolve::VariableOrigin::EnvironmentOverride
    );
    let staging = crate::resolve::variable_views(&demo.workspace, project, demo.staging, &NoValues);
    let port = staging.iter().find(|v| v.name == "port").unwrap();
    assert_eq!(port.display.as_deref(), Some("8080"));
    assert_eq!(port.origin, crate::resolve::VariableOrigin::Project);
    assert!(
        crate::resolve::variable_views(&demo.workspace, ProjectId::new(), demo.staging, &NoValues)
            .is_empty()
    );
}
