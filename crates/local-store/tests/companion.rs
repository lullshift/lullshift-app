//! Companion storage (Sprint 09, ADR-020): migration 8 and the profile/thread/message/usage API.

use local_store::{
    CompanionProfile, MIGRATIONS, NewUserMessage, Store, StoreError, latest_version, migrate,
    month_of,
};
use rusqlite::Connection;
use time::OffsetDateTime;
use time::macros::datetime;

const T0: OffsetDateTime = datetime!(2026-11-05 10:00 UTC);

fn profile(id: &str, default: bool) -> CompanionProfile {
    CompanionProfile {
        id: id.to_owned(),
        kind: "openaiCompatible".to_owned(),
        label: format!("Profile {id}"),
        base_url: "http://localhost:11434/v1".to_owned(),
        model: "llama3".to_owned(),
        monthly_token_budget: Some(1000),
        is_default: default,
    }
}

fn user_msg<'a>(thread: i64, text: &'a str, profile: &'a str) -> NewUserMessage<'a> {
    NewUserMessage {
        thread_id: thread,
        text,
        attached_context: Some("CONTEXT-TEXT"),
        scope_kind: "sessionSummary",
        profile_id: profile,
        model: "llama3",
        host: "localhost:11434",
    }
}

fn open() -> Store {
    Store::open_in_memory().expect("store")
}

// -- migration

#[test]
fn latest_is_at_least_10_and_fresh_install_has_the_companion_tables_without_key_columns() {
    assert!(latest_version() >= 10);
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("l.db");
    let (s, report) = Store::open(&path).expect("open");
    assert_eq!(
        (report.from_version, report.to_version),
        (0, latest_version())
    );
    assert!(report.backup.is_none());
    assert!(s.companion_profiles().expect("p").is_empty());
    drop(s);
    let conn = Connection::open(&path).expect("raw");
    for table in [
        "companion_profiles",
        "companion_threads",
        "companion_messages",
        "companion_usage",
    ] {
        let mut stmt = conn
            .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
            .expect("info");
        let cols: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .expect("q")
            .map(|r| r.expect("col"))
            .collect();
        assert!(!cols.is_empty(), "{table} exists");
        for c in cols {
            for banned in ["key", "secret", "password", "hash", "last4", "last_four"] {
                assert!(!c.contains(banned), "{table}.{c} looks like key material");
            }
        }
    }
}

#[test]
fn version_7_database_upgrades_to_8_with_backup_and_keeps_data() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("l.db");
    {
        let conn = Connection::open(&path).expect("raw");
        migrate(&conn, &MIGRATIONS[..7], None).expect("v7");
        conn.execute(
            "INSERT INTO preferences (key, value_json, updated_utc) VALUES ('k', '1', '2026-11-05T10:00:00Z')",
            [],
        )
        .expect("pref");
    }
    let (s, report) = Store::open(&path).expect("upgrade");
    assert_eq!(
        (report.from_version, report.to_version),
        (7, latest_version())
    );
    let backup = report.backup.expect("backup before upgrade");
    assert_eq!(s.preference("k").expect("pref").as_deref(), Some("1"));
    assert!(s.companion_profiles().expect("p").is_empty());
    let b = Connection::open(&backup).expect("backup");
    let has: bool = b
        .query_row(
            "SELECT EXISTS (SELECT 1 FROM sqlite_schema WHERE name = 'companion_profiles')",
            [],
            |r| r.get(0),
        )
        .expect("q");
    assert!(!has, "the backup is the untouched v7 database");
    let v: i64 = b
        .query_row("SELECT version FROM schema_version", [], |r| r.get(0))
        .expect("v");
    assert_eq!(v, 7);
}

