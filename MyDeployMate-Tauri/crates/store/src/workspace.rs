//! `workspace.json`: load, migrate, validate and save atomically with a backup (DATA-04).

use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use mdm_core::validate::{Issue, IssueCode, ValidationReport};
use mdm_core::{Message, Workspace, defaults, parse_workspace, validate_workspace};

use crate::error::{StoreError, io};
use crate::migrate;
use crate::paths::DataDir;

/// What happened while loading, for the UI to mention once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadOutcome {
    Loaded,
    /// No workspace existed; a new one with the preset environments was created.
    Created,
    /// The file was migrated from an older schema; the original was copied into `backups/`.
    Migrated {
        from: u32,
    },
}

/// Single writer for `workspace.json`. Hold an [`crate::InstanceLock`] while using it.
#[derive(Debug)]
pub struct WorkspaceStore {
    dir: DataDir,
    /// Hash of the file as last read or written by this store, to detect outside edits.
    known_hash: Option<[u8; 32]>,
}

impl WorkspaceStore {
    pub fn new(dir: DataDir) -> Self {
        Self {
            dir,
            known_hash: None,
        }
    }

    pub fn dir(&self) -> &DataDir {
        &self.dir
    }

    /// Read the workspace, creating or migrating it when needed.
    pub fn load(&mut self) -> Result<(Workspace, LoadOutcome), StoreError> {
        self.dir.ensure()?;
        let path = self.dir.workspace_file();
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let ws = self.write(defaults::new_workspace())?;
                return Ok((ws, LoadOutcome::Created));
            }
            Err(e) => return Err(StoreError::Io { path, source: e }),
        };
        let doc: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|e| corrupt(&path, Message::text(e.to_string())))?;
        let (doc, from) = migrate::migrate(doc)?;
        let ws = parse_workspace(doc).map_err(StoreError::Invalid)?;
        let report = validate_workspace(&ws);
        if !report.is_ok() {
            return Err(StoreError::Invalid(report));
        }
        if from == ws.schema_version {
            self.known_hash = Some(sha256(&bytes));
            return Ok((ws, LoadOutcome::Loaded));
        }
        let backup = self
            .dir
            .backups()
            .join(format!("workspace-v{from}-{}.json", unix_secs()));
        fs::write(&backup, &bytes).map_err(io(&backup))?;
        self.known_hash = Some(sha256(&bytes));
        // Saved as-is: migration does not bump the user's revision.
        let ws = self.write_raw(ws)?;
        Ok((ws, LoadOutcome::Migrated { from }))
    }

    /// Validate and save a draft based on revision `draft.revision`. Returns the saved workspace,
    /// whose revision is one higher. Refuses if the file changed since this store last saw it.
    pub fn save(&mut self, draft: Workspace) -> Result<Workspace, StoreError> {
        let report = validate_workspace(&draft);
        if !report.is_ok() {
            return Err(StoreError::Invalid(report));
        }
        let path = self.dir.workspace_file();
        match fs::read(&path) {
            Ok(bytes) => {
                if self.known_hash.is_some_and(|h| h != sha256(&bytes)) {
                    return Err(StoreError::ExternalChange);
                }
                let found = serde_json::from_slice::<serde_json::Value>(&bytes)
                    .ok()
                    .and_then(|d| d.get("revision")?.as_u64())
                    .ok_or_else(|| {
                        let name = "revision".into();
                        corrupt(&path, Message::MissingField { name })
                    })?;
                if found != draft.revision {
                    return Err(StoreError::RevisionConflict {
                        expected: draft.revision,
                        found,
                    });
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && self.known_hash.is_none() => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(StoreError::ExternalChange);
            }
            Err(e) => return Err(StoreError::Io { path, source: e }),
        }
        self.write(draft)
    }

    /// Replace the whole workspace with an imported one (CFG-01, IXD-37). The current file is
    /// first copied into `backups/`; the replacement is written like any save, atomically and
    /// only if the file is still at `expected_revision`. Returns the saved workspace and the
    /// backup's path. On any error the current file is unchanged.
    pub fn replace(
        &mut self,
        mut imported: Workspace,
        expected_revision: u64,
    ) -> Result<(Workspace, std::path::PathBuf), StoreError> {
        let report = validate_workspace(&imported);
        if !report.is_ok() {
            return Err(StoreError::Invalid(report));
        }
        let path = self.dir.workspace_file();
        let bytes = fs::read(&path).map_err(io(&path))?;
        if self.known_hash.is_some_and(|h| h != sha256(&bytes)) {
            return Err(StoreError::ExternalChange);
        }
        let backups = self.dir.backups();
        fs::create_dir_all(&backups).map_err(io(&backups))?;
        let backup = backups.join(format!("workspace-before-import-{}.json", unix_secs()));
        fs::write(&backup, &bytes).map_err(io(&backup))?;
        imported.revision = expected_revision;
        let saved = self.save(imported)?;
        Ok((saved, backup))
    }

    /// Bump the revision and write.
    fn write(&mut self, mut ws: Workspace) -> Result<Workspace, StoreError> {
        ws.revision += 1;
        self.write_raw(ws)
    }

    /// Write via a temp file in the same directory, keep the previous file as `.bak`, then
    /// replace atomically. A crash at any point leaves either the old or the new file intact.
    fn write_raw(&mut self, ws: Workspace) -> Result<Workspace, StoreError> {
        self.dir.ensure()?;
        let mut bytes = serde_json::to_vec_pretty(&ws).expect("workspace serializes");
        bytes.push(b'\n');
        let tmp = self.dir.workspace_temp();
        let target = self.dir.workspace_file();
        {
            use std::io::Write;
            let mut f = fs::File::create(&tmp).map_err(io(&tmp))?;
            f.write_all(&bytes).map_err(io(&tmp))?;
            f.sync_all().map_err(io(&tmp))?;
        }
        if target.exists() {
            let bak = self.dir.workspace_backup();
            retry(|| fs::copy(&target, &bak).map(drop)).map_err(io(&bak))?;
        }
        retry(|| fs::rename(&tmp, &target)).map_err(io(&target))?;
        self.known_hash = Some(sha256(&bytes));
        Ok(ws)
    }
}

