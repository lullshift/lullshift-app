//! Several models per Companion provider (decision 36): migration 17 from a database with real
//! data, its backup, rollback, recovery and idempotence, and the model API.

use local_store::{
    CompanionProfile, MAX_MODELS_PER_PROFILE, MIGRATIONS, Migration, NewUserMessage, Store,
    StoreError, latest_version, migrate, month_of,
};
use rusqlite::{Connection, params};
use time::OffsetDateTime;
use time::macros::datetime;

const T0: OffsetDateTime = datetime!(2026-11-05 10:00 UTC);

/// Every step before 17, whatever is merged in between (16 belongs to another change).
fn before_17() -> Vec<Migration> {
    MIGRATIONS
        .iter()
        .copied()
        .filter(|m| m.version < 17)
        .collect()
}

fn version(conn: &Connection) -> i64 {
    conn.query_row("SELECT max(version) FROM schema_version", [], |r| r.get(0))
        .expect("version")
}

fn has_column(conn: &Connection, table: &str, column: &str) -> bool {
    conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2)",
        params![table, column],
        |r| r.get(0),
    )
    .expect("pragma")
}

/// A pre-17 database with two profiles, a thread with messages from both, and usage.
fn seed_old(path: &std::path::Path) {
    let conn = Connection::open(path).expect("raw");
    migrate(&conn, &before_17(), None).expect("old schema");
    conn.execute_batch(
        "INSERT INTO companion_profiles
           (id, kind, label, base_url, model, monthly_token_budget, is_default, created_utc, updated_utc)
         VALUES
           ('p-a', 'anthropic', 'Work', 'https://api.anthropic.com', 'claude-x', 5000, 1,
            '2026-11-01T10:00:00Z', '2026-11-01T10:00:00Z'),
           ('p-b', 'openaiCompatible', 'Local', 'http://localhost:11434/v1', 'llama3', NULL, 0,
            '2026-11-02T10:00:00Z', '2026-11-02T10:00:00Z');
         INSERT INTO companion_threads (id, title, profile_id, model, created_utc, updated_utc)
         VALUES (1, 'Plans', 'p-b', 'llama3', '2026-11-03T10:00:00Z', '2026-11-03T10:00:00Z');
         INSERT INTO companion_messages
           (thread_id, role, text, scope_kind, status, profile_id, model, host, created_utc)
         VALUES
           (1, 'user', 'hi', 'none', 'sent', 'p-a', 'claude-x', 'api.anthropic.com', '2026-11-03T10:00:00Z'),
           (1, 'assistant', 'hello', 'none', 'sent', 'p-a', 'claude-x', 'api.anthropic.com', '2026-11-03T10:00:01Z'),
           (1, 'assistant', 'old reply', 'none', 'sent', NULL, NULL, NULL, '2026-11-03T10:00:02Z');
         INSERT INTO companion_usage (profile_id, month, input_tokens, output_tokens, recorded_utc)
         VALUES ('p-a', '2026-11', 120, 30, '2026-11-03T10:00:01Z');",
    )
    .expect("seed");
}

