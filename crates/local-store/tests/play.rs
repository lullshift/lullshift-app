//! Sprint 05: migration 0005 (time played, session tracking and title columns).

use local_store::{MIGRATIONS, Store, StoreError, latest_version, migrate};
use rusqlite::Connection;
use time::OffsetDateTime;
use time::macros::datetime;

const T0: OffsetDateTime = datetime!(2026-11-05 10:00:00 UTC);

#[test]
fn version_4_database_upgrades_to_5_and_keeps_data() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("l.db");
    {
        let conn = Connection::open(&path).expect("open raw");
        migrate(&conn, &MIGRATIONS[..4], None).expect("v4");
        conn.execute(
            "INSERT INTO sessions VALUES ('claude-code','s','working','2026-11-05T10:00:00Z','2026-11-05T10:00:00Z','2026-11-05T10:00:00Z')",
            [],
        )
        .expect("session");
        conn.execute(
            "INSERT INTO activity_snapshots VALUES ('app.x.game','{\"m\":1}','2026-11-05T10:00:00Z')",
            [],
        )
        .expect("snapshot");
    }
    let (s, report) = Store::open(&path).expect("upgrade");
    assert_eq!(
        (report.from_version, report.to_version),
        (4, latest_version())
    );
    assert!(report.backup.expect("backup before upgrade").exists());
    assert_eq!(s.sessions().expect("sessions").len(), 1);
    assert!(s.snapshot("app.x.game").expect("get").is_some());
    assert_eq!(s.play_time("app.x.game").expect("play"), 0);
}

#[test]
fn failed_version_5_migration_rolls_back_to_version_4() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("l.db");
    {
        let conn = Connection::open(&path).expect("open raw");
        migrate(&conn, &MIGRATIONS[..4], None).expect("v4");
        // A clashing column makes step 5 fail after activity_play is created.
        conn.execute("ALTER TABLE sessions ADD COLUMN disconnected_utc TEXT", [])
            .expect("clash");
    }
    assert!(matches!(
        Store::open(&path),
        Err(StoreError::Migration { version: 5, .. })
    ));
    let conn = Connection::open(&path).expect("reopen raw");
    let v: i64 = conn
        .query_row("SELECT max(version) FROM schema_version", [], |r| r.get(0))
        .expect("version");
    assert_eq!(v, 4, "left at the last good version");
    let tables: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = 'activity_play'",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(tables, 0, "the half-done step was rolled back");
}

#[test]
fn play_time_adds_up_clamps_and_counts_launches() {
    let s = Store::open_in_memory().expect("open");
    s.add_play_time("g", 90, true, 3600, T0).expect("add");
    s.add_play_time("g", 30, false, 3600, T0).expect("add");
    s.add_play_time("g", -500, false, 3600, T0)
        .expect("negative is ignored");
    s.add_play_time("g", 99_999, false, 3600, T0)
        .expect("clamped");
    assert_eq!(s.play_time("g").expect("get"), 90 + 30 + 3600);
    assert_eq!(s.play_time("other").expect("get"), 0);
    assert_eq!(s.play_times().expect("list"), vec![("g".to_owned(), 3720)]);
}