/// The export file for `ws` (CFG-01): the workspace JSON, optionally without local paths.
pub fn export_json(ws: &Workspace, include_local_paths: bool) -> Vec<u8> {
    let mut ws = ws.clone();
    if !include_local_paths {
        mdm_core::exchange::strip_local_paths(&mut ws);
    }
    let mut bytes = serde_json::to_vec_pretty(&ws).expect("workspace serializes");
    bytes.push(b'\n');
    bytes
}

/// Read an export file for import (CFG-02): UTF-8 JSON, a known schema version (older ones are
/// migrated), the expected types and enums, then full validation. Any problem rejects the
/// whole file; nothing is written.
pub fn parse_import(bytes: &[u8]) -> Result<(Workspace, u32), StoreError> {
    let doc: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| {
        StoreError::Invalid(ValidationReport {
            issues: vec![Issue {
                path: String::new(),
                object: None,
                field: String::new(),
                code: IssueCode::InvalidType,
                message: Message::BadFormat {
                    detail: e.to_string(),
                },
            }],
        })
    })?;
    let (doc, from) = migrate::migrate(doc)?;
    let ws = parse_workspace(doc).map_err(StoreError::Invalid)?;
    let report = validate_workspace(&ws);
    if !report.is_ok() {
        return Err(StoreError::Invalid(report));
    }
    Ok((ws, from))
}

