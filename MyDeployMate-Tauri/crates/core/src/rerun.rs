//! What changed between a past run and the current configuration of the same pipeline, shown
//! before running it again (SRS RUN-07, IXD-34, FLOW-07). Both plans are display-safe: sensitive
//! values are masked the same way, so a changed secret does not show as a change.

use std::collections::{HashMap, HashSet};

use crate::ids::{ServerId, StepId};
use crate::message::{Label, Message};
use crate::plan::{PlanTarget, PlannedTask, ResolvedCommand, ResolvedPlan, TaskAction};

/// Differences from `old` (the run's frozen plan) to `new` (a preview of the same pipeline
/// now), in a stable order: environment and pipeline settings, steps, then targets.
pub fn plan_changes(old: &ResolvedPlan, new: &ResolvedPlan) -> Vec<Message> {
    let mut out = Vec::new();
    if old.environment_id != new.environment_id {
        out.push(Message::RerunEnvironment {
            before: old.environment_name.clone(),
            after: new.environment_name.clone(),
        });
    }
    if old.execution_order != new.execution_order {
        out.push(Message::RerunExecutionOrder);
    }
    if old.failure_policy != new.failure_policy {
        out.push(Message::RerunFailurePolicy);
    }
    step_changes(old, new, &mut out);
    target_changes(&old.targets, &new.targets, &mut out);
    out
}

/// A step's tasks in plan order, keyed by server (`None` for local and confirm tasks).
struct StepTasks<'a> {
    name: &'a str,
    tasks: Vec<&'a PlannedTask>,
}

fn steps(plan: &ResolvedPlan) -> (Vec<StepId>, HashMap<StepId, StepTasks<'_>>) {
    let mut order = Vec::new();
    let mut map: HashMap<StepId, StepTasks<'_>> = HashMap::new();
    let mut tasks: Vec<&PlannedTask> = plan.tasks.iter().collect();
    tasks.sort_by_key(|t| (t.step_index, t.seq));
    for t in tasks {
        map.entry(t.step_id)
            .or_insert_with(|| {
                order.push(t.step_id);
                StepTasks {
                    name: &t.step_name,
                    tasks: Vec::new(),
                }
            })
            .tasks
            .push(t);
    }
    (order, map)
}

fn step_changes(old: &ResolvedPlan, new: &ResolvedPlan, out: &mut Vec<Message>) {
    let (old_order, old_steps) = steps(old);
    let (new_order, new_steps) = steps(new);
    for id in &old_order {
        if !new_steps.contains_key(id) {
            out.push(Message::RerunStepRemoved {
                step: old_steps[id].name.to_owned(),
            });
        }
    }
    for id in &new_order {
        let after = &new_steps[id];
        let Some(before) = old_steps.get(id) else {
            out.push(Message::RerunStepAdded {
                step: after.name.to_owned(),
            });
            continue;
        };
        compare_step(before, after, out);
    }
    let kept_old: Vec<_> = old_order
        .iter()
        .filter(|id| new_steps.contains_key(id))
        .collect();
    let kept_new: Vec<_> = new_order
        .iter()
        .filter(|id| old_steps.contains_key(id))
        .collect();
    if kept_old != kept_new {
        out.push(Message::RerunStepOrder);
    }
}

/// The task to compare a step by: the one on the same server when both plans have it,
/// otherwise the first.
fn pair<'a>(before: &StepTasks<'a>, after: &StepTasks<'a>) -> (&'a PlannedTask, &'a PlannedTask) {
    let server = |t: &PlannedTask| t.server.as_ref().map(|s| s.id);
    let old_servers: HashSet<Option<ServerId>> = before.tasks.iter().map(|t| server(t)).collect();
    let b = after
        .tasks
        .iter()
        .find(|t| old_servers.contains(&server(t)))
        .copied()
        .unwrap_or(after.tasks[0]);
    let a = before
        .tasks
        .iter()
        .find(|t| server(t) == server(b))
        .copied()
        .unwrap_or(before.tasks[0]);
    (a, b)
}

