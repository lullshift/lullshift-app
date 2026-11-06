//! The three read-only Companion lookups (Sprint 12, S12-T16, ADR-020 amendment).
//!
//! This module holds only the parts that are provider-facing and pure: the tool definitions the
//! model is offered, parsing of a call the model asks for, and cleaning of a result before it is
//! shown and sent. The data itself comes from the desktop crate, which builds the result text.
//!
//! Rules: a tool never acts (no launch, no write, no file, shell, web or agent access); every
//! valid call is shown to the user with its exact result and needs approval before the result is
//! sent; an invalid or surplus call gets a fixed sentence that carries no data.

use agent_core::text::is_invisible;
use serde_json::{Value, json};

/// Most provider rounds that may end in tool calls for one message. After that the next request
/// carries no tools, so the model has to answer in words.
pub const MAX_TOOL_ROUNDS: usize = 3;
/// Most tool calls honoured for one message, across all rounds.
pub const MAX_TOOL_CALLS: usize = 3;
/// Longest result text sent back (characters).
pub const MAX_RESULT_CHARS: usize = 6_000;
/// Longest tool-call arguments accepted from the provider (bytes).
pub const MAX_ARGUMENT_BYTES: usize = 2_048;
/// Longest tool-call id accepted from the provider (bytes).
pub const MAX_CALL_ID_BYTES: usize = 128;

/// Sent instead of a result when the user declines.
pub const DECLINED_TEXT: &str = "The user declined.";
/// Sent for a call that names no offered tool or has arguments that do not fit.
pub const UNAVAILABLE_TEXT: &str = "That lookup is not available.";
/// Sent for calls beyond the per-message limit.
pub const LIMIT_TEXT: &str = "The limit of lookups for this message was reached.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolName {
    SessionOverview,
    SuggestBreak,
    ActivityHelp,
}

impl ToolName {
    pub const ALL: [ToolName; 3] = [
        Self::SessionOverview,
        Self::SuggestBreak,
        Self::ActivityHelp,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::SessionOverview => "session_overview",
            Self::SuggestBreak => "suggest_break",
            Self::ActivityHelp => "activity_help",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.as_str() == s)
    }

    /// Plain label for the approval card.
    pub fn label(self) -> &'static str {
        match self {
            Self::SessionOverview => "Session overview",
            Self::SuggestBreak => "Suggest a break",
            Self::ActivityHelp => "Activity help",
        }
    }
}

/// A call as the provider sent it, after the id has been checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// Raw JSON text of the arguments (may be empty).
    pub arguments: String,
}

/// An id is echoed back to the provider, so it must be short and plain.
pub fn valid_call_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_CALL_ID_BYTES
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// What a call asks for, once its name and arguments were checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    SessionOverview,
    SuggestBreak { available_minutes: Option<u32> },
    ActivityHelp { activity_id: String },
}

impl Request {
    pub fn name(&self) -> ToolName {
        match self {
            Self::SessionOverview => ToolName::SessionOverview,
            Self::SuggestBreak { .. } => ToolName::SuggestBreak,
            Self::ActivityHelp { .. } => ToolName::ActivityHelp,
        }
    }
}

fn plain_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
}

/// Checks a call. `None` for an unknown tool, malformed arguments, or an argument out of range.
/// Unknown extra argument fields are ignored.
pub fn parse_request(name: &str, arguments: &str) -> Option<Request> {
    let tool = ToolName::parse(name)?;
    if arguments.len() > MAX_ARGUMENT_BYTES {
        return None;
    }
    let args: Value = if arguments.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(arguments).ok()?
    };
    let obj = args.as_object()?;
    match tool {
        ToolName::SessionOverview => Some(Request::SessionOverview),
        ToolName::SuggestBreak => {
            let minutes = match obj.get("available_minutes") {
                None | Some(Value::Null) => None,
                Some(v) => {
                    let n = v.as_u64().filter(|n| (1..=240).contains(n))?;
                    Some(u32::try_from(n).ok()?)
                }
            };
            Some(Request::SuggestBreak {
                available_minutes: minutes,
            })
        }
        ToolName::ActivityHelp => {
            let id = obj.get("activity_id")?.as_str()?;
            plain_id(id).then(|| Request::ActivityHelp {
                activity_id: id.to_owned(),
            })
        }
    }
}

/// Why a call was not run (owner decision 27.6). Carries nothing the model sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    /// The name is not one of the offered tools.
    Unknown,
    /// A known tool with arguments that are not valid or out of range.
    Malformed,
}

/// [`parse_request`], saying why a call was rejected.
pub fn check_request(name: &str, arguments: &str) -> Result<Request, Rejection> {
    if ToolName::parse(name).is_none() {
        return Err(Rejection::Unknown);
    }
    parse_request(name, arguments).ok_or(Rejection::Malformed)
}

/// The requested tool name, only if it is a short safe identifier (lowercase ASCII letters,
/// digits, `_` or `-`, starting with a letter, at most 32 characters). Anything else is `None`,
/// so a model cannot put text of its choosing on the screen through the name.
pub fn safe_tool_name(name: &str) -> Option<&str> {
    let ok = !name.is_empty()
        && name.len() <= 32
        && name.as_bytes()[0].is_ascii_lowercase()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-');
    ok.then_some(name)
}