/// Antivirus and indexers briefly hold files open on Windows; retry a few times.
pub(crate) fn retry(mut op: impl FnMut() -> std::io::Result<()>) -> std::io::Result<()> {
    let mut attempt = 0;
    loop {
        match op() {
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && attempt < 5 => {
                attempt += 1;
                std::thread::sleep(Duration::from_millis(50 * attempt));
            }
            other => return other,
        }
    }
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn corrupt(path: &Path, reason: Message) -> StoreError {
    StoreError::Corrupt {
        path: path.to_path_buf(),
        reason,
    }
}

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InstanceLock;

    fn store() -> (tempfile::TempDir, WorkspaceStore) {
        let tmp = tempfile::tempdir().unwrap();
        let store = WorkspaceStore::new(DataDir::at(tmp.path().join("MyDeployMate")));
        (tmp, store)
    }

    #[test]
    fn first_load_creates_a_workspace_with_presets() {
        let (_tmp, mut store) = store();
        let (ws, outcome) = store.load().unwrap();
        assert_eq!(outcome, LoadOutcome::Created);
        assert_eq!(ws.revision, 1);
        assert_eq!(ws.environments.len(), 3);
        assert!(store.dir().workspace_file().exists());
        let (again, outcome) = store.load().unwrap();
        assert_eq!(outcome, LoadOutcome::Loaded);
        assert_eq!(again, ws);
    }

    #[test]
    fn save_bumps_revision_and_keeps_previous_as_backup() {
        let (_tmp, mut store) = store();
        let (mut ws, _) = store.load().unwrap();
        ws.environments[0].name = "dev".into();
        let saved = store.save(ws).unwrap();
        assert_eq!(saved.revision, 2);
        let bak: Workspace =
            serde_json::from_slice(&fs::read(store.dir().workspace_backup()).unwrap()).unwrap();
        assert_eq!(bak.revision, 1);
        assert_eq!(bak.environments[0].name, "development");
        assert!(!store.dir().workspace_temp().exists());
    }

    #[test]
    fn stale_draft_is_rejected() {
        let (_tmp, mut store) = store();
        let (ws, _) = store.load().unwrap();
        store.save(ws.clone()).unwrap();
        let err = store.save(ws).unwrap_err();
        assert!(
            matches!(
                err,
                StoreError::RevisionConflict {
                    expected: 1,
                    found: 2
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn outside_edit_is_detected() {
        let (_tmp, mut store) = store();
        let (ws, _) = store.load().unwrap();
        let path = store.dir().workspace_file();
        let mut text = fs::read_to_string(&path).unwrap();
        text = text.replace("\"staging\"", "\"stage\"");
        fs::write(&path, text).unwrap();
        assert!(matches!(store.save(ws), Err(StoreError::ExternalChange)));
        // Reloading accepts the outside version, after which saving works again.
        let (ws, _) = store.load().unwrap();
        assert_eq!(ws.environments[1].name, "stage");
        assert!(store.save(ws).is_ok());
    }

    #[test]
    fn invalid_draft_is_not_written() {
        let (_tmp, mut store) = store();
        let (mut ws, _) = store.load().unwrap();
        let before = fs::read(store.dir().workspace_file()).unwrap();
        ws.environments[0].name = String::new();
        assert!(matches!(store.save(ws), Err(StoreError::Invalid(r)) if r.issues.len() == 1));
        assert_eq!(fs::read(store.dir().workspace_file()).unwrap(), before);
    }

    #[test]
    fn corrupt_and_newer_files_are_refused_without_overwriting() {
        let (_tmp, mut store) = store();
        store.dir().ensure().unwrap();
        let path = store.dir().workspace_file();
        fs::write(&path, "{ not json").unwrap();
        assert!(matches!(store.load(), Err(StoreError::Corrupt { .. })));
        fs::write(&path, r#"{ "schemaVersion": 999 }"#).unwrap();
        assert!(matches!(
            store.load(),
            Err(StoreError::UnsupportedVersion { found: 999, .. })
        ));
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            r#"{ "schemaVersion": 999 }"#
        );
    }

    #[test]
    fn second_lock_on_the_same_directory_fails() {
        let (_tmp, store) = store();
        store.dir().ensure().unwrap();
        let first = InstanceLock::acquire(store.dir()).unwrap();
        assert!(matches!(
            InstanceLock::acquire(store.dir()),
            Err(StoreError::Locked(_))
        ));
        drop(first);
        assert!(InstanceLock::acquire(store.dir()).is_ok());
    }

    // ---- CFG-01 / CFG-02 / AC-24 / FLOW-08: export and import ----

    fn demo_json() -> serde_json::Value {
        let ws = mdm_core::fixtures::demo_workspace().workspace;
        serde_json::from_slice(&export_json(&ws, true)).unwrap()
    }

    fn import(doc: &serde_json::Value) -> Result<(Workspace, u32), StoreError> {
        parse_import(&serde_json::to_vec(doc).unwrap())
    }

    fn issues(err: StoreError) -> Vec<(String, IssueCode)> {
        match err {
            StoreError::Invalid(report) => report
                .issues
                .into_iter()
                .map(|i| (i.path, i.code))
                .collect(),
            other => panic!("expected validation issues, got {other:?}"),
        }
    }

    #[test]
    fn ac24_missing_target_reference_is_rejected_at_its_path() {
        let mut doc = demo_json();
        doc["projects"][0]["targets"][0]["serverIds"][0] =
            serde_json::json!("01a16c63-0000-7000-8000-000000000000");
        let found = issues(import(&doc).unwrap_err());
        assert!(
            found
                .iter()
                .any(|(p, c)| p.starts_with("projects[0].targets[0]")
                    && *c == IssueCode::UnknownReference),
            "{found:?}"
        );
    }

    #[test]
    fn ac24_unknown_step_type_is_rejected() {
        let mut doc = demo_json();
        doc["projects"][0]["pipelines"][0]["steps"][0]["spec"]["type"] = "docker".into();
        let found = issues(import(&doc).unwrap_err());
        assert_eq!(found.len(), 1);
        assert!(found[0].0.starts_with("projects[0].pipelines[0].steps[0]"));
        assert_eq!(found[0].1, IssueCode::InvalidType);
    }

    #[test]
    fn newer_major_version_and_bad_json_are_rejected() {
        let mut doc = demo_json();
        doc["schemaVersion"] = 99.into();
        assert!(matches!(
            import(&doc),
            Err(StoreError::UnsupportedVersion { found: 99, .. })
        ));
        let found = issues(parse_import(b"{ not json").unwrap_err());
        assert_eq!(found[0].1, IssueCode::InvalidType);
    }

    #[test]
    fn flow08_replace_backs_up_then_swaps_the_whole_workspace() {
        let (_tmp, mut store) = store();
        let (current, _) = store.load().unwrap();
        let before = fs::read(store.dir().workspace_file()).unwrap();
        let (imported, _) = import(&demo_json()).unwrap();

        // A stale revision changes nothing.
        let err = store
            .replace(imported.clone(), current.revision + 5)
            .unwrap_err();
        assert!(matches!(err, StoreError::RevisionConflict { .. }));
        assert_eq!(fs::read(store.dir().workspace_file()).unwrap(), before);

        let (saved, backup) = store.replace(imported.clone(), current.revision).unwrap();
        assert_eq!(fs::read(&backup).unwrap(), before);
        assert_eq!(saved.revision, current.revision + 1);
        assert_eq!(saved.projects, imported.projects);
        let (reloaded, _) = store.load().unwrap();
        assert_eq!(reloaded, saved);
    }

    #[test]
    fn export_without_local_paths_round_trips() {
        let ws = mdm_core::fixtures::demo_workspace().workspace;
        let bytes = export_json(&ws, false);
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert!(!text.contains(r"D:\\workspace"));
        assert!(!text.contains(r"C:\\Users"));
        let (back, _) = parse_import(&bytes).unwrap();
        assert!(back.projects.iter().all(|p| p.root.is_none()));
    }

    /// AT-09 / phase 6 follow-up: saved credentials and sensitive variable values live in
    /// the Credential Manager, never in an export.
    #[test]
    fn export_never_contains_saved_secrets() {
        use std::sync::Arc;

        use mdm_core::{CredentialPurpose, CredentialRef, VariableValue};
        use secrecy::SecretString;

        use crate::{MemoryVault, SecretStore};

        let (_tmp, store) = store();
        let mut ws = mdm_core::fixtures::demo_workspace().workspace;
        let var = &mut ws.projects[0].variables[0];
        var.sensitive = true;
        var.value = VariableValue::SecretRef;
        let var_id = var.id;
        let mut secrets = SecretStore::open(store.dir(), Arc::new(MemoryVault::default())).unwrap();
        let variable_secret = "var-secret-7f3a91";
        let ssh_secret = "ssh-secret-c0ffee";
        secrets
            .set_variable(var_id, None, &SecretString::from(variable_secret))
            .unwrap();
        let credential = CredentialRef {
            server_id: ws.servers[0].id,
            user: ws.servers[0].user.clone(),
            purpose: CredentialPurpose::SshPassword,
        };
        secrets
            .remember(&credential, &SecretString::from(ssh_secret), 1)
            .unwrap();

        for include in [true, false] {
            let text = String::from_utf8(export_json(&ws, include)).unwrap();
            assert!(!text.contains(variable_secret));
            assert!(!text.contains(ssh_secret));
            assert!(text.contains(r#""kind": "secretRef""#));
        }
    }
}
