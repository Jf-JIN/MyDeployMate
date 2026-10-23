//! Map IO and russh errors to [`ConnectErrorKind`] (ARC-36). Only error texts from the library
//! reach the message; they never contain secrets.

use std::io;

use mdm_core::{ConnectError, ConnectErrorKind, ConnectStage};

pub(crate) fn tcp_error(e: &io::Error) -> ConnectError {
    let kind = match e.kind() {
        io::ErrorKind::ConnectionRefused => ConnectErrorKind::ConnectionRefused,
        io::ErrorKind::TimedOut => ConnectErrorKind::Timeout,
        _ => ConnectErrorKind::Unreachable,
    };
    ConnectError::at(ConnectStage::Connect, kind).with_detail(e.to_string())
}

pub(crate) fn ssh_error(stage: ConnectStage, e: &russh::Error) -> ConnectError {
    use russh::Error as E;
    match e {
        E::NoCommonAlgo { kind, ours, theirs } => {
            let legacy_rsa_only = matches!(kind, russh::AlgorithmKind::Key)
                && !theirs.is_empty()
                && theirs
                    .iter()
                    .all(|a| a == "ssh-rsa" || a == "ssh-rsa-cert-v01@openssh.com");
            ConnectError::at(
                ConnectStage::Handshake,
                ConnectErrorKind::AlgorithmNegotiation { legacy_rsa_only },
            )
            .with_detail(format!(
                "{kind:?}：本机 {}；服务器 {}",
                ours.join(", "),
                theirs.join(", ")
            ))
        }
        E::Disconnect | E::HUP | E::SendError | E::RecvError => {
            ConnectError::at(stage, ConnectErrorKind::Disconnected)
        }
        E::IO(io) if io.kind() == io::ErrorKind::UnexpectedEof => {
            ConnectError::at(stage, ConnectErrorKind::Disconnected)
        }
        E::ConnectionTimeout | E::InactivityTimeout | E::KeepaliveTimeout | E::Elapsed(_) => {
            ConnectError::at(stage, ConnectErrorKind::Timeout)
        }
        other => ConnectError::at(stage, ConnectErrorKind::Protocol).with_detail(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_rsa_only_server_suggests_the_legacy_toggle() {
        let e = russh::Error::NoCommonAlgo {
            kind: russh::AlgorithmKind::Key,
            ours: vec!["ssh-ed25519".into(), "rsa-sha2-512".into()],
            theirs: vec!["ssh-rsa".into()],
        };
        let err = ssh_error(ConnectStage::Handshake, &e);
        assert_eq!(
            err.kind,
            ConnectErrorKind::AlgorithmNegotiation {
                legacy_rsa_only: true
            }
        );
        assert!(err.message.contains("允许旧算法"), "{}", err.message);
        assert!(err.message.contains("ssh-rsa"), "{}", err.message);
    }

    #[test]
    fn other_negotiation_failures_do_not() {
        let e = russh::Error::NoCommonAlgo {
            kind: russh::AlgorithmKind::Cipher,
            ours: vec!["aes256-gcm@openssh.com".into()],
            theirs: vec!["3des-cbc".into()],
        };
        assert_eq!(
            ssh_error(ConnectStage::Handshake, &e).kind,
            ConnectErrorKind::AlgorithmNegotiation {
                legacy_rsa_only: false
            }
        );
    }

    #[test]
    fn tcp_errors() {
        let refused = io::Error::from(io::ErrorKind::ConnectionRefused);
        assert_eq!(
            tcp_error(&refused).kind,
            ConnectErrorKind::ConnectionRefused
        );
        let timeout = io::Error::from(io::ErrorKind::TimedOut);
        assert_eq!(tcp_error(&timeout).kind, ConnectErrorKind::Timeout);
        let other = io::Error::from(io::ErrorKind::HostUnreachable);
        assert_eq!(tcp_error(&other).kind, ConnectErrorKind::Unreachable);
    }
}
