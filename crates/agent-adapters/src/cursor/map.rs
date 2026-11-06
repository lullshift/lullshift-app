//! Cursor hook payload → normalized agent event (docs/ADAPTER_CAPABILITIES.md).
//!
//! Reads only `conversation_id` (the session key), `hook_event_name` and enum-like hints.
//! `user_email`, `prompt`, `text`, `tool_*`, `workspace_roots`, `transcript_path` and the rest
//! are not declared here, so they are skipped while parsing and never held.
//!
//! Cursor has no event for "waiting for the user", so nothing here ever maps to
//! `needs_attention`, and the capabilities don't claim it.

use agent_core::EventType;
use agent_ipc::{EventContext, MAX_INTENT_CHARS};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::redact;

pub const PROVIDER: &str = "cursor";
pub const CAPABILITIES: [&str; 2] = ["lifecycle", "turn_end"];

/// Hook events Lullshift registers, with the matcher used (if any). Never a permission or
/// `pre*` hook: those can block the agent.
pub const HOOKS: &[(&str, Option<&str>)] = &[
    ("sessionStart", None),
    ("beforeSubmitPrompt", None),
    ("postToolUse", None),
    ("afterAgentResponse", None),
    ("stop", None),
    ("sessionEnd", None),
    // Added after the adapter's first version (owner decision 23.2b). Cursor documents none of
    // these three as able to block. Older installs have only the first six, which shows as
    // "partly installed" until the user connects again.
    ("postToolUseFailure", None),
    ("subagentStop", None),
    ("preCompact", None),
];

#[derive(Deserialize)]
struct HookInput {
    conversation_id: String,
    hook_event_name: String,
    #[serde(default)]
    status: Option<Value>,
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
        "sessionStart" => EventType::SessionStarted,
        // A failed tool call or a finished subagent neither ends nor fails the session; the
        // agent is still working. Compaction is the agent tidying its own context.
        "beforeSubmitPrompt" | "postToolUse" | "afterAgentResponse" | "postToolUseFailure"
        | "subagentStop" | "preCompact" => EventType::Working,
        // `status` is completed, aborted or error. All are the end of a turn; whether the
        // task is done isn't known (the same rule as Claude Code's Stop).
        "stop" => {
            hint("status", &input.status);
            EventType::TurnEnded
        }
        // `reason` is an enum (completed, aborted, error, window_close, user_close). Only the
        // word `error` changes the state; `error_message` is not declared, so never held.
        "sessionEnd" => {
            hint("reason", &input.reason);
            if input.reason.as_ref().and_then(Value::as_str) == Some("error") {
                EventType::Failed
            } else {
                EventType::SessionEnded
            }
        }
        _ => return None,
    };
    metadata.insert("hook".into(), input.hook_event_name.clone().into());
    Some(Mapped {
        provider_session_id: input.conversation_id,
        event_type,
        metadata,
    })
}

/// The only field read in capture mode (ADR-013, ADR-021): the `prompt` of `beforeSubmitPrompt`. Nothing
/// else is declared, so tool data, paths, messages, transcripts and the rest are skipped while
/// parsing and never held. Call this only when the user opted in to capture for `cursor`
/// (the marker file exists); [`map_hook`] never reads the prompt.
#[derive(Deserialize)]
struct ContextInput {
    hook_event_name: String,
    #[serde(default)]
    prompt: Option<Value>,
}