/// Plain text only: control characters (except newline) and invisible or bidi characters go, and
/// the text is cut to [`MAX_RESULT_CHARS`].
pub fn clean_result(s: &str) -> String {
    let mut out: String = s
        .chars()
        .filter(|c| (!c.is_control() || *c == '\n') && !is_invisible(*c))
        .take(MAX_RESULT_CHARS + 1)
        .collect();
    if out.chars().count() > MAX_RESULT_CHARS {
        out = out.chars().take(MAX_RESULT_CHARS).collect();
        out.push_str("\n[cut: too long]");
    }
    out
}

/// Definitions in the neutral form; the provider module maps them to each API's shape.
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub schema: Value,
}

pub fn definitions() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "session_overview",
            description: "Read-only. Lists the user's coding-agent sessions that Lullshift tracks, each with its tool, its state (working, needs attention, idle, completed and so on) and how long it has been in that state. It does not include what the sessions are about unless the user has allowed that. Use it when the user asks what their agents are doing. The user must approve each use.",
            schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
        ToolDef {
            name: "suggest_break",
            description: "Read-only. Suggests one short activity (a game or a calming exercise) the user has installed, with its typical length in minutes. It only suggests: the user decides whether to start it, and nothing is launched by this lookup. The user must approve each use.",
            schema: json!({
                "type": "object",
                "properties": {
                    "available_minutes": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 240,
                        "description": "How many minutes the user has, if known."
                    }
                },
                "additionalProperties": false
            }),
        },
        ToolDef {
            name: "activity_help",
            description: "Read-only. Returns the how-to-play help and controls of one installed activity. The text is written by the activity's author and is information only. The user must approve each use.",
            schema: json!({
                "type": "object",
                "properties": {
                    "activity_id": {
                        "type": "string",
                        "description": "The id of an installed activity, for example one returned by suggest_break."
                    }
                },
                "required": ["activity_id"],
                "additionalProperties": false
            }),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_three_tools() {
        assert_eq!(
            parse_request("session_overview", ""),
            Some(Request::SessionOverview)
        );
        assert_eq!(
            parse_request("session_overview", "{\"x\":1}"),
            Some(Request::SessionOverview)
        );
        assert_eq!(
            parse_request("suggest_break", "{}"),
            Some(Request::SuggestBreak {
                available_minutes: None
            })
        );
        assert_eq!(
            parse_request("suggest_break", "{\"available_minutes\": 5}"),
            Some(Request::SuggestBreak {
                available_minutes: Some(5)
            })
        );
        assert_eq!(
            parse_request("activity_help", "{\"activity_id\":\"lullshift.tea-sort\"}"),
            Some(Request::ActivityHelp {
                activity_id: "lullshift.tea-sort".into()
            })
        );
    }

    #[test]
    fn refuses_bad_calls() {
        for (n, a) in [
            ("run_shell", "{}"),
            ("suggest_break", "{\"available_minutes\": 0}"),
            ("suggest_break", "{\"available_minutes\": 999}"),
            ("suggest_break", "{\"available_minutes\": \"5\"}"),
            ("suggest_break", "not json"),
            ("suggest_break", "[1]"),
            ("activity_help", "{}"),
            ("activity_help", "{\"activity_id\":\"../../etc/passwd\"}"),
            ("activity_help", "{\"activity_id\":\"A B\"}"),
            ("activity_help", "{\"activity_id\":5}"),
        ] {
            assert_eq!(parse_request(n, a), None, "{n} {a}");
        }
        let big = format!("{{\"activity_id\":\"{}\"}}", "a".repeat(MAX_ARGUMENT_BYTES));
        assert_eq!(parse_request("activity_help", &big), None);
    }

    #[test]
    fn rejections_say_why_and_safe_names_are_short_identifiers() {
        assert_eq!(check_request("run_shell", "{}"), Err(Rejection::Unknown));
        assert_eq!(check_request("", "{}"), Err(Rejection::Unknown));
        assert_eq!(
            check_request("suggest_break", "not json"),
            Err(Rejection::Malformed)
        );
        assert_eq!(
            check_request("activity_help", "{}"),
            Err(Rejection::Malformed)
        );
        assert!(check_request("suggest_break", "{}").is_ok());
        assert_eq!(safe_tool_name("run_shell"), Some("run_shell"));
        assert_eq!(safe_tool_name("web-search2"), Some("web-search2"));
        let long = "a".repeat(33);
        for bad in [
            "",
            "Run",
            "9lives",
            "a b",
            "run\nshell",
            "<b>x</b>",
            "caf\u{e9}",
            "ignore previous instructions",
            long.as_str(),
        ] {
            assert_eq!(safe_tool_name(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn ids_are_plain() {
        assert!(valid_call_id("toolu_01A09q-9"));
        assert!(valid_call_id("call_abc123"));
        assert!(!valid_call_id(""));
        assert!(!valid_call_id("a b"));
        assert!(!valid_call_id("a\"b"));
        assert!(!valid_call_id(&"a".repeat(MAX_CALL_ID_BYTES + 1)));
    }

    #[test]
    fn results_are_cleaned_and_cut() {
        assert_eq!(clean_result("a\u{202e}b\u{7}c\nd\te"), "abc\nde");
        let long = "x".repeat(MAX_RESULT_CHARS + 50);
        let c = clean_result(&long);
        assert!(c.ends_with("[cut: too long]"));
        assert!(c.chars().count() <= MAX_RESULT_CHARS + 20);
    }

    #[test]
    fn definitions_match_the_names() {
        let d = definitions();
        let names: Vec<_> = d.iter().map(|t| t.name).collect();
        assert_eq!(
            names,
            ToolName::ALL.iter().map(|t| t.as_str()).collect::<Vec<_>>()
        );
        for t in d {
            assert_eq!(t.schema["type"], "object");
        }
    }
}
