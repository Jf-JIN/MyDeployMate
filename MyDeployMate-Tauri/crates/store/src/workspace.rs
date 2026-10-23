//! `workspace.json`: load, migrate, validate and save atomically with a backup (DATA-04).

use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use mdm_core::{Workspace, defaults, parse_workspace, validate_workspace};

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
        let doc: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|e| corrupt(&path, e.to_string()))?;
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
                    .ok_or_else(|| corrupt(&path, "缺少 revision 字段".into()))?;
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

fn corrupt(path: &Path, message: String) -> StoreError {
    StoreError::Corrupt {
        path: path.to_path_buf(),
        message,
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
}