/// Task text for one Cursor hook, cleaned and limited: `beforeSubmitPrompt.prompt` is the intent. Every
/// other event returns `None` without looking at a prompt.
pub fn map_context(stdin: &[u8]) -> Option<EventContext> {
    let input: ContextInput = serde_json::from_slice(stdin).ok()?;
    if input.hook_event_name != "beforeSubmitPrompt" {
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
    const COMMON: &str = r#""conversation_id":"conv-1","generation_id":"g1","model":"m","cursor_version":"1.7.0","workspace_roots":["/secret"],"user_email":"SECRET@example.com","transcript_path":"/secret/t""#;

    #[test]
    fn maps_each_event_and_never_needs_attention() {
        let cases = [
            (
                r#""hook_event_name":"sessionStart","session_id":"s""#,
                EventType::SessionStarted,
            ),
            (
                r#""hook_event_name":"beforeSubmitPrompt","prompt":"SECRET""#,
                EventType::Working,
            ),
            (
                r#""hook_event_name":"postToolUse","tool_name":"Shell","tool_input":{"c":"SECRET"},"tool_output":"SECRET""#,
                EventType::Working,
            ),
            (
                r#""hook_event_name":"afterAgentResponse","text":"SECRET""#,
                EventType::Working,
            ),
            (
                r#""hook_event_name":"stop","status":"completed","loop_count":0"#,
                EventType::TurnEnded,
            ),
            (
                r#""hook_event_name":"stop","status":"aborted""#,
                EventType::TurnEnded,
            ),
            (
                r#""hook_event_name":"stop","status":"error""#,
                EventType::TurnEnded,
            ),
            (
                r#""hook_event_name":"sessionEnd","reason":"window_close","final_status":"x","error_message":"SECRET""#,
                EventType::SessionEnded,
            ),
            (
                r#""hook_event_name":"sessionEnd","reason":"completed","final_status":"error","error_message":"SECRET""#,
                EventType::SessionEnded,
            ),
            (
                r#""hook_event_name":"sessionEnd","reason":"error","final_status":"error","error_message":"SECRET failure text""#,
                EventType::Failed,
            ),
            (
                r#""hook_event_name":"postToolUseFailure","tool_name":"Shell","error_message":"SECRET","failure_type":"error""#,
                EventType::Working,
            ),
            (
                r#""hook_event_name":"subagentStop","status":"error","summary":"SECRET""#,
                EventType::Working,
            ),
            (
                r#""hook_event_name":"preCompact","trigger":"auto","context_usage_percent":91"#,
                EventType::Working,
            ),
        ];
        for (body, expect) in cases {
            let m = map(&format!("{{{COMMON},{body}}}")).expect(body);
            assert_eq!(m.event_type, expect, "{body}");
            assert_ne!(m.event_type, EventType::NeedsAttention);
            assert_eq!(
                m.provider_session_id, "conv-1",
                "conversation_id is the key"
            );
            let all = serde_json::to_string(&m.metadata).expect("json");
            assert!(
                !all.contains("SECRET") && !all.contains("/secret") && !all.contains("1.7.0"),
                "{all}"
            );
        }
        let m = map(&format!(
            "{{{COMMON},\"hook_event_name\":\"stop\",\"status\":\"aborted\"}}"
        ))
        .expect("m");
        assert_eq!(m.metadata["status"], "aborted");
        assert!(!CAPABILITIES.contains(&"attention"));
        let m = map(&format!(
            "{{{COMMON},\"hook_event_name\":\"sessionEnd\",\"reason\":\"error\",\"error_message\":\"SECRET\"}}"
        ))
        .expect("m");
        assert_eq!(m.metadata["reason"], "error", "the enum hint is kept");
    }

    #[test]
    fn registers_only_hooks_that_cannot_block() {
        let names: Vec<&str> = HOOKS.iter().map(|(e, _)| *e).collect();
        for blocking in [
            "subagentStart",
            "preToolUse",
            "beforeShellExecution",
            "beforeMCPExecution",
            "beforeReadFile",
            "beforeTabFileRead",
        ] {
            assert!(!names.contains(&blocking), "{blocking} can block");
        }
        for name in &names {
            assert!(
                map(&format!("{{{COMMON},\"hook_event_name\":\"{name}\"}}")).is_some(),
                "{name} is registered but not mapped"
            );
        }
        assert_eq!(names.len(), 9);
    }

    #[test]
    fn ignores_blocking_unknown_and_malformed_hooks() {
        for name in [
            "beforeShellExecution",
            "preToolUse",
            "beforeMCPExecution",
            "subagentStart",
            "future",
        ] {
            assert!(
                map(&format!("{{{COMMON},\"hook_event_name\":\"{name}\"}}")).is_none(),
                "{name}"
            );
        }
        assert!(
            map(r#"{"hook_event_name":"stop"}"#).is_none(),
            "no conversation id"
        );
        assert!(map(r#"{"session_id":"only","hook_event_name":"stop"}"#).is_none());
        assert!(map(r#"{"conversation_id":3,"hook_event_name":"stop"}"#).is_none());
        assert!(map("").is_none());
        let m = map(&format!(
            "{{{COMMON},\"hook_event_name\":\"sessionEnd\",\"reason\":\"Free text $(id) \\u202e\",\"unknown\":{{\"x\":[1,{{\"y\":null}}]}}}}"
        ))
        .expect("mapped");
        assert!(!m.metadata.contains_key("reason"));
    }

    #[test]
    fn map_context_reads_only_the_prompt_of_beforesubmitprompt() {
        let ok = format!(
            "{{{COMMON},\"hook_event_name\":\"beforeSubmitPrompt\",\"prompt\":\"fix the bug password=hunter2\",\"extra\":\"SECRET\"}}"
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
            format!("{{{COMMON},\"hook_event_name\":\"beforeSubmitPrompt\",\"prompt\":5}}"),
            format!(
                "{{{COMMON},\"hook_event_name\":\"beforeSubmitPrompt\",\"prompt\":\"\\u0007\\u200b\"}}"
            ),
            format!("{{{COMMON},\"hook_event_name\":\"beforeSubmitPrompt\"}}"),
            "not json".to_owned(),
        ];
        for b in bad {
            assert!(map_context(b.as_bytes()).is_none(), "{b}");
        }
        let long = "word ".repeat(1000);
        let json = format!(
            "{{{COMMON},\"hook_event_name\":\"beforeSubmitPrompt\",\"prompt\":\"{long}\"}}"
        );
        let c = map_context(json.as_bytes()).expect("long");
        assert_eq!(c.intent.map(|s| s.chars().count()), Some(1999));
    }

    #[test]
    fn map_hook_never_holds_the_prompt() {
        let json = format!(
            "{{{COMMON},\"hook_event_name\":\"beforeSubmitPrompt\",\"prompt\":\"SENTINEL\"}}"
        );
        let m = map_hook(json.as_bytes()).expect("mapped");
        assert!(!format!("{m:?}").contains("SENTINEL"));
    }
}
