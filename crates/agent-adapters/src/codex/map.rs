//! Codex hook payload → normalized agent event (docs/ADAPTER_CAPABILITIES.md).
//!
//! Reads only `session_id`, `hook_event_name` and enum-like hints. `prompt`, `tool_*`,
//! `last_assistant_message`, `transcript_path`, `cwd` and everything else are not declared
//! here, so they are skipped while parsing and never held.

use agent_core::EventType;
use agent_ipc::{EventContext, MAX_INTENT_CHARS};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::redact;

pub const PROVIDER: &str = "codex";
/// "attention" is partial: PermissionRequest only; no idle or question event exists.
pub const CAPABILITIES: [&str; 3] = ["lifecycle", "attention", "turn_end"];

/// Hook events Lullshift registers, with the matcher used (if any).
pub const HOOKS: &[(&str, Option<&str>)] = &[
    ("SessionStart", None),
    ("UserPromptSubmit", None),
    ("PostToolUse", None),
    ("PermissionRequest", None),
    ("Stop", None),
    ("Interrupt", None),
    ("SessionEnd", None),
];

#[derive(Deserialize)]
struct HookInput {
    session_id: String,
    hook_event_name: String,
    #[serde(default)]
    source: Option<Value>,
    #[serde(default)]
    reason: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mapped {
    pub provider_session_id: String,
    pub event_type: EventType,
    pub metadata: Map<String, Value>,
}

/// Maps one hook payload. `None` for malformed input and for hooks Lullshift ignores.
pub fn map_hook(stdin: &[u8]) -> Option<Mapped> {
    let input: HookInput = serde_json::from_slice(stdin).ok()?;
    let mut metadata = Map::new();
    let mut hint = |key: &str, v: &Option<Value>| {
        if let Some(s) = v
            .as_ref()
            .and_then(Value::as_str)
            .filter(|s| is_enum_word(s))
        {
            metadata.insert(key.to_owned(), Value::String(s.to_owned()));
        }
    };
    let event_type = match input.hook_event_name.as_str() {
        "SessionStart" => {
            hint("source", &input.source);
            EventType::SessionStarted
        }
        "UserPromptSubmit" | "PostToolUse" => EventType::Working,
        "PermissionRequest" => {
            metadata.insert("reason".into(), "permission".into());
            EventType::NeedsAttention
        }
        "Stop" => EventType::TurnEnded,
        // The user stopped the turn; the session itself carries on, so this is a turn end,
        // not `cancelled` (which is terminal).
        "Interrupt" => {
            metadata.insert("reason".into(), "interrupted".into());
            EventType::TurnEnded
        }
        "SessionEnd" => {
            hint("reason", &input.reason);
            EventType::SessionEnded
        }
        _ => return None,
    };
    metadata.insert("hook".into(), input.hook_event_name.clone().into());
    Some(Mapped {
        provider_session_id: input.session_id,
        event_type,
        metadata,
    })
}

/// The only field read in capture mode (ADR-013, ADR-021): the `prompt` of `UserPromptSubmit`. Nothing
/// else is declared, so tool data, paths, messages, transcripts and the rest are skipped while
/// parsing and never held. Call this only when the user opted in to capture for `codex`
/// (the marker file exists); [`map_hook`] never reads the prompt.
#[derive(Deserialize)]
struct ContextInput {
    hook_event_name: String,
    #[serde(default)]
    prompt: Option<Value>,
}

/// Task text for one Codex hook, cleaned and limited: `UserPromptSubmit.prompt` is the intent. Every
/// other event returns `None` without looking at a prompt.
pub fn map_context(stdin: &[u8]) -> Option<EventContext> {
    let input: ContextInput = serde_json::from_slice(stdin).ok()?;
    if input.hook_event_name != "UserPromptSubmit" {
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

/// Enum-like provider values only: never free text.
fn is_enum_word(s: &str) -> bool {
    (1..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(json: &str) -> Option<Mapped> {
        map_hook(json.as_bytes())
    }
    const COMMON: &str = r#""session_id":"019-abc","transcript_path":"/secret/t.jsonl","cwd":"/secret/repo","model":"gpt","turn_id":"t1","permission_mode":"default""#;

    #[test]
    fn maps_each_event() {
        let cases = [
            (
                r#""hook_event_name":"SessionStart","source":"startup""#,
                EventType::SessionStarted,
            ),
            (
                r#""hook_event_name":"UserPromptSubmit","prompt":"SECRET""#,
                EventType::Working,
            ),
            (
                r#""hook_event_name":"PostToolUse","tool_name":"Bash","tool_input":{"c":"SECRET"},"tool_response":"SECRET""#,
                EventType::Working,
            ),
            (
                r#""hook_event_name":"PermissionRequest","tool_name":"Bash","tool_input":{"command":"rm -rf /"}"#,
                EventType::NeedsAttention,
            ),
            (
                r#""hook_event_name":"Stop","stop_hook_active":false,"last_assistant_message":"SECRET""#,
                EventType::TurnEnded,
            ),
            (r#""hook_event_name":"Interrupt""#, EventType::TurnEnded),
            (
                r#""hook_event_name":"SessionEnd","reason":"other""#,
                EventType::SessionEnded,
            ),
        ];
        for (body, expect) in cases {
            let m = map(&format!("{{{COMMON},{body}}}")).expect(body);
            assert_eq!(m.event_type, expect, "{body}");
            assert_eq!(m.provider_session_id, "019-abc");
            let all = serde_json::to_string(&m.metadata).expect("json");
            assert!(!all.contains("SECRET") && !all.contains("/secret"), "{all}");
        }
        let m = map(&format!("{{{COMMON},\"hook_event_name\":\"Interrupt\"}}")).expect("m");
        assert_eq!(m.metadata["reason"], "interrupted");
        let m = map(&format!(
            "{{{COMMON},\"hook_event_name\":\"SessionEnd\",\"reason\":\"other\"}}"
        ))
        .expect("m");
        assert_eq!(m.metadata["reason"], "other");
    }

    #[test]
    fn ignores_unknown_events_and_bad_input_and_keeps_only_enum_hints() {
        assert!(map(&format!("{{{COMMON},\"hook_event_name\":\"PreToolUse\"}}")).is_none());
        assert!(map(&format!("{{{COMMON},\"hook_event_name\":\"FutureEvent\"}}")).is_none());
        assert!(map(r#"{"hook_event_name":"Stop"}"#).is_none());
        assert!(map(r#"{"session_id":5,"hook_event_name":"Stop"}"#).is_none());
        assert!(map("").is_none());
        assert!(map("not json").is_none());
        let m = map(&format!(
            "{{{COMMON},\"hook_event_name\":\"SessionEnd\",\"reason\":\"Free text SECRET \\u202e\",\"extra\":{{\"a\":[1,2,{{\"b\":null}}]}}}}"
        ))
        .expect("mapped");
        assert!(!m.metadata.contains_key("reason"), "free text is dropped");
    }

    #[test]
    fn hostile_strings_in_unread_fields_are_inert() {
        let evil = "x\\u0000\\u202e\\n\\\"; rm -rf / `id` $(id)";
        let m = map(&format!(
            "{{{COMMON},\"hook_event_name\":\"UserPromptSubmit\",\"prompt\":\"{evil}\",\"source\":\"{evil}\"}}"
        ))
        .expect("mapped");
        let all = serde_json::to_string(&m.metadata).expect("json");
        assert!(!all.contains("rm -rf") && !all.contains("$("), "{all}");
    }

    #[test]
    fn map_context_reads_only_the_prompt_of_userpromptsubmit() {
        let ok = format!(
            "{{{COMMON},\"hook_event_name\":\"UserPromptSubmit\",\"prompt\":\"fix the bug password=hunter2\",\"extra\":\"SECRET\"}}"
        );
        let c = map_context(ok.as_bytes()).expect("intent");
        assert_eq!(c.intent.as_deref(), Some("fix the bug password=[hidden]"));
        assert!(c.ask.is_none() && c.title.is_none());
        for other in [
            "Stop",
            "stop",
            "PostToolUse",
            "postToolUse",
            "SessionStart",
            "sessionStart",
            "PermissionRequest",
            "afterAgentResponse",
        ] {
            let json = format!(
                "{{{COMMON},\"hook_event_name\":\"{other}\",\"prompt\":\"SECRET\",\"message\":\"SECRET\"}}"
            );
            assert!(map_context(json.as_bytes()).is_none(), "{other}");
        }
        let bad = [
            format!("{{{COMMON},\"hook_event_name\":\"UserPromptSubmit\",\"prompt\":5}}"),
            format!(
                "{{{COMMON},\"hook_event_name\":\"UserPromptSubmit\",\"prompt\":\"\\u0007\\u200b\"}}"
            ),
            format!("{{{COMMON},\"hook_event_name\":\"UserPromptSubmit\"}}"),
            "not json".to_owned(),
        ];
        for b in bad {
            assert!(map_context(b.as_bytes()).is_none(), "{b}");
        }
        let long = "word ".repeat(1000);
        let json =
            format!("{{{COMMON},\"hook_event_name\":\"UserPromptSubmit\",\"prompt\":\"{long}\"}}");
        let c = map_context(json.as_bytes()).expect("long");
        assert_eq!(c.intent.map(|s| s.chars().count()), Some(1999));
    }

    #[test]
    fn map_hook_never_holds_the_prompt() {
        let json = format!(
            "{{{COMMON},\"hook_event_name\":\"UserPromptSubmit\",\"prompt\":\"SENTINEL\"}}"
        );
        let m = map_hook(json.as_bytes()).expect("mapped");
        assert!(!format!("{m:?}").contains("SENTINEL"));
    }
}
