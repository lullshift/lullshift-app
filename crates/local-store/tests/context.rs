//! Sprint 04 (ADR-013): migration 0004, Context Keeper storage, retention and deletion.

use agent_core::{AgentEvent, parse_event};
use local_store::{
    MAX_ITEM_CHARS, MAX_ITEMS_PER_SESSION, MAX_NOTES_CHARS, MIGRATIONS, Store, StoreError, migrate,
};
use rusqlite::Connection;
use time::macros::datetime;
use time::{Duration, OffsetDateTime};

const T0: OffsetDateTime = datetime!(2026-11-05 10:00:00 UTC);
const P: &str = "claude-code";

fn ev(n: u32, session: &str, kind: &str, ts: &str) -> AgentEvent {
    parse_event(&format!(
        r#"{{"schemaVersion":1,"eventId":"00000000-0000-4000-8000-{n:012}","provider":"claude-code","providerSessionId":"{session}","eventType":"{kind}","timestampUtc":"{ts}"}}"#
    ))
    .expect("valid event")
}

fn store_with(sessions: &[&str]) -> Store {
    let mut s = Store::open_in_memory().expect("open");
    for (i, id) in sessions.iter().enumerate() {
        let n = u32::try_from(i).expect("small") + 1;
        s.ingest(&ev(n, id, "working", "2026-11-05T10:00:00Z"), T0)
            .expect("ingest");
    }
    s
}

const SESSION_ROW: &str = "INSERT INTO sessions VALUES ('claude-code','s','working','2026-11-05T10:00:00Z','2026-11-05T10:00:00Z','2026-11-05T10:00:00Z')";

#[test]
fn version_3_database_upgrades_to_4_and_keeps_data() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("l.db");
    {
        let conn = Connection::open(&path).expect("open raw");
        migrate(&conn, &MIGRATIONS[..3], None).expect("v3");
        conn.execute(SESSION_ROW, []).expect("session");
        conn.execute(
            "INSERT INTO preferences VALUES ('notifications','{}','2026-11-05T10:00:00Z')",
            [],
        )
        .expect("pref");
        conn.execute(
            "INSERT INTO adapter_installations VALUES ('claude-code','installed','{}','2026-11-05T10:00:00Z','2026-11-05T10:00:00Z')",
            [],
        )
        .expect("adapter");
    }
    let (mut s, report) = Store::open(&path).expect("upgrade");
    assert_eq!((report.from_version, report.to_version), (3, 4));
    assert!(report.backup.expect("backup before upgrade").exists());
    assert_eq!(s.sessions().expect("sessions").len(), 1);
    assert_eq!(
        s.preference("notifications").expect("pref").as_deref(),
        Some("{}")
    );
    assert!(s.adapter_install("claude-code").expect("get").is_some());
    // The new tables work on the upgraded database.
    s.set_notes(P, "s", "hello", T0).expect("notes");
    assert_eq!(s.context(P, "s").expect("ctx").expect("row").notes, "hello");
}

#[test]
fn failed_version_4_migration_rolls_back_to_version_3() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("l.db");
    {
        let conn = Connection::open(&path).expect("open raw");
        migrate(&conn, &MIGRATIONS[..3], None).expect("v3");
        conn.execute(SESSION_ROW, []).expect("session");
        // A clashing table makes step 4 fail half-way (after session_context is created).
        conn.execute("CREATE TABLE session_items (x INTEGER)", [])
            .expect("clash");
    }
    assert!(matches!(
        Store::open(&path),
        Err(StoreError::Migration { version: 4, .. })
    ));
    let conn = Connection::open(&path).expect("open raw");
    let v: i64 = conn
        .query_row("SELECT max(version) FROM schema_version", [], |r| r.get(0))
        .expect("v");
    assert_eq!(v, 3, "left at the last good version");
    let ctx: bool = conn
        .query_row(
            "SELECT EXISTS (SELECT 1 FROM sqlite_schema WHERE name = 'session_context')",
            [],
            |r| r.get(0),
        )
        .expect("q");
    assert!(!ctx, "the partial step was rolled back");
    let n: i64 = conn
        .query_row("SELECT count(*) FROM sessions", [], |r| r.get(0))
        .expect("count");
    assert_eq!(n, 1, "existing data untouched");
}

