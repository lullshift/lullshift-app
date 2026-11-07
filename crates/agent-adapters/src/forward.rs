//! Hook forwarder: runs inside a provider's hook, turns the hook payload into one agent event
//! and sends it to the running app over agent IPC (ADR-009).
//!
//! Zero-effect contract (docs/adapters/CLAUDE_CODE.md): the caller must exit 0 and nothing may
//! be written to stdout or stderr, whatever happens. Everything here returns an outcome instead
//! of failing, and the whole run is bounded by [`DEADLINE`].

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use agent_core::parse_event;
use serde_json::{Map, Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use agent_ipc::EventContext;

use crate::hook_failure::{self, FailureReason};
use crate::{claude_code, codex, cursor, opencode};

/// Directory under the app data dir that holds one consent marker file per adapter. The app
/// creates `<data-dir>/context-capture/<adapter>` when the user turns capture on and deletes it
/// when they turn it off (ADR-013). Without the file the forwarder never reads task text.
pub const CAPTURE_DIR: &str = "context-capture";

/// Whether the user opted in to task-text capture for `adapter`.
pub fn capture_enabled(data_dir: &Path, adapter: &str) -> bool {
    data_dir.join(CAPTURE_DIR).join(adapter).is_file()
}

/// Hard limit for one forwarder run, start to finish.
pub const DEADLINE: Duration = Duration::from_secs(2);
/// Hook payloads larger than this are dropped (tool inputs can be large). The rest of the
/// input is still drained so the agent never sees a broken pipe.
pub const MAX_STDIN_BYTES: u64 = 1024 * 1024;

/// What happened; for tests and diagnostics only (never printed by the hook).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Sent,
    Duplicate,
    Rejected,
    Ignored,
    AppNotRunning,
    BadArguments,
    TooLarge,
    TimedOut,
    Failed,
}

/// Parsed `hook <adapter> --data-dir <dir>` arguments (after `hook`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookArgs {
    pub adapter: String,
    pub data_dir: PathBuf,
}

pub fn parse_args(args: &[String]) -> Option<HookArgs> {
    match args {
        [adapter, flag, dir] if flag == "--data-dir" && !dir.is_empty() => Some(HookArgs {
            adapter: adapter.clone(),
            data_dir: PathBuf::from(dir),
        }),
        _ => None,
    }
}

/// Entry point for the `hook` mode of the Lullshift executable. Reads stdin, never panics
/// into the caller's output, returns within [`DEADLINE`]. `started` is taken when the process
/// starts, before stdin is read, so events from separate hook processes keep the order in
/// which the agent ran the hooks.
pub fn run(args: &[String], stdin: impl Read + Send + 'static, started: OffsetDateTime) -> Outcome {
    let Some(args) = parse_args(args) else {
        return Outcome::BadArguments;
    };
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(run_inner(&args, stdin, started));
    });
    rx.recv_timeout(DEADLINE).unwrap_or(Outcome::TimedOut)
}

