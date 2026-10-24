//! Built-in pipeline templates for P09 (SRS PRJ-03, IXD-35). A template is plain configuration:
//! creating a pipeline from it copies the steps with new IDs, so later template changes never
//! touch existing projects. Language and tool names only appear in example commands; there is
//! no language-specific engine branch (ARC-40).
//!
//! Step names and confirmation prompts are short keys, translated by the UI when it creates the
//! pipeline (`templates.<id>.steps.<key>`), so the copy is in the operator's language. Commands
//! are shell text and stay as they are.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::defaults::{new_pipeline, new_step};
use crate::model::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum TemplateId {
    Blank,
    LocalBuildUpload,
    RemotePullDeploy,
}

/// A project variable the template's commands use, added to the project when it has none.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TemplateVariable {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PipelineTemplate {
    pub id: TemplateId,
    /// Fresh IDs on every call; step names and prompts are keys (see the module docs).
    pub pipeline: Pipeline,
    pub variables: Vec<TemplateVariable>,
    /// The servers fetch code themselves: their access to the repository is set up separately
    /// from the client's SSH login, and the app cannot promise it works (PRJ-03).
    pub needs_repo_access: bool,
}

/// Prompt key of a template's confirm step.
pub const PROMPT_KEY: &str = "prompt";

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

fn upload(name: &str, source: &str, target: &str) -> Step {
    let mut step = new_step(StepKind::Upload, name);
    step.spec = StepSpec::Upload(UploadSpec {
        source: source.into(),
        source_kind: SourceKind::Directory,
        target: target.into(),
        conflict: ConflictPolicy::Error,
    });
    step
}

fn remote(name: &str, cwd: Option<&str>, script: &str) -> Step {
    let mut step = new_step(StepKind::Remote, name);
    step.spec = StepSpec::Remote(RemoteSpec {
        command: RemoteCommand::Script {
            script: script.into(),
        },
        cwd: cwd.map(Into::into),
        privilege: StepPrivilege::Inherit,
    });
    step
}

fn confirm() -> Step {
    let mut step = new_step(StepKind::Confirm, "confirm");
    step.spec = StepSpec::Confirm(ConfirmSpec {
        prompt: PROMPT_KEY.into(),
    });
    step
}

fn health() -> Step {
    let mut step = remote(
        "health",
        None,
        "curl --fail --silent --show-error http://127.0.0.1:${port}/health",
    );
    step.timeout_sec = 30;
    step
}

fn variables() -> Vec<TemplateVariable> {
    [
        ("deploy_dir", "/opt/apps/${project.name}"),
        ("service_name", "${project.name}"),
        ("version", "v1.0.0"),
        ("port", "8080"),
    ]
    .into_iter()
    .map(|(name, value)| TemplateVariable {
        name: name.into(),
        value: value.into(),
    })
    .collect()
}

fn template(id: TemplateId, steps: Vec<Step>, needs_repo_access: bool) -> PipelineTemplate {
    let mut pipeline = new_pipeline("");
    let variables = if steps.is_empty() {
        Vec::new()
    } else {
        variables()
    };
    pipeline.steps = steps;
    PipelineTemplate {
        id,
        pipeline,
        variables,
        needs_repo_access,
    }
}

/// The three P0 templates: blank, local build then upload, remote fetch then deploy.
pub fn pipeline_templates() -> Vec<PipelineTemplate> {
    vec![
        template(TemplateId::Blank, Vec::new(), false),
        template(
            TemplateId::LocalBuildUpload,
            vec![
                local("build", "npm ci\nnpm run build"),
                upload(
                    "upload",
                    "${project.root}/dist",
                    "${deploy_dir}/releases/${version}",
                ),
                remote(
                    "switch",
                    None,
                    "ln -sfn ${deploy_dir}/releases/${version} ${deploy_dir}/current",
                ),
                remote("restart", None, "systemctl --user restart ${service_name}"),
                health(),
                confirm(),
            ],
            false,
        ),
        template(
            TemplateId::RemotePullDeploy,
            vec![
                remote(
                    "fetch",
                    Some("${deploy_dir}/src"),
                    "git fetch --tags origin\ngit checkout --force ${version}",
                ),
                remote("build", Some("${deploy_dir}/src"), "make release"),
                remote("restart", None, "systemctl --user restart ${service_name}"),
                health(),
                confirm(),
            ],
            true,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::demo_workspace;
    use crate::validate::validate_workspace;

    #[test]
    fn templates_are_valid_pipelines_once_named() {
        for t in pipeline_templates() {
            let mut ws = demo_workspace().workspace;
            let project = &mut ws.projects[0];
            let mut pipeline = t.pipeline.clone();
            pipeline.name = format!("{:?}", t.id);
            for s in &mut pipeline.steps {
                s.name = format!("{} step", s.name);
            }
            project.pipelines.push(pipeline);
            let report = validate_workspace(&ws);
            assert!(report.issues.is_empty(), "{:?}: {:#?}", t.id, report.issues);
        }
    }

    #[test]
    fn template_variables_cover_every_reference() {
        use crate::plan::{PlanRequest, RunScope, build_plan};
        use crate::resolve::NoValues;
        for t in pipeline_templates() {
            let demo = demo_workspace();
            let mut ws = demo.workspace;
            let project = &mut ws.projects[2];
            for v in &t.variables {
                project.variables.retain(|p| p.name != v.name);
                project.variables.push(Variable {
                    id: crate::VariableId::new(),
                    name: v.name.clone(),
                    value: VariableValue::Literal {
                        value: v.value.clone(),
                    },
                    sensitive: false,
                });
            }
            let mut pipeline = t.pipeline.clone();
            pipeline.name = "from template".into();
            let req = PlanRequest {
                project_id: project.id,
                pipeline_id: pipeline.id,
                environment_id: project.default_environment_id,
                scope: RunScope::Full,
                server_ids: None,
                version: None,
            };
            project.pipelines.push(pipeline);
            let plan = build_plan(&ws, &req, &NoValues).unwrap();
            let unresolved: Vec<_> = plan
                .issues
                .iter()
                .filter(|i| i.code == crate::validate::IssueCode::UndefinedVariable)
                .collect();
            assert!(unresolved.is_empty(), "{:?}: {:#?}", t.id, unresolved);
        }
    }

    #[test]
    fn every_call_returns_new_ids() {
        let (a, b) = (pipeline_templates(), pipeline_templates());
        for (x, y) in a.iter().zip(&b) {
            assert_ne!(x.pipeline.id, y.pipeline.id);
            for (s, t) in x.pipeline.steps.iter().zip(&y.pipeline.steps) {
                assert_ne!(s.id, t.id);
            }
        }
    }

    #[test]
    fn only_the_remote_fetch_template_needs_repository_access() {
        let ids: Vec<_> = pipeline_templates()
            .into_iter()
            .filter(|t| t.needs_repo_access)
            .map(|t| t.id)
            .collect();
        assert_eq!(ids, vec![TemplateId::RemotePullDeploy]);
    }
}
