//! Migration 20 (ADR-020 amendment "Thinking"): the model's reasoning summary stored with the
//! assistant reply. Upgrade with data, backup, rollback and recovery, idempotence, caps and the
//! API.

use local_store::{
    MAX_THINKING_BYTES, MIGRATIONS, Migration, NewUserMessage, Store, StoreError, clean_thinking,
    latest_version, migrate,
};
use rusqlite::Connection;
use time::OffsetDateTime;
use time::macros::datetime;

const T0: OffsetDateTime = datetime!(2026-11-08 10:00 UTC);

fn before_20() -> Vec<Migration> {
    MIGRATIONS
        .iter()
        .copied()
        .filter(|m| m.version < 20)
        .collect()
}

fn version(conn: &Connection) -> i64 {
    conn.query_row("SELECT max(version) FROM schema_version", [], |r| r.get(0))
        .expect("version")
}

fn has_column(conn: &Connection, column: &str) -> bool {
    conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM pragma_table_info('companion_messages') WHERE name = ?1)",
        [column],
        |r| r.get(0),
    )
    .expect("pragma")
}

fn seed_old(path: &std::path::Path) {
    let conn = Connection::open(path).expect("raw");
    migrate(&conn, &before_20(), None).expect("old schema");
    conn.execute_batch(
        "INSERT INTO companion_profiles
           (id, kind, label, base_url, monthly_token_budget, is_default, created_utc, updated_utc)
         VALUES ('p-a', 'anthropic', 'Anthropic', 'https://api.anthropic.com', NULL, 1,
                 '2026-11-01T10:00:00Z', '2026-11-01T10:00:00Z');
         INSERT INTO companion_threads (id, title, profile_id, model, created_utc, updated_utc)
         VALUES (1, 'Plans', 'p-a', 'm', '2026-11-03T10:00:00Z', '2026-11-03T10:00:00Z');
         INSERT INTO companion_messages
           (thread_id, role, text, scope_kind, status, profile_id, model, host, created_utc)
         VALUES
           (1, 'user', 'hi', 'none', 'sent', 'p-a', 'm', 'api.anthropic.com',
            '2026-11-03T10:00:00Z'),
           (1, 'assistant', 'hello', 'none', 'sent', 'p-a', 'm', 'api.anthropic.com',
            '2026-11-03T10:00:01Z');",
    )
    .expect("seed");
}

#[test]
fn old_database_upgrades_with_backup_and_keeps_every_row() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("l.db");
    seed_old(&path);
    let old = version(&Connection::open(&path).expect("raw"));
    assert!(old < 20);

    let (s, report) = Store::open(&path).expect("open");
    assert_eq!(report.from_version, old);
    assert_eq!(report.to_version, latest_version());
    assert!(latest_version() >= 20);

    let msgs = s.companion_messages(1).expect("msgs");
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[1].text, "hello");
    assert!(
        msgs.iter()
            .all(|m| m.thinking.is_none() && m.thinking_ms.is_none())
    );

    let conn = Connection::open(&path).expect("raw");
    assert!(has_column(&conn, "thinking") && has_column(&conn, "thinking_ms"));
    let bak = report.backup.expect("backup before upgrading");
    let b = Connection::open(&bak).expect("open backup");
    assert_eq!(version(&b), old);
    assert!(!has_column(&b, "thinking"));
}

#[test]
fn upgrading_twice_is_a_no_op() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("l.db");
    seed_old(&path);
    drop(Store::open(&path).expect("first open"));
    let (_, report) = Store::open(&path).expect("second open");
    assert_eq!(report.from_version, report.to_version);
    assert_eq!(report.backup, None);
}

#[test]
fn failed_upgrade_rolls_back_and_a_later_open_recovers() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("l.db");
    seed_old(&path);
    let old;
    {
        let conn = Connection::open(&path).expect("raw");
        old = version(&conn);
        // A clash in the second statement: the first column was already added inside the
        // transaction and must be rolled back with it.
        conn.execute_batch("ALTER TABLE companion_messages ADD COLUMN thinking_ms INTEGER")
            .expect("clash");
    }
    assert!(matches!(
        Store::open(&path),
        Err(StoreError::Migration { version: 20, .. })
    ));
    let conn = Connection::open(&path).expect("reopen");
    assert_eq!(version(&conn), old, "left at the last good version");
    assert!(!has_column(&conn, "thinking"), "no half-applied step");
    let n: i64 = conn
        .query_row("SELECT count(*) FROM companion_messages", [], |r| r.get(0))
        .expect("count");
    assert_eq!(n, 2, "messages kept");

    conn.execute_batch("ALTER TABLE companion_messages DROP COLUMN thinking_ms")
        .expect("drop clash");
    drop(conn);
    let (s, _) = Store::open(&path).expect("recovered");
    assert_eq!(s.companion_messages(1).expect("msgs").len(), 2);
}

