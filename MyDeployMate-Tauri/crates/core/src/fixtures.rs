//! The original demo's `seed()` sample data, rewritten for the SRS model (plan task 1.8).
//! Used by unit tests and UI development. Hosts use documentation address ranges.
//!
//! Differences from the demo: jump hosts, online status and resource metrics are gone (P0 scope),
//! health checks are plain remote commands, and targets are bound per environment (DATA-03).

use crate::defaults::{new_step, new_workspace};
use crate::ids::*;
use crate::model::*;

pub struct DemoWorkspace {
    pub workspace: Workspace,
    pub development: EnvironmentId,
    pub staging: EnvironmentId,
    pub production: EnvironmentId,
}

fn literal(value: &str) -> VariableValue {
    VariableValue::Literal {
        value: value.into(),
    }
}

fn var(name: &str, value: &str) -> Variable {
    Variable {
        id: VariableId::new(),
        name: name.into(),
        value: literal(value),
        sensitive: false,
    }
}

fn local(name: &str, command: &str) -> Step {
    let mut step = new_step(StepKind::Local, name);
    step.spec = StepSpec::Local(LocalSpec {
        command: command.into(),
        cwd: Some("${project.root}".into()),
        shell: Shell::Powershell,
        wsl_distro: None,
        output_encoding: OutputEncoding::Auto,
    });
    step
}

fn upload(name: &str, source: &str, kind: SourceKind, target: &str) -> Step {
    let mut step = new_step(StepKind::Upload, name);
    step.spec = StepSpec::Upload(UploadSpec {
        source: source.into(),
        source_kind: kind,
        target: target.into(),
        conflict: ConflictPolicy::Error,
    });
    step
}

fn remote(name: &str, script: &str) -> Step {
    let mut step = new_step(StepKind::Remote, name);
    step.spec = StepSpec::Remote(RemoteSpec {
        command: RemoteCommand::Script {
            script: script.into(),
        },
        cwd: None,
        privilege: StepPrivilege::Inherit,
    });
    step
}

fn pipeline(name: &str, steps: Vec<Step>) -> Pipeline {
    Pipeline {
        id: PipelineId::new(),
        name: name.into(),
        execution_order: ExecutionOrder::StepFirst,
        failure_policy: PipelineFailurePolicy::Stop,
        steps,
    }
}

fn targets(environment_id: EnvironmentId, servers: &[&Server]) -> EnvironmentTargets {
    EnvironmentTargets {
        environment_id,
        server_ids: servers.iter().map(|s| s.id).collect(),
    }
}

