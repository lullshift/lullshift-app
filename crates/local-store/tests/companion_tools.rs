//! Companion tool-call storage (Sprint 12, S12-T16): migration 10, per-thread lookups, usage.

use local_store::{
    CompanionProfile, MIGRATIONS, NewToolCall, NewUserMessage, Store, StoreError, latest_version,
    migrate, month_of,
};
use rusqlite::Connection;
use time::OffsetDateTime;
use time::macros::datetime;

const T0: OffsetDateTime = datetime!(2026-11-05 10:00 UTC);

fn profile() -> CompanionProfile {
    CompanionProfile {
        id: "p".to_owned(),
        kind: "openaiCompatible".to_owned(),
        label: "Profile".to_owned(),
        base_url: "http://localhost:11434/v1".to_owned(),
        model: "llama3".to_owned(),
        monthly_token_budget: Some(1000),
        is_default: true,
    }
}

fn user_msg(thread: i64) -> NewUserMessage<'static> {
    NewUserMessage {
        thread_id: thread,
        text: "hi",
        attached_context: None,
        scope_kind: "none",
        profile_id: "p",
        model: "llama3",
        host: "localhost:11434",
    }
}

fn tool<'a>(message_id: i64, status: &'a str, result: &'a str) -> NewToolCall<'a> {
    NewToolCall {
        message_id,
        round: 1,
        tool: "suggest_break",
        arguments: "{}",
        result_text: result,
        status,
        card_activity_id: Some("lullshift.tea-sort"),
        card_name: Some("Tea Sort"),
        card_minutes: Some(5),
        host: "localhost:11434",
    }
}

fn setup() -> (Store, i64, i64) {
    let mut s = Store::open_in_memory().expect("store");
    s.companion_save_profile(&profile(), T0).expect("p");
    let t = s.companion_new_thread(None, T0).expect("t");
    let m = s
        .companion_add_user_message(&user_msg(t.id), T0)
        .expect("m");
    (s, t.id, m)
}

#[test]
fn version_9_database_upgrades_to_10_with_backup_and_keeps_companion_data() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("l.db");
    {
        let conn = Connection::open(&path).expect("raw");
        migrate(&conn, &MIGRATIONS[..9], None).expect("v9");
        conn.execute(
            "INSERT INTO companion_threads (title, created_utc, updated_utc) VALUES ('T', 't', 't')",
            [],
        )
        .expect("thread");
        conn.execute(
            "INSERT INTO companion_messages (thread_id, role, text, status, host, created_utc)
             VALUES (1, 'user', 'old message', 'sent', 'h', 't')",
            [],
        )
        .expect("message");
    }
    let (s, report) = Store::open(&path).expect("upgrade");
    assert_eq!(
        (report.from_version, report.to_version),
        (9, latest_version())
    );
    let backup = report.backup.expect("backup before upgrade");
    assert_eq!(
        s.companion_messages(1).expect("msgs")[0].text,
        "old message"
    );
    assert!(s.companion_tool_calls(1).expect("none yet").is_empty());
    let b = Connection::open(&backup).expect("backup");
    let n: i64 = b
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = 'companion_tool_calls'",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(n, 0, "backup is the v9 file");
}

#[test]
fn failed_version_10_migration_rolls_back_to_9() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("l.db");
    {
        let conn = Connection::open(&path).expect("raw");
        migrate(&conn, &MIGRATIONS[..9], None).expect("v9");
        conn.execute("CREATE TABLE companion_tool_calls (x INTEGER)", [])
            .expect("clash");
    }
    assert!(matches!(
        Store::open(&path),
        Err(StoreError::Migration { version: 10, .. })
    ));
    let conn = Connection::open(&path).expect("reopen");
    let v: i64 = conn
        .query_row("SELECT max(version) FROM schema_version", [], |r| r.get(0))
        .expect("version");
    assert_eq!(v, 9, "left at the last good version");
}

#[test]
fn tool_calls_are_stored_per_thread_with_their_host_and_deleted_with_the_thread() {
    let (s, thread, m) = setup();
    s.companion_add_tool_call(&tool(m, "approved", "Tea Sort, about 5 minutes"), T0)
        .expect("a");
    s.companion_add_tool_call(&tool(m, "declined", "The user declined."), T0)
        .expect("b");
    let calls = s.companion_tool_calls(thread).expect("calls");
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].status, "approved");
    assert_eq!(calls[0].host.as_deref(), Some("localhost:11434"));
    assert_eq!(calls[0].card_name.as_deref(), Some("Tea Sort"));
    assert_eq!(calls[1].result_text, "The user declined.");
    assert!(matches!(
        s.companion_add_tool_call(&tool(m, "maybe", "x"), T0),
        Err(StoreError::InvalidInput(_))
    ));
    let mut bad = tool(m, "approved", "x");
    bad.tool = "run_shell";
    assert!(s.companion_add_tool_call(&bad, T0).is_err());
    assert!(
        s.companion_add_tool_call(&tool(9999, "approved", "x"), T0)
            .is_err()
    );
    s.companion_delete_thread(thread).expect("delete");
    assert!(s.companion_tool_calls(thread).expect("after").is_empty());
}

#[test]
fn a_multi_round_reply_books_only_the_last_call_but_shows_the_totals() {
    let (mut s, thread, m) = setup();
    // The first call was booked as it finished.
    s.companion_record_usage("p", T0, 10, 5).expect("round 1");
    s.companion_complete_send_booked(m, "answer", 30, 12, 20, 7, T0)
        .expect("done");
    let msgs = s.companion_messages(thread).expect("msgs");
    assert_eq!(
        (msgs[0].input_tokens, msgs[0].output_tokens),
        (Some(30), Some(12))
    );
    let u = s.companion_usage(&month_of(T0)).expect("usage");
    assert_eq!((u[0].input_tokens, u[0].output_tokens), (30, 12));
}