#[test]
fn old_database_upgrades_with_backup_and_keeps_every_row() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("l.db");
    seed_old(&path);
    let old_version = version(&Connection::open(&path).expect("raw"));

    let (s, report) = Store::open(&path).expect("open");
    assert_eq!(report.from_version, old_version);
    assert_eq!(report.to_version, latest_version());

    // Each profile's model became its provider's default model.
    let profiles = s.companion_profiles().expect("profiles");
    assert_eq!(profiles.len(), 2);
    let a = profiles.iter().find(|p| p.id == "p-a").expect("a");
    let b = profiles.iter().find(|p| p.id == "p-b").expect("b");
    assert_eq!(
        (a.model.as_str(), a.is_default, a.monthly_token_budget),
        ("claude-x", true, Some(5000))
    );
    assert_eq!((b.model.as_str(), b.is_default), ("llama3", false));
    let models = s.companion_models(None).expect("models");
    assert_eq!(models.len(), 2);
    assert!(models.iter().all(|m| m.is_default && m.label.is_none()));
    assert_eq!(models[0].created_utc, "2026-11-01T10:00:00Z");

    // Threads, messages and usage are unchanged; old rows without a model stay without one.
    let t = s.companion_thread(1).expect("t").expect("some");
    assert_eq!(t.model.as_deref(), Some("llama3"));
    let msgs = s.companion_messages(1).expect("msgs");
    assert_eq!(msgs.len(), 3);
    assert_eq!(msgs[1].model.as_deref(), Some("claude-x"));
    assert_eq!(msgs[1].profile_id.as_deref(), Some("p-a"));
    assert_eq!(msgs[2].model, None);
    let usage = s.companion_usage("2026-11").expect("usage");
    let ua = usage.iter().find(|u| u.profile_id == "p-a").expect("ua");
    assert_eq!((ua.input_tokens, ua.output_tokens), (120, 30));

    // The old `model` column is gone from profiles, and kept on threads and messages.
    let conn = Connection::open(&path).expect("raw");
    assert!(!has_column(&conn, "companion_profiles", "model"));
    assert!(has_column(&conn, "companion_threads", "model"));
    assert!(has_column(&conn, "companion_messages", "model"));

    // The backup is the old database, untouched and still holding the single model per profile.
    let bak = report.backup.expect("backup before upgrading");
    let b = Connection::open(&bak).expect("open backup");
    assert_eq!(version(&b), old_version);
    assert!(has_column(&b, "companion_profiles", "model"));
    let m: String = b
        .query_row(
            "SELECT model FROM companion_profiles WHERE id = 'p-b'",
            [],
            |r| r.get(0),
        )
        .expect("old model");
    assert_eq!(m, "llama3");
}

#[test]
fn upgrading_twice_is_a_no_op() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("l.db");
    seed_old(&path);
    drop(Store::open(&path).expect("first open"));
    let (s, report) = Store::open(&path).expect("second open");
    assert_eq!(report.from_version, report.to_version);
    assert_eq!(
        report.backup, None,
        "nothing migrates, nothing is backed up"
    );
    assert_eq!(s.companion_models(None).expect("models").len(), 2);

    // Running the migration list again on the same connection changes nothing either.
    let conn = Connection::open(&path).expect("raw");
    let again = migrate(&conn, MIGRATIONS, None).expect("again");
    assert_eq!(again.from_version, again.to_version);
    let n: i64 = conn
        .query_row("SELECT count(*) FROM companion_models", [], |r| r.get(0))
        .expect("count");
    assert_eq!(n, 2);
}

#[test]
fn failed_upgrade_rolls_back_and_a_later_open_recovers() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("l.db");
    seed_old(&path);
    let old_version;
    {
        let conn = Connection::open(&path).expect("raw");
        old_version = version(&conn);
        // A clash late in the step: the table insert works, a second object fails. The whole
        // step must be rolled back, including the column drop that would follow.
        conn.execute(
            "CREATE INDEX companion_models_one_default ON companion_threads (id)",
            [],
        )
        .expect("clash");
    }
    assert!(matches!(
        Store::open(&path),
        Err(StoreError::Migration { version: 17, .. })
    ));
    let conn = Connection::open(&path).expect("reopen");
    assert_eq!(version(&conn), old_version, "left at the last good version");
    assert!(
        has_column(&conn, "companion_profiles", "model"),
        "column kept"
    );
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS (SELECT 1 FROM sqlite_schema WHERE name = 'companion_models' AND type = 'table')",
            [],
            |r| r.get(0),
        )
        .expect("q");
    assert!(!exists, "no half-applied step");
    let n: i64 = conn
        .query_row("SELECT count(*) FROM companion_profiles", [], |r| r.get(0))
        .expect("count");
    assert_eq!(n, 2, "profiles kept");
    // A backup was written before the attempt.
    let baks = std::fs::read_dir(dir.path())
        .expect("ls")
        .filter(|e| {
            e.as_ref()
                .is_ok_and(|e| e.file_name().to_string_lossy().ends_with(".bak"))
        })
        .count();
    assert!(baks >= 1);

    // Recovery: once the clash is gone, the next open upgrades and keeps everything.
    conn.execute("DROP INDEX companion_models_one_default", [])
        .expect("drop clash");
    drop(conn);
    let (s, _) = Store::open(&path).expect("recovered");
    let p = s.companion_profile("p-a").expect("a").expect("some");
    assert_eq!(p.model, "claude-x");
}

