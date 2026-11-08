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
pub fn excerpt(raw: &str, key: &str, echoes: &Echoes) -> Option<Reason> {
    if echoes.quoted_in(raw) {
        return None;
    }
    let message = sanitise(raw, key);
    (!message.is_empty()).then_some(Reason {
        message,
        param: None,
    })
}

/// Shortest shared run (characters, after cleaning and whitespace collapsing) that counts as the
/// provider quoting what we sent.
pub const ECHO_WINDOW: usize = 24;
/// A sent text shorter than this is not treated as an echo target at all.
const ECHO_MIN: usize = 8;
/// Most characters of sent text considered per text and in total (a long history is cut).
const ECHO_TEXT_CAP: usize = 200_000;
const ECHO_TOTAL_CAP: usize = 1_000_000;

/// Cleaned, whitespace-collapsed, lowercased text.
fn norm(s: &str) -> Vec<char> {
    clean_chunk(s)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .chars()
        .collect()
}

fn window_hash(w: &[char]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    w.hash(&mut h);
    h.finish()
}

/// What we sent, ready to check a provider message against. The check cleans and collapses both
/// sides first (so zero-width characters or odd spacing do not hide a quote) and drops the message
/// if it contains a whole sent text of 8 or more characters, or shares any run of
/// [`ECHO_WINDOW`] characters with one.
#[derive(Debug, Default)]
pub struct Echoes {
    texts: Vec<Vec<char>>,
}

impl Echoes {
    pub fn new<I: IntoIterator<Item = String>>(texts: I) -> Self {
        let mut total = 0usize;
        let mut out = Vec::new();
        for t in texts {
            let mut n = norm(&t);
            n.truncate(ECHO_TEXT_CAP);
            if n.len() < ECHO_MIN || total >= ECHO_TOTAL_CAP {
                continue;
            }
            total += n.len();
            out.push(n);
        }
        Self { texts: out }
    }

    pub fn quoted_in(&self, message: &str) -> bool {
        if self.texts.is_empty() {
            return false;
        }
        let msg = norm(message);
        let msg_str: String = msg.iter().collect();
        let mut windows: Option<std::collections::HashSet<u64>> = None;
        for t in &self.texts {
            let whole: String = t.iter().collect();
            if msg_str.contains(&whole) {
                return true;
            }
            if msg.len() >= ECHO_WINDOW && t.len() >= ECHO_WINDOW {
                let set = windows
                    .get_or_insert_with(|| msg.windows(ECHO_WINDOW).map(window_hash).collect());
                if t.windows(ECHO_WINDOW)
                    .any(|w| set.contains(&window_hash(w)))
                {
                    return true;
                }
            }
        }
        false
    }
}

/// Parses an error body. `echoes` is what we sent (message text, lookup answers): a reason that
/// quotes any of it is dropped.
pub fn parse(body: &[u8], key: &str, echoes: &Echoes) -> Option<Reason> {
    parse_with_facts(body, key, echoes).0
}

/// The structured fields of an error body, kept even when the text is dropped (by the echo check
/// or because it was empty): they are short fixed-shape identifiers used only to decide what to
/// retry, never shown.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ErrorFacts {
    pub param: Option<String>,
    pub code: Option<String>,
    pub err_type: Option<String>,
    /// The (raw) message mentions "reasoning".
    pub mentions_reasoning: bool,
    /// The (raw) message mentions "summary" (OpenAI: reasoning summaries may need verification).
    pub mentions_summary: bool,
    /// The (raw) message names an Anthropic thinking parameter (see [`names_thinking_param`]).
    pub names_thinking_param: bool,
}

impl ErrorFacts {
    /// The model refused the `reasoning` option: a `reasoning...` param, or an unsupported-parameter
    /// style 400 (code `unsupported_parameter`, or type `invalid_request_error`) whose message
    /// names reasoning.
    pub fn refuses_reasoning(&self) -> bool {
        self.param
            .as_deref()
            .is_some_and(|p| p.starts_with("reasoning"))
            || (self.mentions_reasoning
                && (self.code.as_deref() == Some("unsupported_parameter")
                    || self.err_type.as_deref() == Some("invalid_request_error")))
    }

    /// OpenAI refused the reasoning `summary` option: a `reasoning.summary` param, or a message
    /// that names the summary in an unsupported-parameter style 400.
    pub fn refuses_summary(&self) -> bool {
        self.param
            .as_deref()
            .is_some_and(|p| p.starts_with("reasoning.summary") || p == "summary")
            || (self.mentions_summary
                && (self.mentions_reasoning
                    || self.code.as_deref() == Some("unsupported_parameter")
                    || self.code.as_deref() == Some("unsupported_value")))
    }

    /// Anthropic refused the `thinking` (or `output_config.effort`) setting: the error must name
    /// the parameter, not just use the word.
    pub fn refuses_thinking(&self) -> bool {
        self.names_thinking_param
    }

    /// OpenAI refused `stream: true`.
    pub fn refuses_stream(&self) -> bool {
        self.param.as_deref() == Some("stream")
    }
}

/// The message names the reasoning option (OpenAI says "reasoning").
fn names_reasoning(m: &str) -> bool {
    m.to_ascii_lowercase().contains("reasoning")
}