fn run_inner(args: &HookArgs, mut stdin: impl Read, started: OffsetDateTime) -> Outcome {
    let mut buf = Vec::new();
    if (&mut stdin)
        .take(MAX_STDIN_BYTES + 1)
        .read_to_end(&mut buf)
        .is_err()
    {
        return Outcome::Failed;
    }
    if buf.len() as u64 > MAX_STDIN_BYTES {
        // Drain (bounded by the deadline) so the writer doesn't get a broken pipe.
        let _ = std::io::copy(&mut stdin, &mut std::io::sink());
        return Outcome::TooLarge;
    }
    let (event, context) = match args.adapter.as_str() {
        claude_code::ADAPTER_ID => match build_claude_code_event(&buf, started) {
            Some(e) => {
                // Second look at the hook payload and the transcript title, only with consent.
                let context = context_for(
                    &args.data_dir,
                    &args.adapter,
                    &buf,
                    claude_code::title::read_title,
                );
                (e, context)
            }
            None => return Outcome::Ignored,
        },
        // Codex and Cursor: session id, event type and enum hints only. The prompt is read in a
        // second step, and only with the consent marker (ADR-013, ADR-021). The forwarder can't
        // cheaply tell whether the app already holds a prompt for the session (that is in the
        // app's database), so every captured prompt crosses the local pipe to the app, which
        // keeps only the first one of a session and drops the rest.
        codex::ADAPTER_ID => match build_codex_event(&buf, started) {
            Some(e) => {
                let context =
                    prompt_context_for(&args.data_dir, &args.adapter, &buf, codex::map_context);
                (e, context)
            }
            None => return Outcome::Ignored,
        },
        cursor::ADAPTER_ID => match build_cursor_event(&buf, started) {
            Some(e) => {
                let context =
                    prompt_context_for(&args.data_dir, &args.adapter, &buf, cursor::map_context);
                (e, context)
            }
            None => return Outcome::Ignored,
        },
        // OpenCode: the installed plugin sends a small allow-listed message, not OpenCode's own
        // events (ADR-022). The prompt in it is read only with the consent marker.
        opencode::ADAPTER_ID => match build_opencode_event(&buf, started) {
            Some(e) => {
                let context =
                    prompt_context_for(&args.data_dir, &args.adapter, &buf, opencode::map_context);
                (e, context)
            }
            None => return Outcome::Ignored,
        },
        _ => return Outcome::BadArguments,
    };
    send(&args.data_dir, &args.adapter, &event, context.as_ref())
}

/// The optional task text for one Claude Code hook (ADR-013): prompt, question and the session
/// title. Without the consent marker this returns `None` before anything is read; in
/// particular `read_title` is not called, so the transcript file is never opened. `read_title`
/// is a parameter so tests can prove that.
pub fn context_for(
    data_dir: &Path,
    adapter: &str,
    payload: &[u8],
    read_title: impl FnOnce(&Path, &str) -> Option<String>,
) -> Option<EventContext> {
    if !capture_enabled(data_dir, adapter) {
        return None;
    }
    let mut context = claude_code::map_context(payload).unwrap_or_default();
    context.title = claude_code::transcript_ref(payload)
        .and_then(|(path, session)| read_title(&path, &session));
    context.normalized()
}

/// The optional task text for one Codex, Cursor or OpenCode hook (ADR-013): the prompt, as the intent.
/// Without the consent marker this returns `None` before `map` is called, so the prompt field is
/// never read. `map` is a parameter so tests can prove that.
pub fn prompt_context_for(
    data_dir: &Path,
    adapter: &str,
    payload: &[u8],
    map: impl FnOnce(&[u8]) -> Option<EventContext>,
) -> Option<EventContext> {
    if !capture_enabled(data_dir, adapter) {
        return None;
    }
    map(payload)
}

/// Builds and validates the agent event JSON for one Claude Code hook payload.
pub fn build_claude_code_event(stdin: &[u8], now: OffsetDateTime) -> Option<String> {
    let m = claude_code::map_hook(stdin)?;
    build_event(
        claude_code::PROVIDER,
        &m.provider_session_id,
        m.event_type.as_str(),
        now,
        &claude_code::CAPABILITIES,
        m.metadata,
    )
}

/// Builds and validates the agent event JSON for one Codex hook payload.
pub fn build_codex_event(stdin: &[u8], now: OffsetDateTime) -> Option<String> {
    let m = codex::map_hook(stdin)?;
    build_event(
        codex::PROVIDER,
        &m.provider_session_id,
        m.event_type.as_str(),
        now,
        &codex::CAPABILITIES,
        m.metadata,
    )
}

/// Builds and validates the agent event JSON for one Cursor hook payload.
pub fn build_cursor_event(stdin: &[u8], now: OffsetDateTime) -> Option<String> {
    let m = cursor::map_hook(stdin)?;
    build_event(
        cursor::PROVIDER,
        &m.provider_session_id,
        m.event_type.as_str(),
        now,
        &cursor::CAPABILITIES,
        m.metadata,
    )
}

