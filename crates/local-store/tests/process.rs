//! Owner decision 35 (ADR-025): the process check and the log catch-up, migration 0018.

use agent_core::{AgentEvent, SessionState, parse_event};
use local_store::{
    IngestOutcome, LOG_HINT_MARGIN, MIGRATIONS, ProcessKey, Store, StoreError, latest_version,
    migrate,
};
use rusqlite::Connection;
use time::OffsetDateTime;
use time::macros::datetime;

const T0: OffsetDateTime = datetime!(2026-11-07 10:00:00 UTC);
const P: &str = "claude-code";
const PROC: ProcessKey = ProcessKey {
    pid: 4242,
    start: 133_000_000_000_000_000,
};

fn ev(n: u32, session: &str, kind: &str, ts: &str) -> AgentEvent {
    parse_event(&format!(
        r#"{{"schemaVersion":1,"eventId":"00000000-0000-4000-8000-{n:012}","provider":"{P}","providerSessionId":"{session}","eventType":"{kind}","timestampUtc":"{ts}"}}"#
    ))
    .expect("valid event")
}

fn key(s: &str) -> (String, String) {
    (P.to_owned(), s.to_owned())
}

fn minutes(n: i64) -> time::Duration {
    time::Duration::minutes(n)
}

/// A session that last spoke at `T0` and holds `PROC`.
fn working_with_process(s: &mut Store, id: &str, n: u32) {
    s.ingest_with_process(
        &ev(n, id, "working", "2026-11-07T10:00:00Z"),
        T0,
        Some(PROC),
    )
    .expect("ingest");
}

// ---- recording the process ---------------------------------------------------------------

#[test]
fn an_event_records_its_process_and_one_without_clears_it() {
    let mut s = Store::open_in_memory().expect("open");
    working_with_process(&mut s, "a", 1);
    assert_eq!(s.session_process(P, "a").expect("read"), Some(PROC));
    // An event from an older hook has no process: a stale PID is never kept.
    s.ingest(
        &ev(2, "a", "turn_ended", "2026-11-07T10:01:00Z"),
        T0 + minutes(1),
    )
    .expect("ingest");
    assert_eq!(s.session_process(P, "a").expect("read"), None);
    assert_eq!(s.session_process(P, "nobody").expect("read"), None);
}

#[test]
fn out_of_range_processes_are_stored_as_none() {
    let mut s = Store::open_in_memory().expect("open");
    for (n, bad) in [
        ProcessKey { pid: 0, start: 5 },
        ProcessKey { pid: 7, start: 0 },
        ProcessKey {
            pid: u32::MAX,
            start: 5,
        },
        ProcessKey {
            pid: 7,
            start: u64::MAX,
        },
    ]
    .into_iter()
    .enumerate()
    {
        let id = format!("s{n}");
        s.ingest_with_process(
            &ev(
                10 + u32::try_from(n).expect("n"),
                &id,
                "working",
                "2026-11-07T10:00:00Z",
            ),
            T0,
            Some(bad),
        )
        .expect("ingest");
        assert_eq!(s.session_process(P, &id).expect("read"), None, "{bad:?}");
    }
    assert!(s.process_candidates().expect("candidates").is_empty());
}

#[test]
fn candidates_are_unfinished_tracked_sessions_with_a_process() {
    let mut s = Store::open_in_memory().expect("open");
    working_with_process(&mut s, "live", 1);
    working_with_process(&mut s, "done", 2);
    s.ingest_with_process(
        &ev(3, "done", "completed", "2026-11-07T10:01:00Z"),
        T0 + minutes(1),
        Some(PROC),
    )
    .expect("ingest");
    working_with_process(&mut s, "cut-off", 4);
    s.disconnect_provider(P, T0 + minutes(2))
        .expect("disconnect");
    // `cut-off` and `live` were both disconnected; a new event reconnects `live` only.
    s.ingest_with_process(
        &ev(5, "live", "working", "2026-11-07T10:03:00Z"),
        T0 + minutes(3),
        Some(PROC),
    )
    .expect("ingest");
    s.ingest(&ev(6, "no-process", "working", "2026-11-07T10:00:00Z"), T0)
        .expect("ingest");
    s.import_session(P, "imported", SessionState::Working, T0, T0)
        .expect("import");
    let ids: Vec<String> = s
        .process_candidates()
        .expect("candidates")
        .into_iter()
        .map(|c| c.provider_session_id)
        .collect();
    assert_eq!(ids, ["live"]);
}