fn open() -> Store {
    Store::open_in_memory().expect("store")
}

fn profile(id: &str, model: &str) -> CompanionProfile {
    CompanionProfile {
        id: id.to_owned(),
        kind: "openai".to_owned(),
        label: format!("Profile {id}"),
        base_url: "https://api.openai.com".to_owned(),
        model: model.to_owned(),
        monthly_token_budget: None,
        is_default: false,
    }
}

#[test]
fn models_are_added_listed_defaulted_and_removed() {
    let mut s = open();
    s.companion_save_profile(&profile("a", "gpt-a"), T0)
        .expect("a");
    let added = s
        .companion_add_model("a", "gpt-b", Some("  Fast  "), false, T0)
        .expect("b");
    assert_eq!(added.label.as_deref(), Some("Fast"));
    assert!(!added.is_default);
    let list = s.companion_models(Some("a")).expect("list");
    assert_eq!(
        list.iter().map(|m| m.model.as_str()).collect::<Vec<_>>(),
        ["gpt-a", "gpt-b"],
        "default first"
    );

    // Adding again updates the label only; an empty label clears it.
    let again = s
        .companion_add_model("a", "gpt-b", Some(""), false, T0)
        .expect("again");
    assert_eq!(again.label, None);
    assert_eq!(s.companion_models(Some("a")).expect("l").len(), 2);

    s.companion_set_default_model("a", "gpt-b")
        .expect("default");
    assert_eq!(
        s.companion_profile("a").expect("a").expect("some").model,
        "gpt-b"
    );
    assert_eq!(
        s.companion_models(Some("a"))
            .expect("l")
            .iter()
            .filter(|m| m.is_default)
            .count(),
        1
    );
    assert!(matches!(
        s.companion_set_default_model("a", "nope"),
        Err(StoreError::NotFound)
    ));

    // Removing the default promotes the oldest remaining model.
    s.companion_remove_model("a", "gpt-b").expect("remove");
    let m = s.companion_model("a", "gpt-a").expect("m").expect("some");
    assert!(m.is_default);
    // The last model can't go.
    assert!(matches!(
        s.companion_remove_model("a", "gpt-a"),
        Err(StoreError::InvalidInput(_))
    ));
    assert!(matches!(
        s.companion_remove_model("a", "gone"),
        Err(StoreError::NotFound)
    ));
}

#[test]
fn saving_a_profile_with_a_new_model_adds_it_as_default() {
    let mut s = open();
    s.companion_save_profile(&profile("a", "gpt-a"), T0)
        .expect("a");
    s.companion_save_profile(&profile("a", "gpt-c"), T0)
        .expect("a again");
    let list = s.companion_models(Some("a")).expect("list");
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].model, "gpt-c");
    assert!(list[0].is_default);
    // An empty model is refused and nothing changes.
    assert!(matches!(
        s.companion_save_profile(&profile("b", " "), T0),
        Err(StoreError::InvalidInput(_))
    ));
    assert!(s.companion_profile("b").expect("b").is_none());
}

#[test]
fn model_limits_and_unknown_providers() {
    let mut s = open();
    assert!(matches!(
        s.companion_add_model("nope", "m", None, false, T0),
        Err(StoreError::NotFound)
    ));
    s.companion_save_profile(&profile("a", "m0"), T0)
        .expect("a");
    for i in 1..MAX_MODELS_PER_PROFILE {
        s.companion_add_model("a", &format!("m{i}"), None, false, T0)
            .expect("add");
    }
    assert!(matches!(
        s.companion_add_model("a", "one-more", None, false, T0),
        Err(StoreError::TooMany { .. })
    ));
    // Re-adding a listed one is still fine at the limit.
    s.companion_add_model("a", "m3", Some("Three"), false, T0)
        .expect("relabel");
    assert!(matches!(
        s.companion_add_model("a", "x", Some(&"y".repeat(81)), false, T0),
        Err(StoreError::InvalidInput(_)) | Err(StoreError::TooMany { .. })
    ));
}

