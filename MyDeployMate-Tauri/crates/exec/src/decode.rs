//! Incremental output decoding (SRS STEP-02, architecture ARC-18).
//!
//! Output arrives in arbitrary chunks, so a multi-byte character can be split between two
//! reads. Each decoder keeps the incomplete tail and finishes it with the next chunk.
//!
//! Auto mode decides per line: a line that is valid UTF-8 is shown as UTF-8, otherwise the
//! whole line is decoded with the ANSI code page. P0 targets Chinese Windows, where that is
//! code page 936 (GBK). For WSL, `wsl.exe` writes its own messages (such as "distribution not
//! found") as UTF-16LE, so a stream that starts like UTF-16LE is decoded as UTF-16LE.

use encoding_rs::{Decoder, Encoding, GBK, UTF_8, UTF_16LE};
use mdm_core::OutputEncoding;

/// Decoded text from one push. `lossy` means some bytes became U+FFFD.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Decoded {
    pub text: String,
    pub lossy: bool,
}

pub struct OutputDecoder {
    mode: Mode,
    /// Bytes not decoded yet (auto mode only).
    pending: Vec<u8>,
    /// Auto mode: the current line has switched to the ANSI code page.
    line_decoder: Option<Decoder>,
    /// WSL auto mode: the first bytes have not been seen yet.
    sniff_utf16: bool,
}

enum Mode {
    Auto,
    Fixed(Decoder),
}

impl OutputDecoder {
    pub fn new(encoding: OutputEncoding, wsl: bool) -> Self {
        let mode = match (encoding, wsl) {
            (OutputEncoding::Auto, false) => Mode::Auto,
            (OutputEncoding::Auto | OutputEncoding::Utf8, true) | (OutputEncoding::Utf8, false) => {
                Mode::Fixed(UTF_8.new_decoder_without_bom_handling())
            }
            (OutputEncoding::Gbk, _) => Mode::Fixed(GBK.new_decoder_without_bom_handling()),
        };
        OutputDecoder {
            mode,
            pending: Vec::new(),
            line_decoder: None,
            sniff_utf16: wsl && encoding == OutputEncoding::Auto,
        }
    }

    /// Decode the next chunk. Text for an incomplete trailing character is held back.
    pub fn push(&mut self, bytes: &[u8]) -> Decoded {
        if self.sniff_utf16 {
            self.pending.extend_from_slice(bytes);
            if self.pending.len() < 2 {
                return Decoded::default();
            }
            self.sniff_utf16 = false;
            if looks_utf16le(&self.pending) {
                self.mode = Mode::Fixed(UTF_16LE.new_decoder());
            }
            let bytes = std::mem::take(&mut self.pending);
            return self.push(&bytes);
        }
        match &mut self.mode {
            Mode::Fixed(decoder) => decode_with(decoder, bytes, false),
            Mode::Auto => {
                self.pending.extend_from_slice(bytes);
                self.drain_auto(false)
            }
        }
    }

    /// Decode whatever is left at the end of the stream.
    pub fn finish(&mut self) -> Decoded {
        if self.sniff_utf16 {
            self.sniff_utf16 = false;
            let bytes = std::mem::take(&mut self.pending);
            let mut out = self.push(&bytes);
            let rest = self.finish();
            out.text.push_str(&rest.text);
            out.lossy |= rest.lossy;
            return out;
        }
        match &mut self.mode {
            Mode::Fixed(decoder) => decode_with(decoder, &[], true),
            Mode::Auto => self.drain_auto(true),
        }
    }

    fn drain_auto(&mut self, last: bool) -> Decoded {
        let mut out = Decoded::default();
        let mut start = 0;
        let bytes = std::mem::take(&mut self.pending);
        while start < bytes.len() {
            let rest = &bytes[start..];
            let (segment, complete) = match rest.iter().position(|&b| b == b'\n') {
                Some(i) => (&rest[..=i], true),
                None => (rest, last),
            };
            start += segment.len();
            if let Some(decoder) = &mut self.line_decoder {
                append(&mut out, decode_with(decoder, segment, complete));
            } else {
                match std::str::from_utf8(segment) {
                    Ok(text) => out.text.push_str(text),
                    // An incomplete UTF-8 character at the end of a partial line: wait for more.
                    Err(e) if e.error_len().is_none() && !complete => {
                        let valid = e.valid_up_to();
                        out.text.push_str(
                            std::str::from_utf8(&segment[..valid]).expect("checked prefix"),
                        );
                        self.pending.extend_from_slice(&segment[valid..]);
                        return out;
                    }
                    Err(_) => {
                        let mut decoder = ansi_code_page().new_decoder_without_bom_handling();
                        append(&mut out, decode_with(&mut decoder, segment, complete));
                        if !complete {
                            self.line_decoder = Some(decoder);
                        }
                    }
                }
            }
            if complete {
                self.line_decoder = None;
            }
        }
        out
    }
}