#[test]
fn purge_deletes_only_events_older_than_the_cutoff() {
    let mut s = Store::open_in_memory().expect("open");
    let old = T0 - Duration::days(40);
    let recent = T0 - Duration::days(5);
    s.ingest(&ev(1, "a", "working", "2026-09-25T10:00:00Z"), old)
        .expect("ingest");
    s.ingest(
        &ev(2, "a", "needs_attention", "2026-10-31T10:00:00Z"),
        recent,
    )
    .expect("ingest");
    // Fraction digits vary in stored text; they must not confuse the comparison.
    s.ingest(
        &ev(3, "b", "working", "2026-11-05T09:00:00Z"),
        T0 - Duration::days(31) + Duration::nanoseconds(123_456_789),
    )
    .expect("ingest");
    s.set_notes(P, "a", "keep me", T0).expect("notes");

    let cutoff = T0 - Duration::days(30);
    assert_eq!(s.purge_events_before(cutoff).expect("purge"), 2);
    assert_eq!(s.event_count().expect("count"), 1);
    assert_eq!(s.purge_events_before(cutoff).expect("again"), 0);
    // Sessions and context survive; History still lists them.
    assert_eq!(s.sessions().expect("sessions").len(), 2);
    assert_eq!(
        s.context(P, "a").expect("ctx").expect("row").notes,
        "keep me"
    );
    assert_eq!(s.timeline(P, "a", 500).expect("timeline").len(), 1);
}

#[test]
fn delete_session_removes_everything_for_it_and_only_it() {
    let mut s = store_with(&["a", "b", "c"]);
    s.set_notes(P, "a", "n", T0).expect("notes");
    s.add_item(P, "a", "decision", None, "d", T0).expect("item");
    s.add_item(P, "b", "decision", None, "keep", T0)
        .expect("item");
    s.link_sessions((P, "a"), (P, "b"), T0).expect("link");
    s.link_sessions((P, "b"), (P, "c"), T0).expect("link");
    s.ingest(&ev(10, "a", "needs_attention", "2026-11-05T10:01:00Z"), T0)
        .expect("ingest");

    s.delete_session(P, "a").expect("delete");
    assert!(s.session(P, "a").expect("load").is_none());
    assert!(s.context(P, "a").expect("ctx").is_none());
    assert!(s.items(P, "a").expect("items").is_empty());
    assert!(s.timeline(P, "a", 500).expect("timeline").is_empty());
    assert_eq!(s.event_count().expect("count"), 2, "b and c events remain");
    assert_eq!(
        s.links(P, "b").expect("links"),
        vec![(P.to_owned(), "c".to_owned())],
        "links to the deleted session are gone"
    );
    assert_eq!(s.items(P, "b").expect("items").len(), 1);
    assert!(matches!(
        s.delete_session(P, "a"),
        Err(StoreError::NotFound)
    ));
}

fn order(s: &Store, kind: &str) -> Vec<(String, i64)> {
    s.items(P, "s")
        .expect("items")
        .into_iter()
        .filter(|i| i.kind == kind)
        .map(|i| (i.text, i.position))
        .collect()
}

fn texts(s: &Store, kind: &str) -> Vec<String> {
    order(s, kind).into_iter().map(|(t, _)| t).collect()
}

fn assert_contiguous(s: &Store, kind: &str) {
    let p: Vec<i64> = order(s, kind).into_iter().map(|(_, p)| p).collect();
    let want: Vec<i64> = (0..i64::try_from(p.len()).expect("len")).collect();
    assert_eq!(p, want);
}

