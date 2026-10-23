use std::path::{Path, PathBuf};

use crate::error::{StoreError, io};

pub const APP_DIR_NAME: &str = "MyDeployMate";

/// The application data directory and the files inside it (ARC-31).
#[derive(Clone, Debug)]
pub struct DataDir {
    root: PathBuf,
}

impl DataDir {
    /// `%APPDATA%\MyDeployMate` on Windows (the platform config dir elsewhere, for development).
    pub fn default_location() -> Option<Self> {
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

    pub fn lock_file(&self) -> PathBuf {
        self.root.join("app.lock")
    }
}