/// The fallback for auto mode.
fn ansi_code_page() -> &'static Encoding {
    GBK
}

fn looks_utf16le(bytes: &[u8]) -> bool {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return true;
    }
    // ASCII text in UTF-16LE has a NUL in every odd position.
    let pairs = bytes.len() / 2;
    pairs > 0 && (0..pairs).all(|i| bytes[2 * i + 1] == 0 && bytes[2 * i] != 0)
}

fn decode_with(decoder: &mut Decoder, bytes: &[u8], last: bool) -> Decoded {
    let capacity = decoder
        .max_utf8_buffer_length(bytes.len())
        .unwrap_or(bytes.len() * 3 + 16);
    let mut text = String::with_capacity(capacity);
    let (_, _, lossy) = decoder.decode_to_string(bytes, &mut text, last);
    Decoded { text, lossy }
}

fn append(out: &mut Decoded, more: Decoded) {
    out.text.push_str(&more.text);
    out.lossy |= more.lossy;
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn gbk(text: &str) -> Vec<u8> {
        GBK.encode(text).0.into_owned()
    }

    /// Feed `bytes` one byte at a time, then finish.
    fn bytewise(decoder: &mut OutputDecoder, bytes: &[u8]) -> Decoded {
        let mut out = Decoded::default();
        for b in bytes {
            append(&mut out, decoder.push(std::slice::from_ref(b)));
        }
        append(&mut out, decoder.finish());
        out
    }

    #[test]
    fn auto_reads_utf8_split_across_chunks() {
        let bytes = "构建完成 ok\n下一行".as_bytes();
        let mut d = OutputDecoder::new(OutputEncoding::Auto, false);
        let out = bytewise(&mut d, bytes);
        assert_eq!(out.text, "构建完成 ok\n下一行");
        assert!(!out.lossy);
    }

    #[test]
    fn auto_shows_partial_utf8_line_without_waiting_for_newline() {
        let mut d = OutputDecoder::new(OutputEncoding::Auto, false);
        let bytes = "进度 50%".as_bytes();
        // Cut inside the second character: the first one and nothing more is shown.
        let first = d.push(&bytes[..4]);
        assert_eq!(first.text, "进");
        assert_eq!(d.push(&bytes[4..]).text, "度 50%");
    }

    #[test]
    fn auto_falls_back_to_gbk_per_line() {
        let mut bytes = gbk("驱动器 C 中的卷没有标签。\r\n");
        bytes.extend_from_slice("utf8 行\n".as_bytes());
        bytes.extend(gbk("最后一行"));
        let mut d = OutputDecoder::new(OutputEncoding::Auto, false);
        let mut out = d.push(&bytes);
        append(&mut out, d.finish());
        assert_eq!(out.text, "驱动器 C 中的卷没有标签。\r\nutf8 行\n最后一行");
        assert!(!out.lossy);
    }

    #[test]
    fn auto_gbk_split_across_chunks() {
        let bytes = gbk("中文测试\r\n第二行");
        let mut d = OutputDecoder::new(OutputEncoding::Auto, false);
        let out = bytewise(&mut d, &bytes);
        assert_eq!(out.text, "中文测试\r\n第二行");
        assert!(!out.lossy);
    }

    #[test]
    fn wrong_fixed_encoding_is_lossy_but_keeps_going() {
        let mut d = OutputDecoder::new(OutputEncoding::Utf8, false);
        let mut out = d.push(&gbk("中文\n"));
        append(&mut out, d.push(b"after\n"));
        append(&mut out, d.finish());
        assert!(out.lossy);
        assert!(out.text.contains('\u{FFFD}'));
        assert!(out.text.ends_with("\nafter\n"));
    }

    #[test]
    fn fixed_gbk_decodes_gbk() {
        let mut d = OutputDecoder::new(OutputEncoding::Gbk, false);
        let out = bytewise(&mut d, &gbk("部署成功"));
        assert_eq!(out.text, "部署成功");
    }

    #[test]
    fn wsl_reads_utf8_and_wsl_exe_utf16_messages() {
        let mut d = OutputDecoder::new(OutputEncoding::Auto, true);
        let out = bytewise(&mut d, "中文 ok\n".as_bytes());
        assert_eq!(out.text, "中文 ok\n");

        let message: Vec<u8> = "There is no distribution with the supplied name.\r\n"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        let mut d = OutputDecoder::new(OutputEncoding::Auto, true);
        let out = bytewise(&mut d, &message);
        assert_eq!(
            out.text,
            "There is no distribution with the supplied name.\r\n"
        );
    }

    #[test]
    fn empty_and_single_byte_streams() {
        let mut d = OutputDecoder::new(OutputEncoding::Auto, true);
        assert_eq!(d.finish(), Decoded::default());
        let mut d = OutputDecoder::new(OutputEncoding::Auto, true);
        assert_eq!(bytewise(&mut d, b"x").text, "x");
    }
}
