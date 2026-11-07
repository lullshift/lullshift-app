//! Owner decision 34 (ADR-024): sessions imported from an agent's own log, migration 0016.

use agent_core::{AgentEvent, SessionState, parse_event};
use local_store::{IngestOutcome, MIGRATIONS, Store, StoreError, latest_version, migrate};
use rusqlite::Connection;
use time::OffsetDateTime;
use time::macros::datetime;

const T0: OffsetDateTime = datetime!(2026-11-07 10:00:00 UTC);
const P: &str = "claude-code";

fn ev(n: u32, session: &str, kind: &str, ts: &str) -> AgentEvent {
    parse_event(&format!(
        r#"{{"schemaVersion":1,"eventId":"00000000-0000-4000-8000-{n:012}","provider":"{P}","providerSessionId":"{session}","eventType":"{kind}","timestampUtc":"{ts}"}}"#
    ))
    .expect("valid event")
}

fn key(s: &str) -> (String, String) {
    (P.to_owned(), s.to_owned())
}

#[test]
fn import_adds_a_marked_session_with_the_guess_and_no_events() {
    let mut s = Store::open_in_memory().expect("open");
    let activity = T0 - time::Duration::minutes(3);
    assert!(
        s.import_session(P, "a", SessionState::NeedsAttention, activity, T0)
            .expect("import")
    );
    let rec = s.session(P, "a").expect("read").expect("row");
    assert_eq!(rec.state, SessionState::NeedsAttention);
    assert_eq!(rec.last_seen_utc, activity);
    assert_eq!(rec.last_received_utc, activity);
    assert_eq!(rec.state_timestamp_utc, Some(activity));
    assert_eq!(
        s.event_count().expect("count"),
        0,
        "nothing in the event log"
    );
    let marked = s.imported_sessions().expect("marked");
    assert_eq!(marked.get(&key("a")), Some(&T0));
}

#[test]
fn unknown_guess_has_no_state_time_and_future_activity_is_capped() {
    let mut s = Store::open_in_memory().expect("open");
    let future = T0 + time::Duration::hours(2);
    s.import_session(P, "u", SessionState::Unknown, future, T0)
        .expect("import");
    let rec = s.session(P, "u").expect("read").expect("row");
    assert_eq!(rec.state_timestamp_utc, None);
    assert_eq!(rec.last_seen_utc, T0, "a log clock ahead of ours is capped");
}

#[test]
fn import_never_touches_a_tracked_session() {
    let mut s = Store::open_in_memory().expect("open");
    s.ingest(&ev(1, "t", "working", "2026-11-07T09:59:00Z"), T0)
        .expect("ingest");
    assert!(
        !s.import_session(P, "t", SessionState::Idle, T0, T0)
            .expect("import"),
        "already tracked"
    );
    let rec = s.session(P, "t").expect("read").expect("row");
    assert_eq!(rec.state, SessionState::Working);
    assert!(s.imported_sessions().expect("marked").is_empty());
}

#[test]
fn import_refuses_terminal_states_and_empty_keys() {
    let mut s = Store::open_in_memory().expect("open");
    for (p, id, st) in [
        (P, "x", SessionState::Completed),
        (P, "x", SessionState::Ended),
        ("", "x", SessionState::Idle),
        (P, "", SessionState::Idle),
    ] {
        assert!(matches!(
            s.import_session(p, id, st, T0, T0),
            Err(StoreError::InvalidInput(_))
        ));
    }
}

#[test]
fn first_hook_event_clears_the_mark_and_takes_over_even_if_older() {
    let mut s = Store::open_in_memory().expect("open");
    // Guess made from a log whose clock ran ahead: a later real event with an earlier
    // timestamp must still replace it.
    s.import_session(P, "a", SessionState::Idle, T0, T0)
        .expect("import");
    let received = T0 + time::Duration::seconds(5);
    let out = s
        .ingest(
            &ev(1, "a", "needs_attention", "2026-11-07T09:00:00Z"),
            received,
        )
        .expect("ingest");
    match out {
        IngestOutcome::Stored {
            state_changed,
            record,
        } => {
            assert!(state_changed, "the first real event counts as a change");
            assert_eq!(record.state, SessionState::NeedsAttention);
            assert_eq!(record.last_received_utc, received);
        }
        IngestOutcome::Duplicate => panic!("stored"),
    }
    assert!(s.imported_sessions().expect("marked").is_empty());
}

#[test]
fn same_state_from_the_first_event_still_counts_as_a_change() {
    let mut s = Store::open_in_memory().expect("open");
    s.import_session(P, "a", SessionState::NeedsAttention, T0, T0)
        .expect("import");
    let out = s
        .ingest(
            &ev(1, "a", "needs_attention", "2026-11-07T10:00:05Z"),
            T0 + time::Duration::seconds(6),
        )
        .expect("ingest");
    assert!(matches!(
        out,
        IngestOutcome::Stored {
            state_changed: true,
            ..
        }
    ));
}

#[test]
fn delete_removes_an_imported_session() {
    let mut s = Store::open_in_memory().expect("open");
    s.import_session(P, "a", SessionState::Working, T0, T0)
        .expect("import");
    s.delete_session(P, "a").expect("delete");
    assert!(s.session(P, "a").expect("read").is_none());
    assert!(s.imported_sessions().expect("marked").is_empty());
}

#[test]
fn version_15_database_upgrades_to_16_with_backup_and_keeps_sessions() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("l.db");
    {
        let conn = Connection::open(&path).expect("open raw");
        migrate(&conn, &MIGRATIONS[..15], None).expect("v15");
        conn.execute(
            "INSERT INTO sessions (provider, provider_session_id, state, state_timestamp_utc,
               last_seen_utc, last_received_utc)
             VALUES ('claude-code', 'old', 'idle', '2026-11-06T10:00:00Z',
               '2026-11-06T10:00:00Z', '2026-11-06T10:00:00Z')",
            [],
        )
        .expect("row");
    }
    let (s, report) = Store::open(&path).expect("upgrade");
    assert_eq!(latest_version(), 16);
    assert_eq!((report.from_version, report.to_version), (15, 16));
    let backup = report.backup.expect("a backup is taken before migrating");
    assert!(backup.exists());
    let rec = s.session(P, "old").expect("read").expect("kept");
    assert_eq!(rec.state, SessionState::Idle);
    assert!(
        s.imported_sessions().expect("marked").is_empty(),
        "older sessions are not imported ones"
    );
    // The backup is a working version 15 database: recovery is opening it instead.
    let old = Connection::open(&backup).expect("open backup");
    let v: i64 = old
        .query_row("SELECT version FROM schema_version", [], |r| r.get(0))
        .expect("version");
    assert_eq!(v, 15);
    let n: i64 = old
        .query_row("SELECT count(*) FROM sessions", [], |r| r.get(0))
        .expect("rows");
    assert_eq!(n, 1);
}

#[test]
fn failed_version_16_migration_rolls_back_to_15() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("l.db");
    {
        let conn = Connection::open(&path).expect("open raw");
        migrate(&conn, &MIGRATIONS[..15], None).expect("v15");
        conn.execute("ALTER TABLE sessions ADD COLUMN imported_utc INTEGER", [])
            .expect("clash");
    }
    assert!(matches!(
        Store::open(&path),
        Err(StoreError::Migration { version: 16, .. })
    ));
    let conn = Connection::open(&path).expect("reopen raw");
    let v: i64 = conn
        .query_row("SELECT max(version) FROM schema_version", [], |r| r.get(0))
        .expect("version");
    assert_eq!(v, 15, "left at the last good version");
}
