//! Agent event v1. Mirrors `reference/AGENT_EVENT_V1.schema.json` (ADR-006).
//!
//! Input is untrusted (it will come from agent hooks). Parsing goes through a raw wire struct,
//! then every schema rule is checked explicitly so Rust and the TypeScript validator agree.
//! Errors never contain input content: they carry only a fixed reason and a position.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};
use std::fmt;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

pub const SCHEMA_VERSION: u8 = 1;
const MAX_PROVIDER_LEN: usize = 80;
const MAX_ID_LEN: usize = 256;
const MAX_CAPABILITIES: usize = 32;
const MAX_METADATA_KEYS: usize = 20;
const MAX_METADATA_STRING_LEN: usize = 256;
const MAX_SLUG_LEN: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    SessionStarted,
    Working,
    NeedsAttention,
    TurnEnded,
    Completed,
    Failed,
    Cancelled,
    Heartbeat,
}

impl EventType {
    fn from_wire(s: &str) -> Option<Self> {
        Some(match s {
            "session_started" => Self::SessionStarted,
            "working" => Self::Working,
            "needs_attention" => Self::NeedsAttention,
            "turn_ended" => Self::TurnEnded,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            "heartbeat" => Self::Heartbeat,
            _ => return None,
        })
    }

    /// Wire name, e.g. `needs_attention`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SessionStarted => "session_started",
            Self::Working => "working",
            Self::NeedsAttention => "needs_attention",
            Self::TurnEnded => "turn_ended",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Heartbeat => "heartbeat",
        }
    }
}

/// A validated agent event.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentEvent {
    pub schema_version: u8,
    pub event_id: Uuid,
    pub provider: String,
    pub provider_session_id: String,
    pub event_type: EventType,
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp_utc: OffsetDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Map<String, Value>>,
}

/// Raw JSON shape: structure and unknown fields only; values are checked in [`parse_event`].
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireEvent {
    schema_version: Number,
    event_id: String,
    provider: String,
    provider_session_id: String,
    event_type: String,
    timestamp_utc: String,
    #[serde(default, deserialize_with = "present")]
    correlation_id: Option<String>,
    #[serde(default, deserialize_with = "present")]
    capabilities: Option<Vec<String>>,
    #[serde(default, deserialize_with = "present")]
    metadata: Option<Map<String, Value>>,
}

/// Optional field that, when present, must hold a value: explicit `null` is rejected, as in
/// the schema. A missing field still becomes `None` through `#[serde(default)]`.
fn present<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(d).map(Some)
}

/// Why an event was rejected. Deliberately holds no input content, so it is safe to log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventError {
    /// Not valid JSON, wrong JSON types, a missing or unknown field.
    Malformed {
        category: &'static str,
        line: usize,
        column: usize,
    },
    /// Well-formed JSON that breaks a schema rule.
    Invalid(&'static str),
}

impl fmt::Display for EventError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventError::Malformed {
                category,
                line,
                column,
            } => write!(
                f,
                "invalid agent event: malformed ({category}) at line {line} column {column}"
            ),
            EventError::Invalid(why) => write!(f, "invalid agent event: {why}"),
        }
    }
}

impl std::error::Error for EventError {}

fn malformed(e: &serde_json::Error) -> EventError {
    use serde_json::error::Category;
    EventError::Malformed {
        category: match e.classify() {
            Category::Io => "io",
            Category::Syntax => "syntax",
            Category::Data => "data",
            Category::Eof => "eof",
        },
        line: e.line(),
        column: e.column(),
    }
}

/// Lowercase hyphenated UUID only (matches the schema `pattern`).
fn parse_event_id(s: &str) -> Option<Uuid> {
    let b = s.as_bytes();
    let shape_ok = b.len() == 36
        && b.iter().enumerate().all(|(i, &c)| match i {
            8 | 13 | 18 | 23 => c == b'-',
            _ => c.is_ascii_digit() || (b'a'..=b'f').contains(&c),
        });
    if shape_ok {
        Uuid::try_parse(s).ok()
    } else {
        None
    }
}

