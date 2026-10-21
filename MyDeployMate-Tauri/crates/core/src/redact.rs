//! Masking known secrets in text (SRS VAR-03, architecture ARC-27, plan task 2.2).
//!
//! [`Redactor`] masks whole strings such as command snapshots. [`StreamRedactor`] masks output
//! that arrives in chunks: a secret split across two chunks is still caught, because a tail that
//! could be the start of a secret is held back until the next chunk or [`StreamRedactor::finish`].

use std::fmt;

use crate::resolve::MASK;

#[derive(Clone, Default)]
pub struct Redactor {
    /// Longest first, so a secret that contains another is masked as a whole.
    secrets: Vec<String>,
}

impl fmt::Debug for Redactor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Redactor({} secrets)", self.secrets.len())
    }
}

impl Redactor {
    pub fn new(secrets: impl IntoIterator<Item = String>) -> Self {
        let mut secrets: Vec<String> = secrets.into_iter().filter(|s| !s.is_empty()).collect();
        secrets.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        secrets.dedup();
        Redactor { secrets }
    }

    pub fn is_empty(&self) -> bool {
        self.secrets.is_empty()
    }

    pub fn redact(&self, text: &str) -> String {
        let (out, rest) = self.scan(text, true);
        debug_assert!(rest.is_empty());
        out
    }

    /// Mask `text`. Unless `last`, stop before a tail that may continue as a secret and return it.
    fn scan<'t>(&self, text: &'t str, last: bool) -> (String, &'t str) {
        let mut out = String::with_capacity(text.len());
        let mut i = 0;
        while i < text.len() {
            let rest = &text[i..];
            if !last
                && self
                    .secrets
                    .iter()
                    .any(|s| s.len() > rest.len() && s.starts_with(rest))
            {
                return (out, rest);
            }
            if let Some(s) = self.secrets.iter().find(|s| rest.starts_with(s.as_str())) {
                out.push_str(MASK);
                i += s.len();
            } else {
                let c = rest.chars().next().unwrap_or_default();
                out.push(c);
                i += c.len_utf8();
            }
        }
        (out, "")
    }
}

/// Masks a stream of output chunks.
#[derive(Debug, Default)]
pub struct StreamRedactor {
    redactor: Redactor,
    pending: String,
}

impl StreamRedactor {
    pub fn new(redactor: Redactor) -> Self {
        StreamRedactor {
            redactor,
            pending: String::new(),
        }
    }

    /// Mask a chunk. The result may be shorter than the input when a possible secret is held back.
    pub fn push(&mut self, chunk: &str) -> String {
        if self.redactor.is_empty() {
            return chunk.to_owned();
        }
        self.pending.push_str(chunk);
        let buf = std::mem::take(&mut self.pending);
        let (out, rest) = self.redactor.scan(&buf, false);
        self.pending = rest.to_owned();
        out
    }

    /// Flush what is held back at the end of the stream.
    pub fn finish(&mut self) -> String {
        let buf = std::mem::take(&mut self.pending);
        self.redactor.redact(&buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_every_occurrence_longest_first() {
        let r = Redactor::new(["abc".to_string(), "abcdef".into(), String::new()]);
        assert_eq!(r.redact("x abcdef y abc z"), "x ****** y ****** z");
        assert_eq!(r.redact("没有秘密"), "没有秘密");
    }

    #[test]
    fn stream_catches_secret_split_across_chunks() {
        let mut s = StreamRedactor::new(Redactor::new(["密码s3cret".to_string()]));
        let mut out = s.push("token=密");
        out += &s.push("码s3");
        out += &s.push("cret done 密码");
        out += &s.finish();
        assert_eq!(out, "token=****** done 密码");
    }

    #[test]
    fn stream_without_secrets_passes_through() {
        let mut s = StreamRedactor::default();
        assert_eq!(s.push("abc"), "abc");
        assert_eq!(s.finish(), "");
    }

    #[test]
    fn debug_does_not_leak() {
        let r = Redactor::new(["hunter2".to_string()]);
        assert!(!format!("{r:?}").contains("hunter2"));
    }
}