#[test]
fn items_keep_contiguous_positions_through_add_move_delete() {
    let mut s = store_with(&["s"]);
    let mut ids = Vec::new();
    for t in ["a", "b", "c", "d"] {
        ids.push(
            s.add_item(P, "s", "checklist", None, t, T0)
                .expect("add")
                .id,
        );
    }
    s.add_item(P, "s", "decision", None, "other kind", T0)
        .expect("add");
    assert_eq!(texts(&s, "checklist"), ["a", "b", "c", "d"]);

    s.move_item(ids[3], 0).expect("move");
    assert_eq!(texts(&s, "checklist"), ["d", "a", "b", "c"]);
    assert_contiguous(&s, "checklist");
    s.move_item(ids[3], 99).expect("move clamps");
    assert_eq!(texts(&s, "checklist"), ["a", "b", "c", "d"]);
    s.move_item(ids[0], 2).expect("move");
    assert_eq!(texts(&s, "checklist"), ["b", "c", "a", "d"]);
    assert_contiguous(&s, "checklist");

    s.delete_item(ids[1]).expect("delete");
    assert_eq!(texts(&s, "checklist"), ["c", "a", "d"]);
    assert_contiguous(&s, "checklist");
    assert_eq!(order(&s, "decision"), [("other kind".to_owned(), 0)]);
    let added = s.add_item(P, "s", "checklist", None, "e", T0).expect("add");
    assert_eq!(added.position, 3);
    assert!(matches!(s.move_item(9999, 0), Err(StoreError::NotFound)));
    assert!(matches!(s.delete_item(9999), Err(StoreError::NotFound)));
}

#[test]
fn update_item_changes_text_and_done() {
    let mut s = store_with(&["s"]);
    let it = s
        .add_item(P, "s", "next_move", Some("question"), "ask?", T0)
        .expect("add");
    assert_eq!(it.category.as_deref(), Some("question"));
    s.update_item(it.id, Some("  asked?  "), Some(true))
        .expect("update");
    let got = &s.items(P, "s").expect("items")[0];
    assert_eq!((got.text.as_str(), got.done), ("asked?", true));
    s.update_item(it.id, None, Some(false)).expect("update");
    assert!(!s.items(P, "s").expect("items")[0].done);
    assert!(matches!(
        s.update_item(it.id, Some("   "), None),
        Err(StoreError::InvalidInput(_))
    ));
    assert!(matches!(
        s.update_item(9999, None, Some(true)),
        Err(StoreError::NotFound)
    ));
}

#[test]
fn limits_are_enforced() {
    let mut s = store_with(&["s"]);
    assert!(matches!(
        s.add_item(
            P,
            "s",
            "decision",
            None,
            &"x".repeat(MAX_ITEM_CHARS + 1),
            T0
        ),
        Err(StoreError::InvalidInput(_))
    ));
    s.add_item(P, "s", "decision", None, &"x".repeat(MAX_ITEM_CHARS), T0)
        .expect("at the limit");
    for (kind, category, text) in [
        ("decision", None, "  "),
        ("bogus", None, "t"),
        ("checklist", Some("question"), "t"),
        ("next_move", Some("nope"), "t"),
    ] {
        assert!(
            matches!(
                s.add_item(P, "s", kind, category, text, T0),
                Err(StoreError::InvalidInput(_))
            ),
            "{kind} {category:?} {text:?}"
        );
    }
    assert!(matches!(
        s.add_item(P, "missing", "decision", None, "t", T0),
        Err(StoreError::NotFound)
    ));

    for i in 1..MAX_ITEMS_PER_SESSION {
        s.add_item(P, "s", "checklist", None, &format!("i{i}"), T0)
            .expect("add");
    }
    assert!(matches!(
        s.add_item(P, "s", "checklist", None, "one too many", T0),
        Err(StoreError::TooMany { limit: 200 })
    ));
    assert_eq!(s.items(P, "s").expect("items").len(), MAX_ITEMS_PER_SESSION);

    s.set_notes(P, "s", &"n".repeat(MAX_NOTES_CHARS), T0)
        .expect("notes at limit");
    assert!(matches!(
        s.set_notes(P, "s", &"n".repeat(MAX_NOTES_CHARS + 1), T0),
        Err(StoreError::InvalidInput(_))
    ));
    // Characters, not bytes.
    s.set_notes(P, "s", &"é".repeat(MAX_NOTES_CHARS), T0)
        .expect("multibyte at limit");
    assert!(matches!(
        s.set_notes(P, "nope", "n", T0),
        Err(StoreError::NotFound)
    ));
}

