//! S01-T05: restart, duplicate, reorder and stale behaviour through the real store, plus
//! migration, backup, rollback and downgrade-protection tests.

use agent_core::reducer::DEFAULT_STALE_AFTER;
use agent_core::{AgentEvent, SessionState, ViewReason, parse_event, view};
use local_store::{
    IngestOutcome, MIGRATIONS, Migration, Store, StoreError, latest_version, migrate,
};
use rusqlite::Connection;
use time::macros::datetime;
use time::{Duration, OffsetDateTime};

const T0: OffsetDateTime = datetime!(2026-11-05 10:00:00 UTC);

fn ev(id_suffix: u32, session: &str, kind: &str, ts: &str) -> AgentEvent {
    parse_event(&format!(
        r#"{{"schemaVersion":1,"eventId":"00000000-0000-4000-8000-{id_suffix:012}","provider":"claude-code","providerSessionId":"{session}","eventType":"{kind}","timestampUtc":"{ts}"}}"#
    ))
    .expect("valid event")
}

#[test]
fn fresh_database_is_at_latest_version() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (store, report) = Store::open(&dir.path().join("l.db")).expect("open");
    assert_eq!(store.schema_version().expect("version"), latest_version());
    assert_eq!(report.from_version, 0);
    assert_eq!(report.backup, None, "nothing to back up on first run");
}

#[test]
fn duplicates_are_stored_once_and_have_no_effect() {
    let mut s = Store::open_in_memory().expect("open");
    let e = ev(1, "s", "working", "2026-11-05T10:00:00Z");
    assert!(matches!(
        s.ingest(&e, T0).expect("ingest"),
        IngestOutcome::Stored {
            state_changed: true,
            ..
        }
    ));
    assert_eq!(
        s.ingest(&e, T0 + Duration::seconds(5)).expect("ingest"),
        IngestOutcome::Duplicate
    );
    assert_eq!(s.event_count().expect("count"), 1);
    let rec = s
        .session("claude-code", "s")
        .expect("load")
        .expect("exists");
    assert_eq!(
        rec.last_received_utc, T0,
        "duplicate must not refresh liveness"
    );
}

#[test]
fn reordered_events_keep_newest_state() {
    let mut s = Store::open_in_memory().expect("open");
    s.ingest(
        &ev(2, "s", "needs_attention", "2026-11-05T10:00:05Z"),
        T0 + Duration::seconds(10),
    )
    .expect("ingest");
    let out = s
        .ingest(
            &ev(1, "s", "working", "2026-11-05T10:00:01Z"),
            T0 + Duration::seconds(11),
        )
        .expect("ingest");
    assert!(matches!(
        out,
        IngestOutcome::Stored {
            state_changed: false,
            ..
        }
    ));
    let rec = s
        .session("claude-code", "s")
        .expect("load")
        .expect("exists");
    assert_eq!(rec.state, SessionState::NeedsAttention);
    assert_eq!(
        s.event_count().expect("count"),
        2,
        "older event kept for history"
    );
}