#[test]
fn a_tampered_process_row_is_never_a_candidate() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("l.db");
    {
        let (mut s, _) = Store::open(&path).expect("open");
        working_with_process(&mut s, "a", 1);
    }
    {
        let conn = Connection::open(&path).expect("raw");
        conn.execute("UPDATE sessions SET process_pid = -5", [])
            .expect("tamper");
    }
    let (s, _) = Store::open(&path).expect("reopen");
    assert!(s.process_candidates().expect("candidates").is_empty());
    assert_eq!(s.session_process(P, "a").expect("read"), None);
}

// ---- process gone ------------------------------------------------------------------------

#[test]
fn a_gone_process_ends_the_session_without_an_event() {
    let mut s = Store::open_in_memory().expect("open");
    working_with_process(&mut s, "a", 1);
    let events_before = s.event_count().expect("count");
    let now = T0 + minutes(5);
    assert!(
        s.end_session_process_gone(P, "a", PROC, now, now)
            .expect("end")
    );
    let rec = s.session(P, "a").expect("read").expect("row");
    assert_eq!(rec.state, SessionState::Ended);
    assert_eq!(rec.state_timestamp_utc, Some(now));
    assert_eq!(rec.last_received_utc, T0, "nothing was received");
    assert_eq!(s.event_count().expect("count"), events_before, "no event");
    assert_eq!(
        s.ended_reasons().expect("reasons").get(&key("a")),
        Some(&"process_gone".to_owned())
    );
    assert_eq!(s.session_process(P, "a").expect("read"), None);
    assert!(s.process_candidates().expect("candidates").is_empty());
    // Doing it twice changes nothing.
    assert!(
        !s.end_session_process_gone(P, "a", PROC, now, now + minutes(1))
            .expect("again")
    );
}

#[test]
fn a_reused_pid_with_another_start_key_does_not_match() {
    let mut s = Store::open_in_memory().expect("open");
    working_with_process(&mut s, "a", 1);
    let other = ProcessKey {
        start: PROC.start + 1,
        ..PROC
    };
    let now = T0 + minutes(5);
    assert!(
        !s.end_session_process_gone(P, "a", other, now, now)
            .expect("end"),
        "the session holds a different process now"
    );
    assert_eq!(
        s.session(P, "a").expect("read").expect("row").state,
        SessionState::Working
    );
    assert!(s.ended_reasons().expect("reasons").is_empty());
}

#[test]
fn a_newer_event_between_the_look_and_the_write_wins() {
    let mut s = Store::open_in_memory().expect("open");
    working_with_process(&mut s, "a", 1);
    let checked = T0 + minutes(5);
    // The agent spoke again after the process was looked at (its PID may be the same: the
    // process was alive when the event was sent).
    s.ingest_with_process(
        &ev(2, "a", "needs_attention", "2026-11-07T10:05:30Z"),
        checked + time::Duration::seconds(30),
        Some(PROC),
    )
    .expect("ingest");
    assert!(
        !s.end_session_process_gone(P, "a", PROC, checked, checked + minutes(1))
            .expect("end")
    );
    assert_eq!(
        s.session(P, "a").expect("read").expect("row").state,
        SessionState::NeedsAttention
    );
}

