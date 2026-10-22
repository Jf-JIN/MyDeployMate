//! Edits whose side effects the user must see before confirming: what refers to an environment
//! or server, which target bindings a server change removes (IXD-14, IXD-20, SRV-05), and
//! copies that must not carry over bindings or saved credentials (IXD-07).
//!
//! The functions work on a draft; the caller saves the result through the normal save path,
//! so validation still applies.

use serde::Serialize;
use ts_rs::TS;

use crate::ids::{EnvironmentId, PipelineId, ProjectId, ServerId, StepId, VariableId};
use crate::model::{Pipeline, Project, Server, Step, Workspace};

/// Something that refers to an environment or a server.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum Reference {
    /// A server belongs to the environment.
    Server {
        server_id: ServerId,
        server_name: String,
    },
    /// The environment is a project's default.
    ProjectDefault {
        project_id: ProjectId,
        project_name: String,
    },
    /// A project binds the server as a target in the environment.
    TargetBinding {
        project_id: ProjectId,
        project_name: String,
        environment_id: EnvironmentId,
        environment_name: String,
        server_id: ServerId,
        server_name: String,
    },
    /// A project overrides a variable in the environment.
    VariableOverride {
        project_id: ProjectId,
        project_name: String,
        variable_name: String,
    },
}

/// The result of a server edit: the new draft and the bindings it removed.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ServerEdit {
    pub workspace: Workspace,
    pub removed_bindings: Vec<Reference>,
}

fn environment_name(ws: &Workspace, id: EnvironmentId) -> String {
    ws.environments
        .iter()
        .find(|e| e.id == id)
        .map(|e| e.name.clone())
        .unwrap_or_default()
}

fn server_name(ws: &Workspace, id: ServerId) -> String {
    ws.servers
        .iter()
        .find(|s| s.id == id)
        .map(|s| s.name.clone())
        .unwrap_or_default()
}

/// Target bindings of `server`, optionally only those outside `keep_environment`.
fn bindings(ws: &Workspace, server: ServerId, keep: Option<EnvironmentId>) -> Vec<Reference> {
    let mut out = Vec::new();
    for p in &ws.projects {
        for t in &p.targets {
            if Some(t.environment_id) == keep || !t.server_ids.contains(&server) {
                continue;
            }
            out.push(Reference::TargetBinding {
                project_id: p.id,
                project_name: p.name.clone(),
                environment_id: t.environment_id,
                environment_name: environment_name(ws, t.environment_id),
                server_id: server,
                server_name: server_name(ws, server),
            });
        }
    }
    out
}

/// Everything that keeps an environment from being deleted (IXD-14).
pub fn environment_references(ws: &Workspace, id: EnvironmentId) -> Vec<Reference> {
    let mut out: Vec<Reference> = ws
        .servers
        .iter()
        .filter(|s| s.environment_id == id)
        .map(|s| Reference::Server {
            server_id: s.id,
            server_name: s.name.clone(),
        })
        .collect();
    for p in &ws.projects {
        if p.default_environment_id == id {
            out.push(Reference::ProjectDefault {
                project_id: p.id,
                project_name: p.name.clone(),
            });
        }
        for t in p.targets.iter().filter(|t| t.environment_id == id) {
            for &server in &t.server_ids {
                out.push(Reference::TargetBinding {
                    project_id: p.id,
                    project_name: p.name.clone(),
                    environment_id: id,
                    environment_name: environment_name(ws, id),
                    server_id: server,
                    server_name: server_name(ws, server),
                });
            }
        }
        for o in p.overrides.iter().filter(|o| o.environment_id == id) {
            out.push(Reference::VariableOverride {
                project_id: p.id,
                project_name: p.name.clone(),
                variable_name: o.name.clone(),
            });
        }
    }
    out
}

/// Project target bindings of a server, listed before deleting it (IXD-20).
pub fn server_references(ws: &Workspace, id: ServerId) -> Vec<Reference> {
    bindings(ws, id, None)
}