#[test]
fn sessions_survive_restart_and_show_unconfirmed_until_new_event() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("l.db");
    {
        let (mut s, _) = Store::open(&path).expect("open");
        s.ingest(&ev(1, "a", "working", "2026-11-05T10:00:00Z"), T0)
            .expect("ingest");
        s.ingest(&ev(2, "b", "completed", "2026-11-05T10:00:00Z"), T0)
            .expect("ingest");
    }
    let restart = T0 + Duration::minutes(2);
    let (mut s, report) = Store::open(&path).expect("reopen");
    assert_eq!(report.from_version, latest_version());
    assert_eq!(report.backup, None, "no backup when nothing migrates");

    let views: Vec<_> = s
        .sessions()
        .expect("sessions")
        .iter()
        .map(|r| view(r, restart, restart, DEFAULT_STALE_AFTER))
        .collect();
    let a = views
        .iter()
        .find(|v| v.provider_session_id == "a")
        .expect("a");
    assert_eq!(
        (a.state, a.reason, a.last_known_state),
        (
            SessionState::Unknown,
            ViewReason::UnconfirmedAfterRestart,
            SessionState::Working
        )
    );
    let b = views
        .iter()
        .find(|v| v.provider_session_id == "b")
        .expect("b");
    assert_eq!(
        (b.state, b.reason),
        (SessionState::Completed, ViewReason::Observed)
    );

    s.ingest(
        &ev(3, "a", "heartbeat", "2026-11-05T10:02:30Z"),
        restart + Duration::seconds(30),
    )
    .expect("ingest");
    let rec = s
        .session("claude-code", "a")
        .expect("load")
        .expect("exists");
    let v = view(
        &rec,
        restart,
        restart + Duration::seconds(31),
        DEFAULT_STALE_AFTER,
    );
    assert_eq!(
        (v.state, v.reason),
        (SessionState::Working, ViewReason::Observed)
    );
}

#[test]
fn silent_session_turns_stale() {
    let mut s = Store::open_in_memory().expect("open");
    s.ingest(&ev(1, "s", "working", "2026-11-05T10:00:00Z"), T0)
        .expect("ingest");
    let rec = s
        .session("claude-code", "s")
        .expect("load")
        .expect("exists");
    let v = view(&rec, T0, T0 + Duration::minutes(11), DEFAULT_STALE_AFTER);
    assert_eq!(
        (v.state, v.reason, v.last_known_state),
        (
            SessionState::Unknown,
            ViewReason::Stale,
            SessionState::Working
        )
    );
}

#[test]
fn sessions_are_listed_most_recent_first() {
    let mut s = Store::open_in_memory().expect("open");
    s.ingest(&ev(1, "old", "working", "2026-11-05T10:00:00Z"), T0)
        .expect("ingest");
    s.ingest(
        &ev(2, "new", "working", "2026-11-05T10:00:00Z"),
        T0 + Duration::milliseconds(500),
    )
    .expect("ingest");
    let ids: Vec<_> = s
        .sessions()
        .expect("sessions")
        .into_iter()
        .map(|r| r.provider_session_id)
        .collect();
    assert_eq!(ids, ["new", "old"]);
}

#[test]
fn failed_migration_rolls_back_and_keeps_data_and_backup() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("l.db");
    {
        let (mut s, _) = Store::open(&path).expect("open");
        s.ingest(&ev(1, "s", "working", "2026-11-05T10:00:00Z"), T0)
            .expect("ingest");
    }
    let broken = Migration {
        version: latest_version() + 1,
        sql: "CREATE TABLE added (x INTEGER); INSERT INTO no_such_table VALUES (1);",
    };
    let steps: Vec<_> = MIGRATIONS.iter().copied().chain([broken]).collect();
    let conn = Connection::open(&path).expect("open raw");
    let bak = dir.path().join("before.bak");
    let make = |_from: i64| bak.clone();
    let err = migrate(&conn, &steps, Some(&make)).expect_err("must fail");
    assert!(
        matches!(err, StoreError::Migration { version, .. } if version == latest_version() + 1)
    );

    let added: bool = conn
        .query_row(
            "SELECT EXISTS (SELECT 1 FROM sqlite_schema WHERE name = 'added')",
            [],
            |r| r.get(0),
        )
        .expect("query");
    assert!(!added, "partial migration must be rolled back");
    drop(conn);

    assert!(bak.exists(), "backup written before migrating");
    let (s, _) = Store::open(&path).expect("reopen at last good version");
    assert_eq!(s.schema_version().expect("version"), latest_version());
    assert_eq!(s.event_count().expect("count"), 1);

    // The backup is a complete, openable copy.
    let (b, _) = Store::open(&bak).expect("open backup");
    assert_eq!(b.event_count().expect("count"), 1);
}

