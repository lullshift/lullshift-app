//! S05-T11 / S05-A06: the session title read from Claude Code's transcript (ADR-013, "Session
//! titles"). Fixture files in temp dirs only; no real transcript is ever read.

use std::cell::Cell;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use agent_adapters::claude_code::title::{TAIL_BYTES, read_title};
use agent_adapters::forward::{CAPTURE_DIR, Outcome, context_for, run};
use agent_core::AgentEvent;
use agent_ipc::{Config, EventContext, SinkError, serve};
use time::OffsetDateTime;

const SID: &str = "0b1c2d3e-aaaa-bbbb-cccc-1234567890ab";

fn title_line(t: &str) -> String {
    format!(
        "{{\"type\":\"custom-title\",\"customTitle\":{},\"sessionId\":\"{SID}\"}}\n",
        serde_json::to_string(t).expect("json")
    )
}

fn other_line(n: usize) -> String {
    format!("{{\"type\":\"user\",\"message\":{{\"content\":\"line {n}\"}}}}\n")
}

fn transcript(dir: &Path, content: &str) -> PathBuf {
    let p = dir.join(format!("{SID}.jsonl"));
    std::fs::write(&p, content).expect("write");
    p
}

fn read(content: &str) -> Option<String> {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = transcript(dir.path(), content);
    read_title(&p, SID)
}

#[test]
fn reads_a_title() {
    let c = format!("{}{}", other_line(1), title_line("Fix the login bug"));
    assert_eq!(read(&c).as_deref(), Some("Fix the login bug"));
}

#[test]
fn the_last_title_wins() {
    let c = format!(
        "{}{}{}{}",
        title_line("first"),
        other_line(1),
        title_line("second"),
        title_line("third")
    );
    assert_eq!(read(&c).as_deref(), Some("third"));
}

#[test]
fn no_title_record_means_no_title() {
    assert_eq!(read(&format!("{}{}", other_line(1), other_line(2))), None);
    assert_eq!(read(""), None);
    // AI-generated titles are not stored in the transcript; other record types are ignored.
    assert_eq!(
        read(
            "{\"type\":\"ai-title\",\"customTitle\":\"nope\"}\n{\"type\":\"summary\",\"summary\":\"x\"}\n"
        ),
        None
    );
}

#[test]
fn malformed_lines_are_skipped_not_fatal() {
    let c = format!(
        "{}not json at all custom-title\n{{\"type\":\"custom-title\",\"customTitle\":\n{{}}\n",
        title_line("kept")
    );
    assert_eq!(read(&c).as_deref(), Some("kept"));
    assert_eq!(
        read("{\"type\":\"custom-title\",\"customTitle\":\"cut off"),
        None
    );
    assert_eq!(read("\u{0}\u{1}\u{2}custom-title\n\r\n\n"), None);
}

#[test]
fn a_non_string_title_is_not_a_title() {
    for bad in ["5", "null", "[\"a\"]", "{\"x\":1}", "true"] {
        let c = format!("{{\"type\":\"custom-title\",\"customTitle\":{bad}}}\n");
        assert_eq!(read(&c), None, "{bad}");
    }
    // The bad record is ignored and the earlier valid one still counts.
    let c = format!(
        "{}{{\"type\":\"custom-title\",\"customTitle\":7}}\n",
        title_line("good")
    );
    assert_eq!(read(&c).as_deref(), Some("good"));
    // The record type must match exactly.
    assert_eq!(
        read(
            "{\"type\":\"custom-titles\",\"customTitle\":\"x\"}\n{\"type\":5,\"customTitle\":\"x\"}\n"
        ),
        None
    );
}

#[test]
fn long_titles_are_cut_to_120_characters_on_one_line() {
    let long = "é".repeat(500);
    let t = read(&title_line(&long)).expect("title");
    assert_eq!(t.chars().count(), 120);
    let multi = read(&title_line("  one\n\ttwo   three\r\nfour  ")).expect("title");
    assert_eq!(multi, "one two three four");
    // Blank after cleaning is no title.
    assert_eq!(read(&title_line(" \n\t ")), None);
}

