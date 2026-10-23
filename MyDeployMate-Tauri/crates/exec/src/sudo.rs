//! sudo for remote commands (SRS SEC-01/02, architecture ARC-24, ARC-25).
//!
//! Non-interactive mode runs `sudo -n`: without a grant it fails at once and nothing asks for a
//! password. Password mode runs `sudo -S -p <marker>` with a random marker as the prompt; the
//! password is written to standard input only after the marker shows up on stderr, so nothing
//! is sent when the server needs no password (NOPASSWD, a cached timestamp). The business
//! command is never run first to "probe" sudo.
//!
//! Script mode elevates `sh -c <script>`, which needs the server to grant a shell; program
//! mode elevates the program itself, so a sudoers rule for that exact command matches. The
//! working directory is entered before sudo, outside the elevated part (ARC-24).
//!
//! sudo's own error lines are recognised after the command ends: a missing grant, a required
//! terminal (`requiretty`, not supported), wrong passwords, and a missing sudo. Phase 0 found
//! that `sudo -n` reports "a password is required" both for users who may sudo with a password
//! and for users who may not sudo at all, so both read as "not granted non-interactively".

use std::future::Future;
use std::pin::Pin;

use mdm_core::Message;
use secrecy::SecretString;

/// How a remote command is elevated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sudo<'a> {
    NonInteractive {
        run_as: &'a str,
    },
    /// `marker` is the prompt sudo prints; see [`prompt_marker`].
    Password {
        run_as: &'a str,
        marker: &'a str,
    },
}

impl Sudo<'_> {
    pub fn run_as(&self) -> &str {
        match self {
            Sudo::NonInteractive { run_as } | Sudo::Password { run_as, .. } => run_as,
        }
    }
}

/// A fresh prompt marker: random, without `%` (which sudo expands) or anything a command is
/// likely to print.
pub fn prompt_marker(token: &str) -> String {
    let token: String = token
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    format!("[mdm-sudo-{token}]")
}

/// The words that elevate the command that follows them, quoted for `sh`.
pub(crate) fn sudo_words(sudo: &Sudo<'_>, quote: impl Fn(&str) -> String) -> String {
    match sudo {
        Sudo::NonInteractive { run_as } => format!("sudo -n -u {} --", quote(run_as)),
        Sudo::Password { run_as, marker } => {
            format!("sudo -S -p {} -u {} --", quote(marker), quote(run_as))
        }
    }
}

pub type PasswordFuture<'a> = Pin<Box<dyn Future<Output = Option<SecretString>> + Send + 'a>>;

/// Supplies the sudo password when the prompt appears.
pub trait PasswordFeed: Send + Sync {
    /// `attempt` counts prompts from 1; a later attempt means the previous value was
    /// rejected. `None` declines, which cancels the task (SEC-02).
    fn password(&self, attempt: u8) -> PasswordFuture<'_>;
}

/// What happened with sudo while a command ran.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SudoReport {
    /// Password prompts seen.
    pub prompts: u8,
    /// Passwords written.
    pub answered: u8,
    /// Recognised from sudo's messages once the command has failed.
    pub failure: Option<SudoFailure>,
}

