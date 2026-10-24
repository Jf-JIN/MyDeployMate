use std::path::{Path, PathBuf};

use crate::error::{StoreError, io};

pub const APP_DIR_NAME: &str = "MyDeployMate";

/// Overrides the data directory, for measurements and walkthroughs on a copy (plan task 8.1).
pub const DATA_DIR_ENV: &str = "MDM_DATA_DIR";

/// The application data directory and the files inside it (ARC-31).
#[derive(Clone, Debug)]
pub struct DataDir {
    root: PathBuf,
}

impl DataDir {
    /// `%APPDATA%\MyDeployMate` on Windows (the platform config dir elsewhere, for development),
    /// or the directory in `MDM_DATA_DIR` when that is set and not empty.
    pub fn default_location() -> Option<Self> {
        if let Some(dir) = std::env::var_os(DATA_DIR_ENV).filter(|d| !d.is_empty()) {
            return Some(Self::at(dir));
        }
        dirs::config_dir().map(|d| Self::at(d.join(APP_DIR_NAME)))
    }

    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Create the directory tree if missing.
    pub fn ensure(&self) -> Result<(), StoreError> {
        for dir in [self.root.clone(), self.backups()] {
            std::fs::create_dir_all(&dir).map_err(io(&dir))?;
        }
        Ok(())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn workspace_file(&self) -> PathBuf {
        self.root.join("workspace.json")
    }

    /// The previous successful save (DATA-04).
    pub fn workspace_backup(&self) -> PathBuf {
        self.root.join("workspace.json.bak")
    }

    pub(crate) fn workspace_temp(&self) -> PathBuf {
        self.root.join("workspace.json.tmp")
    }

    /// Timestamped copies taken before migrations and imports.
    pub fn backups(&self) -> PathBuf {
        self.root.join("backups")
    }

    /// Run and task records (ARC-31).
    pub fn history_db(&self) -> PathBuf {
        self.root.join("history.sqlite")
    }

    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }

    /// Redacted log of one run, JSON Lines (ARC-31).
    pub fn run_log(&self, run: mdm_core::RunId) -> PathBuf {
        self.logs().join(format!("{run}.jsonl"))
    }

    /// Trusted server keys (ARC-20, ARC-31). Not the user's `~/.ssh/known_hosts`.
    pub fn trust_file(&self) -> PathBuf {
        self.root.join("trust").join("known_hosts.json")
    }

    /// Which credentials are remembered in Credential Manager; never the secrets (SEC-03).
    pub fn credentials_index(&self) -> PathBuf {
        self.root.join("credentials.json")
    }

    pub fn lock_file(&self) -> PathBuf {
        self.root.join("app.lock")
    }
}
