//! Migration 19 (ADR-020 amendment): the provider's own words kept apart from the fixed error
//! message, and the "arrived all at once" flag on a reply. Upgrade with data, backup, rollback and
//! recovery, idempotence, and the API.

use local_store::{
    MIGRATIONS, Migration, NewUserMessage, Store, StoreError, latest_version, migrate,
};
use rusqlite::Connection;
use time::OffsetDateTime;
use time::macros::datetime;

const T0: OffsetDateTime = datetime!(2026-11-07 10:00 UTC);

fn before_19() -> Vec<Migration> {
    MIGRATIONS
        .iter()
        .copied()
        .filter(|m| m.version < 19)
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

/// A pre-19 database with a failed message (fixed text only) and a reply.
fn seed_old(path: &std::path::Path) {
    let conn = Connection::open(path).expect("raw");
    migrate(&conn, &before_19(), None).expect("old schema");
    conn.execute_batch(
        "INSERT INTO companion_profiles
           (id, kind, label, base_url, monthly_token_budget, is_default, created_utc, updated_utc)
         VALUES ('p-a', 'openai', 'OpenAI', 'https://api.openai.com/v1', NULL, 1,
                 '2026-11-01T10:00:00Z', '2026-11-01T10:00:00Z');
         INSERT INTO companion_threads (id, title, profile_id, model, created_utc, updated_utc)
         VALUES (1, 'Plans', 'p-a', 'm', '2026-11-03T10:00:00Z', '2026-11-03T10:00:00Z');
         INSERT INTO companion_messages
           (thread_id, role, text, scope_kind, status, error_kind, error_message, profile_id, model,
            host, created_utc)
         VALUES
           (1, 'user', 'hi', 'none', 'failed', 'providerError',
            'The provider didn''t accept the request (HTTP 400). Check the model name.',
            'p-a', 'm', 'api.openai.com', '2026-11-03T10:00:00Z'),
           (1, 'assistant', 'hello', 'none', 'sent', NULL, NULL, 'p-a', 'm', 'api.openai.com',
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
    assert!(old < 19);

    let (s, report) = Store::open(&path).expect("open");
    assert_eq!(report.from_version, old);
    assert_eq!(report.to_version, latest_version());
    assert!(latest_version() >= 19);

    let msgs = s.companion_messages(1).expect("msgs");
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].error_kind.as_deref(), Some("providerError"));
    assert!(
        msgs[0]
            .error_message
            .as_deref()
            .unwrap()
            .contains("HTTP 400")
    );
    // Old rows: no provider words, not flagged.
    assert_eq!(
        (
            msgs[0].error_reason_by.clone(),
            msgs[0].error_reason.clone()
        ),
        (None, None)
    );
    assert!(!msgs[0].no_stream && !msgs[1].no_stream);

    let conn = Connection::open(&path).expect("raw");
    for c in ["error_reason_by", "error_reason", "no_stream"] {
        assert!(has_column(&conn, c), "{c}");
    }
    // The backup is the old database.
    let bak = report.backup.expect("backup before upgrading");
    let b = Connection::open(&bak).expect("open backup");
    assert_eq!(version(&b), old);
    assert!(!has_column(&b, "error_reason"));
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
    let conn = Connection::open(&path).expect("raw");
    let again = migrate(&conn, MIGRATIONS, None).expect("again");
    assert_eq!(again.from_version, again.to_version);
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
        // A clash in the third statement of the step: the first two columns were already added
        // inside the transaction and must be rolled back with it.
        conn.execute_batch("ALTER TABLE companion_messages ADD COLUMN no_stream INTEGER")
            .expect("clash");
    }
    assert!(matches!(
        Store::open(&path),
        Err(StoreError::Migration { version: 19, .. })
    ));
    let conn = Connection::open(&path).expect("reopen");
    assert_eq!(version(&conn), old, "left at the last good version");
    assert!(!has_column(&conn, "error_reason"), "no half-applied step");
    assert!(
        !has_column(&conn, "error_reason_by"),
        "no half-applied step"
    );
    let n: i64 = conn
        .query_row("SELECT count(*) FROM companion_messages", [], |r| r.get(0))
        .expect("count");
    assert_eq!(n, 2, "messages kept");

    // Recovery: with the clash removed (rebuilding the table is how SQLite drops it), the next
    // open upgrades. Here the clash column is simply dropped.
    conn.execute_batch("ALTER TABLE companion_messages DROP COLUMN no_stream")
        .expect("drop clash");
    drop(conn);
    let (s, _) = Store::open(&path).expect("recovered");
    assert_eq!(s.companion_messages(1).expect("msgs").len(), 2);
}

