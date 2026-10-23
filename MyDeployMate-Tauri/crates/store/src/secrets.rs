//! Secrets kept in Windows Credential Manager (SRS SEC-03, VAR-03; architecture ARC-10, ARC-26).
//!
//! Two kinds of entries live there, under the service name `MyDeployMate`:
//! - remembered passwords and passphrases, account `<serverId>/<user>/<purpose>[:<runAs>]`;
//! - values of sensitive variables, account `var/<variableId>[/<environmentId>]`.
//!
//! Credential Manager cannot list entries by prefix, so remembered credentials are also
//! recorded, without the secret, in `credentials.json`; that index is what the configuration
//! center lists (IXD-36). Values never touch the data directory.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use mdm_core::{CredentialPurpose, CredentialRef, EnvironmentId, ServerId, VariableId};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::atomic::write_json;
use crate::error::StoreError;
use crate::paths::DataDir;

#[cfg(windows)]
const SERVICE: &str = "MyDeployMate";
const FORMAT_VERSION: u32 = 1;

/// Where secrets are stored. Windows Credential Manager in the app; memory in tests and on
/// development machines without it.
pub trait SecretVault: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<SecretString>, String>;
    fn set(&self, account: &str, value: &SecretString) -> Result<(), String>;
    /// Deleting a missing entry is not an error.
    fn delete(&self, account: &str) -> Result<(), String>;
}

/// Windows Credential Manager through `keyring`.
#[cfg(windows)]
#[derive(Debug, Default)]
pub struct CredentialManager;