#[test]
fn finished_unknown_and_disconnected_sessions_are_never_ended_by_the_check() {
    let mut s = Store::open_in_memory().expect("open");
    let now = T0 + minutes(5);
    // Finished: a reported fact.
    s.ingest_with_process(
        &ev(1, "done", "completed", "2026-11-07T10:00:00Z"),
        T0,
        Some(PROC),
    )
    .expect("ingest");
    assert!(
        !s.end_session_process_gone(P, "done", PROC, now, now)
            .expect("end")
    );
    assert_eq!(
        s.session(P, "done").expect("read").expect("row").state,
        SessionState::Completed
    );
    // Unknown session: nothing to end.
    assert!(
        !s.end_session_process_gone(P, "ghost", PROC, now, now)
            .expect("end")
    );
    // Disconnected: no longer tracked.
    working_with_process(&mut s, "cut", 3);
    s.disconnect_provider(P, T0 + minutes(1))
        .expect("disconnect");
    assert!(
        !s.end_session_process_gone(P, "cut", PROC, now, now)
            .expect("end")
    );
}

#[test]
fn a_real_event_after_process_gone_reopens_the_session_and_clears_the_reason() {
    let mut s = Store::open_in_memory().expect("open");
    working_with_process(&mut s, "a", 1);
    let gone_at = T0 + minutes(5);
    s.end_session_process_gone(P, "a", PROC, gone_at, gone_at)
        .expect("end");
    let again = ProcessKey {
        pid: 5151,
        start: PROC.start + 10,
    };
    let out = s
        .ingest_with_process(
            &ev(2, "a", "working", "2026-11-07T10:06:00Z"),
            T0 + minutes(6),
            Some(again),
        )
        .expect("ingest");
    match out {
        IngestOutcome::Stored {
            state_changed,
            record,
        } => {
            assert!(state_changed);
            assert_eq!(record.state, SessionState::Working);
        }
        IngestOutcome::Duplicate => panic!("stored"),
    }
    assert!(s.ended_reasons().expect("reasons").is_empty());
    assert_eq!(s.session_process(P, "a").expect("read"), Some(again));
    assert_eq!(s.process_candidates().expect("candidates").len(), 1);
}

#[test]
fn a_stale_event_does_not_reopen_and_keeps_the_reason() {
    let mut s = Store::open_in_memory().expect("open");
    working_with_process(&mut s, "a", 1);
    let gone_at = T0 + minutes(5);
    s.end_session_process_gone(P, "a", PROC, gone_at, gone_at)
        .expect("end");
    // Sent before the mark, delivered late.
    s.ingest(
        &ev(2, "a", "needs_attention", "2026-11-07T10:02:00Z"),
        T0 + minutes(6),
    )
    .expect("ingest");
    assert_eq!(
        s.session(P, "a").expect("read").expect("row").state,
        SessionState::Ended
    );
    assert!(s.ended_reasons().expect("reasons").contains_key(&key("a")));
}

#[test]
fn a_heartbeat_after_process_gone_shows_the_agent_alive_with_no_known_state() {
    let mut s = Store::open_in_memory().expect("open");
    working_with_process(&mut s, "a", 1);
    let gone_at = T0 + minutes(5);
    s.end_session_process_gone(P, "a", PROC, gone_at, gone_at)
        .expect("end");
    s.ingest(
        &ev(2, "a", "heartbeat", "2026-11-07T10:06:00Z"),
        T0 + minutes(6),
    )
    .expect("ingest");
    let rec = s.session(P, "a").expect("read").expect("row");
    assert_eq!(rec.state, SessionState::Unknown);
    assert_eq!(rec.state_timestamp_utc, None);
    assert!(s.ended_reasons().expect("reasons").is_empty());
}

#[test]
fn a_reported_end_after_process_gone_is_a_reported_end() {
    let mut s = Store::open_in_memory().expect("open");
    working_with_process(&mut s, "a", 1);
    let gone_at = T0 + minutes(5);
    s.end_session_process_gone(P, "a", PROC, gone_at, gone_at)
        .expect("end");
    s.ingest(
        &ev(2, "a", "session_ended", "2026-11-07T10:06:00Z"),
        T0 + minutes(6),
    )
    .expect("ingest");
    assert_eq!(
        s.session(P, "a").expect("read").expect("row").state,
        SessionState::Ended
    );
    assert!(
        s.ended_reasons().expect("reasons").is_empty(),
        "now the provider said so"
    );
}

// ---- log catch-up ------------------------------------------------------------------------

