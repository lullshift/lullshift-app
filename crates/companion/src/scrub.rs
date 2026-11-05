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

#[cfg(test)]
mod tests {
    use super::*;

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
