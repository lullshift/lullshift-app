//! Last line of defence: removes a key from any string before it is shown or stored.

const HIDDEN: &str = "[hidden]";

/// Replaces every occurrence of `key` (plain and JSON-escaped) in `text` with `[hidden]`.
/// An empty or very short key is ignored.
pub fn scrub(text: &str, key: &str) -> String {
    if key.len() < 4 {
        return text.to_owned();
    }
    let mut out = text.replace(key, HIDDEN);
    let json = serde_json::to_string(key).unwrap_or_default();
    if let Some(inner) = json.strip_prefix('"').and_then(|s| s.strip_suffix('"'))
        && inner != key
    {
        out = out.replace(inner, HIDDEN);
    }
    out
}

/// Replaces anything in `text` that looks like an API key, whether or not it is the key in use:
/// a word starting `sk-` / `sk_`, or a long unbroken run of letters, digits, `-` and `_` that
/// mixes letters and digits. Works word by word on text whose whitespace is already collapsed.
/// This is for provider text shown to the user, where a provider may echo a masked key.
pub fn scrub_key_like(text: &str) -> String {
    let word_char = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    text.split(' ')
        .map(|word| {
            let core = word.trim_matches(|c: char| !word_char(c));
            if core.is_empty() {
                return word.to_owned();
            }
            let lower = core.to_ascii_lowercase();
            let prefixed =
                (lower.starts_with("sk-") || lower.starts_with("sk_")) && core.len() >= 8;
            let long = core.len() >= 32
                && core.chars().all(word_char)
                && core.chars().any(|c| c.is_ascii_digit())
                && core.chars().any(|c| c.is_ascii_alphabetic());
            if prefixed || long {
                word.replacen(core, HIDDEN, 1)
            } else {
                word.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_like_words_are_hidden_and_plain_words_are_not() {
        let t = scrub_key_like(
            "Incorrect API key provided: sk-proj-****abcd. See https://platform.openai.com/account/api-keys and abcdEFGH1234abcdEFGH1234abcdEFGH1234.",
        );
        assert!(!t.contains("sk-proj"), "{t}");
        assert!(!t.contains("EFGH1234"), "{t}");
        assert!(t.contains("Incorrect API key provided"));
        assert_eq!(
            scrub_key_like("The model does not exist"),
            "The model does not exist"
        );
    }

    #[test]
    fn removes_every_occurrence() {
        let k = "sk-live-ABCDEF123456";
        let s = format!("auth: {k}; again {k}. Bearer {k}");
        let out = scrub(&s, k);
        assert!(!out.contains("ABCDEF"));
        assert_eq!(out.matches("[hidden]").count(), 3);
    }

    #[test]
    fn removes_json_escaped_form() {
        let k = r#"ab"cd\ef-12345"#;
        let escaped = serde_json::to_string(k).unwrap();
        let out = scrub(&format!("body={escaped}"), k);
        assert!(!out.contains("ef-12345"));
    }

    #[test]
    fn short_or_empty_key_changes_nothing() {
        assert_eq!(scrub("hello", ""), "hello");
        assert_eq!(scrub("hello", "he"), "hello");
    }
}
