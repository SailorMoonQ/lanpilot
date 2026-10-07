//! Sanitizing text received from peers before it is shown to the user.

/// Maximum length of a display name, in characters.
pub const MAX_DISPLAY_NAME_CHARS: usize = 64;

/// Unicode format characters (general category Cf) that can disguise text:
/// bidi embeddings, overrides and isolates, zero-width characters, invisible
/// operators, the soft hyphen and interlinear annotation marks. Listed
/// explicitly to avoid a Unicode tables dependency.
const FORMAT_CHARS: &[(char, char)] = &[
    ('\u{00AD}', '\u{00AD}'), // soft hyphen
    ('\u{061C}', '\u{061C}'), // Arabic letter mark
    ('\u{180E}', '\u{180E}'), // Mongolian vowel separator
    ('\u{200B}', '\u{200F}'), // zero-width space/joiners, LRM, RLM
    ('\u{202A}', '\u{202E}'), // bidi embeddings and overrides
    ('\u{2060}', '\u{2064}'), // word joiner, invisible operators
    ('\u{2066}', '\u{2069}'), // bidi isolates
    ('\u{206A}', '\u{206F}'), // deprecated format characters
    ('\u{FEFF}', '\u{FEFF}'), // zero-width no-break space (BOM)
    ('\u{FFF9}', '\u{FFFB}'), // interlinear annotation
];

/// Control characters (Cc, via `char::is_control`) and the listed format characters.
fn is_hidden(c: char) -> bool {
    c.is_control() || FORMAT_CHARS.iter().any(|(lo, hi)| (*lo..=*hi).contains(&c))
}

/// Makes a peer-supplied device name safe to display: drops control (Cc) and
/// the format (Cf) characters listed in `FORMAT_CHARS`, trims surrounding
/// whitespace and keeps at most [`MAX_DISPLAY_NAME_CHARS`] characters.
pub fn sanitize_display_name(s: &str) -> String {
    let visible: String = s.chars().filter(|c| !is_hidden(*c)).collect();
    let capped: String = visible
        .trim()
        .chars()
        .take(MAX_DISPLAY_NAME_CHARS)
        .collect();
    capped.trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_bidi_overrides_and_isolates() {
        assert_eq!(
            sanitize_display_name("PC\u{202E}gpj.exe\u{202C}\u{2066}x\u{2069}"),
            "PCgpj.exex"
        );
    }

    #[test]
    fn drops_zero_width_and_other_format_chars() {
        assert_eq!(
            sanitize_display_name("D\u{200B}e\u{200D}s\u{FEFF}k\u{00AD}\u{2060}\u{FFF9}"),
            "Desk"
        );
    }

    #[test]
    fn drops_nul_and_newlines() {
        assert_eq!(sanitize_display_name("a\u{0}b\nc\r\td\u{7F}\u{85}"), "abcd");
    }

    #[test]
    fn caps_at_64_chars() {
        let name = sanitize_display_name(&"x".repeat(100));
        assert_eq!(name.chars().count(), MAX_DISPLAY_NAME_CHARS);
    }

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(sanitize_display_name("  My PC \u{3000}"), "My PC");
        // Whitespace left at the end by the cap is trimmed too.
        let name = sanitize_display_name(&format!("{} y", "x".repeat(63)));
        assert_eq!(name, "x".repeat(63));
    }

    #[test]
    fn preserves_cjk_and_inner_spaces() {
        assert_eq!(sanitize_display_name("书房 台式机"), "书房 台式机");
    }
}