fn compare_step(before: &StepTasks<'_>, after: &StepTasks<'_>, out: &mut Vec<Message>) {
    let step = after.name.to_owned();
    if before.name != after.name {
        out.push(Message::RerunStepRenamed {
            before: before.name.to_owned(),
            after: step.clone(),
        });
    }
    let (a, b) = pair(before, after);
    let setting = |label| Message::RerunStepSetting {
        step: step.clone(),
        label,
    };
    if a.kind != b.kind {
        out.push(setting(Label::StepKind));
        return;
    }
    let mut text = |label, x: &str, y: &str| {
        if x != y {
            out.push(Message::RerunStepText {
                step: step.clone(),
                label,
                before: x.to_owned(),
                after: y.to_owned(),
            });
        }
    };
    let opt = |t: &Option<crate::resolve::ResolvedText>| {
        t.as_ref().map(|t| t.display.clone()).unwrap_or_default()
    };
    let mut settings = Vec::new();
    match (&a.action, &b.action) {
        (
            TaskAction::Local {
                command: c1,
                cwd: d1,
                shell: s1,
                wsl_distro: w1,
                output_encoding: e1,
            },
            TaskAction::Local {
                command: c2,
                cwd: d2,
                shell: s2,
                wsl_distro: w2,
                output_encoding: e2,
            },
        ) => {
            text(Label::Command, &c1.display, &c2.display);
            text(Label::Cwd, &opt(d1), &opt(d2));
            if s1 != s2 || w1 != w2 {
                settings.push(Label::Shell);
            }
            if e1 != e2 {
                settings.push(Label::OutputEncoding);
            }
        }
        (
            TaskAction::Upload {
                source: s1,
                source_kind: k1,
                target: t1,
                conflict: c1,
            },
            TaskAction::Upload {
                source: s2,
                source_kind: k2,
                target: t2,
                conflict: c2,
            },
        ) => {
            text(Label::Source, &s1.display, &s2.display);
            text(Label::Target, &t1.display, &t2.display);
            if k1 != k2 {
                settings.push(Label::SourceKind);
            }
            if c1 != c2 {
                settings.push(Label::ConflictPolicy);
            }
        }
        (
            TaskAction::Remote {
                command: c1,
                cwd: d1,
                privilege: p1,
                ..
            },
            TaskAction::Remote {
                command: c2,
                cwd: d2,
                privilege: p2,
                ..
            },
        ) => {
            text(Label::Command, &command_text(c1), &command_text(c2));
            text(Label::RemoteCwd, &opt(d1), &opt(d2));
            if p1 != p2 {
                settings.push(Label::Privilege);
            }
        }
        (TaskAction::Confirm { prompt: p1 }, TaskAction::Confirm { prompt: p2 }) => {
            text(Label::Prompt, &p1.display, &p2.display);
        }
        // The kinds are equal, so the actions are too.
        _ => {}
    }
    if a.timeout_sec != b.timeout_sec {
        let secs = |t: Option<u32>| t.map(|s| s.to_string()).unwrap_or_default();
        text(Label::Timeout, &secs(a.timeout_sec), &secs(b.timeout_sec));
    }
    if a.failure_policy != b.failure_policy {
        settings.push(Label::FailurePolicy);
    }
    out.extend(settings.into_iter().map(setting));
}