#[test]
fn control_bidi_and_invisible_characters_are_removed() {
    let t = read(&title_line("a\u{202e}b\u{200b}c\u{0007}d")).expect("title");
    assert_eq!(t, "abcd");
}

#[test]
fn secrets_in_the_title_are_hidden() {
    let key = concat!("sk-", "abcd1234efgh5678");
    let t = read(&title_line(&format!("deploy with {key} now"))).expect("title");
    assert!(!t.contains(key) && t.contains("[hidden]"), "{t}");
    let pw = concat!("pass", "word=hunter2hunter2");
    let t = read(&title_line(&format!("login {pw}"))).expect("title");
    assert!(!t.contains("hunter2"), "{t}");
    // The cut comes after redaction, so a secret near the limit can't be half kept.
    let padded = format!("{} {key}", "x".repeat(110));
    let t = read(&title_line(&padded)).expect("title");
    assert!(!t.contains("sk-") && t.chars().count() <= 120, "{t}");
}

#[test]
fn only_the_last_256_kib_are_read() {
    let line = other_line(0);
    let filler = line.repeat((TAIL_BYTES as usize / line.len()) + 50);
    // Title only before the window: not found.
    let c = format!("{}{filler}", title_line("too early"));
    assert!(c.len() as u64 > TAIL_BYTES + 1000);
    assert_eq!(read(&c), None);
    // Title inside the window of a large file: found.
    let c = format!("{filler}{}{}", title_line("recent"), other_line(1));
    assert_eq!(read(&c).as_deref(), Some("recent"));
    // The cut-off first line of the window is never parsed, even if the rest of it is a full
    // title record: a window that starts inside a title line yields no title from that line.
    let early = title_line("hidden by the cut");
    let c = format!("{early}{}", "a\n".repeat(TAIL_BYTES as usize / 2 - 10));
    assert!(c.len() as u64 > TAIL_BYTES);
    assert_eq!(read(&c), None);
}

#[test]
fn oversize_lines_are_never_parsed() {
    let big = format!(
        "{{\"type\":\"custom-title\",\"customTitle\":\"{}\"}}\n",
        "a".repeat(40_000)
    );
    assert_eq!(read(&big), None);
}

#[test]
fn path_rules() {
    let dir = tempfile::tempdir().expect("tempdir");
    let good = transcript(dir.path(), &title_line("ok"));
    assert_eq!(read_title(&good, SID).as_deref(), Some("ok"));
    // Wrong file name for this session id.
    assert_eq!(read_title(&good, "another-session"), None);
    let other = dir.path().join("notes.jsonl");
    std::fs::write(&other, title_line("ok")).expect("write");
    assert_eq!(read_title(&other, SID), None);
    assert_eq!(
        read_title(&dir.path().join(format!("{SID}.json")), SID),
        None
    );
    // Relative paths are refused.
    assert_eq!(read_title(Path::new(&format!("{SID}.jsonl")), SID), None);
    assert_eq!(
        read_title(Path::new(&format!("./x/{SID}.jsonl")), SID),
        None
    );
    // Missing file.
    let gone = dir.path().join("sub").join(format!("{SID}.jsonl"));
    assert_eq!(read_title(&gone, SID), None);
    // A directory with the right name is not a file.
    let d = dir.path().join("d");
    std::fs::create_dir_all(d.join(format!("{SID}.jsonl"))).expect("mkdir");
    assert_eq!(read_title(&d.join(format!("{SID}.jsonl")), SID), None);
    assert_eq!(read_title(&good, ""), None);
}

#[test]
fn a_symlink_is_not_followed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let real = dir.path().join("real.jsonl");
    std::fs::write(&real, title_line("via link")).expect("write");
    let link = dir.path().join(format!("{SID}.jsonl"));
    #[cfg(unix)]
    std::os::unix::fs::symlink(&real, &link).expect("symlink");
    #[cfg(windows)]
    if std::os::windows::fs::symlink_file(&real, &link).is_err() {
        // Creating symlinks needs Developer Mode or elevation on Windows. Skipped there; the
        // check itself (symlink_metadata + reparse-point attribute) is covered on Unix.
        eprintln!("skipped: cannot create a symlink without privileges");
        return;
    }
    assert_eq!(read_title(&link, SID), None);
}