#[test]
fn a_log_hint_never_creates_a_session() {
    let mut s = Store::open_in_memory().expect("open");
    assert!(
        !s.apply_log_hint(
            P,
            "unknown-session",
            SessionState::Idle,
            T0,
            T0 + minutes(1)
        )
        .expect("hint")
    );
    assert!(s.sessions().expect("sessions").is_empty());
    assert!(s.imported_sessions().expect("marked").is_empty());
}

#[test]
fn a_newer_log_fills_the_gap_as_a_marked_guess_and_a_real_event_takes_over() {
    let mut s = Store::open_in_memory().expect("open");
    working_with_process(&mut s, "a", 1);
    let log = T0 + minutes(30);
    let now = T0 + minutes(40);
    assert!(
        s.apply_log_hint(P, "a", SessionState::Idle, log, now)
            .expect("hint")
    );
    let rec = s.session(P, "a").expect("read").expect("row");
    assert_eq!(rec.state, SessionState::Idle);
    assert_eq!(rec.state_timestamp_utc, Some(log));
    assert_eq!(rec.last_seen_utc, log);
    assert_eq!(
        s.imported_sessions().expect("marked").get(&key("a")),
        Some(&now)
    );
    assert_eq!(s.event_count().expect("count"), 1, "no event is written");
    assert_eq!(
        s.session_process(P, "a").expect("read"),
        None,
        "old process is not trusted"
    );
    // Idempotent.
    assert!(
        !s.apply_log_hint(P, "a", SessionState::Idle, log, now + minutes(1))
            .expect("again")
    );
    // The first real event discards the guess and counts as a change.
    let out = s
        .ingest(
            &ev(2, "a", "needs_attention", "2026-11-07T10:41:00Z"),
            now + minutes(1),
        )
        .expect("ingest");
    assert!(matches!(
        out,
        IngestOutcome::Stored {
            state_changed: true,
            ..
        }
    ));
    assert!(s.imported_sessions().expect("marked").is_empty());
}

#[test]
fn a_log_hint_never_overrides_newer_or_equal_reported_state() {
    let mut s = Store::open_in_memory().expect("open");
    working_with_process(&mut s, "a", 1);
    let now = T0 + minutes(40);
    for log in [
        T0 - minutes(5),
        T0,
        T0 + LOG_HINT_MARGIN,
        T0 + LOG_HINT_MARGIN - time::Duration::seconds(1),
    ] {
        assert!(
            !s.apply_log_hint(P, "a", SessionState::Idle, log, now)
                .expect("hint"),
            "{log}"
        );
    }
    assert_eq!(
        s.session(P, "a").expect("read").expect("row").state,
        SessionState::Working
    );
    assert!(s.imported_sessions().expect("marked").is_empty());
    // One second past the margin is newer.
    assert!(
        s.apply_log_hint(
            P,
            "a",
            SessionState::Idle,
            T0 + LOG_HINT_MARGIN + time::Duration::seconds(1),
            now
        )
        .expect("hint")
    );
}

#[test]
fn a_log_clock_ahead_of_ours_is_capped_at_now() {
    let mut s = Store::open_in_memory().expect("open");
    working_with_process(&mut s, "a", 1);
    let now = T0 + minutes(1);
    // The log claims two hours ahead: capped to `now`, which is only a minute past the last
    // event, so it is newer than the margin and applies, with `now` as its time.
    assert!(
        s.apply_log_hint(P, "a", SessionState::Working, now + minutes(120), now)
            .expect("hint")
    );
    assert_eq!(
        s.session(P, "a").expect("read").expect("row").last_seen_utc,
        now
    );
}