/// Builds and validates the agent event JSON for one OpenCode plugin message.
pub fn build_opencode_event(stdin: &[u8], now: OffsetDateTime) -> Option<String> {
    let m = opencode::map_hook(stdin)?;
    build_event(
        opencode::PROVIDER,
        &m.provider_session_id,
        m.event_type.as_str(),
        now,
        &opencode::CAPABILITIES,
        m.metadata,
    )
}

fn build_event(
    provider: &str,
    session: &str,
    event_type: &str,
    now: OffsetDateTime,
    capabilities: &[&str],
    metadata: Map<String, Value>,
) -> Option<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).ok()?;
    let event_id = uuid::Builder::from_random_bytes(bytes).into_uuid();
    let json = json!({
        "schemaVersion": 1,
        "eventId": event_id.hyphenated().to_string(),
        "provider": provider,
        "providerSessionId": session,
        "eventType": event_type,
        "timestampUtc": now.format(&Rfc3339).ok()?,
        "capabilities": capabilities,
        "metadata": metadata,
    })
    .to_string();
    // Same validation as the app: never send what the app would reject.
    parse_event(&json).ok().map(|_| json)
}

fn send(
    data_dir: &Path,
    adapter: &str,
    event_json: &str,
    context: Option<&EventContext>,
) -> Outcome {
    let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return Outcome::Failed;
    };
    let reply = rt.block_on(agent_ipc::send_event_with_context(
        data_dir,
        event_json,
        context,
        DEADLINE.saturating_sub(Duration::from_millis(250)),
    ));
    let outcome = match &reply {
        Ok(r) if r.ok && r.duplicate => Outcome::Duplicate,
        Ok(r) if r.ok => Outcome::Sent,
        Ok(_) => Outcome::Rejected,
        Err(agent_ipc::ClientError::NotRunning) => Outcome::AppNotRunning,
        Err(_) => Outcome::Failed,
    };
    if let Some(reason) = failure_reason(&reply, data_dir) {
        // Data-free marker so the app can say why events stopped (decision 33). Silent and
        // best effort; it runs inside the same deadline as the rest of the hook.
        let _ = hook_failure::record(data_dir, adapter, reason, OffsetDateTime::now_utc());
    }
    outcome
}

