//! Companion tool-call storage (Sprint 12, S12-T16): migration 10, per-thread lookups, usage.

use local_store::{
    CLEARED_RESULT_TEXT, CompanionProfile, MIGRATIONS, NewToolCall, NewUserMessage, Store,
    StoreError, latest_version, migrate, month_of,
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

#[test]
fn a_cancelled_send_stores_nothing_even_if_the_user_message_exists() {
    // L4: the request was cancelled between its last check and the store transaction.
    let (mut s, thread, m) = setup();
    let none = s
        .companion_complete_send_if(m, "late reply", 5, 5, 5, 5, T0, &|| false)
        .expect("ok");
    assert!(none.is_none());
    let msgs = s.companion_messages(thread).expect("msgs");
    assert_eq!(msgs.len(), 1, "no assistant message was stored");
    assert_eq!(msgs[0].status, "pending", "the user message is untouched");
    let used = s.companion_usage(&month_of(T0)).expect("usage");
    assert!(
        used.iter()
            .all(|u| u.input_tokens == 0 && u.output_tokens == 0)
    );
    // A reply that is allowed goes through.
    let some = s
        .companion_complete_send_if(m, "reply", 5, 5, 5, 5, T0, &|| true)
        .expect("ok");
    assert!(some.is_some());
    assert_eq!(s.companion_messages(thread).expect("msgs").len(), 2);
}

#[test]
fn clearing_overview_results_keeps_other_lookups_and_the_rows() {
    // L2: session overview text can hold a label from captured task text.
    let (s, thread, m) = setup();
    let mut ov = tool(
        m,
        "approved",
        "claude-code: working (label: SECRET-PROMPT-LINE)",
    );
    ov.tool = "session_overview";
    ov.card_activity_id = None;
    ov.card_name = None;
    ov.card_minutes = None;
    s.companion_add_tool_call(&ov, T0).expect("overview");
    s.companion_add_tool_call(&tool(m, "approved", "Tea Sort, about 5 minutes"), T0)
        .expect("break");
    assert_eq!(s.companion_clear_overview_results(None).expect("clear"), 1);
    let calls = s.companion_tool_calls(thread).expect("calls");
    assert_eq!(calls.len(), 2, "rows stay");
    assert_eq!(calls[0].result_text, CLEARED_RESULT_TEXT);
    assert!(!calls[0].result_text.contains("SECRET"));
    assert_eq!(calls[1].result_text, "Tea Sort, about 5 minutes");
    // Clearing again changes nothing.
    assert_eq!(s.companion_clear_overview_results(None).expect("again"), 0);
}

#[test]
fn clearing_overview_results_by_age_leaves_newer_ones() {
    let (s, thread, m) = setup();
    for (when, text) in [
        (T0 - time::Duration::days(40), "old SECRET"),
        (T0 - time::Duration::days(1), "new text"),
    ] {
        let mut ov = tool(m, "approved", text);
        ov.tool = "session_overview";
        ov.card_activity_id = None;
        ov.card_name = None;
        ov.card_minutes = None;
        s.companion_add_tool_call(&ov, when).expect("add");
    }
    let cutoff = T0 - time::Duration::days(30);
    assert_eq!(
        s.companion_clear_overview_results(Some(cutoff))
            .expect("clear"),
        1
    );
    let calls = s.companion_tool_calls(thread).expect("calls");
    assert_eq!(calls[0].result_text, CLEARED_RESULT_TEXT);
    assert_eq!(calls[1].result_text, "new text");
}

#[test]
fn clearing_attached_session_context_leaves_message_text_and_other_scopes() {
    let mut s = Store::open_in_memory().expect("store");
    s.companion_save_profile(&profile(), T0).expect("p");
    let t = s.companion_new_thread(None, T0).expect("t");
    let mut ids = Vec::new();
    for (kind, ctx) in [
        ("sessionSummary", Some("INTENT: SECRET")),
        ("selectedNotes", Some("my notes")),
        ("none", None),
    ] {
        let mut m = user_msg(t.id);
        m.scope_kind = kind;
        m.attached_context = ctx;
        ids.push(s.companion_add_user_message(&m, T0).expect("m"));
    }
    assert_eq!(s.companion_clear_attached_session_context().expect("c"), 1);
    let msgs = s.companion_messages(t.id).expect("msgs");
    assert_eq!(msgs[0].attached_context, None);
    assert_eq!(msgs[0].text, "hi");
    assert_eq!(msgs[1].attached_context.as_deref(), Some("my notes"));
}
