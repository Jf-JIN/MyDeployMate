//! Configuration export and the D05 import comparison (SRS CFG-01, CFG-02; IXD-36, IXD-37).
//!
//! An export is the workspace JSON itself: it never holds private key contents, passwords,
//! sensitive variable values, the trust store, saved credentials or run logs, because the
//! workspace does not hold them either; credentials appear only as references (ARC-10).
//! Import is a full replace, so the comparison lists what the replacement adds, changes and
//! removes, and the local paths that do not exist on this computer.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::*;

/// Remove what only makes sense on this computer: project root folders, and private key
/// locations, of which only the file name is kept so the server stays valid and shows
/// which key to pick again.
pub fn strip_local_paths(ws: &mut Workspace) {
    for p in &mut ws.projects {
        p.root = None;
    }
    for s in &mut ws.servers {
        if let AuthMethod::KeyFile { path } = &mut s.auth {
            let name = path.rsplit(['\\', '/']).next().unwrap_or_default();
            *path = name.to_owned();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum DiffKind {
    Environment,
    Server,
    Project,
    Pipeline,
    /// A project's target servers per environment: the references that change (IXD-37).
    Targets,
    GlobalVariable,
    Preferences,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Change {
    Added,
    Modified,
    Removed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DiffEntry {
    pub kind: DiffKind,
    pub change: Change,
    pub name: String,
    /// The project of a pipeline or of target bindings.
    pub parent: Option<String>,
}

/// Compare objects by ID: present only in `new` is added, only in `old` removed, in both but
/// different is modified.
fn by_id<T: PartialEq>(
    old: &[T],
    new: &[T],
    id: impl Fn(&T) -> String,
    mut push: impl FnMut(Change, &T),
) {
    for o in old {
        if !new.iter().any(|n| id(n) == id(o)) {
            push(Change::Removed, o);
        }
    }
    for n in new {
        match old.iter().find(|o| id(o) == id(n)) {
            None => push(Change::Added, n),
            Some(o) if o != n => push(Change::Modified, n),
            Some(_) => {}
        }
    }
}

/// What replacing `old` with `new` changes, grouped by kind in a stable order.
pub fn workspace_diff(old: &Workspace, new: &Workspace) -> Vec<DiffEntry> {
    let mut out = Vec::new();
    let entry = |kind, change, name: &str, parent: Option<&str>| DiffEntry {
        kind,
        change,
        name: name.to_owned(),
        parent: parent.map(str::to_owned),
    };
    by_id(
        &old.environments,
        &new.environments,
        |e| e.id.to_string(),
        |c, e| out.push(entry(DiffKind::Environment, c, &e.name, None)),
    );
    by_id(
        &old.servers,
        &new.servers,
        |s| s.id.to_string(),
        |c, s| out.push(entry(DiffKind::Server, c, &s.name, None)),
    );
    // A project counts as modified for its own fields only; pipelines and target bindings
    // are listed separately.
    let own = |p: &Project| Project {
        pipelines: Vec::new(),
        targets: Vec::new(),
        ..p.clone()
    };
    let (old_own, new_own): (Vec<_>, Vec<_>) = (
        old.projects.iter().map(own).collect(),
        new.projects.iter().map(own).collect(),
    );
    by_id(
        &old_own,
        &new_own,
        |p| p.id.to_string(),
        |c, p| out.push(entry(DiffKind::Project, c, &p.name, None)),
    );
    for p in &new.projects {
        let Some(o) = old.projects.iter().find(|o| o.id == p.id) else {
            continue;
        };
        by_id(
            &o.pipelines,
            &p.pipelines,
            |x| x.id.to_string(),
            |c, x| out.push(entry(DiffKind::Pipeline, c, &x.name, Some(&p.name))),
        );
        if o.targets != p.targets {
            out.push(entry(DiffKind::Targets, Change::Modified, &p.name, None));
        }
    }
    by_id(
        &old.global_variables,
        &new.global_variables,
        |v| v.id.to_string(),
        |c, v| out.push(entry(DiffKind::GlobalVariable, c, &v.name, None)),
    );
    if old.preferences != new.preferences {
        out.push(entry(DiffKind::Preferences, Change::Modified, "", None));
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LocalPathKind {
    ProjectRoot,
    KeyFile,
}

/// A local path the imported configuration refers to; the app checks whether it exists.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LocalPath {
    pub kind: LocalPathKind,
    /// The project or server name.
    pub owner: String,
    pub path: String,
}

/// Project roots and private key files named in `ws`.
pub fn local_paths(ws: &Workspace) -> Vec<LocalPath> {
    let roots = ws.projects.iter().filter_map(|p| {
        let root = p.root.as_deref()?.trim();
        (!root.is_empty()).then(|| LocalPath {
            kind: LocalPathKind::ProjectRoot,
            owner: p.name.clone(),
            path: root.to_owned(),
        })
    });
    let keys = ws.servers.iter().filter_map(|s| match &s.auth {
        AuthMethod::KeyFile { path } => Some(LocalPath {
            kind: LocalPathKind::KeyFile,
            owner: s.name.clone(),
            path: path.clone(),
        }),
        _ => None,
    });
    roots.chain(keys).collect()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::fixtures::demo_workspace;
    use crate::validate::validate_workspace;

    #[test]
    fn stripped_export_keeps_key_file_names_and_stays_valid() {
        let mut ws = demo_workspace().workspace;
        strip_local_paths(&mut ws);
        assert!(ws.projects.iter().all(|p| p.root.is_none()));
        let keys: Vec<_> = local_paths(&ws).into_iter().map(|p| p.path).collect();
        assert!(keys.iter().all(|k| k == "id_ed25519"), "{keys:?}");
        assert!(validate_workspace(&ws).is_ok());
    }

    #[test]
    fn identical_workspaces_have_no_differences() {
        let ws = demo_workspace().workspace;
        assert_eq!(workspace_diff(&ws, &ws), vec![]);
    }

    #[test]
    fn lists_added_modified_and_removed_objects() {
        let old = demo_workspace().workspace;
        let mut new = old.clone();
        new.servers[0].port = 2200;
        let removed = new.servers.remove(4);
        new.projects[0].pipelines[0].steps.clear();
        new.projects[1].targets.clear();
        new.projects[2].description = "changed".into();
        let mut extra = new.projects[3].pipelines[0].clone();
        extra.id = crate::PipelineId::new();
        extra.name = "second".into();
        new.projects[3].pipelines.push(extra);
        new.environments
            .push(crate::defaults::new_environment("qa"));
        new.preferences.history_retention_days = 30;

        let diff = workspace_diff(&old, &new);
        let e = |kind, change, name: &str, parent: Option<&str>| DiffEntry {
            kind,
            change,
            name: name.into(),
            parent: parent.map(Into::into),
        };
        let p = |i: usize| new.projects[i].name.as_str();
        assert_eq!(
            diff,
            vec![
                e(DiffKind::Environment, Change::Added, "qa", None),
                e(DiffKind::Server, Change::Removed, &removed.name, None),
                e(
                    DiffKind::Server,
                    Change::Modified,
                    &new.servers[0].name,
                    None
                ),
                e(DiffKind::Project, Change::Modified, p(2), None),
                e(
                    DiffKind::Pipeline,
                    Change::Modified,
                    &new.projects[0].pipelines[0].name,
                    Some(p(0))
                ),
                e(DiffKind::Targets, Change::Modified, p(1), None),
                e(DiffKind::Pipeline, Change::Added, "second", Some(p(3))),
                e(DiffKind::Preferences, Change::Modified, "", None),
            ]
        );
    }
}