/// The message names an Anthropic thinking parameter: a `thinking` field path or key
/// (`thinking.type`, `thinking: ...`, a quoted "thinking"), `budget_tokens`, `adaptive`, or
/// `output_config.effort`. The bare word "thinking" in prose does not count.
fn names_thinking_param(m: &str) -> bool {
    let l = m.to_ascii_lowercase();
    [
        "thinking.",
        "thinking:",
        "\"thinking\"",
        "`thinking`",
        "'thinking'",
        "budget_tokens",
        "adaptive",
        "output_config",
    ]
    .iter()
    .any(|k| l.contains(k))
}

fn short_id(s: Option<&str>) -> Option<String> {
    s.map(|p| {
        clean_chunk(p)
            .trim()
            .chars()
            .take(64)
            .collect::<String>()
            .to_ascii_lowercase()
    })
}

/// Like [`parse`], also returning the [`ErrorFacts`], which survive a dropped text.
pub fn parse_with_facts(body: &[u8], key: &str, echoes: &Echoes) -> (Option<Reason>, ErrorFacts) {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(body) else {
        return (None, ErrorFacts::default());
    };
    let Some(err) = v.get("error") else {
        return (None, ErrorFacts::default());
    };
    let (raw, facts) = match err {
        serde_json::Value::String(s) => (
            Some(s.as_str()),
            ErrorFacts {
                mentions_reasoning: names_reasoning(s),
                mentions_summary: s.to_ascii_lowercase().contains("summary"),
                names_thinking_param: names_thinking_param(s),
                ..ErrorFacts::default()
            },
        ),
        serde_json::Value::Object(_) => {
            let msg = str_field(err, "message")
                .or_else(|| str_field(err, "code"))
                .or_else(|| str_field(err, "type"));
            (
                msg,
                ErrorFacts {
                    param: short_id(str_field(err, "param")),
                    code: short_id(str_field(err, "code")),
                    err_type: short_id(str_field(err, "type")),
                    mentions_reasoning: str_field(err, "message").is_some_and(names_reasoning),
                    mentions_summary: str_field(err, "message")
                        .is_some_and(|m| m.to_ascii_lowercase().contains("summary")),
                    names_thinking_param: str_field(err, "message")
                        .is_some_and(names_thinking_param),
                },
            )
        }
        _ => return (None, ErrorFacts::default()),
    };
    let Some(raw) = raw else {
        return (None, facts);
    };
    if echoes.quoted_in(raw) {
        return (None, facts);
    }
    let message = sanitise(raw, key);
    if message.is_empty() {
        return (None, facts);
    }
    let param = facts.param.clone();
    (Some(Reason { message, param }), facts)
}

/// The reason as carried on an error, spoken by `kind`'s provider.
pub fn provider_reason(
    kind: crate::provider::ProviderKind,
    reason: Option<&Reason>,
) -> Option<crate::error::ProviderReason> {
    reason.map(|r| crate::error::ProviderReason {
        speaker: kind.speaker(),
        text: r.message.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "sk-test-SUPERSECRETKEY-0123456789";

    fn p(body: &str) -> Option<Reason> {
        parse(body.as_bytes(), KEY, &Echoes::default())
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
        let sent = Echoes::new(vec![
            "my private message about the build".to_owned(),
            "hi".to_owned(),
        ]);
        let body = r#"{"error":{"message":"Invalid input: my private message about the build"}}"#;
        assert_eq!(parse(body.as_bytes(), KEY, &sent), None);
        // Short sent strings are not treated as echoes.
        let ok = r#"{"error":{"message":"hi is not valid"}}"#;
        assert!(parse(ok.as_bytes(), KEY, &sent).is_some());
    }

    #[test]
    fn partial_quotes_and_zero_width_quotes_are_dropped() {
        let sent = Echoes::new(vec![
            "please summarise the quarterly numbers for the payroll migration".to_owned(),
        ]);
        // A 30-character piece of a longer sent text.
        let partial =
            r#"{"error":{"message":"bad value near 'summarise the quarterly numbers' here"}}"#;
        assert_eq!(parse(partial.as_bytes(), KEY, &sent), None);
        // The same piece with zero-width characters and extra spacing inserted.
        let hidden = "{\"error\":{\"message\":\"near sum\\u200bmarise  the\\n quarterly nu\\u2060mbers ok\"}}";
        assert_eq!(parse(hidden.as_bytes(), KEY, &sent), None);
        // Case differences do not hide it either.
        let upper = r#"{"error":{"message":"SUMMARISE THE QUARTERLY NUMBERS FOR"}}"#;
        assert_eq!(parse(upper.as_bytes(), KEY, &sent), None);
        // A shorter overlap (under 24 characters) is allowed through.
        let short = r#"{"error":{"message":"the quarterly numbers"}}"#;
        assert!(parse(short.as_bytes(), KEY, &sent).is_some());
        // Streamed messages go through the same check.
        assert_eq!(
            excerpt("failed on: summarise the quarterly numbers for", KEY, &sent),
            None
        );
        assert!(excerpt("nothing to see", KEY, &sent).is_some());
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
    fn the_reason_is_carried_apart_from_the_fixed_message() {
        use crate::provider::ProviderKind;
        let r = Reason {
            message: "no".into(),
            param: None,
        };
        let p = provider_reason(ProviderKind::OpenAi, Some(&r)).unwrap();
        assert_eq!((p.speaker, p.text.as_str()), ("OpenAI", "no"));
        assert_eq!(provider_reason(ProviderKind::Anthropic, None), None);
        assert_eq!(
            provider_reason(ProviderKind::OpenAiCompatible, Some(&r))
                .unwrap()
                .speaker,
            "The provider"
        );
    }
}