#[test]
fn failed_version_8_migration_rolls_back_to_7() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("l.db");
    {
        let conn = Connection::open(&path).expect("raw");
        migrate(&conn, &MIGRATIONS[..7], None).expect("v7");
        // A clash on the second table: the first one the step made must be rolled back too.
        conn.execute("CREATE TABLE companion_threads (x INTEGER)", [])
            .expect("clash");
    }
    assert!(matches!(
        Store::open(&path),
        Err(StoreError::Migration { version: 8, .. })
    ));
    let conn = Connection::open(&path).expect("reopen");
    let v: i64 = conn
        .query_row("SELECT max(version) FROM schema_version", [], |r| r.get(0))
        .expect("version");
    assert_eq!(v, 7, "left at the last good version");
    let partial: bool = conn
        .query_row(
            "SELECT EXISTS (SELECT 1 FROM sqlite_schema WHERE name = 'companion_profiles')",
            [],
            |r| r.get(0),
        )
        .expect("q");
    assert!(!partial, "no half-applied step");
}

#[test]
fn constraints_refuse_bad_rows_even_in_raw_sql() {
    let dir = tempfile::tempdir().expect("dir");
    let (_s, _) = Store::open(&dir.path().join("l.db")).expect("open");
    let conn = Connection::open(dir.path().join("l.db")).expect("raw");
    let ins = |kind: &str, url: &str, def: i64| {
        conn.execute(
            "INSERT INTO companion_profiles (id, kind, label, base_url, model, is_default, created_utc, updated_utc)
             VALUES (hex(randomblob(4)), ?1, 'l', ?2, 'm', ?3, 't', 't')",
            rusqlite::params![kind, url, def],
        )
    };
    assert!(ins("evil", "https://x.example", 0).is_err());
    assert!(ins("openai", "https://user:pw@x.example", 0).is_err());
    assert!(ins("openai", "https://x.example", 1).is_ok());
    assert!(
        ins("openai", "https://y.example", 1).is_err(),
        "only one default"
    );
    assert!(
        conn.execute(
            "INSERT INTO companion_usage (profile_id, month, input_tokens, output_tokens, recorded_utc)
             VALUES ('nope', '2026-11', 1, 1, 't')",
            []
        )
        .is_err(),
        "usage needs a profile"
    );
}

// -- profiles

#[test]
fn first_profile_is_default_and_default_moves() {
    let mut s = open();
    let a = s
        .companion_save_profile(&profile("a", false), T0)
        .expect("a");
    assert!(a.is_default);
    let b = s
        .companion_save_profile(&profile("b", true), T0)
        .expect("b");
    assert!(b.is_default);
    let list = s.companion_profiles().expect("list");
    assert_eq!(list.iter().filter(|p| p.is_default).count(), 1);
    assert_eq!(list[0].id, "b", "default first");
    // Saving a without asking for default keeps b as the default.
    s.companion_save_profile(&profile("a", false), T0)
        .expect("a again");
    assert!(
        s.companion_profile("b")
            .expect("b")
            .expect("some")
            .is_default
    );
    // Deleting the default promotes the oldest remaining.
    s.companion_delete_profile("b").expect("delete");
    assert!(
        s.companion_profile("a")
            .expect("a")
            .expect("some")
            .is_default
    );
    assert!(matches!(
        s.companion_delete_profile("b"),
        Err(StoreError::NotFound)
    ));
}

#[test]
fn profile_values_are_checked() {
    let mut s = open();
    let mut p = profile("a", false);
    p.kind = "weird".into();
    assert!(matches!(
        s.companion_save_profile(&p, T0),
        Err(StoreError::InvalidInput(_))
    ));
    let mut p = profile("a", false);
    p.monthly_token_budget = Some(0);
    assert!(s.companion_save_profile(&p, T0).is_err());
    let mut p = profile("a", false);
    p.base_url = "https://u:p@h.example".into();
    assert!(s.companion_save_profile(&p, T0).is_err());
    assert!(s.companion_profiles().expect("list").is_empty());
}

