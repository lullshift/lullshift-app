//! Claude Code hook payload → normalized agent event (docs/adapters/CLAUDE_CODE.md).
//!
//! Only `session_id`, `hook_event_name` and a few enum-like fields are read. Prompts, titles,
//! messages, tool data, paths and transcripts are never read into Lullshift types.

use agent_core::EventType;
use serde::Deserialize;
use serde_json::{Map, Value};

pub const PROVIDER: &str = "claude-code";
pub const CAPABILITIES: [&str; 3] = ["lifecycle", "attention", "turn_end"];

/// Hook events Lullshift registers, with the matcher used (if any).
pub const HOOKS: &[(&str, Option<&str>)] = &[
    ("SessionStart", None),
    ("UserPromptSubmit", None),
    ("PostToolUse", None),
    ("PermissionRequest", None),
    (
        "Notification",
        Some("permission_prompt|idle_prompt|elicitation_dialog"),
    ),
    ("Stop", None),
    ("StopFailure", None),
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
    #[serde(default)]
    notification_type: Option<Value>,
    #[serde(default)]
    background_tasks: Option<Value>,
}

/// A mapped hook, ready to become an agent event.
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
        "Notification" => {
            let kind = input.notification_type.as_ref().and_then(Value::as_str)?;
            if !matches!(
                kind,
                "permission_prompt" | "idle_prompt" | "elicitation_dialog"
            ) {
                return None;
            }
            metadata.insert("reason".into(), kind.into());
            EventType::NeedsAttention
        }
        "Stop" => {
            let busy = input
                .background_tasks
                .as_ref()
                .and_then(Value::as_array)
                .is_some_and(|a| !a.is_empty());
            if busy {
                metadata.insert("reason".into(), "background_tasks".into());
                EventType::Heartbeat
            } else {
                EventType::TurnEnded
            }
        }
        "StopFailure" => EventType::Failed,
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

/// Enum-like provider values only (`startup`, `prompt_input_exit`): never free text.
fn is_enum_word(s: &str) -> bool {
    (1..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(json: &str) -> Option<Mapped> {
        map_hook(json.as_bytes())
    }

    const COMMON: &str = r#""session_id":"abc-123","transcript_path":"/secret/t.jsonl","cwd":"/secret/repo","permission_mode":"default""#;

    #[test]
    fn maps_lifecycle_working_and_attention() {
        let m = map(&format!(
            r#"{{{COMMON},"hook_event_name":"SessionStart","source":"startup","model":"x"}}"#
        ))
        .expect("mapped");
        assert_eq!(m.event_type, EventType::SessionStarted);
        assert_eq!(m.metadata["source"], "startup");
        assert_eq!(m.provider_session_id, "abc-123");

        let m = map(&format!(
            r#"{{{COMMON},"hook_event_name":"UserPromptSubmit","prompt":"SECRET PROMPT"}}"#
        ))
        .expect("mapped");
        assert_eq!(m.event_type, EventType::Working);

        let m = map(&format!(r#"{{{COMMON},"hook_event_name":"Notification","message":"SECRET","notification_type":"permission_prompt"}}"#)).expect("mapped");
        assert_eq!(
            (m.event_type, m.metadata["reason"].as_str()),
            (EventType::NeedsAttention, Some("permission_prompt"))
        );

        let m = map(&format!(r#"{{{COMMON},"hook_event_name":"PermissionRequest","tool_name":"Bash","tool_input":{{"command":"rm -rf /"}}}}"#)).expect("mapped");
        assert_eq!(m.event_type, EventType::NeedsAttention);
    }

    #[test]
    fn stop_is_turn_end_never_completion() {
        let m = map(&format!(r#"{{{COMMON},"hook_event_name":"Stop","stop_hook_active":false,"last_assistant_message":"All done!","background_tasks":[]}}"#)).expect("mapped");
        assert_eq!(m.event_type, EventType::TurnEnded);
        let m = map(&format!(
            r#"{{{COMMON},"hook_event_name":"Stop","background_tasks":[{{"id":"b1"}}]}}"#
        ))
        .expect("mapped");
        assert_eq!(m.event_type, EventType::Heartbeat);
        let m = map(&format!(
            r#"{{{COMMON},"hook_event_name":"SessionEnd","reason":"prompt_input_exit"}}"#
        ))
        .expect("mapped");
        assert_eq!(
            (m.event_type, m.metadata["reason"].as_str()),
            (EventType::SessionEnded, Some("prompt_input_exit"))
        );
        let m = map(&format!(
            r#"{{{COMMON},"hook_event_name":"StopFailure","error":"overloaded"}}"#
        ))
        .expect("mapped");
        assert_eq!(m.event_type, EventType::Failed);
    }

    #[test]
    fn never_carries_task_content() {
        for json in [
            format!(
                r#"{{{COMMON},"hook_event_name":"UserPromptSubmit","prompt":"SECRET","session_title":"SECRET"}}"#
            ),
            format!(
                r#"{{{COMMON},"hook_event_name":"PostToolUse","tool_name":"Bash","tool_input":{{"c":"SECRET"}},"tool_response":"SECRET"}}"#
            ),
            format!(r#"{{{COMMON},"hook_event_name":"Stop","last_assistant_message":"SECRET"}}"#),
            format!(r#"{{{COMMON},"hook_event_name":"SessionEnd","reason":"Free text SECRET"}}"#),
        ] {
            let m = map(&json).expect("mapped");
            let all = serde_json::to_string(&m.metadata).expect("json");
            assert!(!all.contains("SECRET") && !all.contains("/secret"), "{all}");
        }
    }

    #[test]
    fn ignores_unknown_and_malformed_input() {
        assert!(map(&format!(r#"{{{COMMON},"hook_event_name":"PreCompact"}}"#)).is_none());
        assert!(map(&format!(r#"{{{COMMON},"hook_event_name":"Notification","notification_type":"auth_success"}}"#)).is_none());
        assert!(
            map(r#"{"hook_event_name":"Stop"}"#).is_none(),
            "no session id"
        );
        assert!(map(r#"{"session_id":7,"hook_event_name":"Stop"}"#).is_none());
        assert!(map(b"\xff\xfe".as_slice().escape_ascii().to_string().as_str()).is_none());
        assert!(map("").is_none());
    }
}