pub fn demo_workspace() -> DemoWorkspace {
    let mut ws = new_workspace();
    let [development, staging, production] = [0, 1, 2].map(|i| ws.environments[i].id);

    let server = |name: &str,
                  host: &str,
                  port: u32,
                  user: &str,
                  env: EnvironmentId,
                  auth: AuthMethod,
                  tags: &[&str]| {
        Server {
            id: ServerId::new(),
            name: name.into(),
            host: host.into(),
            port,
            user: user.into(),
            environment_id: env,
            auth,
            privilege: PrivilegePolicy::None,
            allow_legacy_rsa: false,
            connect_timeout_sec: 10,
            tags: tags.iter().map(|t| (*t).into()).collect(),
            notes: String::new(),
        }
    };
    let key = || AuthMethod::KeyFile {
        path: r"C:\Users\deploy\.ssh\id_ed25519".into(),
    };
    let s1 = server(
        "staging-api-01",
        "192.0.2.21",
        22,
        "deploy",
        staging,
        key(),
        &["API", "华东"],
    );
    let s2 = server(
        "staging-api-02",
        "192.0.2.22",
        22,
        "deploy",
        staging,
        AuthMethod::Agent {
            agent: AgentKind::Openssh,
        },
        &["API", "华东"],
    );
    let mut s3 = server(
        "prod-api-01",
        "198.51.100.11",
        22,
        "deploy",
        production,
        key(),
        &["API", "主节点"],
    );
    s3.privilege = PrivilegePolicy::SudoPassword {
        run_as: "root".into(),
    };
    let s4 = server(
        "prod-api-02",
        "198.51.100.12",
        22,
        "deploy",
        production,
        key(),
        &["API", "备用"],
    );
    let s5 = server(
        "dev-worker-01",
        "203.0.113.30",
        2222,
        "developer",
        development,
        AuthMethod::Password {
            source: PasswordSource::EnvVar {
                name: "SM_SSH_PASSWORD".into(),
            },
        },
        &["任务队列"],
    );

    let mut health = remote(
        "检查服务健康状态",
        "curl --fail http://127.0.0.1:${port}/health",
    );
    health.timeout_sec = 30;
    let mut confirm = new_step(StepKind::Confirm, "确认发布结果");
    confirm.enabled = false;
    confirm.spec = StepSpec::Confirm(ConfirmSpec {
        prompt: "确认关键接口和业务指标正常。".into(),
    });
    let account_api = Project {
        id: ProjectId::new(),
        name: "account-api".into(),
        description: "账户与认证服务".into(),
        root: Some(r"D:\workspace\account-api".into()),
        default_environment_id: staging,
        targets: vec![
            targets(staging, &[&s1, &s2]),
            targets(production, &[&s3, &s4]),
        ],
        variables: vec![
            var("deploy_dir", "/opt/apps/account-api"),
            var("service_name", "account-api"),
            var("version", "v1.8.0"),
            var("port", "8080"),
        ],
        overrides: vec![VariableOverride {
            environment_id: production,
            name: "port".into(),
            value: literal("8081"),
        }],
        pipelines: vec![pipeline(
            "构建并部署",
            vec![
                local("构建发布产物", "cargo build --release"),
                upload(
                    "上传发布文件",
                    "${project.root}/target/release/account-api",
                    SourceKind::File,
                    "${deploy_dir}/releases/${version}/account-api",
                ),
                remote(
                    "切换版本并重启",
                    "ln -sfn ${deploy_dir}/releases/${version} ${deploy_dir}/current\nsystemctl --user restart ${service_name}",
                ),
                health,
                confirm,
            ],
        )],
    };
    let web_console = Project {
        id: ProjectId::new(),
        name: "web-console".into(),
        description: "管理控制台".into(),
        root: Some(r"D:\workspace\web-console".into()),
        default_environment_id: staging,
        targets: vec![targets(staging, &[&s1]), targets(production, &[&s3])],
        variables: vec![
            var("deploy_dir", "/var/www/console"),
            var("version", "v2.4.0"),
        ],
        overrides: Vec::new(),
        pipelines: vec![pipeline(
            "发布前端",
            vec![
                local(
                    "安装依赖并构建",
                    "pnpm install --frozen-lockfile\npnpm build",
                ),
                upload(
                    "上传静态资源",
                    "${project.root}/dist/",
                    SourceKind::Directory,
                    "${deploy_dir}/releases/${version}/",
                ),
                remote(
                    "切换站点目录",
                    "ln -sfn ${deploy_dir}/releases/${version} ${deploy_dir}/current",
                ),
            ],
        )],
    };
    let task_worker = Project {
        id: ProjectId::new(),
        name: "task-worker".into(),
        description: "异步任务处理".into(),
        root: Some(r"D:\workspace\task-worker".into()),
        default_environment_id: development,
        targets: vec![targets(development, &[&s5])],
        variables: vec![
            var("deploy_dir", "/opt/apps/worker"),
            var("service_name", "worker"),
            var("version", "v0.9.2"),
        ],
        overrides: Vec::new(),
        pipelines: vec![pipeline(
            "更新工作进程",
            vec![
                local("构建分发包", "python -m build"),
                upload(
                    "上传构建目录",
                    "${project.root}/dist/",
                    SourceKind::Directory,
                    "${deploy_dir}/packages/",
                ),
                remote(
                    "安装并重启",
                    "${deploy_dir}/venv/bin/pip install --upgrade ${deploy_dir}/packages/*.whl\nsystemctl --user restart ${service_name}",
                ),
            ],
        )],
    };
    let edge_gateway = Project {
        id: ProjectId::new(),
        name: "edge-gateway".into(),
        description: "边缘网关".into(),
        root: Some(r"D:\workspace\edge-gateway".into()),
        default_environment_id: production,
        targets: vec![targets(production, &[&s3])],
        variables: vec![var("deploy_dir", "/opt/gateway"), var("version", "v3.1.0")],
        overrides: Vec::new(),
        pipelines: vec![pipeline(
            "部署网关",
            vec![
                local("构建网关", "go build -o dist/gateway ./cmd/gateway"),
                upload(
                    "上传二进制文件",
                    "${project.root}/dist/gateway",
                    SourceKind::File,
                    "${deploy_dir}/gateway",
                ),
            ],
        )],
    };

    ws.servers = vec![s1, s2, s3, s4, s5];
    ws.projects = vec![account_api, web_console, task_worker, edge_gateway];
    ws.global_variables = vec![var("app_root", "/opt/apps")];
    DemoWorkspace {
        workspace: ws,
        development,
        staging,
        production,
    }
}