#[test]
fn captured_and_user_text_are_separate_and_clearable() {
    let mut s = store_with(&["s", "t"]);
    s.set_user_intent(P, "s", "my goal", T0).expect("intent");
    s.set_notes(P, "s", "my notes", T0).expect("notes");
    s.record_prompt(P, "s", "first prompt", T0).expect("prompt");
    s.record_prompt(P, "s", "second prompt", T0 + Duration::minutes(1))
        .expect("prompt");
    s.record_ask(P, "s", "Permission to use Bash", T0 + Duration::minutes(2))
        .expect("ask");
    s.record_prompt(P, "t", "other", T0).expect("prompt");

    let c = s.context(P, "s").expect("ctx").expect("row");
    let ci = c.captured_intent.as_ref().expect("ci");
    assert_eq!(ci.text, "first prompt", "the first prompt stays the intent");
    assert!(ci.captured_utc.starts_with("2026-11-05T10:00:00"));
    assert!(
        !format!("{c:?}").contains("second prompt"),
        "only the first prompt is kept"
    );
    assert_eq!(c.ask.as_ref().expect("ask").text, "Permission to use Bash");
    assert!(c.has_captured());

    s.clear_captured(P, "s").expect("clear");
    let c = s.context(P, "s").expect("ctx").expect("row");
    assert!(!c.has_captured());
    assert_eq!(
        (c.user_intent.as_deref(), c.notes.as_str()),
        (Some("my goal"), "my notes")
    );
    assert!(
        s.context(P, "t").expect("ctx").expect("row").has_captured(),
        "other session untouched"
    );

    s.clear_all_captured().expect("clear all");
    assert!(!s.context(P, "t").expect("ctx").expect("row").has_captured());

    s.set_user_intent(P, "s", "   ", T0).expect("blank removes");
    assert_eq!(
        s.context(P, "s").expect("ctx").expect("row").user_intent,
        None
    );
    assert!(matches!(
        s.record_prompt(P, "s", &"p".repeat(2001), T0),
        Err(StoreError::InvalidInput(_))
    ));
    assert!(matches!(
        s.record_ask(P, "s", &"p".repeat(301), T0),
        Err(StoreError::InvalidInput(_))
    ));
}

#[test]
fn event_log_never_holds_context_text() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("l.db");
    let (mut s, _) = Store::open(&path).expect("open");
    s.ingest(&ev(1, "s", "working", "2026-11-05T10:00:00Z"), T0)
        .expect("ingest");
    s.record_prompt(P, "s", "SENTINEL_PROMPT_TEXT", T0)
        .expect("prompt");
    s.record_ask(P, "s", "SENTINEL_ASK_TEXT", T0).expect("ask");
    s.set_notes(P, "s", "SENTINEL_NOTES_TEXT", T0)
        .expect("notes");
    s.ingest(&ev(2, "s", "needs_attention", "2026-11-05T10:01:00Z"), T0)
        .expect("ingest");

    let conn = Connection::open(&path).expect("raw");
    let mut stmt = conn
        .prepare("SELECT event_json || event_type || provider_session_id FROM events")
        .expect("prepare");
    let all: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .expect("query")
        .map(|r| r.expect("row"))
        .collect();
    assert_eq!(all.len(), 2);
    assert!(all.iter().all(|e| !e.contains("SENTINEL")));
}

#[test]
fn links_are_symmetric_idempotent_and_validated() {
    let mut s = store_with(&["a", "b"]);
    s.link_sessions((P, "b"), (P, "a"), T0).expect("link");
    s.link_sessions((P, "a"), (P, "b"), T0).expect("again");
    assert_eq!(
        s.links(P, "a").expect("links"),
        vec![(P.to_owned(), "b".to_owned())]
    );
    assert_eq!(
        s.links(P, "b").expect("links"),
        vec![(P.to_owned(), "a".to_owned())]
    );
    assert!(matches!(
        s.link_sessions((P, "a"), (P, "a"), T0),
        Err(StoreError::InvalidInput(_))
    ));
    assert!(matches!(
        s.link_sessions((P, "a"), (P, "zz"), T0),
        Err(StoreError::NotFound)
    ));
    s.unlink_sessions((P, "a"), (P, "b")).expect("unlink");
    assert!(s.links(P, "a").expect("links").is_empty());
}