#[test]
fn profile_limit() {
    let mut s = open();
    for i in 0..local_store::MAX_PROFILES {
        s.companion_save_profile(&profile(&format!("p{i}"), false), T0)
            .expect("save");
    }
    assert!(matches!(
        s.companion_save_profile(&profile("extra", false), T0),
        Err(StoreError::TooMany { .. })
    ));
    // Updating an existing one still works.
    s.companion_save_profile(&profile("p0", false), T0)
        .expect("update");
}

// -- threads and messages

#[test]
fn thread_and_message_lifecycle() {
    let mut s = open();
    s.companion_save_profile(&profile("a", false), T0)
        .expect("p");
    let t = s.companion_new_thread(None, T0).expect("thread");
    assert_eq!(t.title, local_store::DEFAULT_THREAD_TITLE);
    let id = s
        .companion_add_user_message(&user_msg(t.id, "hello", "a"), T0)
        .expect("add");
    let m = s.companion_message(id).expect("get").expect("some");
    assert_eq!((m.status.as_str(), m.role.as_str()), ("pending", "user"));
    assert_eq!(m.attached_context.as_deref(), Some("CONTEXT-TEXT"));
    assert_eq!(m.scope_kind, "sessionSummary");
    assert_eq!(m.host.as_deref(), Some("localhost:11434"));
    let later = T0 + time::Duration::minutes(1);
    let reply = s
        .companion_complete_send(id, "hi there", 12, 5, later)
        .expect("complete");
    assert_eq!(
        (reply.role.as_str(), reply.status.as_str()),
        ("assistant", "sent")
    );
    assert_eq!(reply.output_tokens, Some(5));
    // The host is stored on the question and copied to the reply, so history can be kept to one host.
    assert_eq!(reply.host.as_deref(), Some("localhost:11434"));
    let msgs = s.companion_messages(t.id).expect("msgs");
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].status, "sent");
    assert_eq!(msgs[0].input_tokens, Some(12));
    let th = s.companion_thread(t.id).expect("t").expect("some");
    assert_eq!(th.profile_id.as_deref(), Some("a"));
    assert_eq!(th.model.as_deref(), Some("llama3"));
    assert!(th.updated_utc > th.created_utc);
    let u = s.companion_usage(&month_of(later)).expect("usage");
    assert_eq!((u[0].input_tokens, u[0].output_tokens), (12, 5));
}

#[test]
fn failed_then_retried() {
    let mut s = open();
    s.companion_save_profile(&profile("a", false), T0)
        .expect("p");
    let t = s
        .companion_new_thread(Some("  Plans "), T0)
        .expect("thread");
    assert_eq!(t.title, "Plans");
    let id = s
        .companion_add_user_message(&user_msg(t.id, "x", "a"), T0)
        .expect("add");
    s.companion_fail_message(id, "offline", "Can't reach the provider.")
        .expect("fail");
    let m = s.companion_message(id).expect("get").expect("some");
    assert_eq!(m.status, "failed");
    assert_eq!(m.error_kind.as_deref(), Some("offline"));
    // Only failed messages go back to pending.
    s.companion_mark_pending(id).expect("pending");
    assert!(matches!(
        s.companion_mark_pending(id),
        Err(StoreError::NotFound)
    ));
    s.companion_complete_send(id, "ok", 1, 1, T0)
        .expect("complete");
    let m = s.companion_message(id).expect("get").expect("some");
    assert_eq!((m.status.as_str(), m.error_kind), ("sent", None));
}

