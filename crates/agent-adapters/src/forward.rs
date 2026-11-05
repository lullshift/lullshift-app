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

use crate::claude_code;

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
    let event = match args.adapter.as_str() {
        claude_code::ADAPTER_ID => match build_claude_code_event(&buf, started) {
            Some(e) => e,
            None => return Outcome::Ignored,
        },
        _ => return Outcome::BadArguments,
    };
    send(&args.data_dir, &event)
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

fn send(data_dir: &Path, event_json: &str) -> Outcome {
    let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return Outcome::Failed;
    };
    let reply = rt.block_on(agent_ipc::send_event(
        data_dir,
        event_json,
        DEADLINE.saturating_sub(Duration::from_millis(250)),
    ));
    match reply {
        Ok(r) if r.ok && r.duplicate => Outcome::Duplicate,
        Ok(r) if r.ok => Outcome::Sent,
        Ok(_) => Outcome::Rejected,
        Err(agent_ipc::ClientError::NotRunning) => Outcome::AppNotRunning,
        Err(_) => Outcome::Failed,
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
