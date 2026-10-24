//! Upload artifacts and remote paths (SRS STEP-03/04, architecture ARC-28–30).
//!
//! The executor scans the local source into an [`ArtifactManifest`] and uploads file by file;
//! this module holds the parts that need no IO: the manifest itself, the comparison of two
//! manifests for later targets of the same step, and the remote path rules.

use serde::{Deserialize, Serialize};

use crate::model::SourceKind;

/// One local file of an upload source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestEntry {
    /// Path relative to the source directory with `/` separators; empty for a file source.
    pub path: String,
    pub size: u64,
    /// Modification time in Unix milliseconds, when the file system reports one.
    pub modified_ms: Option<i64>,
    /// Lowercase hex SHA-256 of the content.
    pub sha256: String,
}

/// The files of an upload source at the moment it was scanned (ARC-28).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactManifest {
    pub kind: SourceKind,
    /// Sorted by path.
    pub files: Vec<ManifestEntry>,
    /// Subdirectories relative to the source directory, parents first. Empty ones are uploaded
    /// too; always empty for a file source.
    pub dirs: Vec<String>,
}

impl ArtifactManifest {
    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
}

/// How a later target's source differs from the manifest recorded for the step (ARC-28).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "path", rename_all = "camelCase")]
pub enum ArtifactChange {
    Added(String),
    Removed(String),
    /// Same path, different size or content.
    Modified(String),
}

impl ArtifactChange {
    pub fn path(&self) -> &str {
        match self {
            Self::Added(p) | Self::Removed(p) | Self::Modified(p) => p,
        }
    }
}

/// Files added, removed or changed since `recorded`, sorted by path. The modification time
/// alone does not count: a rebuild that writes the same bytes is the same artifact.
pub fn compare_manifests(
    recorded: &ArtifactManifest,
    now: &ArtifactManifest,
) -> Vec<ArtifactChange> {
    use std::collections::BTreeMap;
    let old: BTreeMap<_, _> = recorded
        .files
        .iter()
        .map(|f| (f.path.as_str(), f))
        .collect();
    let new: BTreeMap<_, _> = now.files.iter().map(|f| (f.path.as_str(), f)).collect();
    let mut out = Vec::new();
    for (path, f) in &old {
        match new.get(path) {
            None => out.push(ArtifactChange::Removed((*path).to_owned())),
            Some(n) if n.size != f.size || n.sha256 != f.sha256 => {
                out.push(ArtifactChange::Modified((*path).to_owned()))
            }
            Some(_) => {}
        }
    }
    for path in new.keys().filter(|p| !old.contains_key(*p)) {
        out.push(ArtifactChange::Added((*path).to_owned()));
    }
    out.sort_by(|a, b| a.path().cmp(b.path()));
    out
}

/// The remote path a manifest entry is committed to. A file source goes to `target` itself;
/// a directory source puts its contents under `target` without the source directory's name
/// (STEP-03).
pub fn remote_path(kind: SourceKind, target: &str, relative: &str) -> String {
    match kind {
        SourceKind::File => target.to_owned(),
        SourceKind::Directory => join(target, relative),
    }
}

/// `dir` joined with a relative `/` path; `relative` may be empty.
pub fn join(dir: &str, relative: &str) -> String {
    if relative.is_empty() {
        return dir.to_owned();
    }
    let dir = dir.trim_end_matches('/');
    format!("{dir}/{relative}")
}

/// The directory holding `path`, or `None` for `/`.
pub fn parent(path: &str) -> Option<&str> {
    let trimmed = path.trim_end_matches('/');
    let i = trimmed.rfind('/')?;
    Some(if i == 0 { "/" } else { &trimmed[..i] })
}

/// Every directory from the root down to `dir`, excluding `/`: `/a/b` gives `/a`, `/a/b`.
pub fn ancestors(dir: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for part in dir.split('/').filter(|p| !p.is_empty()) {
        cur.push('/');
        cur.push_str(part);
        out.push(cur.clone());
    }
    out
}

/// The temporary file a file is written to before it is renamed onto `path` (ARC-29): same
/// directory, hidden, and named after this task so only its own temporaries are cleaned up.
pub fn temp_path(path: &str, token: &str) -> String {
    let (dir, name) = match path.rfind('/') {
        Some(i) => (&path[..=i], &path[i + 1..]),
        None => ("", path),
    };
    format!("{dir}.{name}.mdm-{token}.tmp")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, size: u64, sha: &str) -> ManifestEntry {
        ManifestEntry {
            path: path.into(),
            size,
            modified_ms: Some(1),
            sha256: sha.into(),
        }
    }

    fn manifest(files: Vec<ManifestEntry>) -> ArtifactManifest {
        ArtifactManifest {
            kind: SourceKind::Directory,
            files,
            dirs: vec![],
        }
    }

    #[test]
    fn compare_reports_added_removed_and_modified_only() {
        let old = manifest(vec![
            entry("a", 1, "aa"),
            entry("b", 2, "bb"),
            entry("c", 3, "cc"),
        ]);
        let mut same_bytes = entry("a", 1, "aa");
        same_bytes.modified_ms = Some(99);
        let new = manifest(vec![same_bytes, entry("c", 3, "cd"), entry("d", 4, "dd")]);
        assert_eq!(
            compare_manifests(&old, &new),
            vec![
                ArtifactChange::Removed("b".into()),
                ArtifactChange::Modified("c".into()),
                ArtifactChange::Added("d".into()),
            ]
        );
        assert!(compare_manifests(&old, &old).is_empty());
    }

    #[test]
    fn size_change_alone_counts() {
        let old = manifest(vec![entry("a", 1, "aa")]);
        let new = manifest(vec![entry("a", 2, "aa")]);
        assert_eq!(
            compare_manifests(&old, &new),
            vec![ArtifactChange::Modified("a".into())]
        );
    }

    #[test]
    fn remote_paths() {
        assert_eq!(
            remote_path(SourceKind::File, "/srv/app.jar", ""),
            "/srv/app.jar"
        );
        assert_eq!(
            remote_path(SourceKind::Directory, "/srv/www/", "css/.hidden"),
            "/srv/www/css/.hidden"
        );
        assert_eq!(
            remote_path(SourceKind::Directory, "/srv/www", ""),
            "/srv/www"
        );
        assert_eq!(parent("/srv/www/a"), Some("/srv/www"));
        assert_eq!(parent("/srv"), Some("/"));
        assert_eq!(parent("/"), None);
        assert_eq!(ancestors("/srv/www/"), vec!["/srv", "/srv/www"]);
        assert!(ancestors("/").is_empty());
        assert_eq!(temp_path("/srv/app.jar", "t1"), "/srv/.app.jar.mdm-t1.tmp");
    }
}
