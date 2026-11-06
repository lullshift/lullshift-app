//! The host's plain-text rule for every string in a content pack's data (security review LOW-2,
//! decision 18.7).
//!
//! Engine data schemas refuse the host's hidden characters with their plain-text `pattern`. The
//! other half of the host rule, "no more than four combining marks in a row", needs a lookahead,
//! which the app's pattern translation ([`crate::data_schema::compile`]) refuses. So, rather than
//! loosen that translation, every string in pack data (values and field names) is also checked
//! here with the host's own [`hidden_char`] and [`long_mark_run`], after the schema. The CLI and
//! the engine compatibility check run the same walk (`packTextProblems` in
//! `@lullshift/activity-sdk`), with the same words.

use serde_json::Value;

use crate::data_schema;
use crate::manifest::{hidden_char, long_mark_run};

/// Same words as the schema's plain-text `pattern` problem, so the two de-duplicate.
const HIDDEN_PROBLEM: &str =
    "contains a line break, tab or invisible character, which isn't allowed";
const MARKS_PROBLEM: &str = "has more than four combining marks in a row, which isn't allowed";
/// At most this many text problems are reported.
const MAX_PROBLEMS: usize = 20;

/// Problems with the pack data: the engine schema's, then the plain-text rule's, de-duplicated.
/// Empty means valid. Every place that checks pack data (install review, bundled and installed
/// packs at load, the engine fixture tests) uses this, never the schema check alone.
pub fn pack_data_problems(schema: &Value, data: &Value) -> Vec<String> {
    let mut out = data_schema::validate(schema, data);
    for p in text_problems(data) {
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// Strings (values and field names) in `data` that break the host's plain-text rule, as
/// `cards[3].front contains ...` lines.
pub fn text_problems(data: &Value) -> Vec<String> {
    let mut out = Vec::new();
    walk(data, &mut String::new(), &mut out);
    out
}

fn problem(text: &str) -> Option<&'static str> {
    if text.chars().any(hidden_char) {
        Some(HIDDEN_PROBLEM)
    } else if long_mark_run(text) {
        Some(MARKS_PROBLEM)
    } else {
        None
    }
}

fn say(at: &str, message: &str) -> String {
    if at.is_empty() {
        format!("the data {message}")
    } else {
        format!("{at} {message}")
    }
}

fn walk(v: &Value, at: &mut String, out: &mut Vec<String>) {
    if out.len() >= MAX_PROBLEMS {
        return;
    }
    match v {
        Value::String(s) => {
            if let Some(p) = problem(s) {
                out.push(say(at, p));
            }
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                let len = at.len();
                at.push_str(&format!("[{i}]"));
                walk(item, at, out);
                at.truncate(len);
            }
        }
        Value::Object(map) => {
            for (k, item) in map {
                let len = at.len();
                push_key(at, k);
                if let Some(p) = problem(k)
                    && out.len() < MAX_PROBLEMS
                {
                    out.push(format!("the field name {}", say(at, p)));
                }
                walk(item, at, out);
                at.truncate(len);
            }
        }
        _ => {}
    }
}

/// `cards[3].front`; odd field names quoted, cut short and cleaned, as the schema check shows
/// them (`data_schema`'s path text, kept in step by the shared fixtures).
fn push_key(out: &mut String, k: &str) {
    let simple = k.len() <= 64
        && k.chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if simple {
        if !out.is_empty() {
            out.push('.');
        }
        out.push_str(k);
    } else {
        let clean: String = k
            .chars()
            .take(40)
            .map(|c| if hidden_char(c) || c == '"' { '?' } else { c })
            .collect();
        out.push_str(&format!("[\"{clean}\"]"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_string_and_field_name_is_checked() {
        let marks = format!("a{}", "\u{301}".repeat(5));
        let data = json!({
            "title": "Fine, caf\u{e9} and e\u{301}\u{302}\u{303}\u{304}",
            "cards": [
                {"front": "ok", "back": marks},
                {"front": "tag\u{E0041}char"},
                {"front": "filler\u{3164}"}
            ],
            "we\u{200b}ird": 1
        });
        assert_eq!(
            text_problems(&data),
            [
                "cards[0].back has more than four combining marks in a row, which isn't allowed",
                "cards[1].front contains a line break, tab or invisible character, which isn't allowed",
                "cards[2].front contains a line break, tab or invisible character, which isn't allowed",
                "the field name [\"we?ird\"] contains a line break, tab or invisible character, which isn't allowed",
            ]
        );
        assert!(text_problems(&json!({"a": ["plain", 1, null, true]})).is_empty());
        assert_eq!(
            text_problems(&json!("x\u{2028}y")),
            ["the data contains a line break, tab or invisible character, which isn't allowed"]
        );
    }

    #[test]
    fn schema_and_text_problems_are_merged_without_repeats() {
        let schema = json!({
            "type": "object",
            "properties": {"t": {"type": "string", "pattern": "^[^\\u0000-\\u001F\\u200B]*$"}}
        });
        // The schema's pattern and the walk word a hidden character the same way: one line.
        assert_eq!(
            pack_data_problems(&schema, &json!({"t": "a\u{200b}b"})),
            ["t contains a line break, tab or invisible character, which isn't allowed"]
        );
        // The walk alone finds what the pattern can't express.
        let marks = format!("a{}", "\u{300}".repeat(6));
        assert_eq!(
            pack_data_problems(&schema, &json!({"t": marks})),
            ["t has more than four combining marks in a row, which isn't allowed"]
        );
        assert!(pack_data_problems(&schema, &json!({"t": "fine"})).is_empty());
    }

    #[test]
    fn reports_are_capped() {
        let many: Vec<Value> = (0..100).map(|_| json!("x\u{0}")).collect();
        assert_eq!(text_problems(&Value::Array(many)).len(), MAX_PROBLEMS);
    }
}