#[test]
fn summaries_report_content_and_review_time() {
    let mut s = store_with(&["a", "b", "c"]);
    s.set_notes(P, "a", "x", T0).expect("notes");
    s.add_item(P, "b", "decision", None, "d", T0).expect("item");
    s.mark_reviewed(P, "a", T0 + Duration::hours(1))
        .expect("reviewed");
    s.set_notes(P, "c", "", T0).expect("empty notes only");
    let m = s.context_summaries().expect("summaries");
    let key = |id: &str| (P.to_owned(), id.to_owned());
    assert!(m[&key("a")].has_content);
    assert_eq!(m[&key("a")].reviewed_utc, Some(T0 + Duration::hours(1)));
    assert!(m[&key("b")].has_content);
    assert_eq!(m[&key("b")].reviewed_utc, None);
    assert!(!m[&key("c")].has_content, "an empty row is not content");
}

#[test]
fn timeline_is_oldest_first_limited_and_reason_only() {
    let mut s = Store::open_in_memory().expect("open");
    for n in 1..=5u32 {
        let e = parse_event(&format!(
            r#"{{"schemaVersion":1,"eventId":"00000000-0000-4000-8000-{n:012}","provider":"claude-code","providerSessionId":"s","eventType":"needs_attention","timestampUtc":"2026-11-05T10:00:0{n}Z","metadata":{{"reason":"permission","hook":"PermissionRequest"}}}}"#
        ))
        .expect("event");
        s.ingest(&e, T0 + Duration::seconds(i64::from(n)))
            .expect("ingest");
    }
    let t = s.timeline(P, "s", 3).expect("timeline");
    assert_eq!(t.len(), 3);
    assert_eq!(t[0].timestamp_utc, "2026-11-05T10:00:03Z");
    assert_eq!(t[2].timestamp_utc, "2026-11-05T10:00:05Z");
    assert_eq!(t[0].reason.as_deref(), Some("permission"));
    assert_eq!(t[0].event_type, "needs_attention");
}

#[test]
fn a_new_prompt_clears_the_answered_question() {
    let mut s = store_with(&["s"]);
    s.record_prompt(P, "s", "first", T0).expect("prompt");
    s.record_ask(P, "s", "Permission to use Bash", T0)
        .expect("ask");
    assert!(s.context(P, "s").expect("ctx").expect("row").ask.is_some());
    s.record_prompt(P, "s", "second", T0 + Duration::minutes(1))
        .expect("prompt");
    let c = s.context(P, "s").expect("ctx").expect("row");
    assert!(c.ask.is_none(), "the previous question was answered");
    assert_eq!(c.captured_intent.expect("intent").text, "first");
}

#[test]
fn secure_delete_is_on_and_deletes_checkpoint_the_wal() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("l.db");
    let (mut s, _) = Store::open(&path).expect("open");
    assert!(s.secure_delete_enabled().expect("pragma"));
    assert!(
        Store::open_in_memory()
            .expect("mem")
            .secure_delete_enabled()
            .expect("pragma")
    );
    s.ingest(&ev(1, "s", "working", "2026-11-05T10:00:00Z"), T0)
        .expect("ingest");
    s.record_prompt(P, "s", "SENTINEL_WAL_TEXT", T0)
        .expect("prompt");
    s.clear_captured(P, "s").expect("clear");
    let wal = dir.path().join("l.db-wal");
    let len = std::fs::metadata(&wal).map_or(0, |m| m.len());
    assert_eq!(len, 0, "the WAL is truncated after clearing captured text");
    s.record_prompt(P, "s", "SENTINEL_WAL_TEXT", T0)
        .expect("prompt");
    s.delete_session(P, "s").expect("delete");
    assert_eq!(std::fs::metadata(&wal).map_or(0, |m| m.len()), 0);
    let bytes = std::fs::read(&path).expect("read db");
    assert!(!String::from_utf8_lossy(&bytes).contains("SENTINEL_WAL_TEXT"));
}
