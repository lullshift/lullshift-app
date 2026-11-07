//! The provider's stated reason for a refused request (ADR-020, decision 36 amendment, "provider
//! error excerpts").
//!
//! A 4xx/5xx body is read with a strict cap and a deadline, and only the standard JSON error
//! shapes are understood:
//! - OpenAI and compatible servers: `{"error":{"message","type","code","param"}}`, and the bare
//!   `{"error":"text"}` some local servers send;
//! - Anthropic: `{"type":"error","error":{"type","message"}}`.
//!
//! Anything else (HTML, plain text, arrays) is ignored. The one sentence kept is sanitised: control,
//! bidi and invisible characters dropped, whitespace collapsed, the key and key-like words hidden,
//! quotes softened, length capped. If it quotes anything we sent, it is dropped whole. The body
//! is never logged and never stored; only the sanitised excerpt reaches the UI.

use std::io::Read;
use std::time::{Duration, Instant};

use crate::provider::clean_chunk;
use crate::scrub::{scrub, scrub_key_like};

/// Most error-body bytes read. A larger body is ignored entirely.
pub const ERROR_BODY_CAP: usize = 16 * 1024;
/// Longest the body read may take once the status line has arrived.
pub const ERROR_BODY_TIME: Duration = Duration::from_secs(5);
/// Longest excerpt shown (characters).
pub const MAX_REASON_CHARS: usize = 240;
/// The `error_message` column holds at most this many characters.
pub const MAX_MESSAGE_CHARS: usize = 500;

/// The part of a provider error body that is kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reason {
    /// Sanitised, at most [`MAX_REASON_CHARS`] characters, no double quotes.
    pub message: String,
    /// The offending request parameter the provider named (OpenAI), lowercased and cleaned.
    pub param: Option<String>,
}

/// Reads at most [`ERROR_BODY_CAP`] bytes. `None` when the body is larger, the read fails,
/// the deadline passes or `abandoned` says to stop. The deadline is checked between reads.
pub fn read_capped<R: Read>(r: &mut R, mut abandoned: impl FnMut() -> bool) -> Option<Vec<u8>> {
    let started = Instant::now();
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        if abandoned() || started.elapsed() > ERROR_BODY_TIME {
            return None;
        }
        match r.read(&mut chunk) {
            Ok(0) => return Some(buf),
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.len() > ERROR_BODY_CAP {
                    return None;
                }
            }
            Err(_) => return None,
        }
    }
}

fn str_field<'a>(v: &'a serde_json::Value, k: &str) -> Option<&'a str> {
    v.get(k)
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.trim().is_empty())
}

/// Cuts `s` to `max` characters, ending in an ellipsis when it was cut.
pub(crate) fn cap_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('\u{2026}');
    out
}

fn sanitise(raw: &str, key: &str) -> String {
    let cleaned = clean_chunk(raw).replace('"', "'");
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let out = scrub_key_like(&scrub(&collapsed, key));
    // The key is hidden in the whole text first, then the result is cut.
    cap_chars(&out, MAX_REASON_CHARS)
}

/// Sanitises one provider message that arrived inside a stream (no body to parse). `None` when
/// nothing is left.
pub fn excerpt(raw: &str, key: &str) -> Option<Reason> {
    let message = sanitise(raw, key);
    (!message.is_empty()).then_some(Reason {
        message,
        param: None,
    })
}

/// Parses an error body. `echoes` are strings we sent (message text, lookup answers): a reason that
/// quotes any of them is dropped.
pub fn parse(body: &[u8], key: &str, echoes: &[String]) -> Option<Reason> {
    let v: serde_json::Value = serde_json::from_slice(body).ok()?;
    let err = v.get("error")?;
    let (raw, param) = match err {
        serde_json::Value::String(s) => (s.as_str(), None),
        serde_json::Value::Object(_) => {
            let msg = str_field(err, "message")
                .or_else(|| str_field(err, "code"))
                .or_else(|| str_field(err, "type"))?;
            (msg, str_field(err, "param"))
        }
        _ => return None,
    };
    if echoes
        .iter()
        .any(|e| e.chars().count() >= 8 && raw.contains(e.as_str()))
    {
        return None;
    }
    let message = sanitise(raw, key);
    if message.is_empty() {
        return None;
    }
    let param = param.map(|p| {
        clean_chunk(p)
            .trim()
            .chars()
            .take(64)
            .collect::<String>()
            .to_ascii_lowercase()
    });
    Some(Reason { message, param })
}

