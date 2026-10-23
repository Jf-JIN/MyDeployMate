//! Host trust store `trust/known_hosts.json` (SRS SRV-03, architecture ARC-20).
//!
//! Every change is written to disk before it shows in memory, so a failed write never leaves
//! a key looking trusted. A file that cannot be read is an error, not an empty store: an
//! empty store would turn a changed key into a first-trust prompt.

use std::fs;
use std::path::PathBuf;

use mdm_core::{HostEndpoint, HostKey, Message, TrustCheck, TrustedHost, check_host_key};
use serde::{Deserialize, Serialize};

use crate::atomic::write_json;
use crate::error::StoreError;
use crate::paths::DataDir;

const FORMAT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrustFile {
    version: u32,
    hosts: Vec<TrustedHost>,
}

#[derive(Debug)]
pub struct TrustStore {
    path: PathBuf,
    hosts: Vec<TrustedHost>,
}

impl TrustStore {
    pub fn open(dir: &DataDir) -> Result<Self, StoreError> {
        let path = dir.trust_file();
        let hosts = match fs::read(&path) {
            Ok(bytes) => {
                let file: TrustFile =
                    serde_json::from_slice(&bytes).map_err(|e| StoreError::Corrupt {
                        path: path.clone(),
                        reason: Message::text(e.to_string()),
                    })?;
                if file.version > FORMAT_VERSION {
                    return Err(StoreError::UnsupportedVersion {
                        found: file.version,
                        supported: FORMAT_VERSION,
                    });
                }
                file.hosts
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(StoreError::Io { path, source: e }),
        };
        Ok(Self { path, hosts })
    }

    /// All recorded keys, sorted by endpoint.
    pub fn list(&self) -> &[TrustedHost] {
        &self.hosts
    }

    pub fn keys_for(&self, endpoint: &HostEndpoint) -> Vec<HostKey> {
        self.hosts
            .iter()
            .filter(|h| &h.endpoint == endpoint)
            .map(|h| h.key.clone())
            .collect()
    }

    pub fn check(&self, endpoint: &HostEndpoint, presented: &HostKey) -> TrustCheck {
        check_host_key(&self.keys_for(endpoint), presented)
    }

    /// First trust after the operator checked the fingerprint (D02). Refused when the endpoint
    /// already has keys: a change goes through [`TrustStore::replace`] only.
    pub fn trust_new(
        &mut self,
        endpoint: &HostEndpoint,
        key: &HostKey,
        now_ms: i64,
    ) -> Result<(), StoreError> {
        match self.check(endpoint, key) {
            TrustCheck::Trusted => Ok(()),
            TrustCheck::Changed { .. } => Err(StoreError::TrustConflict(endpoint.to_string())),
            TrustCheck::Unknown => {
                let mut hosts = self.hosts.clone();
                hosts.push(TrustedHost {
                    endpoint: endpoint.clone(),
                    key: key.clone(),
                    trusted_at_ms: now_ms,
                });
                self.commit(hosts)
            }
        }
    }

    /// Explicit update from trust management: forget every key of `endpoint`, keep `key`.
    pub fn replace(
        &mut self,
        endpoint: &HostEndpoint,
        key: &HostKey,
        now_ms: i64,
    ) -> Result<(), StoreError> {
        let mut hosts: Vec<_> = self
            .hosts
            .iter()
            .filter(|h| &h.endpoint != endpoint)
            .cloned()
            .collect();
        hosts.push(TrustedHost {
            endpoint: endpoint.clone(),
            key: key.clone(),
            trusted_at_ms: now_ms,
        });
        self.commit(hosts)
    }

    /// Forget an endpoint. Returns whether anything was recorded.
    pub fn remove(&mut self, endpoint: &HostEndpoint) -> Result<bool, StoreError> {
        let hosts: Vec<_> = self
            .hosts
            .iter()
            .filter(|h| &h.endpoint != endpoint)
            .cloned()
            .collect();
        if hosts.len() == self.hosts.len() {
            return Ok(false);
        }
        self.commit(hosts)?;
        Ok(true)
    }

    fn commit(&mut self, mut hosts: Vec<TrustedHost>) -> Result<(), StoreError> {
        hosts.sort_by(|a, b| a.endpoint.cmp(&b.endpoint));
        let file = TrustFile {
            version: FORMAT_VERSION,
            hosts,
        };
        write_json(&self.path, &file)?;
        self.hosts = file.hosts;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(fp: &str) -> HostKey {
        HostKey {
            algorithm: "ssh-ed25519".into(),
            fingerprint: fp.into(),
        }
    }

    fn open() -> (tempfile::TempDir, DataDir, TrustStore) {
        let tmp = tempfile::tempdir().unwrap();
        let dir = DataDir::at(tmp.path().join("MyDeployMate"));
        let store = TrustStore::open(&dir).unwrap();
        (tmp, dir, store)
    }

    #[test]
    fn missing_file_is_an_empty_store() {
        let (_tmp, _dir, store) = open();
        assert!(store.list().is_empty());
    }

    #[test]
    fn first_trust_persists_and_reloads() {
        let (_tmp, dir, mut store) = open();
        let ep = HostEndpoint::new("Server.Example", 22);
        store.trust_new(&ep, &key("SHA256:a"), 1000).unwrap();
        let again = TrustStore::open(&dir).unwrap();
        assert_eq!(again.list(), store.list());
        assert_eq!(
            again.check(&HostEndpoint::new("server.example", 22), &key("SHA256:a")),
            TrustCheck::Trusted
        );
        assert_eq!(
            again.check(&HostEndpoint::new("server.example", 2222), &key("SHA256:a")),
            TrustCheck::Unknown
        );
    }

    #[test]
    fn first_trust_never_overwrites_a_known_endpoint() {
        let (_tmp, _dir, mut store) = open();
        let ep = HostEndpoint::new("h", 22);
        store.trust_new(&ep, &key("SHA256:a"), 1).unwrap();
        let err = store.trust_new(&ep, &key("SHA256:b"), 2).unwrap_err();
        assert!(matches!(err, StoreError::TrustConflict(_)));
        assert_eq!(store.keys_for(&ep), vec![key("SHA256:a")]);
    }

    #[test]
    fn replace_and_remove() {
        let (_tmp, dir, mut store) = open();
        let ep = HostEndpoint::new("h", 22);
        store.trust_new(&ep, &key("SHA256:a"), 1).unwrap();
        store.replace(&ep, &key("SHA256:b"), 2).unwrap();
        assert_eq!(store.keys_for(&ep), vec![key("SHA256:b")]);
        assert!(store.remove(&ep).unwrap());
        assert!(!store.remove(&ep).unwrap());
        assert!(TrustStore::open(&dir).unwrap().list().is_empty());
    }

    #[test]
    fn corrupt_file_is_an_error_not_an_empty_store() {
        let (_tmp, dir, _store) = open();
        fs::create_dir_all(dir.trust_file().parent().unwrap()).unwrap();
        fs::write(dir.trust_file(), b"{not json").unwrap();
        assert!(matches!(
            TrustStore::open(&dir),
            Err(StoreError::Corrupt { .. })
        ));
    }

    #[test]
    fn failed_write_leaves_memory_unchanged() {
        let (_tmp, dir, mut store) = open();
        // A plain file where the trust folder should be makes every write fail.
        fs::create_dir_all(dir.root()).unwrap();
        fs::write(dir.root().join("trust"), b"").unwrap();
        let ep = HostEndpoint::new("h", 22);
        assert!(store.trust_new(&ep, &key("SHA256:a"), 1).is_err());
        assert_eq!(store.check(&ep, &key("SHA256:a")), TrustCheck::Unknown);
    }
}