/// Sizes of the NFR-01 acceptance baseline.
pub const BASELINE_SERVERS: usize = 100;
pub const BASELINE_PROJECTS: usize = 50;
pub const BASELINE_STEPS: usize = 50;

/// The NFR-01 baseline (plan task 8.1): 100 servers, 50 projects with one 50-step pipeline
/// each. Servers are spread over the three environments; each project deploys to 2 staging
/// and 4 production servers. Every pipeline previews without issues.
pub fn baseline_workspace() -> DemoWorkspace {
    let mut ws = new_workspace();
    let [development, staging, production] = [0, 1, 2].map(|i| ws.environments[i].id);
    let envs = [development, staging, production];

    ws.servers = (0..BASELINE_SERVERS)
        .map(|i| Server {
            id: ServerId::new(),
            name: format!("node-{i:03}"),
            host: format!("10.20.{}.{}", i / 200, i % 200 + 10),
            port: 22,
            user: "deploy".into(),
            environment_id: envs[i % 3],
            auth: AuthMethod::Agent {
                agent: AgentKind::Openssh,
            },
            privilege: PrivilegePolicy::None,
            allow_legacy_rsa: false,
            connect_timeout_sec: 10,
            tags: vec![format!("group-{}", i % 10)],
            notes: String::new(),
        })
        .collect();
    let in_env = |env: EnvironmentId| -> Vec<&Server> {
        ws.servers
            .iter()
            .filter(|s| s.environment_id == env)
            .collect()
    };
    let (staging_servers, production_servers) = (in_env(staging), in_env(production));

    let projects = (0..BASELINE_PROJECTS)
        .map(|p| {
            let pick = |list: &[&Server], n: usize| -> Vec<Server> {
                (0..n)
                    .map(|k| list[(p * n + k) % list.len()].clone())
                    .collect()
            };
            let (stage, prod) = (pick(&staging_servers, 2), pick(&production_servers, 4));
            let steps = (0..BASELINE_STEPS)
                .map(|s| match s % 10 {
                    0 => local(&format!("build {s}"), "cargo build --release"),
                    1 => upload(
                        &format!("upload {s}"),
                        "${project.root}/target/release/app",
                        SourceKind::File,
                        "${deploy_dir}/releases/${version}/app",
                    ),
                    _ => remote(
                        &format!("remote {s}"),
                        "cd ${deploy_dir} && ./bin/check --service ${service_name} --step 1",
                    ),
                })
                .collect();
            Project {
                id: ProjectId::new(),
                name: format!("service-{p:02}"),
                description: String::new(),
                root: Some(format!(r"D:\workspace\service-{p:02}")),
                default_environment_id: staging,
                targets: vec![
                    targets(staging, &stage.iter().collect::<Vec<_>>()),
                    targets(production, &prod.iter().collect::<Vec<_>>()),
                ],
                variables: vec![
                    var("deploy_dir", &format!("/opt/apps/service-{p:02}")),
                    var("service_name", &format!("service-{p:02}")),
                    var("version", "v1.0.0"),
                ],
                overrides: Vec::new(),
                pipelines: vec![pipeline("deploy", steps)],
            }
        })
        .collect();
    ws.projects = projects;
    DemoWorkspace {
        workspace: ws,
        development,
        staging,
        production,
    }
}
