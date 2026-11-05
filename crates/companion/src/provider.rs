//! Provider request and response mapping (ADR-020).
//!
//! Shapes follow the vendors' public API references:
//! - Anthropic Messages API, `POST {base}/v1/messages`, headers `x-api-key` and
//!   `anthropic-version: 2023-06-01`, body `model`, `max_tokens`, `system`, `messages`;
//!   reply `content[]` text blocks and `usage.input_tokens` / `usage.output_tokens`.
//! - OpenAI Chat Completions, `POST {base}/chat/completions` with `Authorization: Bearer`,
//!   body `model`, `messages`, and `max_completion_tokens` (OpenAI) or `max_tokens`
//!   (compatible servers); reply `choices[0].message.content` and `usage.prompt_tokens` /
//!   `usage.completion_tokens`.
//!
//! Requests never contain `tools`, `functions`, `tool_choice` or any other agent capability.

use agent_core::text::is_invisible;
use serde_json::{Value, json};
use zeroize::Zeroizing;

use crate::error::{CompanionError, ErrorKind};

/// The only system text ever sent. Fixed, neutral, and claims no abilities.
pub const SYSTEM_PROMPT: &str = "You are a chat companion. You can only reply with text in this conversation; you cannot run code, use tools, or take actions on the user's computer or in any other system.";

pub const ANTHROPIC_VERSION: &str = "2023-06-01";
pub const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com";
pub const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";

/// Output cap per reply.
pub const MAX_OUTPUT_TOKENS: u32 = 1024;
/// Output cap for the connection test.
pub const TEST_OUTPUT_TOKENS: u32 = 16;
/// Replies longer than this are cut (characters).
pub const MAX_REPLY_CHARS: usize = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    Anthropic,
    OpenAi,
    OpenAiCompatible,
}