#[test]
fn message_limits_and_unknown_ids() {
    let mut s = open();
    s.companion_save_profile(&profile("a", false), T0)
        .expect("p");
    let t = s.companion_new_thread(None, T0).expect("thread");
    assert!(
        s.companion_add_user_message(&user_msg(t.id, "  ", "a"), T0)
            .is_err()
    );
    let long = "x".repeat(local_store::MAX_MESSAGE_CHARS + 1);
    assert!(
        s.companion_add_user_message(&user_msg(t.id, &long, "a"), T0)
            .is_err()
    );
    assert!(
        s.companion_add_user_message(&user_msg(9999, "hi", "a"), T0)
            .is_err()
    );
    assert!(
        s.companion_add_user_message(&user_msg(t.id, "hi", "nope"), T0)
            .is_err()
    );
    assert!(matches!(
        s.companion_complete_send(9999, "r", 0, 0, T0),
        Err(StoreError::NotFound)
    ));
    assert!(s.companion_new_thread(Some(&"t".repeat(121)), T0).is_err());
    assert!(s.companion_rename_thread(t.id, "").is_err());
    assert!(matches!(
        s.companion_rename_thread(9999, "x"),
        Err(StoreError::NotFound)
    ));
}

#[test]
fn stale_pending_become_failed_and_cancelled() {
    let mut s = open();
    s.companion_save_profile(&profile("a", false), T0)
        .expect("p");
    let t = s.companion_new_thread(None, T0).expect("thread");
    let id = s
        .companion_add_user_message(&user_msg(t.id, "x", "a"), T0)
        .expect("add");
    assert_eq!(s.companion_fail_stale_pending().expect("n"), 1);
    let m = s.companion_message(id).expect("g").expect("some");
    assert_eq!(
        (m.status.as_str(), m.error_kind.as_deref()),
        ("failed", Some("cancelled"))
    );
    assert_eq!(s.companion_fail_stale_pending().expect("n"), 0);
}

#[test]
fn deleting_a_thread_removes_messages_and_the_text_from_the_file() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("l.db");
    let (mut s, _) = Store::open(&path).expect("open");
    s.companion_save_profile(&profile("a", false), T0)
        .expect("p");
    let t = s.companion_new_thread(None, T0).expect("thread");
    let id = s
        .companion_add_user_message(&user_msg(t.id, "UNIQUE-DELETE-ME-TEXT", "a"), T0)
        .expect("add");
    s.companion_complete_send(id, "UNIQUE-REPLY-TEXT", 1, 1, T0)
        .expect("c");
    s.companion_delete_thread(t.id).expect("delete");
    assert!(s.companion_messages(t.id).expect("m").is_empty());
    assert!(matches!(
        s.companion_delete_thread(t.id),
        Err(StoreError::NotFound)
    ));
    drop(s);
    for name in ["l.db", "l.db-wal"] {
        if let Ok(bytes) = std::fs::read(dir.path().join(name)) {
            for needle in ["UNIQUE-DELETE-ME-TEXT", "UNIQUE-REPLY-TEXT", "CONTEXT-TEXT"] {
                assert!(
                    !bytes.windows(needle.len()).any(|w| w == needle.as_bytes()),
                    "{needle} lingers in {name}"
                );
            }
        }
    }
}

// -- usage

#[test]
fn usage_is_per_profile_per_utc_month() {
    let mut s = open();
    s.companion_save_profile(&profile("a", false), T0)
        .expect("a");
    s.companion_save_profile(&profile("b", false), T0)
        .expect("b");
    s.companion_record_usage("a", datetime!(2026-10-31 23:59 UTC), 100, 100)
        .expect("oct");
    s.companion_record_usage("a", datetime!(2026-11-01 00:00 UTC), 10, 20)
        .expect("nov");
    s.companion_record_usage("a", datetime!(2026-11-30 23:59 UTC), 1, 2)
        .expect("nov2");
    assert_eq!(month_of(datetime!(2026-11-01 00:00 UTC)), "2026-11");
    let nov = s.companion_usage("2026-11").expect("nov");
    let a = nov.iter().find(|u| u.profile_id == "a").expect("a");
    assert_eq!((a.input_tokens, a.output_tokens), (11, 22));
    let b = nov.iter().find(|u| u.profile_id == "b").expect("b");
    assert_eq!(
        (b.input_tokens, b.output_tokens),
        (0, 0),
        "no usage shows zero"
    );
    let oct = s.companion_usage("2026-10").expect("oct");
    assert_eq!(
        oct.iter()
            .find(|u| u.profile_id == "a")
            .expect("a")
            .input_tokens,
        100
    );
    assert!(s.companion_record_usage("missing", T0, 1, 1).is_err());
}

