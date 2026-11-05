//! ADR-013 / S04-A04: task text leaves the hook process only with the consent marker, is
//! cleaned and limited, reads only the allowed fields, and the zero-effect contract holds.

use std::io::Cursor;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agent_adapters::claude_code::map_context;
use agent_adapters::forward::{CAPTURE_DIR, Outcome, capture_enabled, run};
use agent_core::AgentEvent;
use agent_ipc::{Config, EventContext, SinkError, serve};
use time::OffsetDateTime;

type Seen = Arc<Mutex<Vec<(String, Option<EventContext>)>>>;

const COMMON: &str = r#""session_id":"s-1","transcript_path":"/SENTINEL_TRANSCRIPT/t.jsonl","cwd":"/SENTINEL_CWD/repo","permission_mode":"default""#;

fn args(dir: &Path) -> Vec<String> {
    vec![
        "claude-code".into(),
        "--data-dir".into(),
        dir.to_string_lossy().into_owned(),
    ]
}

fn enable_capture(dir: &Path) {
    let d = dir.join(CAPTURE_DIR);
    std::fs::create_dir_all(&d).expect("mkdir");
    std::fs::write(d.join("claude-code"), b"").expect("marker");
}

fn hook(extra: &str) -> Vec<u8> {
    format!("{{{COMMON},{extra}}}").into_bytes()
}

fn start(dir: &Path) -> (agent_ipc::Running, Seen) {
    let seen: Seen = Arc::default();
    let s2 = Arc::clone(&seen);
    let sink = move |e: AgentEvent, c: Option<EventContext>| -> Result<bool, SinkError> {
        s2.lock()
            .map_err(|_| SinkError)?
            .push((e.event_type.as_str().to_owned(), c));
        Ok(true)
    };
    (
        serve(dir, Arc::new(sink), Config::default()).expect("serve"),
        seen,
    )
}

async fn forward(dir: &Path, payload: Vec<u8>) -> Outcome {
    let a = args(dir);
    tokio::task::spawn_blocking(move || run(&a, Cursor::new(payload), OffsetDateTime::now_utc()))
        .await
        .expect("join")
}

