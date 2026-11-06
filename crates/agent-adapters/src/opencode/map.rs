//! OpenCode plugin message → normalized agent event (ADR-022, docs/ADAPTER_CAPABILITIES.md).
//!
//! The installed plugin (`plugin.js`) does not forward OpenCode's events as they are. It builds a
//! small message from an allow-list: `{"v":1,"event":<kind>,"sessionId":<id>,"hint":<enum word>,
//! "prompt":<text, only with consent>}`. This module reads only those fields and does the
//! mapping, so it is tested here; anything else in the input is not declared and never held.
//!
//! `event` is one of [`EVENTS`]: the OpenCode 2.x event type for most, plus `turn_ended` and
//! `failed` which the plugin derives (a turn that ended after a failure is not reported twice).

use agent_core::EventType;
use agent_ipc::{EventContext, MAX_INTENT_CHARS};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::redact;

pub const PROVIDER: &str = "opencode";
/// "attention" is real but partial: `permission.asked` only. OpenCode 2.x has no idle-prompt or
/// question event in the plugin event stream.
pub const CAPABILITIES: [&str; 3] = ["lifecycle", "attention", "turn_end"];

/// The message kinds the plugin sends, with what each means.
pub const EVENTS: &[(&str, EventType)] = &[
    ("session.created", EventType::SessionStarted),
    ("session.execution.started", EventType::Working),
    ("session.tool.called", EventType::Working),
    ("session.text.started", EventType::Working),
    ("session.status", EventType::Working),
    ("session.inbox.enqueued", EventType::Working),
    ("permission.asked", EventType::NeedsAttention),
    ("permission.replied", EventType::Working),
    ("turn_ended", EventType::TurnEnded),
    ("failed", EventType::Failed),
    ("session.deleted", EventType::SessionEnded),
];

#[derive(Deserialize)]
struct Message {
    #[serde(default)]
    v: Option<u64>,
    event: String,
    #[serde(rename = "sessionId")]
    session_id: String,
    #[serde(default)]
    hint: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mapped {
    pub provider_session_id: String,
    pub event_type: EventType,
    pub metadata: Map<String, Value>,
}

/// Maps one plugin message. `None` for malformed input, an unknown version and kinds Lullshift
/// ignores. The prompt field is not read here (see [`map_context`]).
pub fn map_hook(stdin: &[u8]) -> Option<Mapped> {
    let m: Message = serde_json::from_slice(stdin).ok()?;
    if m.v != Some(1) {
        return None;
    }
    let event_type = EVENTS.iter().find(|(k, _)| *k == m.event)?.1;
    let mut metadata = Map::new();
    if let Some(hint) = m.hint.as_ref().and_then(Value::as_str)
        && is_enum_word(hint)
    {
        metadata.insert("hint".into(), hint.into());
    }
    metadata.insert("hook".into(), m.event.into());
    Some(Mapped {
        provider_session_id: m.session_id,
        event_type,
        metadata,
    })
}

/// The only field read in capture mode (ADR-013): the prompt of a user message
/// (`session.inbox.enqueued`, an item of type `user`; the plugin sends nothing else under that
/// kind). Call this only when the user opted in to capture for `opencode` (the marker file
/// exists); [`map_hook`] never reads the prompt.
#[derive(Deserialize)]
struct ContextInput {
    event: String,
    #[serde(default)]
    prompt: Option<Value>,
}

/// Task text for one plugin message, cleaned and limited: the user's prompt as the intent. Every
/// other kind returns `None` without looking at a prompt.
pub fn map_context(stdin: &[u8]) -> Option<EventContext> {
    let input: ContextInput = serde_json::from_slice(stdin).ok()?;
    if input.event != "session.inbox.enqueued" {
        return None;
    }
    let prompt = input.prompt.as_ref().and_then(Value::as_str)?;
    EventContext {
        intent: Some(redact::clean(prompt, MAX_INTENT_CHARS)),
        ask: None,
        title: None,
    }
    .normalized()
}

/// Enum-like values only: never free text.
fn is_enum_word(s: &str) -> bool {
    (1..=32).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(json: &str) -> Option<Mapped> {
        map_hook(json.as_bytes())
    }

    #[test]
    fn maps_every_kind_and_only_permission_asked_needs_attention() {
        for (kind, expect) in EVENTS {
            let m = map(&format!(
                r#"{{"v":1,"event":"{kind}","sessionId":"ses_1"}}"#
            ))
            .expect(kind);
            assert_eq!(m.event_type, *expect, "{kind}");
            assert_eq!(m.provider_session_id, "ses_1");
            assert_eq!(
                m.event_type == EventType::NeedsAttention,
                *kind == "permission.asked"
            );
            assert_eq!(m.metadata.get("hook").and_then(Value::as_str), Some(*kind));
        }
    }

    #[test]
    fn unknown_kinds_versions_and_garbage_are_ignored() {
        for body in [
            r#"{"v":1,"event":"session.text.delta","sessionId":"s"}"#,
            r#"{"v":2,"event":"session.created","sessionId":"s"}"#,
            r#"{"event":"session.created","sessionId":"s"}"#,
            r#"{"v":1,"event":"session.created"}"#,
            r#"[1]"#,
            "not json",
            "",
        ] {
            assert!(map(body).is_none(), "{body}");
        }
    }

    #[test]
    fn only_declared_fields_and_enum_hints_are_kept() {
        let m = map(
            r#"{"v":1,"event":"permission.replied","sessionId":"s","hint":"once","prompt":"SECRET","path":"/secret","tool_input":{"c":"SECRET"}}"#,
        )
        .expect("mapped");
        let dump = format!("{m:?}");
        assert!(!dump.contains("SECRET") && !dump.contains("/secret"));
        assert_eq!(m.metadata.get("hint").and_then(Value::as_str), Some("once"));
        // Free text is not an enum word and is dropped.
        let m = map(r#"{"v":1,"event":"session.status","sessionId":"s","hint":"Do the thing!"}"#)
            .expect("mapped");
        assert!(!m.metadata.contains_key("hint"));
        let m = map(r#"{"v":1,"event":"session.status","sessionId":"s","hint":{"a":1}}"#)
            .expect("mapped");
        assert!(!m.metadata.contains_key("hint"));
    }

    #[test]
    fn prompt_is_read_only_from_a_user_message_and_is_cleaned() {
        let c = map_context(
            br#"{"v":1,"event":"session.inbox.enqueued","sessionId":"s","prompt":"fix it password=hunter2"}"#,
        )
        .expect("context");
        assert_eq!(c.intent.as_deref(), Some("fix it password=[hidden]"));
        for body in [
            r#"{"v":1,"event":"session.tool.called","sessionId":"s","prompt":"SECRET"}"#,
            r#"{"v":1,"event":"session.inbox.enqueued","sessionId":"s"}"#,
            r#"{"v":1,"event":"session.inbox.enqueued","sessionId":"s","prompt":5}"#,
            r#"{"v":1,"event":"session.inbox.enqueued","sessionId":"s","prompt":"   "}"#,
        ] {
            assert!(map_context(body.as_bytes()).is_none(), "{body}");
        }
    }
}