/// Insert or replace a server. Moving it to another environment removes its bindings in other
/// environments, because a target must belong to the environment it is bound in (DATA-03).
pub fn put_server(mut ws: Workspace, server: Server) -> ServerEdit {
    let removed = bindings(&ws, server.id, Some(server.environment_id));
    for p in &mut ws.projects {
        for t in &mut p.targets {
            if t.environment_id != server.environment_id {
                t.server_ids.retain(|&s| s != server.id);
            }
        }
    }
    match ws.servers.iter_mut().find(|s| s.id == server.id) {
        Some(existing) => *existing = server,
        None => ws.servers.push(server),
    }
    ServerEdit {
        workspace: ws,
        removed_bindings: removed,
    }
}

/// Delete a server and its target bindings. Run history keeps its own copy of the name (SRV-05).
pub fn remove_server(mut ws: Workspace, id: ServerId) -> ServerEdit {
    let removed = bindings(&ws, id, None);
    for p in &mut ws.projects {
        for t in &mut p.targets {
            t.server_ids.retain(|&s| s != id);
        }
    }
    ws.servers.retain(|s| s.id != id);
    ServerEdit {
        workspace: ws,
        removed_bindings: removed,
    }
}

/// A copy of a pipeline with new IDs for it and its steps.
pub fn copy_pipeline(pipeline: &Pipeline, name: impl Into<String>) -> Pipeline {
    let mut copy = pipeline.clone();
    copy.id = PipelineId::new();
    copy.name = name.into();
    for step in &mut copy.steps {
        step.id = StepId::new();
    }
    copy
}

/// A copy of a step with a new ID, inserted after the original by the caller (IXD-09).
pub fn copy_step(step: &Step) -> Step {
    let mut copy = step.clone();
    copy.id = StepId::new();
    copy.name = format!("{} 副本", step.name);
    copy
}