#[cfg(windows)]
impl SecretVault for CredentialManager {
    fn get(&self, account: &str) -> Result<Option<SecretString>, String> {
        let entry = keyring::Entry::new(SERVICE, account).map_err(|e| e.to_string())?;
        match entry.get_password() {
            Ok(v) => Ok(Some(SecretString::from(v))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn set(&self, account: &str, value: &SecretString) -> Result<(), String> {
        use secrecy::ExposeSecret;
        let entry = keyring::Entry::new(SERVICE, account).map_err(|e| e.to_string())?;
        entry
            .set_password(value.expose_secret())
            .map_err(|e| e.to_string())
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        let entry = keyring::Entry::new(SERVICE, account).map_err(|e| e.to_string())?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// Secrets in process memory only.
#[derive(Default)]
pub struct MemoryVault(Mutex<HashMap<String, SecretString>>);

impl SecretVault for MemoryVault {
    fn get(&self, account: &str) -> Result<Option<SecretString>, String> {
        let map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        Ok(map.get(account).cloned())
    }

    fn set(&self, account: &str, value: &SecretString) -> Result<(), String> {
        let mut map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        map.insert(account.to_owned(), value.clone());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(account);
        Ok(())
    }
}

/// The vault the app uses on this platform.
pub fn platform_vault() -> Arc<dyn SecretVault> {
    #[cfg(windows)]
    {
        Arc::new(CredentialManager)
    }
    #[cfg(not(windows))]
    {
        Arc::new(MemoryVault::default())
    }
}

/// One remembered credential as the configuration center lists it. No secret.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SavedCredential {
    #[serde(flatten)]
    pub credential: CredentialRef,
    #[ts(type = "number")]
    pub saved_at_ms: i64,
}

#[derive(Serialize, Deserialize)]
struct IndexFile {
    version: u32,
    saved: Vec<SavedCredential>,
}

pub struct SecretStore {
    vault: Arc<dyn SecretVault>,
    index_path: PathBuf,
    saved: Vec<SavedCredential>,
}

fn vault_error(e: String) -> StoreError {
    StoreError::Vault(e)
}

pub fn credential_account(c: &CredentialRef) -> String {
    let purpose = match &c.purpose {
        CredentialPurpose::SshPassword => "ssh-password".to_owned(),
        CredentialPurpose::KeyPassphrase => "key-passphrase".to_owned(),
        CredentialPurpose::SudoPassword { run_as } => format!("sudo:{run_as}"),
    };
    format!("{}/{}/{purpose}", c.server_id, c.user)
}

pub fn variable_account(variable: VariableId, environment: Option<EnvironmentId>) -> String {
    match environment {
        Some(env) => format!("var/{variable}/{env}"),
        None => format!("var/{variable}"),
    }
}

impl SecretStore {
    /// Open with the index in `dir`. A missing index is empty; a damaged one is an error.
    pub fn open(dir: &DataDir, vault: Arc<dyn SecretVault>) -> Result<Self, StoreError> {
        let index_path = dir.credentials_index();
        let saved = match std::fs::read(&index_path) {
            Ok(bytes) => {
                let file: IndexFile =
                    serde_json::from_slice(&bytes).map_err(|e| StoreError::Corrupt {
                        path: index_path.clone(),
                        message: e.to_string(),
                    })?;
                if file.version > FORMAT_VERSION {
                    return Err(StoreError::UnsupportedVersion {
                        found: file.version,
                        supported: FORMAT_VERSION,
                    });
                }
                file.saved
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => {
                return Err(StoreError::Io {
                    path: index_path,
                    source: e,
                });
            }
        };
        Ok(Self {
            vault,
            index_path,
            saved,
        })
    }

    pub fn list(&self) -> &[SavedCredential] {
        &self.saved
    }

    /// A remembered credential, if one is recorded and still in the vault.
    pub fn credential(&self, c: &CredentialRef) -> Result<Option<SecretString>, StoreError> {
        if !self.saved.iter().any(|s| &s.credential == c) {
            return Ok(None);
        }
        self.vault.get(&credential_account(c)).map_err(vault_error)
    }

    /// Remember a credential that just worked (ARC-26: only after it was accepted).
    pub fn remember(
        &mut self,
        c: &CredentialRef,
        secret: &SecretString,
        now_ms: i64,
    ) -> Result<(), StoreError> {
        self.vault
            .set(&credential_account(c), secret)
            .map_err(vault_error)?;
        let mut saved: Vec<_> = self
            .saved
            .iter()
            .filter(|s| &s.credential != c)
            .cloned()
            .collect();
        saved.push(SavedCredential {
            credential: c.clone(),
            saved_at_ms: now_ms,
        });
        self.commit(saved)
    }

    /// Forget one credential. Returns whether it was recorded.
    pub fn forget(&mut self, c: &CredentialRef) -> Result<bool, StoreError> {
        self.forget_where(|s| &s.credential == c)
    }

    /// Forget every credential of a server, e.g. when it is deleted (IXD-20).
    pub fn forget_server(&mut self, server: ServerId) -> Result<usize, StoreError> {
        let before = self.saved.len();
        self.forget_where(|s| s.credential.server_id == server)?;
        Ok(before - self.saved.len())
    }

    fn forget_where(
        &mut self,
        matches: impl Fn(&SavedCredential) -> bool,
    ) -> Result<bool, StoreError> {
        let (gone, kept): (Vec<_>, Vec<_>) = self.saved.iter().cloned().partition(|s| matches(s));
        if gone.is_empty() {
            return Ok(false);
        }
        for s in &gone {
            self.vault
                .delete(&credential_account(&s.credential))
                .map_err(vault_error)?;
        }
        self.commit(kept)?;
        Ok(true)
    }

    fn commit(&mut self, saved: Vec<SavedCredential>) -> Result<(), StoreError> {
        let file = IndexFile {
            version: FORMAT_VERSION,
            saved,
        };
        write_json(&self.index_path, &file)?;
        self.saved = file.saved;
        Ok(())
    }

    /// The value of a sensitive variable (or its override for one environment).
    pub fn variable(
        &self,
        variable: VariableId,
        environment: Option<EnvironmentId>,
    ) -> Result<Option<SecretString>, StoreError> {
        self.vault
            .get(&variable_account(variable, environment))
            .map_err(vault_error)
    }

    pub fn set_variable(
        &self,
        variable: VariableId,
        environment: Option<EnvironmentId>,
        value: &SecretString,
    ) -> Result<(), StoreError> {
        self.vault
            .set(&variable_account(variable, environment), value)
            .map_err(vault_error)
    }

    pub fn delete_variable(
        &self,
        variable: VariableId,
        environment: Option<EnvironmentId>,
    ) -> Result<(), StoreError> {
        self.vault
            .delete(&variable_account(variable, environment))
            .map_err(vault_error)
    }
}

#[cfg(test)]
mod tests {
    use secrecy::ExposeSecret;

    use super::*;

    fn open() -> (tempfile::TempDir, DataDir, Arc<MemoryVault>, SecretStore) {
        let tmp = tempfile::tempdir().unwrap();
        let dir = DataDir::at(tmp.path().join("MyDeployMate"));
        let vault = Arc::new(MemoryVault::default());
        let store = SecretStore::open(&dir, vault.clone()).unwrap();
        (tmp, dir, vault, store)
    }

    fn cred(server: ServerId, purpose: CredentialPurpose) -> CredentialRef {
        CredentialRef {
            server_id: server,
            user: "deploy".into(),
            purpose,
        }
    }

    #[test]
    fn remember_lists_reads_and_forgets() {
        let (_tmp, dir, vault, mut store) = open();
        let server = ServerId::new();
        let c = cred(server, CredentialPurpose::SshPassword);
        assert!(store.credential(&c).unwrap().is_none());
        store
            .remember(&c, &SecretString::from("p@ss 中文"), 5)
            .unwrap();
        assert_eq!(
            store.credential(&c).unwrap().unwrap().expose_secret(),
            "p@ss 中文"
        );

        let again = SecretStore::open(&dir, vault.clone()).unwrap();
        assert_eq!(again.list().len(), 1);
        assert_eq!(again.list()[0].saved_at_ms, 5);
        // The index never holds the secret.
        let index = std::fs::read_to_string(dir.credentials_index()).unwrap();
        assert!(!index.contains("p@ss"), "{index}");

        assert!(store.forget(&c).unwrap());
        assert!(store.credential(&c).unwrap().is_none());
        assert!(vault.get(&credential_account(&c)).unwrap().is_none());
    }

    #[test]
    fn remembering_again_replaces_the_entry() {
        let (_tmp, _dir, _vault, mut store) = open();
        let c = cred(ServerId::new(), CredentialPurpose::KeyPassphrase);
        store.remember(&c, &SecretString::from("a"), 1).unwrap();
        store.remember(&c, &SecretString::from("b"), 2).unwrap();
        assert_eq!(store.list().len(), 1);
        assert_eq!(store.credential(&c).unwrap().unwrap().expose_secret(), "b");
    }

    #[test]
    fn forget_server_removes_all_of_its_entries_only() {
        let (_tmp, _dir, _vault, mut store) = open();
        let a = ServerId::new();
        let b = ServerId::new();
        for c in [
            cred(a, CredentialPurpose::SshPassword),
            cred(
                a,
                CredentialPurpose::SudoPassword {
                    run_as: "root".into(),
                },
            ),
            cred(b, CredentialPurpose::SshPassword),
        ] {
            store.remember(&c, &SecretString::from("x"), 1).unwrap();
        }
        assert_eq!(store.forget_server(a).unwrap(), 2);
        assert_eq!(store.list().len(), 1);
        assert_eq!(store.list()[0].credential.server_id, b);
    }

    #[test]
    fn account_names() {
        let server = ServerId::new();
        let c = cred(
            server,
            CredentialPurpose::SudoPassword {
                run_as: "root".into(),
            },
        );
        assert_eq!(credential_account(&c), format!("{server}/deploy/sudo:root"));
        let v = VariableId::new();
        let e = EnvironmentId::new();
        assert_eq!(variable_account(v, None), format!("var/{v}"));
        assert_eq!(variable_account(v, Some(e)), format!("var/{v}/{e}"));
    }

    #[test]
    fn variable_values() {
        let (_tmp, _dir, _vault, store) = open();
        let v = VariableId::new();
        let e = EnvironmentId::new();
        store
            .set_variable(v, Some(e), &SecretString::from("token"))
            .unwrap();
        assert!(store.variable(v, None).unwrap().is_none());
        assert_eq!(
            store.variable(v, Some(e)).unwrap().unwrap().expose_secret(),
            "token"
        );
        store.delete_variable(v, Some(e)).unwrap();
        assert!(store.variable(v, Some(e)).unwrap().is_none());
    }
}
