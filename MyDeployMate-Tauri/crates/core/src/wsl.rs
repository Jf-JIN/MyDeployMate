//! Reading `wsl.exe --list --quiet`, which prints UTF-16LE on Windows (STEP-02).

/// Distribution names from the raw output of `wsl.exe --list --quiet`.
pub fn parse_wsl_list(raw: &[u8]) -> Vec<String> {
    // wsl.exe writes UTF-16LE; ASCII names then leave NUL in every other byte.
    let looks_utf16 = raw.len().is_multiple_of(2) && raw.contains(&0);
    let text = if looks_utf16 {
        let units: Vec<u16> = raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(raw).into_owned()
    };
    text.lines()
        .map(|l| l.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}' || c == '\0'))
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn utf16(text: &str) -> Vec<u8> {
        text.encode_utf16().flat_map(u16::to_le_bytes).collect()
    }

    #[test]
    fn reads_utf16_output() {
        assert_eq!(
            parse_wsl_list(&utf16("\u{feff}Ubuntu-22.04\r\ndocker-desktop\r\n\r\n")),
            vec!["Ubuntu-22.04", "docker-desktop"]
        );
    }

    #[test]
    fn reads_utf8_and_empty_output() {
        assert_eq!(parse_wsl_list(b"Debian\n"), vec!["Debian"]);
        assert_eq!(parse_wsl_list(b""), Vec::<String>::new());
    }
}
