//! Host identity rules (SRS SRV-03, architecture ARC-20).
//!
//! Trust is kept per normalised endpoint (host and port), never per display name. A presented
//! key is trusted when its fingerprint matches any key already recorded for that endpoint;
//! a known endpoint presenting anything else is a change and blocks the connection, whatever
//! the algorithm. Only an explicit update replaces recorded keys.

use std::fmt;
use std::net::Ipv6Addr;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Where a server listens, normalised so that `Example.COM` and `example.com`, or two
/// spellings of one IPv6 address, share one trust entry.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HostEndpoint {
    /// Lower-case DNS name or IP address; IPv6 without brackets, in canonical form.
    pub host: String,
    pub port: u16,
}

impl HostEndpoint {
    pub fn new(host: &str, port: u16) -> Self {
        let host = host.trim();
        let bare = host
            .strip_prefix('[')
            .and_then(|h| h.strip_suffix(']'))
            .unwrap_or(host);
        let host = match bare.parse::<Ipv6Addr>() {
            Ok(ip) => ip.to_string(),
            Err(_) => bare.to_ascii_lowercase(),
        };
        Self { host, port }
    }

    fn is_ipv6(&self) -> bool {
        self.host.contains(':')
    }
}

impl fmt::Display for HostEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_ipv6() {
            write!(f, "[{}]:{}", self.host, self.port)
        } else {
            write!(f, "{}:{}", self.host, self.port)
        }
    }
}

/// A server public key as the operator compares it (IXD-19).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HostKey {
    /// Key algorithm, e.g. `ssh-ed25519`.
    pub algorithm: String,
    /// SHA-256 fingerprint as `ssh-keygen -lf` prints it, e.g. `SHA256:abc…`.
    pub fingerprint: String,
}

/// One recorded key of one endpoint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TrustedHost {
    pub endpoint: HostEndpoint,
    pub key: HostKey,
    /// When the operator trusted the key, in Unix milliseconds.
    #[ts(type = "number")]
    pub trusted_at_ms: i64,
}

/// What the trust store says about a presented key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrustCheck {
    Trusted,
    /// Nothing is recorded for the endpoint: ask the operator (D02, first trust).
    Unknown,
    /// The endpoint is known with other keys: block (D02, key changed).
    Changed {
        known: Vec<HostKey>,
    },
}

/// Check `presented` against the keys recorded for its endpoint.
pub fn check_host_key(known: &[HostKey], presented: &HostKey) -> TrustCheck {
    if known.is_empty() {
        TrustCheck::Unknown
    } else if known.iter().any(|k| k.fingerprint == presented.fingerprint) {
        TrustCheck::Trusted
    } else {
        TrustCheck::Changed {
            known: known.to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(alg: &str, fp: &str) -> HostKey {
        HostKey {
            algorithm: alg.into(),
            fingerprint: fp.into(),
        }
    }

    #[test]
    fn endpoint_normalises_case_brackets_and_ipv6_spelling() {
        assert_eq!(
            HostEndpoint::new(" Example.COM ", 22),
            HostEndpoint::new("example.com", 22)
        );
        let a = HostEndpoint::new("[2001:DB8:0:0::1]", 2222);
        assert_eq!(a, HostEndpoint::new("2001:db8::1", 2222));
        assert_eq!(a.to_string(), "[2001:db8::1]:2222");
        assert_eq!(HostEndpoint::new("10.0.0.5", 22).to_string(), "10.0.0.5:22");
        assert_ne!(
            HostEndpoint::new("10.0.0.5", 22),
            HostEndpoint::new("10.0.0.5", 2222)
        );
    }

    #[test]
    fn unknown_trusted_and_changed() {
        let ed = key("ssh-ed25519", "SHA256:aaa");
        assert_eq!(check_host_key(&[], &ed), TrustCheck::Unknown);
        assert_eq!(
            check_host_key(std::slice::from_ref(&ed), &ed),
            TrustCheck::Trusted
        );
        let other = key("ssh-ed25519", "SHA256:bbb");
        assert_eq!(
            check_host_key(std::slice::from_ref(&ed), &other),
            TrustCheck::Changed {
                known: vec![ed.clone()]
            }
        );
    }

    #[test]
    fn another_algorithm_on_a_known_endpoint_is_a_change() {
        let ed = key("ssh-ed25519", "SHA256:aaa");
        let rsa = key("rsa-sha2-512", "SHA256:rrr");
        assert!(matches!(
            check_host_key(&[ed], &rsa),
            TrustCheck::Changed { .. }
        ));
    }

    #[test]
    fn same_rsa_key_under_another_signature_name_is_trusted() {
        // Turning on "allow legacy" changes the negotiated name, not the key.
        let sha2 = key("rsa-sha2-512", "SHA256:rrr");
        let sha1 = key("ssh-rsa", "SHA256:rrr");
        assert_eq!(check_host_key(&[sha2], &sha1), TrustCheck::Trusted);
    }
}