#[test]
fn a_log_hint_leaves_alone_what_is_not_a_gap() {
    let mut s = Store::open_in_memory().expect("open");
    let log = T0 + minutes(30);
    let now = T0 + minutes(40);
    // Finished by the agent's own report: logs are written after a session ends.
    s.ingest(&ev(1, "done", "completed", "2026-11-07T10:00:00Z"), T0)
        .expect("ingest");
    s.ingest(&ev(2, "ended", "session_ended", "2026-11-07T10:00:00Z"), T0)
        .expect("ingest");
    // Ended by the process check.
    working_with_process(&mut s, "gone", 3);
    s.end_session_process_gone(P, "gone", PROC, T0 + minutes(5), T0 + minutes(5))
        .expect("end");
    // Disconnected.
    working_with_process(&mut s, "cut", 4);
    s.disconnect_provider(P, T0 + minutes(1))
        .expect("disconnect");
    // A live one, but the guess says nothing.
    working_with_process(&mut s, "live", 5);
    for id in ["done", "ended", "gone", "cut"] {
        assert!(
            !s.apply_log_hint(P, id, SessionState::Idle, log, now)
                .expect("hint"),
            "{id}"
        );
    }
    for st in [
        SessionState::Unknown,
        SessionState::Completed,
        SessionState::Ended,
    ] {
        assert!(
            !s.apply_log_hint(P, "live", st, log, now).expect("hint"),
            "{st:?}"
        );
    }
    assert!(s.imported_sessions().expect("marked").is_empty());
    assert_eq!(
        s.session(P, "done").expect("read").expect("row").state,
        SessionState::Completed
    );
}

// ---- migration 0018 ----------------------------------------------------------------------

fn version_16_database(path: &std::path::Path) {
    let conn = Connection::open(path).expect("open raw");
    let upto_16: Vec<_> = MIGRATIONS.iter().take_while(|m| m.version <= 16).collect();
    assert_eq!(upto_16.len(), 16);
    migrate(&conn, &MIGRATIONS[..16], None).expect("v16");
    conn.execute(
        "INSERT INTO sessions (provider, provider_session_id, state, state_timestamp_utc,
           last_seen_utc, last_received_utc)
         VALUES ('claude-code', 'old', 'working', '2026-11-06T10:00:00Z',
           '2026-11-06T10:00:00Z', '2026-11-06T10:00:00Z')",
        [],
    )
    .expect("row");
}

#[test]
fn version_16_database_upgrades_with_backup_and_keeps_sessions() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("l.db");
    version_16_database(&path);
    let (s, report) = Store::open(&path).expect("upgrade");
    assert_eq!(report.from_version, 16);
    assert_eq!(report.to_version, latest_version());
    assert!(latest_version() >= 18);
    let backup = report.backup.expect("a backup is taken before migrating");
    assert!(backup.exists());
    let rec = s.session(P, "old").expect("read").expect("kept");
    assert_eq!(rec.state, SessionState::Working);
    assert_eq!(s.session_process(P, "old").expect("read"), None);
    assert!(s.ended_reasons().expect("reasons").is_empty());
    assert!(
        s.process_candidates().expect("candidates").is_empty(),
        "an old session has no recorded process, so the check leaves it alone"
    );
    // The backup is a working version 16 database: recovery is opening it instead.
    let old = Connection::open(&backup).expect("open backup");
    let v: i64 = old
        .query_row("SELECT version FROM schema_version", [], |r| r.get(0))
        .expect("version");
    assert_eq!(v, 16);
    assert!(
        old.prepare("SELECT process_pid FROM sessions").is_err(),
        "the backup predates the new columns"
    );
}

#[test]
fn failed_version_18_migration_rolls_back_to_16() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("l.db");
    {
        version_16_database(&path);
        let conn = Connection::open(&path).expect("raw");
        // A clash in the last of the three statements: the first two must roll back too.
        conn.execute("ALTER TABLE sessions ADD COLUMN ended_reason TEXT", [])
            .expect("clash");
    }
    assert!(matches!(
        Store::open(&path),
        Err(StoreError::Migration { version: 18, .. })
    ));
    let conn = Connection::open(&path).expect("reopen raw");
    let v: i64 = conn
        .query_row("SELECT max(version) FROM schema_version", [], |r| r.get(0))
        .expect("version");
    assert_eq!(v, 16, "left at the last good version");
    assert!(
        conn.prepare("SELECT process_pid FROM sessions").is_err(),
        "the earlier statements of the failed step were rolled back"
    );
    let n: i64 = conn
        .query_row("SELECT count(*) FROM sessions", [], |r| r.get(0))
        .expect("rows");
    assert_eq!(n, 1, "data kept");
}