/// Copy a project under a new name (IXD-07): pipelines and variables are copied with new IDs;
/// target bindings are not, so the copy never points at production by accident. Saved
/// credentials are keyed by variable ID, so sensitive values are not carried over either.
pub fn copy_project(project: &Project, name: impl Into<String>) -> Project {
    Project {
        id: ProjectId::new(),
        name: name.into(),
        description: project.description.clone(),
        root: project.root.clone(),
        default_environment_id: project.default_environment_id,
        targets: Vec::new(),
        variables: project
            .variables
            .iter()
            .map(|v| {
                let mut v = v.clone();
                v.id = VariableId::new();
                v
            })
            .collect(),
        overrides: project.overrides.clone(),
        pipelines: project
            .pipelines
            .iter()
            .map(|p| copy_pipeline(p, p.name.clone()))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::fixtures::demo_workspace;
    use crate::validate::validate_workspace;

    fn binding_names(refs: &[Reference]) -> Vec<(String, String)> {
        refs.iter()
            .map(|r| match r {
                Reference::TargetBinding {
                    project_name,
                    environment_name,
                    ..
                } => (project_name.clone(), environment_name.clone()),
                other => panic!("unexpected {other:?}"),
            })
            .collect()
    }

    #[test]
    fn production_is_referenced_by_servers_bindings_and_overrides() {
        let demo = demo_workspace();
        let refs = environment_references(&demo.workspace, demo.production);
        assert!(refs.iter().any(
            |r| matches!(r, Reference::Server { server_name, .. } if server_name == "prod-api-01")
        ));
        assert!(refs.iter().any(|r| matches!(r, Reference::TargetBinding { project_name, .. } if project_name == "account-api")));
        assert!(refs.iter().any(|r| matches!(r, Reference::VariableOverride { variable_name, .. } if variable_name == "port")));
    }

    #[test]
    fn unused_environment_has_no_references() {
        let mut ws = demo_workspace().workspace;
        let extra = crate::defaults::new_environment("qa");
        let id = extra.id;
        ws.environments.push(extra);
        assert_eq!(environment_references(&ws, id), vec![]);
    }

    #[test]
    fn moving_a_server_removes_bindings_in_its_old_environment() {
        let demo = demo_workspace();
        let mut server = demo
            .workspace
            .servers
            .iter()
            .find(|s| s.name == "staging-api-01")
            .unwrap()
            .clone();
        server.environment_id = demo.production;
        let edit = put_server(demo.workspace.clone(), server.clone());
        let removed = binding_names(&edit.removed_bindings);
        assert!(removed.contains(&("account-api".to_owned(), "staging".to_owned())));
        assert!(removed.iter().all(|(_, env)| env == "staging"));
        let bound = edit
            .workspace
            .projects
            .iter()
            .flat_map(|p| &p.targets)
            .any(|t| t.server_ids.contains(&server.id));
        assert!(!bound);
        assert!(validate_workspace(&edit.workspace).is_ok());
    }

    #[test]
    fn editing_without_moving_keeps_bindings() {
        let demo = demo_workspace();
        let mut server = demo.workspace.servers[0].clone();
        server.notes = "已更换磁盘".into();
        let edit = put_server(demo.workspace.clone(), server.clone());
        assert_eq!(edit.removed_bindings, vec![]);
        assert_eq!(edit.workspace.servers[0], server);
        assert_eq!(edit.workspace.projects, demo.workspace.projects);
    }

    #[test]
    fn new_server_is_appended_unbound() {
        let demo = demo_workspace();
        let mut server = crate::defaults::new_server(demo.staging);
        server.name = "staging-api-03".into();
        server.host = "192.0.2.23".into();
        server.auth = crate::model::AuthMethod::Agent {
            agent: crate::model::AgentKind::Openssh,
        };
        let edit = put_server(demo.workspace.clone(), server.clone());
        assert_eq!(
            edit.workspace.servers.len(),
            demo.workspace.servers.len() + 1
        );
        assert_eq!(edit.workspace.projects, demo.workspace.projects);
        assert!(validate_workspace(&edit.workspace).is_ok());
    }

    #[test]
    fn removing_a_server_unbinds_it_everywhere() {
        let demo = demo_workspace();
        let id = demo.workspace.servers[0].id;
        let refs = server_references(&demo.workspace, id);
        assert!(binding_names(&refs).contains(&("account-api".to_owned(), "staging".to_owned())));
        let edit = remove_server(demo.workspace, id);
        assert_eq!(edit.removed_bindings, refs);
        assert!(edit.workspace.servers.iter().all(|s| s.id != id));
        assert!(validate_workspace(&edit.workspace).is_ok());
    }

    #[test]
    fn copied_project_has_new_ids_and_no_targets() {
        let demo = demo_workspace();
        let source = &demo.workspace.projects[0];
        let copy = copy_project(source, "account-api-copy");
        assert_ne!(copy.id, source.id);
        assert_eq!(copy.name, "account-api-copy");
        assert!(copy.targets.is_empty());
        assert_eq!(copy.pipelines.len(), source.pipelines.len());
        for (a, b) in copy.pipelines.iter().zip(&source.pipelines) {
            assert_ne!(a.id, b.id);
            assert_eq!(a.name, b.name);
            assert!(
                a.steps
                    .iter()
                    .zip(&b.steps)
                    .all(|(x, y)| x.id != y.id && x.spec == y.spec)
            );
        }
        assert!(
            copy.variables
                .iter()
                .zip(&source.variables)
                .all(|(a, b)| a.id != b.id && a.name == b.name)
        );
        assert_eq!(copy.overrides, source.overrides);
        let mut ws = demo.workspace.clone();
        ws.projects.push(copy);
        assert!(validate_workspace(&ws).is_ok());
    }

    #[test]
    fn copied_step_gets_new_id_and_name() {
        let demo = demo_workspace();
        let step = &demo.workspace.projects[0].pipelines[0].steps[0];
        let copy = copy_step(step);
        assert_ne!(copy.id, step.id);
        assert_eq!(copy.name, format!("{} 副本", step.name));
        assert_eq!(copy.spec, step.spec);
    }
}
