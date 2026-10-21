use std::fs::{File, OpenOptions, TryLockError};

use crate::error::{StoreError, io};
use crate::paths::DataDir;

/// Exclusive OS-level lock on the data directory, held for the life of the process (ARC-08).
/// The OS releases it when the process exits, so a crash never leaves a stale lock.
#[derive(Debug)]
pub struct InstanceLock {
    _file: File,
}

impl InstanceLock {
    pub fn acquire(dir: &DataDir) -> Result<Self, StoreError> {
        let path = dir.lock_file();
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(io(&path))?;
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(TryLockError::WouldBlock) => Err(StoreError::Locked(dir.root().to_path_buf())),
            Err(TryLockError::Error(e)) => Err(StoreError::Io { path, source: e }),
        }
    }
}