impl SudoReport {
    /// The last password written was taken: no prompt followed it and sudo did not complain.
    pub fn accepted(&self) -> bool {
        self.answered > 0 && self.answered == self.prompts && self.failure.is_none()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SudoFailure {
    /// `sudo -n` without a grant, or a grant that needs a password (IXD-33).
    NotGranted,
    RequiresTty,
    /// Passwords were rejected until sudo gave up.
    WrongPassword,
    /// Authenticated, but sudoers does not allow this command as this user.
    NotAllowed,
    Missing,
}

impl SudoFailure {
    pub fn message(&self, run_as: &str) -> Message {
        match self {
            SudoFailure::NotGranted => Message::SudoNotGranted,
            SudoFailure::RequiresTty => Message::SudoRequiresTty,
            SudoFailure::WrongPassword => Message::SudoWrongPassword,
            SudoFailure::NotAllowed => Message::SudoNotAllowed {
                target: run_as.into(),
            },
            SudoFailure::Missing => Message::SudoMissing,
        }
    }
}

/// Recognise sudo's own complaint in the start of a failed command's stderr.
pub(crate) fn classify(stderr: &str, exit_code: Option<u32>) -> Option<SudoFailure> {
    if exit_code == Some(0) {
        return None;
    }
    let has = |s: &str| stderr.contains(s);
    if has("must have a tty") {
        Some(SudoFailure::RequiresTty)
    } else if has("incorrect password attempt") {
        Some(SudoFailure::WrongPassword)
    } else if has("a password is required") {
        Some(SudoFailure::NotGranted)
    } else if has("is not in the sudoers file")
        || has("is not allowed to execute")
        || has("may not run sudo")
        || has("not allowed to run sudo")
    {
        Some(SudoFailure::NotAllowed)
    } else if exit_code == Some(127) && (has("sudo: not found") || has("sudo: command not found")) {
        Some(SudoFailure::Missing)
    } else {
        None
    }
}

/// Finds the prompt marker in stderr, also when it is split across chunks, and removes it.
pub(crate) struct MarkerScan {
    marker: Vec<u8>,
    held: Vec<u8>,
}

impl MarkerScan {
    pub(crate) fn new(marker: &str) -> Self {
        Self {
            marker: marker.as_bytes().to_vec(),
            held: Vec::new(),
        }
    }

    /// Returns the bytes to show and how many prompts they contained.
    pub(crate) fn push(&mut self, data: &[u8]) -> (Vec<u8>, u8) {
        let mut buf = std::mem::take(&mut self.held);
        buf.extend_from_slice(data);
        let m = &self.marker;
        let mut out = Vec::with_capacity(buf.len());
        let mut found = 0u8;
        let mut i = 0;
        while i < buf.len() {
            if buf[i..].starts_with(m) {
                found = found.saturating_add(1);
                i += m.len();
            } else if m.starts_with(&buf[i..]) {
                // A marker may be cut off at the end of this chunk.
                self.held = buf[i..].to_vec();
                break;
            } else {
                out.push(buf[i]);
                i += 1;
            }
        }
        (out, found)
    }

    pub(crate) fn finish(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.held)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_is_found_across_chunks_and_removed() {
        let mut scan = MarkerScan::new("[mdm-sudo-ab]");
        let (a, n1) = scan.push(b"lecture\n[mdm-su");
        let (b, n2) = scan.push(b"do-ab]Sorry, try again.\n[mdm-sudo-ab]");
        let (c, n3) = scan.push(b"[mdm");
        let rest = scan.finish();
        assert_eq!((n1, n2, n3), (0, 2, 0));
        assert_eq!(
            [a, b, c, rest].concat(),
            b"lecture\nSorry, try again.\n[mdm".to_vec()
        );
    }

    #[test]
    fn marker_has_no_percent_signs() {
        assert_eq!(prompt_marker("01af-%u"), "[mdm-sudo-01afu]");
    }

    #[test]
    fn sudo_messages() {
        let c = |s: &str, code| classify(s, Some(code));
        assert_eq!(
            c("sudo: a password is required\n", 1),
            Some(SudoFailure::NotGranted)
        );
        assert_eq!(
            c("sudo: sorry, you must have a tty to run sudo\n", 1),
            Some(SudoFailure::RequiresTty)
        );
        assert_eq!(
            c(
                "Sorry, try again.\nsudo: 3 incorrect password attempts\n",
                1
            ),
            Some(SudoFailure::WrongPassword)
        );
        assert_eq!(
            c("nosudo is not in the sudoers file.\n", 1),
            Some(SudoFailure::NotAllowed)
        );
        assert_eq!(
            c("sh: 1: exec: sudo: not found\n", 127),
            Some(SudoFailure::Missing)
        );
        assert_eq!(c("app failed\n", 3), None);
        assert_eq!(c("sudo: a password is required\n", 0), None);
    }

    #[test]
    fn accepted_needs_every_prompt_answered_and_no_failure() {
        let r = |prompts, answered, failure| SudoReport {
            prompts,
            answered,
            failure,
        };
        assert!(r(1, 1, None).accepted());
        assert!(r(2, 2, None).accepted());
        assert!(!r(0, 0, None).accepted());
        assert!(!r(2, 1, None).accepted());
        assert!(!r(1, 1, Some(SudoFailure::NotAllowed)).accepted());
    }
}
