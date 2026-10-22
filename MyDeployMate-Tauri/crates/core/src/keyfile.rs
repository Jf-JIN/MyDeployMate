//! Recognise a private key file from its first bytes, so the server form can say what was
//! picked (IXD-17, SRV-02). Only headers are read; key material is never parsed or kept.

use serde::Serialize;
use ts_rs::TS;

/// Bytes of a key file that are enough to recognise its format.
pub const KEY_HEAD_LEN: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(
    tag = "format",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum KeyFormat {
    /// `-----BEGIN OPENSSH PRIVATE KEY-----`.
    OpenSsh {
        encrypted: bool,
    },
    /// Traditional PEM (`BEGIN RSA/EC/DSA PRIVATE KEY`) or PKCS#8.
    Pem {
        encrypted: bool,
    },
    /// PuTTY `.ppk`, version 2 or 3.
    Putty {
        version: u8,
        encrypted: bool,
    },
    Unknown,
}

/// Recognise the format from the start of a key file (at most [`KEY_HEAD_LEN`] bytes needed).
pub fn detect_key_format(head: &[u8]) -> KeyFormat {
    let text = String::from_utf8_lossy(head);
    let text = text.trim_start_matches('\u{feff}').trim_start();
    if let Some(rest) = text.strip_prefix("-----BEGIN OPENSSH PRIVATE KEY-----") {
        return match openssh_encrypted(rest) {
            Some(encrypted) => KeyFormat::OpenSsh { encrypted },
            None => KeyFormat::Unknown,
        };
    }
    if text.starts_with("-----BEGIN ENCRYPTED PRIVATE KEY-----") {
        return KeyFormat::Pem { encrypted: true };
    }
    if text.starts_with("-----BEGIN PRIVATE KEY-----") {
        return KeyFormat::Pem { encrypted: false };
    }
    if let Some(first) = text.lines().next()
        && first.starts_with("-----BEGIN ")
        && first.ends_with(" PRIVATE KEY-----")
    {
        let encrypted = text
            .lines()
            .take(4)
            .any(|l| l.trim() == "Proc-Type: 4,ENCRYPTED");
        return KeyFormat::Pem { encrypted };
    }
    for (prefix, version) in [("PuTTY-User-Key-File-2:", 2), ("PuTTY-User-Key-File-3:", 3)] {
        if text.starts_with(prefix) {
            let encryption = text
                .lines()
                .find_map(|l| l.strip_prefix("Encryption:"))
                .map(str::trim);
            return match encryption {
                Some(e) => KeyFormat::Putty {
                    version,
                    encrypted: e != "none",
                },
                None => KeyFormat::Unknown,
            };
        }
    }
    KeyFormat::Unknown
}

/// The OpenSSH container starts with `openssh-key-v1\0` and the cipher name; `none` means the
/// key has no passphrase.
fn openssh_encrypted(body: &str) -> Option<bool> {
    let b64: String = body
        .chars()
        .filter(|c| !c.is_whitespace())
        .take_while(|&c| c != '-')
        .take(64)
        .collect();
    let bytes = decode_base64_prefix(&b64);
    let rest = bytes.strip_prefix(b"openssh-key-v1\0")?;
    let len = u32::from_be_bytes(rest.get(..4)?.try_into().ok()?) as usize;
    let cipher = rest.get(4..4 + len)?;
    Some(cipher != b"none")
}

/// Decode whole base64 quads until the first character that is not base64.
fn decode_base64_prefix(text: &str) -> Vec<u8> {
    fn value(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some(u32::from(c - b'A')),
            b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::new();
    for quad in text.as_bytes().chunks_exact(4) {
        let Some(values) = quad.iter().map(|&c| value(c)).collect::<Option<Vec<_>>>() else {
            break;
        };
        let n = values.iter().fold(0u32, |acc, v| (acc << 6) | v);
        out.extend_from_slice(&n.to_be_bytes()[1..]);
    }
    out
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    // Headers only; the bodies are truncated and hold no usable key.
    const OPENSSH_PLAIN: &str = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW\n";
    const OPENSSH_ENCRYPTED: &str = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAACmFlczI1Ni1jdHIAAAAGYmNyeXB0AAAAGAAAABD\n";

    #[test]
    fn recognises_openssh_with_and_without_passphrase() {
        assert_eq!(
            detect_key_format(OPENSSH_PLAIN.as_bytes()),
            KeyFormat::OpenSsh { encrypted: false }
        );
        assert_eq!(
            detect_key_format(OPENSSH_ENCRYPTED.as_bytes()),
            KeyFormat::OpenSsh { encrypted: true }
        );
    }

    #[test]
    fn recognises_pem_variants() {
        let rsa = "-----BEGIN RSA PRIVATE KEY-----\nMIIEow...\n";
        let rsa_enc =
            "-----BEGIN RSA PRIVATE KEY-----\nProc-Type: 4,ENCRYPTED\nDEK-Info: AES-128-CBC,00\n";
        assert_eq!(
            detect_key_format(rsa.as_bytes()),
            KeyFormat::Pem { encrypted: false }
        );
        assert_eq!(
            detect_key_format(rsa_enc.as_bytes()),
            KeyFormat::Pem { encrypted: true }
        );
        assert_eq!(
            detect_key_format(b"-----BEGIN ENCRYPTED PRIVATE KEY-----\n"),
            KeyFormat::Pem { encrypted: true }
        );
    }

    #[test]
    fn recognises_putty_versions() {
        let v3 =
            "PuTTY-User-Key-File-3: ssh-ed25519\r\nEncryption: aes256-cbc\r\nComment: deploy\r\n";
        let v2 = "PuTTY-User-Key-File-2: ssh-rsa\nEncryption: none\n";
        assert_eq!(
            detect_key_format(v3.as_bytes()),
            KeyFormat::Putty {
                version: 3,
                encrypted: true
            }
        );
        assert_eq!(
            detect_key_format(v2.as_bytes()),
            KeyFormat::Putty {
                version: 2,
                encrypted: false
            }
        );
    }

    #[test]
    fn public_keys_and_garbage_are_unknown() {
        assert_eq!(
            detect_key_format(b"ssh-ed25519 AAAAC3Nza deploy@host\n"),
            KeyFormat::Unknown
        );
        assert_eq!(detect_key_format(&[0xff, 0x00, 0x13]), KeyFormat::Unknown);
        assert_eq!(
            detect_key_format(b"-----BEGIN OPENSSH PRIVATE KEY-----\nAAAA\n"),
            KeyFormat::Unknown
        );
    }
}