#[test]
fn deleting_a_profile_deletes_usage_and_unlinks_threads_but_keeps_text() {
    let mut s = open();
    s.companion_save_profile(&profile("a", false), T0)
        .expect("a");
    let t = s.companion_new_thread(None, T0).expect("thread");
    let id = s
        .companion_add_user_message(&user_msg(t.id, "keep me", "a"), T0)
        .expect("add");
    s.companion_complete_send(id, "reply", 3, 4, T0).expect("c");
    s.companion_delete_profile("a").expect("delete");
    assert!(s.companion_usage("2026-11").expect("u").is_empty());
    let th = s.companion_thread(t.id).expect("t").expect("some");
    assert_eq!(th.profile_id, None);
    let msgs = s.companion_messages(t.id).expect("m");
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].text, "keep me");
    assert_eq!(msgs[0].profile_id, None);
}

#[test]
fn version_8_database_upgrades_to_9_with_backup_and_keeps_companion_data() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("l.db");
    {
        let conn = Connection::open(&path).expect("raw");
        migrate(&conn, &MIGRATIONS[..8], None).expect("v8");
        conn.execute(
            "INSERT INTO companion_profiles (id, kind, label, base_url, model, is_default, created_utc, updated_utc)
             VALUES ('p', 'openai', 'L', 'https://api.openai.com/v1', 'm', 1, 't', 't')",
            [],
        )
        .expect("profile");
        conn.execute(
            "INSERT INTO companion_threads (title, created_utc, updated_utc) VALUES ('T', 't', 't')",
            [],
        )
        .expect("thread");
        conn.execute(
            "INSERT INTO companion_messages (thread_id, role, text, status, created_utc)
             VALUES (1, 'user', 'old message', 'sent', 't')",
            [],
        )
        .expect("message");
    }
    let (s, report) = Store::open(&path).expect("upgrade");
    assert_eq!(
        (report.from_version, report.to_version),
        (8, latest_version())
    );
    let backup = report.backup.expect("backup before upgrade");
    let msgs = s.companion_messages(1).expect("msgs");
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].text, "old message");
    assert_eq!(msgs[0].host, None, "old rows have no host");
    assert_eq!(s.companion_profiles().expect("p").len(), 1);
    let b = Connection::open(&backup).expect("backup");
    let cols: Vec<String> = b
        .prepare("SELECT name FROM pragma_table_info('companion_messages')")
        .expect("info")
        .query_map([], |r| r.get(0))
        .expect("q")
        .map(|r| r.expect("c"))
        .collect();
    assert!(!cols.iter().any(|c| c == "host"), "backup is the v8 file");
    let v: i64 = b
        .query_row("SELECT version FROM schema_version", [], |r| r.get(0))
        .expect("v");
    assert_eq!(v, 8);
}

#[test]
fn failed_version_9_migration_rolls_back_to_8() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("l.db");
    {
        let conn = Connection::open(&path).expect("raw");
        migrate(&conn, &MIGRATIONS[..8], None).expect("v8");
        // Make the step fail: the column name is already taken by a view-free trick, a trigger
        // would not stop ALTER, so drop the table the step needs.
        conn.execute("ALTER TABLE companion_messages ADD COLUMN host TEXT", [])
            .expect("clash");
    }
    assert!(matches!(
        Store::open(&path),
        Err(StoreError::Migration { version: 9, .. })
    ));
    let conn = Connection::open(&path).expect("reopen");
    let v: i64 = conn
        .query_row("SELECT max(version) FROM schema_version", [], |r| r.get(0))
        .expect("version");
    assert_eq!(v, 8, "left at the last good version");
}
