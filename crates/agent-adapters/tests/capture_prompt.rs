//! ADR-013 / ADR-021: Codex and Cursor prompts leave the hook process only with the consent
//! marker for that adapter, cleaned and limited, and nothing else of the payload goes with them.

use std::io::Cursor;
use std::path::Path;
use std::sync::{Arc, Mutex};

use agent_adapters::forward::{CAPTURE_DIR, Outcome, run};
use agent_core::AgentEvent;
use agent_ipc::{Config, EventContext, SinkError, serve};
use time::OffsetDateTime;

type Seen = Arc<Mutex<Vec<(String, Option<EventContext>)>>>;

struct Case {
    adapter: &'static str,
    prompt_hook: &'static str,
    other_hooks: [&'static str; 3],
}

const CASES: [Case; 3] = [
    Case {
        adapter: "codex",
        prompt_hook: r#"{"session_id":"s-1","hook_event_name":"UserPromptSubmit","cwd":"/SENTINEL_CWD","transcript_path":"/SENTINEL_T","prompt":"SENTINEL_PROMPT fix it password=hunter2"}"#,
        other_hooks: [
            r#"{"session_id":"s-1","hook_event_name":"Stop","prompt":"SENTINEL_STOP","last_assistant_message":"SENTINEL_ASSISTANT"}"#,
            r#"{"session_id":"s-1","hook_event_name":"PostToolUse","prompt":"SENTINEL_TOOL","tool_input":{"c":"SENTINEL_COMMAND"}}"#,
            r#"{"session_id":"s-1","hook_event_name":"PermissionRequest","prompt":"SENTINEL_PERM","tool_input":{"c":"SENTINEL_COMMAND"}}"#,
        ],
    },
    Case {
        adapter: "cursor",
        prompt_hook: r#"{"conversation_id":"c-1","hook_event_name":"beforeSubmitPrompt","user_email":"SENTINEL@x.y","workspace_roots":["/SENTINEL_CWD"],"prompt":"SENTINEL_PROMPT fix it password=hunter2","attachments":[{"file_path":"/SENTINEL_ATTACH"}]}"#,
        other_hooks: [
            r#"{"conversation_id":"c-1","hook_event_name":"stop","status":"completed","prompt":"SENTINEL_STOP"}"#,
            r#"{"conversation_id":"c-1","hook_event_name":"afterAgentResponse","text":"SENTINEL_ASSISTANT","prompt":"SENTINEL_AFTER"}"#,
            r#"{"conversation_id":"c-1","hook_event_name":"postToolUse","prompt":"SENTINEL_TOOL","tool_input":{"c":"SENTINEL_COMMAND"}}"#,
        ],
    },
    // The OpenCode plugin's own allow-listed message (ADR-022), not OpenCode's event.
    Case {
        adapter: "opencode",
        prompt_hook: r#"{"v":1,"event":"session.inbox.enqueued","sessionId":"o-1","path":"/SENTINEL_CWD","prompt":"SENTINEL_PROMPT fix it password=hunter2"}"#,
        other_hooks: [
            r#"{"v":1,"event":"turn_ended","sessionId":"o-1","prompt":"SENTINEL_STOP"}"#,
            r#"{"v":1,"event":"session.tool.called","sessionId":"o-1","prompt":"SENTINEL_TOOL","tool_input":{"c":"SENTINEL_COMMAND"}}"#,
            r#"{"v":1,"event":"permission.asked","sessionId":"o-1","prompt":"SENTINEL_PERM","tool_input":{"c":"SENTINEL_COMMAND"}}"#,
        ],
    },
];

fn args(adapter: &str, dir: &Path) -> Vec<String> {
    vec![
        adapter.into(),
        "--data-dir".into(),
        dir.to_string_lossy().into_owned(),
    ]
}

fn enable(dir: &Path, adapter: &str) {
    let d = dir.join(CAPTURE_DIR);
    std::fs::create_dir_all(&d).expect("mkdir");
    std::fs::write(d.join(adapter), b"").expect("marker");
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

async fn forward(adapter: &'static str, dir: &Path, payload: &str) -> Outcome {
    let a = args(adapter, dir);
    let body = payload.as_bytes().to_vec();
    tokio::task::spawn_blocking(move || run(&a, Cursor::new(body), OffsetDateTime::now_utc()))
        .await
        .expect("join")
}

#[tokio::test(flavor = "multi_thread")]
async fn opt_in_off_the_prompt_is_never_sent() {
    for case in &CASES {
        let dir = tempfile::tempdir().expect("tempdir");
        let (_running, seen) = start(dir.path());
        for payload in std::iter::once(&case.prompt_hook).chain(case.other_hooks.iter()) {
            assert_eq!(
                forward(case.adapter, dir.path(), payload).await,
                Outcome::Sent
            );
        }
        let seen = seen.lock().expect("lock");
        assert_eq!(seen.len(), 4, "{}", case.adapter);
        assert!(
            seen.iter().all(|(_, c)| c.is_none()),
            "{}: no context reaches the app: {seen:?}",
            case.adapter
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn another_adapters_marker_does_not_open_the_gate() {
    for case in &CASES {
        let dir = tempfile::tempdir().expect("tempdir");
        for other in ["claude-code", "codex", "cursor", "opencode"] {
            if other != case.adapter {
                enable(dir.path(), other);
            }
        }
        let (_running, seen) = start(dir.path());
        assert_eq!(
            forward(case.adapter, dir.path(), case.prompt_hook).await,
            Outcome::Sent
        );
        assert!(
            seen.lock().expect("lock")[0].1.is_none(),
            "{}",
            case.adapter
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn opt_in_on_only_the_prompt_of_the_prompt_hook_is_sent_cleaned() {
    for case in &CASES {
        let dir = tempfile::tempdir().expect("tempdir");
        enable(dir.path(), case.adapter);
        let (_running, seen) = start(dir.path());
        for payload in std::iter::once(&case.prompt_hook).chain(case.other_hooks.iter()) {
            assert_eq!(
                forward(case.adapter, dir.path(), payload).await,
                Outcome::Sent
            );
        }
        let seen = seen.lock().expect("lock").clone();
        assert_eq!(
            seen[0].1,
            Some(EventContext {
                intent: Some("SENTINEL_PROMPT fix it password=[hidden]".into()),
                ask: None,
                title: None
            }),
            "{}",
            case.adapter
        );
        for (i, (_, c)) in seen.iter().enumerate().skip(1) {
            assert!(c.is_none(), "{} hook {i} carries no text", case.adapter);
        }
        let dump = format!("{seen:?}");
        for never in [
            "SENTINEL_CWD",
            "SENTINEL_T",
            "SENTINEL_ATTACH",
            "SENTINEL@",
            "SENTINEL_STOP",
            "SENTINEL_ASSISTANT",
            "SENTINEL_TOOL",
            "SENTINEL_COMMAND",
            "SENTINEL_PERM",
            "SENTINEL_AFTER",
            "hunter2",
        ] {
            assert!(
                !dump.contains(never),
                "{} leaked {never}: {dump}",
                case.adapter
            );
        }
    }
}

#[test]
fn zero_effect_contract_holds_with_capture_on() {
    for case in &CASES {
        let dir = tempfile::tempdir().expect("tempdir");
        enable(dir.path(), case.adapter);
        let a = args(case.adapter, dir.path());
        let started = std::time::Instant::now();
        let out = run(
            &a,
            Cursor::new(case.prompt_hook.as_bytes().to_vec()),
            OffsetDateTime::now_utc(),
        );
        assert_eq!(out, Outcome::AppNotRunning);
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        let big = vec![b' '; 1024 * 1024 + 10];
        assert_eq!(
            run(&a, Cursor::new(big), OffsetDateTime::now_utc()),
            Outcome::TooLarge
        );
        assert_eq!(
            run(
                &a,
                Cursor::new(b"garbage".to_vec()),
                OffsetDateTime::now_utc()
            ),
            Outcome::Ignored
        );
    }
}