/// Appends `<label> said: "<reason>"` to `base`, keeping the whole under the column limit.
pub fn append_reason(base: &str, label: &str, reason: Option<&Reason>) -> String {
    let Some(r) = reason else {
        return base.to_owned();
    };
    let lead = format!("{base} {label} said: \"");
    let room = MAX_MESSAGE_CHARS.saturating_sub(lead.chars().count() + 1);
    if room < 8 {
        return base.to_owned();
    }
    format!("{lead}{}\"", cap_chars(&r.message, room))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "sk-test-SUPERSECRETKEY-0123456789";

    fn p(body: &str) -> Option<Reason> {
        parse(body.as_bytes(), KEY, &[])
    }

    #[test]
    fn openai_shape_gives_message_and_param() {
        let r = p(r#"{"error":{"message":"Unsupported value: 'stream' does not support true.","type":"invalid_request_error","param":"Stream","code":"unsupported_value"}}"#).unwrap();
        assert_eq!(
            r.message,
            "Unsupported value: 'stream' does not support true."
        );
        assert_eq!(r.param.as_deref(), Some("stream"));
    }

    #[test]
    fn anthropic_shape_gives_message() {
        let r = p(
            r#"{"type":"error","error":{"type":"invalid_request_error","message":"model: Field required"}}"#,
        )
        .unwrap();
        assert_eq!(r.message, "model: Field required");
        assert_eq!(r.param, None);
    }

    #[test]
    fn bare_string_error_and_code_fallback() {
        assert_eq!(
            p(r#"{"error":"model 'x' not found"}"#).unwrap().message,
            "model 'x' not found"
        );
        assert_eq!(
            p(r#"{"error":{"message":"  ","code":"model_not_found"}}"#)
                .unwrap()
                .message,
            "model_not_found"
        );
    }

    #[test]
    fn non_json_and_other_shapes_are_ignored() {
        for b in [
            "<html><body>Bad gateway</body></html>",
            "plain text",
            "[]",
            "{}",
            r#"{"error":null}"#,
            r#"{"error":{"message":42}}"#,
            r#"{"message":"top level is not a standard shape"}"#,
            "",
        ] {
            assert_eq!(p(b), None, "{b}");
        }
    }

    #[test]
    fn hostile_characters_whitespace_quotes_and_length_are_handled() {
        let long = "x".repeat(1000);
        let body = serde_json::json!({
            "error": { "message": format!("a\u{202e}b\u{7}\n\n  c\u{200b}d \"quoted\"\t{long}") }
        })
        .to_string();
        let r = p(&body).unwrap();
        assert!(r.message.starts_with("ab cd 'quoted' xxx"), "{}", r.message);
        assert!(!r.message.contains('"') && !r.message.contains('\u{202e}'));
        assert_eq!(r.message.chars().count(), MAX_REASON_CHARS);
        assert!(r.message.ends_with('\u{2026}'));
    }

    #[test]
    fn the_key_never_survives_even_split_by_invisible_characters() {
        let split = KEY.replace("SUPER", "SU\u{200b}PER");
        for msg in [
            format!("bad key {KEY} here"),
            format!("bad key {split} here"),
        ] {
            let body = serde_json::json!({ "error": { "message": msg } }).to_string();
            let r = p(&body).unwrap();
            assert!(!r.message.contains("SUPERSECRET"), "{}", r.message);
            assert!(r.message.contains("[hidden]"));
        }
        // A masked key echoed by OpenAI.
        let r = p(r#"{"error":{"message":"Incorrect API key provided: sk-proj-********wxyz."}}"#)
            .unwrap();
        assert!(!r.message.contains("sk-proj"), "{}", r.message);
    }

    #[test]
    fn a_reason_that_quotes_what_we_sent_is_dropped() {
        let sent = vec![
            "my private message about the build".to_owned(),
            "hi".to_owned(),
        ];
        let body = r#"{"error":{"message":"Invalid input: my private message about the build"}}"#;
        assert_eq!(parse(body.as_bytes(), KEY, &sent), None);
        // Short sent strings are not treated as echoes.
        let ok = r#"{"error":{"message":"hi is not valid"}}"#;
        assert!(parse(ok.as_bytes(), KEY, &sent).is_some());
    }

    #[test]
    fn read_capped_refuses_oversized_bodies_and_honours_abandon() {
        let big = vec![b'a'; ERROR_BODY_CAP + 1];
        assert_eq!(read_capped(&mut &big[..], || false), None);
        let fit = vec![b'a'; ERROR_BODY_CAP];
        assert_eq!(
            read_capped(&mut &fit[..], || false).map(|v| v.len()),
            Some(ERROR_BODY_CAP)
        );
        assert_eq!(read_capped(&mut &b"{}"[..], || true), None);
    }

    #[test]
    fn appended_message_fits_the_column() {
        let r = Reason {
            message: "y".repeat(MAX_REASON_CHARS),
            param: None,
        };
        let base = "x".repeat(300);
        let m = append_reason(&base, "The provider", Some(&r));
        assert!(m.chars().count() <= MAX_MESSAGE_CHARS);
        assert!(m.ends_with("\u{2026}\"") || m.ends_with('"'));
        assert_eq!(append_reason("base", "OpenAI", None), "base");
        assert_eq!(
            append_reason(
                "base.",
                "OpenAI",
                Some(&Reason {
                    message: "no".into(),
                    param: None
                })
            ),
            "base. OpenAI said: \"no\""
        );
    }
}
