//! Masking known secrets in text (SRS VAR-03, architecture ARC-27, plan task 2.2).
//!
//! [`Redactor`] masks whole strings such as command snapshots. [`StreamRedactor`] masks output
//! that arrives in chunks: a secret split across two chunks is still caught, because a tail that
//! could be the start of a secret is held back until the next chunk or [`StreamRedactor::finish`].

use std::fmt;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::resolve::MASK;

/// Keys that hold a serde enum tag; their values name a variant and are never user text.
const TAG_KEYS: [&str; 3] = ["code", "kind", "type"];

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

    /// Also mask `secret` from now on, e.g. a password typed during the run.
    pub fn add(&mut self, secret: &str) {
        if secret.is_empty() || self.secrets.iter().any(|s| s == secret) {
            return;
        }
        self.secrets.push(secret.to_owned());
        self.secrets
            .sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
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

    /// Mask every string inside a record before it is stored, e.g. a failure reason or an upload
    /// detail whose paths or third-party text came from resolved fields (SEC-04, ARC-27). Enum
    /// tags are left alone. If the masked record no longer reads back (a secret equal to an enum
    /// value), the record is returned unchanged.
    pub fn redact_value<T: Serialize + DeserializeOwned>(&self, value: T) -> T {
        if self.is_empty() {
            return value;
        }
        let Ok(mut json) = serde_json::to_value(&value) else {
            return value;
        };
        if !self.redact_json(&mut json) {
            return value;
        }
        serde_json::from_value(json).unwrap_or(value)
    }

    /// Returns whether anything was masked.
    fn redact_json(&self, value: &mut Value) -> bool {
        match value {
            Value::String(s) => {
                let masked = self.redact(s);
                let changed = masked != *s;
                *s = masked;
                changed
            }
            Value::Array(items) => items
                .iter_mut()
                .fold(false, |changed, v| self.redact_json(v) | changed),
            Value::Object(fields) => fields.iter_mut().fold(false, |changed, (k, v)| {
                if TAG_KEYS.contains(&k.as_str()) && v.is_string() {
                    changed
                } else {
                    self.redact_json(v) | changed
                }
            }),
            _ => false,
        }
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

    /// Also mask `secret` from now on.
    pub fn add(&mut self, secret: &str) {
        self.redactor.add(secret);
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
    fn added_secret_is_masked_from_then_on() {
        let mut s = StreamRedactor::default();
        assert_eq!(s.push("pw=hunter2\n"), "pw=hunter2\n");
        s.add("hunter2");
        s.add("");
        let mut out = s.push("pw=hun");
        out += &s.push("ter2");
        out += &s.finish();
        assert_eq!(out, "pw=******");
    }

    #[test]
    fn stream_without_secrets_passes_through() {
        let mut s = StreamRedactor::default();
        assert_eq!(s.push("abc"), "abc");
        assert_eq!(s.finish(), "");
    }

    #[test]
    fn redact_value_masks_nested_text_but_not_tags() {
        use crate::message::Message;
        let r = Redactor::new(["tok-9f2".to_string(), "text".into()]);
        let reason = Message::SourceMissing {
            path: "/srv/tok-9f2/app".into(),
        }
        .with_detail("open /srv/tok-9f2/app: denied");
        assert_eq!(
            r.redact_value(reason),
            Message::SourceMissing {
                path: "/srv/******/app".into(),
            }
            .with_detail("open /srv/******/app: denied")
        );
        // The secret "text" equals the tag of Message::Text, which stays readable.
        assert_eq!(
            r.redact_value(Message::text("plain text")),
            Message::text("plain ******")
        );
        assert_eq!(Redactor::default().redact_value(7_u32), 7);
    }

    #[test]
    fn debug_does_not_leak() {
        let r = Redactor::new(["hunter2".to_string()]);
        assert!(!format!("{r:?}").contains("hunter2"));
    }
}