fn open_new(dir: &std::path::Path) -> (Store, i64) {
    let path = dir.join("n.db");
    let conn = Connection::open(&path).expect("raw");
    migrate(&conn, MIGRATIONS, None).expect("schema");
    conn.execute_batch(
        "INSERT INTO companion_profiles
           (id, kind, label, base_url, monthly_token_budget, is_default, created_utc, updated_utc)
         VALUES ('p-a', 'openai', 'OpenAI', 'https://api.openai.com/v1', NULL, 1,
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
                host: "api.openai.com",
            },
            T0,
        )
        .expect("add");
    (s, id)
}

#[test]
fn a_failed_message_keeps_the_providers_words_apart_and_a_retry_clears_them() {
    let dir = tempfile::tempdir().expect("dir");
    let (s, id) = open_new(dir.path());
    s.companion_fail_message_with(
        id,
        "providerError",
        "The provider didn't accept the request (HTTP 400). Check the model name.",
        Some(("OpenAI", "Unsupported value: 'stream'.")),
    )
    .expect("fail");
    let thread = s.companion_threads().expect("threads")[0].id;
    let m = &s.companion_messages(thread).expect("msgs")[0];
    assert_eq!(m.error_reason_by.as_deref(), Some("OpenAI"));
    assert_eq!(
        m.error_reason.as_deref(),
        Some("Unsupported value: 'stream'.")
    );
    assert!(!m.error_message.as_deref().unwrap().contains("Unsupported"));

    // A failure without a reason stores none (and the old method does the same).
    s.companion_fail_message(id, "offline", "Can't reach the provider.")
        .expect("fail");
    let m = &s.companion_messages(thread).expect("msgs")[0];
    assert_eq!(
        (m.error_reason_by.clone(), m.error_reason.clone()),
        (None, None)
    );

    // A reason is capped by the database too.
    let long = "x".repeat(301);
    assert!(matches!(
        s.companion_fail_message_with(id, "providerError", "m", Some(("OpenAI", &long))),
        Err(StoreError::InvalidInput(_))
    ));

    // Retrying clears the stored words.
    s.companion_fail_message_with(id, "providerError", "m", Some(("OpenAI", "why")))
        .expect("fail");
    s.companion_mark_pending(id).expect("pending");
    let m = &s.companion_messages(thread).expect("msgs")[0];
    assert_eq!(
        (
            m.error_kind.clone(),
            m.error_reason_by.clone(),
            m.error_reason.clone()
        ),
        (None, None, None)
    );
}

#[test]
fn the_not_streamed_flag_is_stored_on_the_reply_and_never_in_its_text() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut s, id) = open_new(dir.path());
    let reply = s
        .companion_complete_send_flagged(id, "the reply", (5, 7), (5, 7), T0, true, &|| true)
        .expect("complete")
        .expect("stored");
    assert_eq!(reply.text, "the reply");
    assert!(reply.no_stream);
    let msgs = s.companion_messages(reply.thread_id).expect("msgs");
    assert!(!msgs[0].no_stream, "the user message is not flagged");
    assert!(msgs[1].no_stream);
    assert_eq!(msgs[1].text, "the reply");

    // The ordinary path leaves it off.
    let (mut s2, id2) = open_new(&dir.path().join("..").join(dir.path().file_name().unwrap()));
    let r2 = s2
        .companion_complete_send_if(id2, "plain", 1, 1, 1, 1, T0, &|| true)
        .expect("complete")
        .expect("stored");
    assert!(!r2.no_stream);
}