fn all_hooks() -> Vec<Vec<u8>> {
    vec![
        hook(r#""hook_event_name":"UserPromptSubmit","prompt":"SENTINEL_PROMPT fix the bug""#),
        hook(
            r#""hook_event_name":"Notification","notification_type":"permission_prompt","message":"SENTINEL_MESSAGE Claude needs permission""#,
        ),
        hook(
            r#""hook_event_name":"PermissionRequest","tool_name":"Bash","tool_input":{"command":"SENTINEL_COMMAND"}"#,
        ),
        hook(r#""hook_event_name":"Stop","last_assistant_message":"SENTINEL_ASSISTANT""#),
        hook(
            r#""hook_event_name":"PostToolUse","tool_name":"Bash","tool_response":"SENTINEL_OUTPUT""#,
        ),
    ]
}

#[tokio::test(flavor = "multi_thread")]
async fn without_the_marker_no_text_is_ever_sent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (_running, seen) = start(dir.path());
    assert!(!capture_enabled(dir.path(), "claude-code"));
    for payload in all_hooks() {
        assert_eq!(forward(dir.path(), payload).await, Outcome::Sent);
    }
    let seen = seen.lock().expect("lock");
    assert_eq!(seen.len(), 5);
    assert!(
        seen.iter().all(|(_, c)| c.is_none()),
        "no context key reaches the app: {seen:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn with_the_marker_only_the_allowed_fields_are_sent() {
    let dir = tempfile::tempdir().expect("tempdir");
    enable_capture(dir.path());
    assert!(capture_enabled(dir.path(), "claude-code"));
    let (_running, seen) = start(dir.path());
    for payload in all_hooks() {
        assert_eq!(forward(dir.path(), payload).await, Outcome::Sent);
    }
    let seen = seen.lock().expect("lock").clone();
    assert_eq!(
        seen[0].1,
        Some(EventContext {
            intent: Some("SENTINEL_PROMPT fix the bug".into()),
            ask: None,
            title: None
        })
    );
    assert_eq!(
        seen[1].1,
        Some(EventContext {
            intent: None,
            ask: Some("SENTINEL_MESSAGE Claude needs permission".into()),
            title: None
        })
    );
    assert_eq!(
        seen[2].1,
        Some(EventContext {
            intent: None,
            ask: Some("Permission to use Bash".into()),
            title: None
        })
    );
    assert_eq!(seen[3].1, None, "Stop: assistant message is never read");
    assert_eq!(seen[4].1, None, "PostToolUse: tool output is never read");
    let dump = format!("{seen:?}");
    for never in [
        "SENTINEL_TRANSCRIPT",
        "SENTINEL_CWD",
        "SENTINEL_COMMAND",
        "SENTINEL_ASSISTANT",
        "SENTINEL_OUTPUT",
    ] {
        assert!(!dump.contains(never), "{never} leaked: {dump}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn secrets_are_hidden_before_leaving_the_hook() {
    let dir = tempfile::tempdir().expect("tempdir");
    enable_capture(dir.path());
    let (_running, seen) = start(dir.path());
    let long = "A1b2".repeat(10);
    let prompt = format!(
        "deploy with password=hunter2 and sk-ant-api03-abcdef123456 then {long}\\n-----BEGIN PRIVATE KEY-----\\nMIIE\\n-----END PRIVATE KEY----- done\\u202e"
    );
    let message = "Use token=abcd1234 for ghp_0123456789abcdef?";
    let p1 = hook(&format!(
        r#""hook_event_name":"UserPromptSubmit","prompt":"{prompt}""#
    ));
    let p2 = hook(&format!(
        r#""hook_event_name":"Notification","notification_type":"idle_prompt","message":"{message}""#
    ));
    assert_eq!(forward(dir.path(), p1).await, Outcome::Sent);
    assert_eq!(forward(dir.path(), p2).await, Outcome::Sent);
    let seen = seen.lock().expect("lock").clone();
    let intent = seen[0]
        .1
        .as_ref()
        .and_then(|c| c.intent.clone())
        .expect("intent");
    for secret in [
        "hunter2",
        "sk-ant-api03",
        long.as_str(),
        "MIIE",
        "PRIVATE KEY",
        "\u{202e}",
    ] {
        assert!(!intent.contains(secret), "{secret} in {intent:?}");
    }
    assert!(intent.contains("[hidden]") && intent.contains("deploy with password="));
    assert!(intent.ends_with("done"));
    let ask = seen[1].1.as_ref().and_then(|c| c.ask.clone()).expect("ask");
    assert_eq!(ask, "Use token=[hidden] for [hidden]?");
}

#[tokio::test(flavor = "multi_thread")]
async fn limits_and_tool_name_rules_apply() {
    let dir = tempfile::tempdir().expect("tempdir");
    enable_capture(dir.path());
    let (_running, seen) = start(dir.path());
    let long_prompt = "word ".repeat(1000);
    let long_message = "word ".repeat(200);
    for payload in [
        hook(&format!(
            r#""hook_event_name":"UserPromptSubmit","prompt":"{long_prompt}""#
        )),
        hook(&format!(
            r#""hook_event_name":"Notification","notification_type":"elicitation_dialog","message":"{long_message}""#
        )),
        hook(r#""hook_event_name":"PermissionRequest","tool_name":"Bash; rm -rf /""#),
        hook(r#""hook_event_name":"PermissionRequest","tool_name":"mcp__git__commit""#),
        hook("\"hook_event_name\":\"UserPromptSubmit\",\"prompt\":\"\\u0007\\u200b\""),
    ] {
        assert_eq!(forward(dir.path(), payload).await, Outcome::Sent);
    }
    let seen = seen.lock().expect("lock").clone();
    assert_eq!(
        seen[0]
            .1
            .as_ref()
            .and_then(|c| c.intent.as_ref())
            .map(|s| s.chars().count()),
        Some(1999)
    );
    assert_eq!(
        seen[1]
            .1
            .as_ref()
            .and_then(|c| c.ask.as_ref())
            .map(|s| s.chars().count()),
        Some(299)
    );
    assert_eq!(seen[2].1, None, "tool name must be a short identifier");
    assert_eq!(
        seen[3].1.as_ref().and_then(|c| c.ask.as_deref()),
        Some("Permission to use mcp__git__commit")
    );
    assert_eq!(seen[4].1, None, "text that cleans to nothing is not sent");
}

#[test]
fn map_context_ignores_other_hooks_and_notifications() {
    assert!(
        map_context(&hook(
            r#""hook_event_name":"SessionStart","source":"startup","prompt":"x""#
        ))
        .is_none()
    );
    assert!(
        map_context(&hook(
            r#""hook_event_name":"Notification","notification_type":"auth_success","message":"x""#
        ))
        .is_none()
    );
    assert!(map_context(b"not json").is_none());
    assert!(map_context(&hook(r#""hook_event_name":"UserPromptSubmit","prompt":5"#)).is_none());
}

#[test]
fn zero_effect_contract_holds_with_capture_on() {
    // App not running: fails fast and quietly, whatever the payload holds.
    let dir = tempfile::tempdir().expect("tempdir");
    enable_capture(dir.path());
    let a = args(dir.path());
    let started = Instant::now();
    let out = run(
        &a,
        Cursor::new(hook(
            r#""hook_event_name":"UserPromptSubmit","prompt":"hello""#,
        )),
        OffsetDateTime::now_utc(),
    );
    assert_eq!(out, Outcome::AppNotRunning);
    assert!(started.elapsed() < Duration::from_secs(1));

    // Oversized input is still dropped before any text is read.
    let big = vec![b' '; 1024 * 1024 + 10];
    assert_eq!(
        run(&a, Cursor::new(big), OffsetDateTime::now_utc()),
        Outcome::TooLarge
    );
    // Malformed input is ignored.
    assert_eq!(
        run(
            &a,
            Cursor::new(b"garbage".to_vec()),
            OffsetDateTime::now_utc()
        ),
        Outcome::Ignored
    );
}