/// Strict RFC 3339 `date-time` (matches the schema `pattern`): `T` separator, seconds 00-59,
/// 1-9 fractional digits, and `Z` or `+HH:MM` / `-HH:MM`.
fn parse_timestamp(s: &str) -> Option<OffsetDateTime> {
    let b = s.as_bytes();
    let digits =
        |r: std::ops::Range<usize>| b.get(r).is_some_and(|x| x.iter().all(u8::is_ascii_digit));
    let two = |at: usize| -> Option<u8> {
        if digits(at..at + 2) {
            Some((b[at] - b'0') * 10 + (b[at + 1] - b'0'))
        } else {
            None
        }
    };
    if b.len() < 20
        || !digits(0..4)
        || b[4] != b'-'
        || two(5).is_none()
        || b[7] != b'-'
        || two(8).is_none()
        || b[10] != b'T'
        || two(11)? > 23
        || b[13] != b':'
        || two(14)? > 59
        || b[16] != b':'
        || two(17)? > 59
    {
        return None;
    }
    let mut i = 19;
    if b.get(i) == Some(&b'.') {
        let start = i + 1;
        i = start;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if !(1..=9).contains(&(i - start)) {
            return None;
        }
    }
    let zone_ok = match b.get(i) {
        Some(b'Z') => i + 1 == b.len(),
        Some(b'+' | b'-') => {
            i + 6 == b.len()
                && two(i + 1).is_some_and(|h| h <= 23)
                && b[i + 3] == b':'
                && two(i + 4).is_some_and(|m| m <= 59)
        }
        _ => false,
    };
    if !zone_ok {
        return None;
    }
    OffsetDateTime::parse(s, &Rfc3339).ok()
}

/// Characters that can hide or reorder displayed text (ADR-008): C0/C1 controls and DEL, the
/// Arabic letter mark, zero-width and directional marks (U+200B-200F), line/paragraph
/// separators, bidi embeddings/overrides/isolates, invisible operators (U+2060-2064), the BOM
/// and interlinear annotation controls. Must match the schema's opaque-text pattern.
pub fn is_forbidden_char(c: char) -> bool {
    matches!(
        c,
        '\u{0}'..='\u{1F}'
            | '\u{7F}'..='\u{9F}'
            | '\u{61C}'
            | '\u{200B}'..='\u{200F}'
            | '\u{2028}'..='\u{202E}'
            | '\u{2060}'..='\u{2069}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
    )
}

/// 1-256 characters, none forbidden. Matches the schema's opaque-id pattern.
fn is_opaque_id(s: &str) -> bool {
    let n = s.chars().count();
    (1..=MAX_ID_LEN).contains(&n) && !s.chars().any(is_forbidden_char)
}

/// `^[first][first|0-9|._-]{0,max-1}$` with `first` deciding the allowed leading letters.
fn is_slug(s: &str, max: usize, first: impl Fn(char) -> bool) -> bool {
    let mut chars = s.chars();
    let Some(c0) = chars.next() else { return false };
    first(c0)
        && s.chars().count() <= max
        && chars
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
}