#[test]
fn successful_upgrade_writes_backup() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("l.db");
    drop(Store::open(&path).expect("open"));
    let next = Migration {
        version: latest_version() + 1,
        sql: "CREATE TABLE added (x INTEGER) STRICT;",
    };
    let steps: Vec<_> = MIGRATIONS.iter().copied().chain([next]).collect();
    let conn = Connection::open(&path).expect("open raw");
    let bak = dir.path().join("up.bak");
    let make = |_from: i64| bak.clone();
    let report = migrate(&conn, &steps, Some(&make)).expect("migrate");
    assert_eq!(
        (report.from_version, report.to_version),
        (latest_version(), latest_version() + 1)
    );
    assert_eq!(report.backup.as_deref(), Some(bak.as_path()));
}

#[test]
fn newer_database_is_refused_untouched() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("l.db");
    drop(Store::open(&path).expect("open"));
    {
        let conn = Connection::open(&path).expect("open raw");
        conn.execute("UPDATE schema_version SET version = 999", [])
            .expect("bump");
    }
    match Store::open(&path) {
        Err(StoreError::TooNew {
            found: 999,
            supported,
        }) => assert_eq!(supported, latest_version()),
        other => panic!("expected TooNew, got {:?}", other.map(|(_, r)| r)),
    }
    let conn = Connection::open(&path).expect("open raw");
    let v: i64 = conn
        .query_row("SELECT version FROM schema_version", [], |r| r.get(0))
        .expect("v");
    assert_eq!(v, 999);
}

#[test]
fn tampered_rows_are_reported_not_panicked() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("l.db");
    {
        let (mut s, _) = Store::open(&path).expect("open");
        s.ingest(&ev(1, "s", "working", "2026-11-05T10:00:00Z"), T0)
            .expect("ingest");
    }
    Connection::open(&path)
        .expect("open raw")
        .execute("UPDATE sessions SET state = 'bogus'", [])
        .expect("tamper");
    let (s, _) = Store::open(&path).expect("open");
    assert!(matches!(s.sessions(), Err(StoreError::Corrupt(_))));
}

#[test]
fn version_1_database_upgrades_with_backup_and_keeps_sessions() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("l.db");
    {
        let conn = Connection::open(&path).expect("open raw");
        migrate(&conn, &MIGRATIONS[..1], None).expect("v1");
    }
    {
        // Write a session through the v1 schema.
        let conn = Connection::open(&path).expect("open raw");
        conn.execute(
            "INSERT INTO sessions VALUES ('claude-code','s','working','2026-11-05T10:00:00Z','2026-11-05T10:00:00Z','2026-11-05T10:00:00Z')",
            [],
        )
        .expect("insert");
    }
    let (s, report) = Store::open(&path).expect("upgrade");
    assert_eq!(
        (report.from_version, report.to_version),
        (1, latest_version())
    );
    let backup = report.backup.expect("backup before upgrade");
    assert!(backup.exists());
    assert_eq!(s.sessions().expect("sessions").len(), 1);

    // The backup is the v1 database, still openable at v1.
    let b = Connection::open(&backup).expect("open backup");
    let v: i64 = b
        .query_row("SELECT version FROM schema_version", [], |r| r.get(0))
        .expect("v");
    assert_eq!(v, 1);
}

#[test]
fn adapter_install_records_round_trip() {
    let s = Store::open_in_memory().expect("open");
    assert_eq!(s.adapter_install("claude-code").expect("get"), None);
    s.save_adapter_install("claude-code", r#"{"a":1}"#, T0)
        .expect("save");
    s.save_adapter_install("claude-code", r#"{"a":2}"#, T0)
        .expect("replace");
    assert_eq!(
        s.adapter_install("claude-code").expect("get").as_deref(),
        Some(r#"{"a":2}"#)
    );
    s.mark_adapter_uninstalled("claude-code")
        .expect("uninstall");
    assert_eq!(s.adapter_install("claude-code").expect("get"), None);
}
