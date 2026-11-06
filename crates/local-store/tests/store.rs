//! S01-T05: restart, duplicate, reorder and stale behaviour through the real store, plus
//! migration, backup, rollback and downgrade-protection tests.

use agent_core::reducer::DEFAULT_STALE_AFTER;
use agent_core::{AgentEvent, SessionState, ViewReason, parse_event, view};
use local_store::{
    ACTIVITY_MAX_KEYS, ACTIVITY_ROW_OVERHEAD, IngestOutcome, MIGRATIONS, Migration, Store,
    StoreError, latest_version, migrate,
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

#[test]
fn version_2_database_upgrades_and_keeps_adapter_records() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("l.db");
    {
        let conn = Connection::open(&path).expect("open raw");
        migrate(&conn, &MIGRATIONS[..2], None).expect("v2");
        conn.execute(
            "INSERT INTO adapter_installations VALUES ('claude-code','installed','{}','2026-11-05T10:00:00Z','2026-11-05T10:00:00Z')",
            [],
        )
        .expect("insert");
    }
    let (s, report) = Store::open(&path).expect("upgrade");
    assert_eq!(
        (report.from_version, report.to_version),
        (2, latest_version())
    );
    assert!(report.backup.expect("backup").exists());
    assert_eq!(
        s.adapter_install("claude-code").expect("get").as_deref(),
        Some("{}")
    );
    assert_eq!(s.preference("notifications").expect("pref"), None);
    assert!(s.snapshots().expect("snapshots").is_empty());
}

#[test]
fn failed_version_3_migration_rolls_back_to_version_2() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("l.db");
    {
        let conn = Connection::open(&path).expect("open raw");
        migrate(&conn, &MIGRATIONS[..2], None).expect("v2");
        // A clashing table makes step 3 fail half-way.
        conn.execute("CREATE TABLE activity_snapshots (x INTEGER)", [])
            .expect("clash");
    }
    assert!(matches!(
        Store::open(&path),
        Err(StoreError::Migration { version: 3, .. })
    ));
    let conn = Connection::open(&path).expect("open raw");
    let v: i64 = conn
        .query_row("SELECT max(version) FROM schema_version", [], |r| r.get(0))
        .expect("v");
    assert_eq!(v, 2, "left at the last good version");
    let prefs: bool = conn
        .query_row(
            "SELECT EXISTS (SELECT 1 FROM sqlite_schema WHERE name = 'preferences')",
            [],
            |r| r.get(0),
        )
        .expect("q");
    assert!(!prefs, "the partial step was rolled back");
}

#[test]
fn preferences_round_trip_and_replace() {
    let s = Store::open_in_memory().expect("open");
    assert_eq!(s.preference("notifications").expect("get"), None);
    s.set_preference("notifications", r#"{"a":1}"#)
        .expect("set");
    s.set_preference("notifications", r#"{"a":2}"#)
        .expect("replace");
    assert_eq!(
        s.preference("notifications").expect("get").as_deref(),
        Some(r#"{"a":2}"#)
    );
}

#[test]
fn activity_storage_is_namespaced_and_bounded() {
    // Every row costs key + value + ACTIVITY_ROW_OVERHEAD bytes against the limit.
    const OVER: usize = ACTIVITY_ROW_OVERHEAD;
    let limit = OVER + 16;
    let mut s = Store::open_in_memory().expect("open");
    s.set_activity_value("a", "best", "12", limit)
        .expect("fits");
    s.set_activity_value("b", "best", "99", limit)
        .expect("other namespace");
    assert_eq!(
        s.activity_value("a", "best").expect("get").as_deref(),
        Some("12")
    );
    assert_eq!(
        s.activity_value("b", "best").expect("get").as_deref(),
        Some("99")
    );
    assert_eq!(s.activity_usage("a").expect("usage"), 6 + OVER);

    // Replacing a value counts only the new size.
    s.set_activity_value("a", "best", "0123456789ab", limit)
        .expect("exactly at limit");
    assert!(matches!(
        s.set_activity_value("a", "more", "x", limit),
        Err(StoreError::QuotaExceeded { limit: l }) if l == limit
    ));
    assert_eq!(
        s.activity_value("a", "more").expect("get"),
        None,
        "nothing written"
    );
    // Multi-byte characters count as bytes.
    assert!(s.set_activity_value("c", "k", "éééééééééé", limit).is_err());

    s.remove_activity_value("a", "best").expect("remove");
    assert_eq!(s.activity_usage("a").expect("usage"), 0);
}

#[test]
fn empty_keys_are_not_free_and_the_key_count_is_capped() {
    let mut s = Store::open_in_memory().expect("open");
    // The byte quota used by the app (256 KiB) comfortably holds the key cap, so the cap is
    // what stops this loop.
    let quota = 256 * 1024;
    for i in 0..ACTIVITY_MAX_KEYS {
        s.set_activity_value("a", &format!("k{i}"), "", quota)
            .unwrap_or_else(|e| panic!("key {i}: {e}"));
    }
    assert!(matches!(
        s.set_activity_value("a", "one-too-many", "", quota),
        Err(StoreError::TooManyKeys { limit }) if limit == ACTIVITY_MAX_KEYS
    ));
    assert_eq!(
        s.activity_value("a", "one-too-many").expect("get"),
        None,
        "nothing written"
    );
    // Existing keys can still be replaced, other activities are unaffected, and removing a key
    // makes room again.
    s.set_activity_value("a", "k0", "now with a value", quota)
        .expect("replace");
    s.set_activity_value("b", "k0", "", quota).expect("other");
    s.remove_activity_value("a", "k1").expect("remove");
    s.set_activity_value("a", "one-more", "", quota)
        .expect("room again");
    // Rows cost their overhead: the usage is far above the bare key bytes.
    let usage = s.activity_usage("a").expect("usage");
    assert!(usage >= ACTIVITY_MAX_KEYS * ACTIVITY_ROW_OVERHEAD);
    // A tiny quota is spent by overhead alone.
    let mut t = Store::open_in_memory().expect("open");
    let small = ACTIVITY_ROW_OVERHEAD * 3;
    for i in 0..3 {
        t.set_activity_value("a", &format!("{i}"), "", small + 3)
            .expect("fits");
    }
    assert!(matches!(
        t.set_activity_value("a", "x", "", small + 3),
        Err(StoreError::QuotaExceeded { .. })
    ));
}

#[test]
fn snapshots_replace_refuse_oversize_and_clear() {
    let s = Store::open_in_memory().expect("open");
    s.save_snapshot("game", r#"{"moves":1}"#, T0, 64)
        .expect("save");
    s.save_snapshot("game", r#"{"moves":2}"#, T0 + Duration::seconds(5), 64)
        .expect("replace");
    s.save_snapshot("break", "{}", T0, 64).expect("save other");
    let big = "x".repeat(65);
    assert!(matches!(
        s.save_snapshot("game", &big, T0, 64),
        Err(StoreError::QuotaExceeded { limit: 64 })
    ));
    let (snap, saved) = s.snapshot("game").expect("get").expect("some");
    assert_eq!(snap, r#"{"moves":2}"#, "oversize write kept the old one");
    assert_eq!(saved, "2026-11-05T10:00:05Z");
    let ids: Vec<String> = s
        .snapshots()
        .expect("list")
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(ids, ["game", "break"]);
    s.clear_snapshot("game").expect("clear");
    assert_eq!(s.snapshot("game").expect("get"), None);
}