impl ProviderKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "anthropic" => Some(Self::Anthropic),
            "openai" => Some(Self::OpenAi),
            "openaiCompatible" => Some(Self::OpenAiCompatible),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::OpenAi => "openai",
            Self::OpenAiCompatible => "openaiCompatible",
        }
    }

    /// The fixed base URL, or `None` where the user enters it.
    pub fn fixed_base_url(self) -> Option<&'static str> {
        match self {
            Self::Anthropic => Some(ANTHROPIC_BASE_URL),
            Self::OpenAi => Some(OPENAI_BASE_URL),
            Self::OpenAiCompatible => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

impl Role {
    fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct ProviderSpec {
    pub kind: ProviderKind,
    /// Already checked with `url_check::check_base_url`, without a trailing slash.
    pub base_url: String,
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub text: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

/// A request ready to send. `headers` holds the auth header, which is the only place the key
/// goes.
pub struct BuiltRequest {
    pub url: String,
    /// Header values are zeroised on drop (best effort; the HTTP library keeps its own copy).
    pub headers: Vec<(&'static str, Zeroizing<String>)>,
    pub body: Vec<u8>,
}

// Manual `Debug`: the headers hold the key.
impl std::fmt::Debug for BuiltRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BuiltRequest { .. }")
    }
}

/// Merges consecutive same-role messages and drops leading assistant messages, which both
/// APIs may refuse. Empty content is dropped.
pub fn normalise(messages: &[ChatMessage]) -> Vec<ChatMessage> {
    let mut out: Vec<ChatMessage> = Vec::new();
    for m in messages {
        if m.content.trim().is_empty() {
            continue;
        }
        match out.last_mut() {
            Some(last) if last.role == m.role => {
                last.content.push_str("\n\n");
                last.content.push_str(&m.content);
            }
            None if m.role == Role::Assistant => {}
            _ => out.push(m.clone()),
        }
    }
    out
}

pub fn build_request(
    spec: &ProviderSpec,
    key: &str,
    messages: &[ChatMessage],
    max_output_tokens: u32,
) -> BuiltRequest {
    let msgs: Vec<Value> = normalise(messages)
        .iter()
        .map(|m| json!({ "role": m.role.as_str(), "content": m.content }))
        .collect();
    let base = spec.base_url.trim_end_matches('/');
    match spec.kind {
        ProviderKind::Anthropic => BuiltRequest {
            url: format!("{base}/v1/messages"),
            headers: vec![
                ("x-api-key", Zeroizing::new(key.to_owned())),
                (
                    "anthropic-version",
                    Zeroizing::new(ANTHROPIC_VERSION.to_owned()),
                ),
            ],
            body: json!({
                "model": spec.model,
                "max_tokens": max_output_tokens,
                "system": SYSTEM_PROMPT,
                "messages": msgs,
            })
            .to_string()
            .into_bytes(),
        },
        ProviderKind::OpenAi | ProviderKind::OpenAiCompatible => {
            let mut all = vec![json!({ "role": "system", "content": SYSTEM_PROMPT })];
            all.extend(msgs);
            let cap_field = if spec.kind == ProviderKind::OpenAi {
                "max_completion_tokens"
            } else {
                "max_tokens"
            };
            let mut body = json!({ "model": spec.model, "messages": all });
            body[cap_field] = json!(max_output_tokens);
            BuiltRequest {
                url: format!("{base}/chat/completions"),
                headers: vec![("authorization", Zeroizing::new(format!("Bearer {key}")))],
                body: body.to_string().into_bytes(),
            }
        }
    }
}

/// Drops control characters (except newline and tab) and every bidi or invisible formatting
/// character, so a reply cannot reorder or hide text. `agent_core::text::is_invisible` covers
/// U+202A-202E, U+2066-2069, U+200B-200F, U+061C, U+00AD, U+FEFF, U+2028 and U+2029, and more.
fn clean_reply(s: &str) -> String {
    s.chars()
        .filter(|c| (!c.is_control() || *c == '\n' || *c == '\t') && !is_invisible(*c))
        .take(MAX_REPLY_CHARS)
        .collect()
}

fn token(v: &Value) -> Option<i64> {
    v.as_i64().filter(|n| *n >= 0)
}

/// Rough count used only when a provider reports no usage, so the budget still moves.
pub(crate) fn estimate(chars: usize) -> i64 {
    i64::try_from(chars / 4 + 1).unwrap_or(i64::MAX)
}

/// Parses a 2xx body. `sent_chars` is the size of what was sent, for the usage estimate.
pub fn parse_reply(
    kind: ProviderKind,
    body: &[u8],
    sent_chars: usize,
) -> Result<Reply, CompanionError> {
    let v: Value = serde_json::from_slice(body).map_err(|_| CompanionError::invalid_response())?;
    let (text, input, output) = match kind {
        ProviderKind::Anthropic => {
            let blocks = v
                .get("content")
                .and_then(Value::as_array)
                .ok_or_else(CompanionError::invalid_response)?;
            let text: String = blocks
                .iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|b| b.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("");
            let u = v.get("usage");
            (
                text,
                u.and_then(|u| u.get("input_tokens")).and_then(token),
                u.and_then(|u| u.get("output_tokens")).and_then(token),
            )
        }
        ProviderKind::OpenAi | ProviderKind::OpenAiCompatible => {
            let text = v
                .pointer("/choices/0/message/content")
                .and_then(Value::as_str)
                .ok_or_else(CompanionError::invalid_response)?
                .to_owned();
            let u = v.get("usage");
            (
                text,
                u.and_then(|u| u.get("prompt_tokens")).and_then(token),
                u.and_then(|u| u.get("completion_tokens")).and_then(token),
            )
        }
    };
    let text = clean_reply(&text);
    if text.trim().is_empty() {
        return Err(CompanionError::invalid_response());
    }
    let output_tokens = output.unwrap_or_else(|| estimate(text.chars().count()));
    let input_tokens = input.unwrap_or_else(|| estimate(sent_chars));
    Ok(Reply {
        text,
        input_tokens,
        output_tokens,
    })
}

/// Maps a non-2xx status to a fixed error. Provider bodies and headers are never used, except
/// a numeric `retry-after` in seconds.
pub fn map_status(status: u16, retry_after: Option<&str>) -> CompanionError {
    match status {
        401 | 403 => CompanionError::key_rejected(),
        429 => {
            let secs = retry_after
                .and_then(|s| s.trim().parse::<u64>().ok())
                .filter(|s| *s <= 86_400);
            let msg = match secs {
                Some(s) => format!(
                    "The provider is rate limiting requests. Try again in about {s} second{}.",
                    if s == 1 { "" } else { "s" }
                ),
                None => "The provider is rate limiting requests. Try again in a little while."
                    .to_owned(),
            };
            CompanionError::new(ErrorKind::RateLimited, msg)
        }
        300..=399 => CompanionError::new(
            ErrorKind::ProviderError,
            "The provider answered with a redirect, which Lullshift doesn't follow. Check the provider address.",
        ),
        400 | 404 | 422 => CompanionError::new(
            ErrorKind::ProviderError,
            format!(
                "The provider didn't accept the request (HTTP {status}). Check the model name and the provider address."
            ),
        ),
        500..=599 => CompanionError::new(
            ErrorKind::ProviderError,
            format!("The provider had a problem (HTTP {status}). Try again later."),
        ),
        _ => CompanionError::new(
            ErrorKind::ProviderError,
            format!("The provider answered with an unexpected status (HTTP {status})."),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(kind: ProviderKind) -> ProviderSpec {
        ProviderSpec {
            kind,
            base_url: "https://x.example/v1".into(),
            model: "m".into(),
        }
    }

    fn user(s: &str) -> ChatMessage {
        ChatMessage {
            role: Role::User,
            content: s.into(),
        }
    }

    #[test]
    fn anthropic_request_shape() {
        let r = build_request(
            &spec(ProviderKind::Anthropic),
            "KEY12345",
            &[user("hi")],
            64,
        );
        assert_eq!(r.url, "https://x.example/v1/v1/messages");
        assert!(
            r.headers
                .iter()
                .any(|h| h.0 == "x-api-key" && h.1.as_str() == "KEY12345")
        );
        assert!(r.headers.iter().any(|h| h.0 == "anthropic-version"));
        let b: Value = serde_json::from_slice(&r.body).unwrap();
        assert_eq!(b["max_tokens"], 64);
        assert_eq!(b["system"], SYSTEM_PROMPT);
        assert_eq!(b["messages"][0]["content"], "hi");
        assert!(!String::from_utf8_lossy(&r.body).contains("KEY12345"));
    }

    #[test]
    fn openai_request_shapes_differ_only_in_cap_field() {
        let a = build_request(&spec(ProviderKind::OpenAi), "KEY12345", &[user("hi")], 64);
        let b: Value = serde_json::from_slice(&a.body).unwrap();
        assert_eq!(a.url, "https://x.example/v1/chat/completions");
        assert_eq!(a.headers.len(), 1);
        assert_eq!(a.headers[0].0, "authorization");
        assert_eq!(a.headers[0].1.as_str(), "Bearer KEY12345");
        assert_eq!(b["max_completion_tokens"], 64);
        assert!(b.get("max_tokens").is_none());
        assert_eq!(b["messages"][0]["role"], "system");
        let c = build_request(
            &spec(ProviderKind::OpenAiCompatible),
            "KEY12345",
            &[user("hi")],
            64,
        );
        let b: Value = serde_json::from_slice(&c.body).unwrap();
        assert_eq!(b["max_tokens"], 64);
    }

    #[test]
    fn no_tool_keys_anywhere_in_any_body() {
        for k in [
            ProviderKind::Anthropic,
            ProviderKind::OpenAi,
            ProviderKind::OpenAiCompatible,
        ] {
            let r = build_request(&spec(k), "KEY12345", &[user("hi")], 64);
            let v: Value = serde_json::from_slice(&r.body).unwrap();
            for banned in ["tools", "functions", "tool_choice", "function_call"] {
                assert!(v.get(banned).is_none(), "{banned}");
            }
        }
    }

    #[test]
    fn normalise_merges_and_drops_leading_assistant() {
        let m = [
            ChatMessage {
                role: Role::Assistant,
                content: "stray".into(),
            },
            user("a"),
            user("b"),
            ChatMessage {
                role: Role::Assistant,
                content: "  ".into(),
            },
        ];
        let n = normalise(&m);
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].content, "a\n\nb");
    }

    #[test]
    fn parses_replies_and_usage() {
        let a = br#"{"content":[{"type":"text","text":"he"},{"type":"tool_use","name":"x"},{"type":"text","text":"llo"}],"usage":{"input_tokens":7,"output_tokens":3}}"#;
        let r = parse_reply(ProviderKind::Anthropic, a, 10).unwrap();
        assert_eq!(
            (r.text.as_str(), r.input_tokens, r.output_tokens),
            ("hello", 7, 3)
        );
        let o = br#"{"choices":[{"message":{"content":"yo"}}],"usage":{"prompt_tokens":5,"completion_tokens":2,"total_tokens":7}}"#;
        let r = parse_reply(ProviderKind::OpenAi, o, 10).unwrap();
        assert_eq!(
            (r.text.as_str(), r.input_tokens, r.output_tokens),
            ("yo", 5, 2)
        );
    }

    #[test]
    fn missing_usage_is_estimated_not_zero() {
        let o = br#"{"choices":[{"message":{"content":"yo there"}}]}"#;
        let r = parse_reply(ProviderKind::OpenAiCompatible, o, 400).unwrap();
        assert!(r.input_tokens >= 100 && r.output_tokens >= 1);
    }

    #[test]
    fn malformed_bodies_are_invalid_responses() {
        for b in [
            &b"not json"[..],
            br#"{}"#,
            br#"{"choices":[]}"#,
            br#"{"choices":[{"message":{"content":null}}]}"#,
            br#"{"content":[{"type":"tool_use"}]}"#,
            br#"{"choices":[{"message":{"content":"   "}}]}"#,
        ] {
            for k in [ProviderKind::Anthropic, ProviderKind::OpenAi] {
                assert_eq!(
                    parse_reply(k, b, 1).unwrap_err().kind,
                    ErrorKind::InvalidResponse
                );
            }
        }
    }

    #[test]
    fn replies_are_cleaned_of_control_characters() {
        let o = "{\"choices\":[{\"message\":{\"content\":\"a\\u0007b\\nc\\u001b[31m\"}}]}";
        let r = parse_reply(ProviderKind::OpenAi, o.as_bytes(), 1).unwrap();
        assert_eq!(r.text, "ab\nc[31m");
    }

    #[test]
    fn statuses_map_to_fixed_messages() {
        assert_eq!(map_status(401, None).kind, ErrorKind::KeyRejected);
        assert_eq!(map_status(403, None).kind, ErrorKind::KeyRejected);
        let r = map_status(429, Some("30"));
        assert_eq!(r.kind, ErrorKind::RateLimited);
        assert!(r.message.contains("30 seconds"));
        assert!(
            !map_status(429, Some("Wed, 21 Oct 2026 07:28:00 GMT"))
                .message
                .contains("Oct")
        );
        assert_eq!(map_status(500, None).kind, ErrorKind::ProviderError);
        assert_eq!(map_status(302, None).kind, ErrorKind::ProviderError);
        assert_eq!(map_status(400, None).kind, ErrorKind::ProviderError);
    }

    #[test]
    fn replies_lose_bidi_and_invisible_characters() {
        let hostile = [
            '\u{202A}', '\u{202B}', '\u{202C}', '\u{202D}', '\u{202E}', '\u{2066}', '\u{2067}',
            '\u{2068}', '\u{2069}', '\u{200B}', '\u{200C}', '\u{200D}', '\u{200E}', '\u{200F}',
            '\u{061C}', '\u{00AD}', '\u{FEFF}', '\u{2028}', '\u{2029}',
        ];
        let mut input = String::from("safe\nline\ttab");
        for c in hostile {
            input.push(c);
            input.push('x');
        }
        let body = serde_json::json!({
            "choices": [{ "message": { "content": input } }]
        })
        .to_string();
        let r = parse_reply(ProviderKind::OpenAi, body.as_bytes(), 10).unwrap();
        assert_eq!(
            r.text,
            format!("safe\nline\ttab{}", "x".repeat(hostile.len()))
        );
        for c in hostile {
            assert!(!r.text.contains(c), "U+{:04X} survived", u32::from(c));
        }
    }
}