#[test]
fn an_older_build_ignores_the_new_columns() {
    // Rollback: a database already at 20 still answers the pre-20 queries (the columns are
    // nullable and nothing older selects them).
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("l.db");
    seed_old(&path);
    drop(Store::open(&path).expect("open"));
    let conn = Connection::open(&path).expect("raw");
    let n: i64 = conn
        .query_row(
            "SELECT count(*) FROM companion_messages WHERE role = 'assistant' AND text = 'hello'",
            [],
            |r| r.get(0),
        )
        .expect("old-style query");
    assert_eq!(n, 1);
    conn.execute(
        "INSERT INTO companion_messages (thread_id, role, text, scope_kind, status, created_utc)
         VALUES (1, 'assistant', 'old build reply', 'none', 'sent', '2026-11-08T10:00:00Z')",
        [],
    )
    .expect("old-style insert");
}

fn open_new(dir: &std::path::Path) -> (Store, i64) {
    let path = dir.join("n.db");
    let conn = Connection::open(&path).expect("raw");
    migrate(&conn, MIGRATIONS, None).expect("schema");
    conn.execute_batch(
        "INSERT INTO companion_profiles
           (id, kind, label, base_url, monthly_token_budget, is_default, created_utc, updated_utc)
         VALUES ('p-a', 'anthropic', 'Anthropic', 'https://api.anthropic.com', NULL, 1,
                 '2026-11-01T10:00:00Z', '2026-11-01T10:00:00Z');
         INSERT INTO companion_models (profile_id, model, label, is_default, created_utc)
         VALUES ('p-a', 'm', NULL, 1, '2026-11-01T10:00:00Z');",
    )
    .expect("seed");
    drop(conn);
    let (mut s, _) = Store::open(&path).expect("open");
    let t = s.companion_new_thread(None, T0).expect("thread");
    let id = s
        .companion_add_user_message(
            &NewUserMessage {
                thread_id: t.id,
                text: "hello",
                attached_context: None,
                scope_kind: "none",
                profile_id: "p-a",
                model: "m",
                host: "api.anthropic.com",
            },
            T0,
        )
        .expect("add");
    (s, id)
}

#[test]
fn thinking_is_stored_with_the_reply_apart_from_its_text() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut s, id) = open_new(dir.path());
    let reply = s
        .companion_complete_send_thinking(
            id,
            "the reply",
            (5, 7),
            (5, 7),
            T0,
            false,
            Some(("Weigh the\u{7} options.\u{202e}\nSecond line", 12_345)),
            &|| true,
        )
        .expect("complete")
        .expect("stored");
    assert_eq!(reply.text, "the reply");
    // Cleaned like reply text.
    assert_eq!(
        reply.thinking.as_deref(),
        Some("Weigh the options.\nSecond line")
    );
    assert_eq!(reply.thinking_ms, Some(12_345));
    let msgs = s.companion_messages(reply.thread_id).expect("msgs");
    assert!(msgs[0].thinking.is_none(), "the user message has none");
    assert_eq!(msgs[1].thinking, reply.thinking);
    assert!(!msgs[1].text.contains("Weigh"));
}

#[test]
fn replies_without_thinking_store_none() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut s, id) = open_new(dir.path());
    let reply = s
        .companion_complete_send_thinking(
            id,
            "r",
            (1, 1),
            (1, 1),
            T0,
            false,
            Some(("  \u{200b}\n ", 10)),
            &|| true,
        )
        .expect("complete")
        .expect("stored");
    assert_eq!((reply.thinking, reply.thinking_ms), (None, None));
    let dir2 = tempfile::tempdir().expect("dir");
    let (mut s2, id2) = open_new(dir2.path());
    let r2 = s2
        .companion_complete_send_if(id2, "plain", 1, 1, 1, 1, T0, &|| true)
        .expect("complete")
        .expect("stored");
    assert_eq!((r2.thinking, r2.thinking_ms), (None, None));
}

#[test]
fn thinking_is_capped_on_a_character_boundary_with_a_note() {
    // 'é' is two bytes, so the cap falls inside a character for an odd limit.
    let big = "é".repeat(MAX_THINKING_BYTES);
    let t = clean_thinking(&big).expect("kept");
    assert!(t.len() < MAX_THINKING_BYTES + 64);
    assert!(t.ends_with("[Thinking cut here.]"));
    assert!(t.starts_with("éé"));
    let dir = tempfile::tempdir().expect("dir");
    let (mut s, id) = open_new(dir.path());
    let reply = s
        .companion_complete_send_thinking(
            id,
            "r",
            (1, 1),
            (1, 1),
            T0,
            false,
            Some((&big, 1)),
            &|| true,
        )
        .expect("complete")
        .expect("stored");
    assert_eq!(reply.thinking.as_deref(), Some(t.as_str()));
    // A short one is untouched.
    assert_eq!(clean_thinking("short").as_deref(), Some("short"));
}

#[test]
fn the_database_refuses_oversized_thinking() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("n.db");
    let conn = Connection::open(&path).expect("raw");
    migrate(&conn, MIGRATIONS, None).expect("schema");
    conn.execute_batch(
        "INSERT INTO companion_threads (id, title, created_utc, updated_utc)
         VALUES (1, 't', '2026-11-08T10:00:00Z', '2026-11-08T10:00:00Z');",
    )
    .expect("thread");
    let big = "x".repeat(66_001);
    let r = conn.execute(
        "INSERT INTO companion_messages (thread_id, role, text, scope_kind, status, created_utc, thinking)
         VALUES (1, 'assistant', 'r', 'none', 'sent', '2026-11-08T10:00:00Z', ?1)",
        [&big],
    );
    assert!(r.is_err());
}
