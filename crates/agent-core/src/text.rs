//! Text checks shared by the forwarder and the store: characters that can disguise text or
//! break its layout (bidi controls, zero-width and other invisible formatting characters).

/// True for bidi controls and invisible formatting characters.
pub fn is_invisible(c: char) -> bool {
    matches!(
        u32::from(c),
        0x00AD
            | 0x034F
            | 0x061C
            | 0x115F..=0x1160
            | 0x17B4..=0x17B5
            | 0x180B..=0x180E
            | 0x200B..=0x200F
            | 0x2028..=0x202E
            | 0x2060..=0x206F
            | 0x3164
            | 0xFEFF
            | 0xFFA0
            | 0xFFF9..=0xFFFB
            | 0x1BCA0..=0x1BCA3
            | 0x1D173..=0x1D17A
            | 0xE0000..=0xE007F
    )
}

/// True when `text` has a control character (other than newline and tab when `multiline`) or
/// an invisible one.
pub fn has_unsafe_chars(text: &str, multiline: bool) -> bool {
    text.chars()
        .any(|c| (c.is_control() && !(multiline && (c == '\n' || c == '\t'))) || is_invisible(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spots_bidi_invisible_and_control_characters() {
        assert!(has_unsafe_chars("a\u{202E}b", false));
        assert!(has_unsafe_chars("a\u{200B}b", false));
        assert!(has_unsafe_chars("a\nb", false));
        assert!(!has_unsafe_chars("a\nb", true));
        assert!(!has_unsafe_chars("Fix the login bug", false));
    }

    /// The shared plain-text corpus (decision 18.7): every case refused for a hidden character
    /// is refused here too, and nothing the corpus allows is. (Mark runs are the marketplace's
    /// extra rule for creator text, so those cases are not checked here.)
    #[test]
    fn agrees_with_the_shared_plain_text_corpus() {
        let doc: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/plain-text/cases.json"
        ))
        .expect("corpus");
        for case in doc["cases"].as_array().expect("cases") {
            let name = case["name"].as_str().expect("name");
            let text = case["text"].as_str().expect("text");
            match case["reason"].as_str() {
                Some("hidden") => assert!(has_unsafe_chars(text, false), "{name}"),
                Some(_) => {}
                None => assert!(!has_unsafe_chars(text, false), "{name}"),
            }
        }
    }
}