/// `^[A-Za-z][A-Za-z0-9_.-]{0,max-1}$` (metadata keys).
fn is_slug_any_case(s: &str, max: usize) -> bool {
    let mut chars = s.chars();
    let Some(c0) = chars.next() else { return false };
    c0.is_ascii_alphabetic()
        && s.chars().count() <= max
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Parses and validates one agent event from JSON. Errors never echo input content.
pub fn parse_event(json: &str) -> Result<AgentEvent, EventError> {
    let w: WireEvent = serde_json::from_str(json).map_err(|e| malformed(&e))?;

    // JSON numbers 1 and 1.0 are the same value to JSON Schema; accept both.
    let version_ok = w.schema_version.as_u64() == Some(1) || w.schema_version.as_f64() == Some(1.0);
    if !version_ok {
        return Err(EventError::Invalid("schemaVersion must be 1"));
    }
    let event_id = parse_event_id(&w.event_id).ok_or(EventError::Invalid(
        "eventId must be a lowercase hyphenated UUID",
    ))?;
    if !is_slug(&w.provider, MAX_PROVIDER_LEN, |c| {
        c.is_ascii_lowercase() || c.is_ascii_digit()
    }) {
        return Err(EventError::Invalid(
            "provider must be a lowercase slug of 1-80 characters",
        ));
    }
    if !is_opaque_id(&w.provider_session_id) {
        return Err(EventError::Invalid(
            "providerSessionId must be 1-256 characters without control or bidi characters",
        ));
    }
    let event_type =
        EventType::from_wire(&w.event_type).ok_or(EventError::Invalid("unknown eventType"))?;
    let timestamp_utc = parse_timestamp(&w.timestamp_utc).ok_or(EventError::Invalid(
        "timestampUtc must be a strict RFC 3339 date-time",
    ))?;
    if w.correlation_id
        .as_deref()
        .is_some_and(|c| !is_opaque_id(c))
    {
        return Err(EventError::Invalid(
            "correlationId must be 1-256 characters without control or bidi characters",
        ));
    }
    if let Some(caps) = &w.capabilities {
        if caps.len() > MAX_CAPABILITIES {
            return Err(EventError::Invalid("capabilities exceeds 32 items"));
        }
        if !caps
            .iter()
            .all(|c| is_slug(c, MAX_SLUG_LEN, |ch| ch.is_ascii_lowercase()))
        {
            return Err(EventError::Invalid("capabilities must be lowercase slugs"));
        }
        let mut seen = std::collections::HashSet::new();
        if !caps.iter().all(|c| seen.insert(c)) {
            return Err(EventError::Invalid("capabilities must be unique"));
        }
    }
    if let Some(meta) = &w.metadata {
        if meta.len() > MAX_METADATA_KEYS {
            return Err(EventError::Invalid("metadata exceeds 20 properties"));
        }
        if !meta.keys().all(|k| is_slug_any_case(k, MAX_SLUG_LEN)) {
            return Err(EventError::Invalid("metadata keys must be identifiers"));
        }
        let scalar_ok = |v: &Value| match v {
            Value::Null | Value::Bool(_) | Value::Number(_) => true,
            Value::String(s) => {
                s.chars().count() <= MAX_METADATA_STRING_LEN && !s.chars().any(is_forbidden_char)
            }
            Value::Array(_) | Value::Object(_) => false,
        };
        if !meta.values().all(scalar_ok) {
            return Err(EventError::Invalid(
                "metadata values must be short scalars without control characters",
            ));
        }
    }

    Ok(AgentEvent {
        schema_version: SCHEMA_VERSION,
        event_id,
        provider: w.provider,
        provider_session_id: w.provider_session_id,
        event_type,
        timestamp_utc,
        correlation_id: w.correlation_id,
        capabilities: w.capabilities,
        metadata: w.metadata,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = r#"{"schemaVersion":1,"eventId":"6f1c2a52-8a63-4f53-9a0e-1d1f6b7e2c11","provider":"claude-code","providerSessionId":"s1","eventType":"needs_attention","timestampUtc":"2026-11-05T10:15:30Z"}"#;

    #[test]
    fn parses_minimal_event() {
        let e = parse_event(BASE).expect("valid");
        assert_eq!(e.event_type, EventType::NeedsAttention);
        assert_eq!(e.provider, "claude-code");
    }

    #[test]
    fn round_trips_through_json() {
        let e = parse_event(BASE).expect("valid");
        let again = parse_event(&serde_json::to_string(&e).expect("serialize")).expect("valid");
        assert_eq!(e, again);
    }

    #[test]
    fn rejects_provider_over_limit() {
        let json = BASE.replace("claude-code", &"x".repeat(81));
        assert!(matches!(parse_event(&json), Err(EventError::Invalid(_))));
    }

    #[test]
    fn accepts_schema_version_written_as_float() {
        assert!(
            parse_event(&BASE.replace(r#""schemaVersion":1"#, r#""schemaVersion":1.0"#)).is_ok()
        );
    }

    #[test]
    fn errors_never_echo_input() {
        const SENTINEL: &str = "SENTINEL_PROMPT_TEXT";
        let attempts = [
            BASE.replace(
                r#""schemaVersion":1"#,
                &format!(r#""schemaVersion":"{SENTINEL}""#),
            ),
            BASE.replace("needs_attention", SENTINEL),
            BASE.replace(r#""provider":"#, &format!(r#""{SENTINEL}":1,"provider":"#)),
            BASE.replace("6f1c2a52-8a63-4f53-9a0e-1d1f6b7e2c11", SENTINEL),
            BASE.replace("2026-11-05T10:15:30Z", SENTINEL),
            format!("{{{SENTINEL}"),
        ];
        for json in attempts {
            let err = parse_event(&json).expect_err("must be rejected");
            assert!(!err.to_string().contains(SENTINEL), "Display leaked: {err}");
            assert!(
                !format!("{err:?}").contains(SENTINEL),
                "Debug leaked: {err:?}"
            );
        }
    }

    #[test]
    fn strict_uuid_and_timestamp_forms() {
        for bad_id in [
            "6f1c2a528a634f539a0e1d1f6b7e2c11",
            "{6f1c2a52-8a63-4f53-9a0e-1d1f6b7e2c11}",
            "6F1C2A52-8A63-4F53-9A0E-1D1F6B7E2C11",
            "urn:uuid:6f1c2a52-8a63-4f53-9a0e-1d1f6b7e2c11",
        ] {
            assert!(parse_event_id(bad_id).is_none(), "{bad_id}");
        }
        for bad_ts in [
            "2026-11-05T10:15:30+0500",
            "2026-11-05T23:59:60Z",
            "2026-11-05 10:15:30Z",
            "2026-11-05t10:15:30z",
            "2026-11-05T10:15:30.1234567890Z",
            "2026-13-05T10:15:30Z",
        ] {
            assert!(parse_timestamp(bad_ts).is_none(), "{bad_ts}");
        }
        for good_ts in ["2026-11-05T10:15:30Z", "2026-11-05T10:20:01.250+03:00"] {
            assert!(parse_timestamp(good_ts).is_some(), "{good_ts}");
        }
    }

    #[test]
    fn input_limit_helpers_match_schema_boundaries() {
        let provider = |s: &str| {
            is_slug(s, MAX_PROVIDER_LEN, |c| {
                c.is_ascii_lowercase() || c.is_ascii_digit()
            })
        };
        assert!(provider("claude-code") && provider("1agent") && provider(&"a".repeat(80)));
        for bad in ["", "Claude", "a b", "-x", &"a".repeat(81)] {
            assert!(!provider(bad), "{bad:?}");
        }
        assert!(is_opaque_id("s") && is_opaque_id(&"s".repeat(256)) && is_opaque_id("sessão-日本"));
        for bad in [
            "",
            "a\u{0}b",
            "a\u{85}b",
            "a\u{202E}b",
            "a\u{2066}b",
            &"s".repeat(257),
        ] {
            assert!(!is_opaque_id(bad), "{bad:?}");
        }
        assert!(is_slug_any_case("Tool.name_2", MAX_SLUG_LEN));
        for bad in ["", "1st", "a b", &"k".repeat(65)] {
            assert!(!is_slug_any_case(bad, MAX_SLUG_LEN), "{bad:?}");
        }
    }

    #[test]
    fn metadata_and_capability_rules() {
        let with = |extra: &str| BASE.replace(r#""eventType""#, &format!(r#"{extra},"eventType""#));
        assert!(parse_event(&with(r#""metadata":{"a":1,"b":true,"c":null,"d":"x"}"#)).is_ok());
        assert!(parse_event(&with(r#""capabilities":["lifecycle","needs.attention"]"#)).is_ok());
        for bad in [
            r#""metadata":{"a":{"b":1}}"#,
            r#""metadata":{"a":[1]}"#,
            r#""metadata":{"a":"x\ny"}"#,
            r#""capabilities":["a","a"]"#,
            r#""capabilities":["A"]"#,
            r#""correlationId":"""#,
        ] {
            assert!(
                matches!(parse_event(&with(bad)), Err(EventError::Invalid(_))),
                "{bad}"
            );
        }
    }
}
