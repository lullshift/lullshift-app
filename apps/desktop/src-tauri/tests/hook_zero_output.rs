//! Decision 33: the hook now leaves a failure marker, and must still keep the zero-effect
//! contract (ADR-010): exit 0, nothing on stdout or stderr, within the deadline. This runs the
//! real executable in `hook` mode against a data folder with stale connection files (the case
//! that hid Claude Code sessions) and against one with none.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn run_hook(data_dir: &std::path::Path, stdin: &[u8]) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lullshift-desktop"))
        .arg("hook")
        .arg("claude-code")
        .arg("--data-dir")
        .arg(data_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the app in hook mode");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(stdin)
        .expect("write stdin");
    child.wait_with_output().expect("wait")
}

const STOP: &[u8] = br#"{"session_id":"s-1","hook_event_name":"Stop","cwd":"C:/secret"}"#;

#[test]
fn stale_connection_files_leave_a_marker_but_no_output() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ipc = dir.path().join("ipc");
    std::fs::create_dir(&ipc).expect("ipc");
    let address = if cfg!(windows) {
        r"\\.\pipe\lullshift-ipc-v1-00000000000000000000000000000000".to_owned()
    } else {
        ipc.join("gone.sock").to_string_lossy().into_owned()
    };
    let transport = if cfg!(windows) {
        "windows_named_pipe"
    } else {
        "unix_socket"
    };
    std::fs::write(
        ipc.join("endpoint.json"),
        serde_json::json!({"protocol": 1, "transport": transport, "address": address}).to_string(),
    )
    .expect("endpoint.json");
    std::fs::write(ipc.join("token"), "a".repeat(64)).expect("token");

    let started = Instant::now();
    let out = run_hook(dir.path(), STOP);
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(out.status.success(), "exit code {:?}", out.status.code());
    assert!(out.stdout.is_empty(), "stdout: {:?}", out.stdout);
    assert!(out.stderr.is_empty(), "stderr: {:?}", out.stderr);

    let marker =
        std::fs::read_to_string(ipc.join("last-hook-failure.json")).expect("failure marker");
    let v: serde_json::Value = serde_json::from_str(&marker).expect("json");
    let keys: Vec<&str> = v
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["adapter", "reason", "utc"]);
    assert_eq!(v["adapter"], "claude-code");
    assert_eq!(v["reason"], "unreachable");
    assert!(!marker.contains("s-1") && !marker.contains("secret"));

    // A second failure within the minute doesn't rewrite it, and still says nothing.
    let out = run_hook(dir.path(), STOP);
    assert!(out.status.success() && out.stdout.is_empty() && out.stderr.is_empty());
    assert_eq!(
        std::fs::read_to_string(ipc.join("last-hook-failure.json")).expect("marker"),
        marker
    );
}

#[test]
fn no_connection_files_no_marker_and_no_output() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = run_hook(dir.path(), STOP);
    assert!(out.status.success());
    assert!(out.stdout.is_empty() && out.stderr.is_empty());
    assert!(!dir.path().join("ipc").exists());
}