/// A remote command as one line: the script, or the program and its arguments.
fn command_text(c: &ResolvedCommand) -> String {
    match c {
        ResolvedCommand::Script { script } => script.display.clone(),
        ResolvedCommand::Program { executable, args } => std::iter::once(executable)
            .chain(args)
            .map(|a| a.display.as_str())
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn target_changes(old: &[PlanTarget], new: &[PlanTarget], out: &mut Vec<Message>) {
    let selected = |list: &[PlanTarget]| -> Vec<PlanTarget> {
        list.iter().filter(|t| t.selected).cloned().collect()
    };
    let (old, new) = (selected(old), selected(new));
    let endpoint = |t: &PlanTarget| format!("{}@{}:{}", t.user, t.host, t.port);
    for t in &old {
        if !new.iter().any(|n| n.server_id == t.server_id) {
            out.push(Message::RerunTargetRemoved {
                server: t.name.clone(),
            });
        }
    }
    for t in &new {
        match old.iter().find(|o| o.server_id == t.server_id) {
            None => out.push(Message::RerunTargetAdded {
                server: t.name.clone(),
            }),
            Some(o) if endpoint(o) != endpoint(t) => out.push(Message::RerunTargetChanged {
                server: t.name.clone(),
                before: endpoint(o),
                after: endpoint(t),
            }),
            Some(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::fixtures::demo_workspace;
    use crate::plan::{PlanRequest, RunScope, build_plan};
    use crate::resolve::NoValues;
    use crate::*;

    fn plan_of(ws: &Workspace, env: EnvironmentId) -> ResolvedPlan {
        let p = &ws.projects[0];
        let req = PlanRequest {
            project_id: p.id,
            pipeline_id: p.pipelines[0].id,
            environment_id: env,
            scope: RunScope::Full,
            server_ids: None,
            version: None,
        };
        build_plan(ws, &req, &NoValues).unwrap()
    }

    #[test]
    fn same_configuration_has_no_changes() {
        let demo = demo_workspace();
        let plan = plan_of(&demo.workspace, demo.staging);
        assert_eq!(plan_changes(&plan, &plan), vec![]);
    }

    #[test]
    fn flow07_lists_changed_commands_steps_and_targets() {
        let demo = demo_workspace();
        let old = plan_of(&demo.workspace, demo.staging);

        let mut ws = demo.workspace.clone();
        let steps = &mut ws.projects[0].pipelines[0].steps;
        // Changed command and timeout on "切换版本并重启".
        if let StepSpec::Remote(spec) = &mut steps[2].spec {
            spec.command = RemoteCommand::Script {
                script: "systemctl --user restart account-api".into(),
            };
        }
        steps[2].timeout_sec = 120;
        // Removed upload, added a new step, renamed the build step.
        let upload = steps.remove(1);
        let mut extra = steps[0].clone();
        extra.id = StepId::new();
        extra.name = "生成文档".into();
        steps.push(extra);
        steps[0].name = "构建".into();
        // The second staging server is unbound, the first one moves.
        let s1 = ws.projects[0].targets[0].server_ids[0];
        ws.projects[0].targets[0].server_ids.truncate(1);
        ws.servers.iter_mut().find(|s| s.id == s1).unwrap().host = "192.0.2.99".into();

        let new = plan_of(&ws, demo.staging);
        let changes = plan_changes(&old, &new);
        assert_eq!(
            changes,
            vec![
                Message::RerunStepRemoved {
                    step: upload.name.clone()
                },
                Message::RerunStepRenamed {
                    before: "构建发布产物".into(),
                    after: "构建".into()
                },
                Message::RerunStepText {
                    step: "切换版本并重启".into(),
                    label: Label::Command,
                    before: "ln -sfn /opt/apps/account-api/releases/v1.8.0 /opt/apps/account-api/current\nsystemctl --user restart account-api".into(),
                    after: "systemctl --user restart account-api".into(),
                },
                Message::RerunStepText {
                    step: "切换版本并重启".into(),
                    label: Label::Timeout,
                    before: old.tasks.iter().find(|t| t.step_name == "切换版本并重启").unwrap().timeout_sec.unwrap().to_string(),
                    after: "120".into(),
                },
                Message::RerunStepAdded {
                    step: "生成文档".into()
                },
                Message::RerunTargetRemoved {
                    server: "staging-api-02".into()
                },
                Message::RerunTargetChanged {
                    server: "staging-api-01".into(),
                    before: "deploy@192.0.2.21:22".into(),
                    after: "deploy@192.0.2.99:22".into(),
                },
            ]
        );
    }

    #[test]
    fn reordered_steps_and_another_environment_are_reported() {
        let demo = demo_workspace();
        let old = plan_of(&demo.workspace, demo.staging);
        let mut ws = demo.workspace.clone();
        ws.projects[0].pipelines[0].steps.swap(2, 3);
        ws.projects[0].pipelines[0].execution_order = ExecutionOrder::ServerFirst;
        let new = plan_of(&ws, demo.production);
        let changes = plan_changes(&old, &new);
        assert_eq!(
            changes[..2],
            [
                Message::RerunEnvironment {
                    before: "staging".into(),
                    after: "production".into()
                },
                Message::RerunExecutionOrder,
            ]
        );
        // Production values differ too, e.g. the port in the health check.
        assert!(changes.iter().any(|m| matches!(
            m,
            Message::RerunStepText {
                label: Label::Command,
                ..
            }
        )));
        assert!(changes.contains(&Message::RerunStepOrder));
        assert!(changes.contains(&Message::RerunTargetAdded {
            server: "prod-api-01".into()
        }));
    }
}