/// Which failures leave a marker: a token refusal, and a failed exchange while `endpoint.json`
/// existed (an address that leads nowhere). No connection files at all means Lullshift isn't
/// set up or running here, which is not worth reporting.
fn failure_reason(
    reply: &Result<agent_ipc::Reply, agent_ipc::ClientError>,
    data_dir: &Path,
) -> Option<FailureReason> {
    match reply {
        Ok(r) if !r.ok && r.error == Some(agent_ipc::ErrorCode::Unauthorized) => {
            Some(FailureReason::Rejected)
        }
        Ok(_) => None,
        Err(_) => data_dir
            .join(agent_ipc::transport::IPC_DIR)
            .join(agent_ipc::transport::ENDPOINT_FILE)
            .is_file()
            .then_some(FailureReason::Unreachable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn parses_hook_arguments() {
        let a = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            parse_args(&a(&["claude-code", "--data-dir", "C:/x"])),
            Some(HookArgs {
                adapter: "claude-code".into(),
                data_dir: "C:/x".into()
            })
        );
        assert!(parse_args(&a(&["claude-code"])).is_none());
        assert!(parse_args(&a(&["claude-code", "--data-dir", ""])).is_none());
        assert!(parse_args(&a(&["claude-code", "--other", "x"])).is_none());
    }

    #[test]
    fn builds_valid_events_with_fresh_ids() {
        let hook = br#"{"session_id":"s-1","hook_event_name":"Notification","notification_type":"idle_prompt","message":"x"}"#;
        let now = datetime!(2026-11-05 10:00:00 UTC);
        let a = build_claude_code_event(hook, now).expect("event");
        let b = build_claude_code_event(hook, now).expect("event");
        let (ea, eb) = (
            parse_event(&a).expect("valid"),
            parse_event(&b).expect("valid"),
        );
        assert_ne!(ea.event_id, eb.event_id, "each hook run is a new event");
        assert_eq!(ea.event_type, agent_core::EventType::NeedsAttention);
        assert_eq!(ea.provider, "claude-code");
    }

    #[test]
    fn builds_codex_and_cursor_events_under_their_own_providers() {
        let now = datetime!(2026-11-06 10:00:00 UTC);
        let c = build_codex_event(
            br#"{"session_id":"s-1","hook_event_name":"PermissionRequest"}"#,
            now,
        )
        .expect("codex");
        let e = parse_event(&c).expect("valid");
        assert_eq!(
            (e.provider.as_str(), e.event_type),
            ("codex", agent_core::EventType::NeedsAttention)
        );
        let u = build_cursor_event(br#"{"conversation_id":"c-1","hook_event_name":"stop","status":"completed","user_email":"a@b.c"}"#, now).expect("cursor");
        assert!(!u.contains("a@b.c"));
        let e = parse_event(&u).expect("valid");
        assert_eq!(
            (
                e.provider.as_str(),
                e.event_type,
                e.provider_session_id.as_str()
            ),
            ("cursor", agent_core::EventType::TurnEnded, "c-1")
        );
        // The Claude Code entry point drops Cursor's payloads instead of mislabelling them.
        let from_cursor = br#"{"session_id":"s","hook_event_name":"Stop","cursor_version":"1.7"}"#;
        assert!(build_claude_code_event(from_cursor, now).is_none());
        // Each adapter reads only its own session key.
        assert!(
            build_cursor_event(br#"{"session_id":"s","hook_event_name":"stop"}"#, now).is_none()
        );
        assert!(
            build_codex_event(br#"{"conversation_id":"s","hook_event_name":"Stop"}"#, now)
                .is_none()
        );
    }

    #[test]
    fn builds_opencode_events_from_the_plugin_message_only() {
        let now = datetime!(2026-11-06 10:00:00 UTC);
        let out = build_opencode_event(
            br#"{"v":1,"event":"permission.asked","sessionId":"ses_1","path":"/secret","prompt":"SECRET"}"#,
            now,
        )
        .expect("opencode");
        assert!(!out.contains("SECRET") && !out.contains("/secret"));
        let e = parse_event(&out).expect("valid");
        assert_eq!(
            (
                e.provider.as_str(),
                e.event_type,
                e.provider_session_id.as_str()
            ),
            ("opencode", agent_core::EventType::NeedsAttention, "ses_1")
        );
        // OpenCode's own event shape is not accepted: only the plugin's message is.
        assert!(
            build_opencode_event(br#"{"type":"session.idle","data":{"sessionID":"s"}}"#, now)
                .is_none()
        );
        assert!(build_codex_event(br#"{"v":1,"event":"failed","sessionId":"s"}"#, now).is_none());
    }

    #[test]
    fn forwarder_accepts_the_opencode_argument_and_writes_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let args = vec![
            "opencode".to_owned(),
            "--data-dir".to_owned(),
            dir.path().to_string_lossy().into_owned(),
        ];
        let body = br#"{"v":1,"event":"turn_ended","sessionId":"s"}"#.to_vec();
        assert_eq!(
            run(&args, std::io::Cursor::new(body), OffsetDateTime::now_utc()),
            Outcome::AppNotRunning
        );
        assert_eq!(
            run(
                &args,
                std::io::Cursor::new(b"{\"v\":1,\"event\":\"nope\",\"sessionId\":\"s\"}".to_vec()),
                OffsetDateTime::now_utc()
            ),
            Outcome::Ignored
        );
    }

    #[test]
    fn forwarder_accepts_the_agent_argument_and_writes_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        for (adapter, body) in [
            (
                "codex",
                br#"{"session_id":"s","hook_event_name":"Stop"}"#.to_vec(),
            ),
            (
                "cursor",
                br#"{"conversation_id":"s","hook_event_name":"stop"}"#.to_vec(),
            ),
        ] {
            let args = vec![
                adapter.to_owned(),
                "--data-dir".to_owned(),
                dir.path().to_string_lossy().into_owned(),
            ];
            assert_eq!(
                run(&args, std::io::Cursor::new(body), OffsetDateTime::now_utc()),
                Outcome::AppNotRunning,
                "{adapter}"
            );
            let big = vec![b' '; (MAX_STDIN_BYTES + 10) as usize];
            assert_eq!(
                run(&args, std::io::Cursor::new(big), OffsetDateTime::now_utc()),
                Outcome::TooLarge,
                "{adapter}"
            );
        }
        let args = vec!["other".to_owned(), "--data-dir".to_owned(), "x".to_owned()];
        assert_eq!(
            run(
                &args,
                std::io::Cursor::new(b"{}".to_vec()),
                OffsetDateTime::now_utc()
            ),
            Outcome::BadArguments
        );
    }

    #[test]
    fn prompt_is_not_read_without_the_marker() {
        let dir = tempfile::tempdir().expect("tempdir");
        for adapter in ["codex", "cursor", "opencode"] {
            let ctx = prompt_context_for(dir.path(), adapter, b"{}", |_| {
                panic!("the prompt must not be read without consent")
            });
            assert!(ctx.is_none());
            let marker = dir.path().join(CAPTURE_DIR);
            std::fs::create_dir_all(&marker).expect("mkdir");
            std::fs::write(marker.join(adapter), b"").expect("marker");
            let got = prompt_context_for(dir.path(), adapter, b"{}", |_| {
                Some(EventContext {
                    intent: Some("x".into()),
                    ask: None,
                    title: None,
                })
            });
            assert!(got.is_some(), "{adapter} with marker");
        }
        // One adapter's marker never opens another's gate.
        let other = tempfile::tempdir().expect("tempdir");
        let m = other.path().join(CAPTURE_DIR);
        std::fs::create_dir_all(&m).expect("mkdir");
        std::fs::write(m.join("claude-code"), b"").expect("marker");
        assert!(prompt_context_for(other.path(), "codex", b"{}", |_| panic!("gate")).is_none());
        assert!(prompt_context_for(other.path(), "cursor", b"{}", |_| panic!("gate")).is_none());
        assert!(prompt_context_for(other.path(), "opencode", b"{}", |_| panic!("gate")).is_none());
    }

    #[test]
    fn drops_session_ids_the_app_would_reject() {
        let hook = "{\"session_id\":\"a\u{202e}b\",\"hook_event_name\":\"Stop\"}";
        assert!(build_claude_code_event(hook.as_bytes(), OffsetDateTime::now_utc()).is_none());
    }

    #[test]
    fn oversized_input_is_dropped_and_missing_app_fails_fast() {
        let dir = tempfile::tempdir().expect("tempdir");
        let args = vec![
            "claude-code".to_owned(),
            "--data-dir".to_owned(),
            dir.path().to_string_lossy().into_owned(),
        ];
        let big = vec![b' '; (MAX_STDIN_BYTES + 10) as usize];
        assert_eq!(
            run(&args, std::io::Cursor::new(big), OffsetDateTime::now_utc()),
            Outcome::TooLarge
        );
        let ok = br#"{"session_id":"s","hook_event_name":"Stop"}"#.to_vec();
        let started = std::time::Instant::now();
        assert_eq!(
            run(&args, std::io::Cursor::new(ok), OffsetDateTime::now_utc()),
            Outcome::AppNotRunning
        );
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    fn hook_args(dir: &Path) -> Vec<String> {
        vec![
            "claude-code".to_owned(),
            "--data-dir".to_owned(),
            dir.to_string_lossy().into_owned(),
        ]
    }

    const STOP: &[u8] = br#"{"session_id":"s","hook_event_name":"Stop"}"#;

    #[test]
    fn an_old_address_leaves_an_unreachable_marker_and_nothing_else() {
        // The stale private copy of decision 33: endpoint.json names a pipe that no longer
        // exists, beside a token.
        let dir = tempfile::tempdir().expect("tempdir");
        let ipc = dir.path().join("ipc");
        std::fs::create_dir(&ipc).expect("ipc");
        let old = agent_ipc::Endpoint::for_launch(dir.path()).expect("endpoint");
        std::fs::write(
            ipc.join("endpoint.json"),
            serde_json::to_vec(&old).expect("json"),
        )
        .expect("endpoint.json");
        std::fs::write(ipc.join("token"), "a".repeat(64)).expect("token");
        let started = std::time::Instant::now();
        assert_eq!(
            run(
                &hook_args(dir.path()),
                std::io::Cursor::new(STOP.to_vec()),
                OffsetDateTime::now_utc()
            ),
            Outcome::AppNotRunning
        );
        assert!(started.elapsed() < DEADLINE);
        let m = hook_failure::read(&ipc).expect("marker");
        assert_eq!(
            (m.adapter, m.reason),
            ("claude-code", FailureReason::Unreachable)
        );
        let text = std::fs::read_to_string(ipc.join(hook_failure::MARKER_FILE)).expect("text");
        assert!(!text.contains("\"s\"") && !text.contains(&*dir.path().to_string_lossy()));
        // Only the marker was added; the connection files are untouched.
        let mut names: Vec<String> = std::fs::read_dir(&ipc)
            .expect("dir")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["endpoint.json", "last-hook-failure.json", "token"]);
    }

    #[test]
    fn no_connection_files_leave_no_marker() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(dir.path().join("ipc")).expect("ipc");
        assert_eq!(
            run(
                &hook_args(dir.path()),
                std::io::Cursor::new(STOP.to_vec()),
                OffsetDateTime::now_utc()
            ),
            Outcome::AppNotRunning
        );
        assert!(hook_failure::read(&dir.path().join("ipc")).is_none());
        // And with no ipc directory at all, none is created.
        let empty = tempfile::tempdir().expect("tempdir");
        let _ = run(
            &hook_args(empty.path()),
            std::io::Cursor::new(STOP.to_vec()),
            OffsetDateTime::now_utc(),
        );
        assert!(!empty.path().join("ipc").exists());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_refused_token_leaves_a_rejected_marker() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sink = |_e: agent_core::AgentEvent,
                    _c: Option<EventContext>|
         -> Result<bool, agent_ipc::SinkError> { Ok(true) };
        let _running = agent_ipc::serve(
            dir.path(),
            std::sync::Arc::new(sink),
            agent_ipc::Config::default(),
        )
        .expect("serve");
        // The hook now holds an out-of-date token (the app keeps its own in memory).
        std::fs::write(dir.path().join("ipc").join("token"), "0".repeat(64)).expect("token");
        let args = hook_args(dir.path());
        let outcome = tokio::task::spawn_blocking(move || {
            run(
                &args,
                std::io::Cursor::new(STOP.to_vec()),
                OffsetDateTime::now_utc(),
            )
        })
        .await
        .expect("join");
        assert_eq!(outcome, Outcome::Rejected);
        let m = hook_failure::read(&dir.path().join("ipc")).expect("marker");
        assert_eq!(m.reason, FailureReason::Rejected);
    }

    #[test]
    fn stalled_stdin_is_cut_off_at_the_deadline() {
        struct Stall;
        impl Read for Stall {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                std::thread::sleep(Duration::from_secs(30));
                Ok(0)
            }
        }
        let args = vec![
            "claude-code".to_owned(),
            "--data-dir".to_owned(),
            "x".to_owned(),
        ];
        let started = std::time::Instant::now();
        assert_eq!(
            run(&args, Stall, OffsetDateTime::now_utc()),
            Outcome::TimedOut
        );
        assert!(started.elapsed() < DEADLINE + Duration::from_millis(500));
    }
}