#[test]
fn capture_off_never_opens_the_transcript() {
    let dir = tempfile::tempdir().expect("tempdir");
    let payload = format!(
        "{{\"session_id\":\"{SID}\",\"transcript_path\":{},\"hook_event_name\":\"Stop\"}}",
        serde_json::to_string(&dir.path().join(format!("{SID}.jsonl")).to_string_lossy())
            .expect("json")
    );
    let called = Cell::new(false);
    let ctx = context_for(dir.path(), "claude-code", payload.as_bytes(), |_, _| {
        called.set(true);
        Some("x".into())
    });
    assert_eq!(ctx, None);
    assert!(!called.get(), "the reader must not run without the marker");

    // With the marker the reader gets exactly the path and session id from the hook.
    let m = dir.path().join(CAPTURE_DIR);
    std::fs::create_dir_all(&m).expect("mkdir");
    std::fs::write(m.join("claude-code"), b"").expect("marker");
    let seen = Cell::new(false);
    let ctx = context_for(dir.path(), "claude-code", payload.as_bytes(), |p, s| {
        seen.set(p.ends_with(format!("{SID}.jsonl")) && s == SID);
        Some("Named".into())
    });
    assert!(seen.get());
    assert_eq!(
        ctx,
        Some(EventContext {
            title: Some("Named".into()),
            ..EventContext::default()
        })
    );
    // Other adapters never match the claude-code marker.
    assert_eq!(
        context_for(dir.path(), "cursor", payload.as_bytes(), |_, _| Some(
            "x".into()
        )),
        None
    );
}

type Seen = Arc<Mutex<Vec<Option<EventContext>>>>;

fn args(dir: &Path) -> Vec<String> {
    vec![
        "claude-code".into(),
        "--data-dir".into(),
        dir.to_string_lossy().into_owned(),
    ]
}

#[tokio::test(flavor = "multi_thread")]
async fn the_title_travels_with_the_event_only_with_consent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let seen: Seen = Arc::default();
    let s2 = Arc::clone(&seen);
    let sink = move |_e: AgentEvent, c: Option<EventContext>| -> Result<bool, SinkError> {
        s2.lock().map_err(|_| SinkError)?.push(c);
        Ok(true)
    };
    let _running = serve(dir.path(), Arc::new(sink), Config::default()).expect("serve");
    let tdir = tempfile::tempdir().expect("tempdir");
    let path = transcript(
        tdir.path(),
        &format!("{}{}", other_line(1), title_line("My task")),
    );
    let payload = format!(
        "{{\"session_id\":\"{SID}\",\"transcript_path\":{},\"hook_event_name\":\"PostToolUse\",\"tool_name\":\"Bash\"}}",
        serde_json::to_string(&path.to_string_lossy()).expect("json")
    )
    .into_bytes();

    let go = |payload: Vec<u8>| {
        let a = args(dir.path());
        tokio::task::spawn_blocking(move || {
            run(&a, Cursor::new(payload), OffsetDateTime::now_utc())
        })
    };
    // No marker: no context at all.
    assert_eq!(go(payload.clone()).await.expect("join"), Outcome::Sent);
    // Marker: the title arrives, nothing else.
    let m = dir.path().join(CAPTURE_DIR);
    std::fs::create_dir_all(&m).expect("mkdir");
    std::fs::write(m.join("claude-code"), b"").expect("marker");
    assert_eq!(go(payload.clone()).await.expect("join"), Outcome::Sent);
    // A missing transcript still sends the event and exits cleanly.
    std::fs::remove_file(&path).expect("rm");
    assert_eq!(go(payload).await.expect("join"), Outcome::Sent);

    let got = seen.lock().expect("lock").clone();
    assert_eq!(
        got,
        vec![
            None,
            Some(EventContext {
                title: Some("My task".into()),
                ..EventContext::default()
            }),
            None
        ]
    );
}