#[test]
fn deleting_a_provider_removes_its_models_and_keeps_messages() {
    let mut s = open();
    s.companion_save_profile(&profile("a", "gpt-a"), T0)
        .expect("a");
    s.companion_add_model("a", "gpt-b", None, false, T0)
        .expect("b");
    let t = s.companion_new_thread(None, T0).expect("t");
    let id = s
        .companion_add_user_message(
            &NewUserMessage {
                thread_id: t.id,
                text: "hi",
                attached_context: None,
                scope_kind: "none",
                profile_id: "a",
                model: "gpt-b",
                host: "api.openai.com",
            },
            T0,
        )
        .expect("msg");
    s.companion_complete_send(id, "hello", 1, 1, T0)
        .expect("done");
    assert_eq!(s.companion_usage(&month_of(T0)).expect("u").len(), 1);
    s.companion_delete_profile("a").expect("delete");
    assert!(s.companion_models(None).expect("models").is_empty());
    let msgs = s.companion_messages(t.id).expect("msgs");
    assert_eq!(
        msgs[1].model.as_deref(),
        Some("gpt-b"),
        "the answer keeps its model"
    );
    assert_eq!(msgs[1].profile_id, None);
}

/// A version 15 database (before decision 34's migration 16 and this change's 17) with a
/// session and a provider goes through 16 and then 17 in one open, with one backup of the v15 file.
#[test]
fn version_15_database_upgrades_through_16_and_17_with_data() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("l.db");
    {
        let conn = Connection::open(&path).expect("raw");
        let v15: Vec<Migration> = MIGRATIONS
            .iter()
            .copied()
            .filter(|m| m.version <= 15)
            .collect();
        migrate(&conn, &v15, None).expect("v15");
        conn.execute_batch(
            "INSERT INTO sessions (provider, provider_session_id, state, state_timestamp_utc,
               last_seen_utc, last_received_utc)
             VALUES ('claude-code', 'old', 'idle', '2026-11-06T10:00:00Z',
               '2026-11-06T10:00:00Z', '2026-11-06T10:00:00Z');
             INSERT INTO companion_profiles
               (id, kind, label, base_url, model, monthly_token_budget, is_default, created_utc, updated_utc)
             VALUES ('p-a', 'anthropic', 'Work', 'https://api.anthropic.com', 'claude-x', NULL, 1,
               '2026-11-01T10:00:00Z', '2026-11-01T10:00:00Z');",
        )
        .expect("seed");
    }
    let (s, report) = Store::open(&path).expect("upgrade");
    assert_eq!(
        (report.from_version, report.to_version),
        (15, latest_version())
    );
    assert!(latest_version() >= 17);

    // 16: the session is kept and is not an imported one.
    let conn = Connection::open(&path).expect("raw");
    assert!(has_column(&conn, "sessions", "imported_utc"));
    let imported: Option<i64> = conn
        .query_row(
            "SELECT imported_utc FROM sessions WHERE provider_session_id = 'old'",
            [],
            |r| r.get(0),
        )
        .expect("session kept");
    assert_eq!(imported, None);
    assert!(s.imported_sessions().expect("imported").is_empty());

    // 17: the profile's model became its default model.
    let p = s.companion_profile("p-a").expect("read").expect("kept");
    assert_eq!(p.model, "claude-x");
    let models = s.companion_models(Some("p-a")).expect("models");
    assert_eq!(models.len(), 1);
    assert!(models[0].is_default);

    // One backup, of the version 15 file.
    let bak = report.backup.expect("backup");
    let b = Connection::open(&bak).expect("open backup");
    assert_eq!(version(&b), 15);
    assert!(!has_column(&b, "sessions", "imported_utc"));
    assert!(has_column(&b, "companion_profiles", "model"));
}
