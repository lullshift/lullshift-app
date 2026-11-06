//! Sprint 08: migration 0006 (marketplaces, installed activities, built-in disable) and the
//! typed API over it.

use local_store::{
    ActivityKind, AddressChange, BUILTIN_MARKETPLACE_ID, MIGRATIONS, NewInstalledActivity,
    SavedState, Store, StoreError, latest_version, migrate,
};
use rusqlite::Connection;
use time::OffsetDateTime;
use time::macros::datetime;

const T0: OffsetDateTime = datetime!(2026-11-05 10:00:00 UTC);
const T1: OffsetDateTime = datetime!(2026-11-06 11:00:00 UTC);
const SHA_A: &str = "0123456789abcdef0123456789abcdef01234567";
const SHA_B: &str = "fedcba9876543210fedcba9876543210fedcba98";
const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn install(id: &str, market: &str) -> NewInstalledActivity {
    NewInstalledActivity {
        activity_id: id.to_owned(),
        marketplace_id: market.to_owned(),
        source_url: "https://example.org/team/pack".to_owned(),
        source_path: "packs/quiz".to_owned(),
        commit_sha: SHA_A.to_owned(),
        manifest_version: "1.0.0".to_owned(),
        kind: ActivityKind::Content,
        engine: Some("engine.trivia@1".to_owned()),
        permissions: vec!["storage".to_owned()],
        digest: DIGEST.to_owned(),
        size_bytes: 1234,
    }
}

fn invalid<T: std::fmt::Debug>(r: Result<T, StoreError>) {
    assert!(matches!(r, Err(StoreError::InvalidInput(_))), "{r:?}");
}

// ---- migration -------------------------------------------------------------------------

#[test]
fn latest_is_8_and_fresh_install_has_builtin_row() {
    assert_eq!(latest_version(), 12);
    let (s, report) =
        Store::open(&tempfile::tempdir().expect("dir").path().join("l.db")).expect("open");
    assert_eq!(
        (report.from_version, report.to_version),
        (0, latest_version())
    );
    assert!(report.backup.is_none());
    let list = s.marketplaces().expect("list");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, BUILTIN_MARKETPLACE_ID);
    assert!(list[0].builtin);
    assert!(list[0].last_fetched_commit.is_none());
    assert!(s.installed_activities().expect("installs").is_empty());
    assert!(s.disabled_builtins().expect("disabled").is_empty());
}

#[test]
fn version_5_database_upgrades_to_8_with_backup_and_keeps_data() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("l.db");
    {
        let conn = Connection::open(&path).expect("open raw");
        migrate(&conn, &MIGRATIONS[..5], None).expect("v5");
        conn.execute(
            "INSERT INTO sessions (provider, provider_session_id, state, state_timestamp_utc, last_seen_utc, last_received_utc)
             VALUES ('claude-code','s','working','2026-11-05T10:00:00Z','2026-11-05T10:00:00Z','2026-11-05T10:00:00Z')",
            [],
        )
        .expect("session");
        conn.execute(
            "INSERT INTO activity_snapshots VALUES ('app.x.game','{\"m\":1}','2026-11-05T10:00:00Z')",
            [],
        )
        .expect("snapshot");
        conn.execute(
            "INSERT INTO activity_play VALUES ('app.x.game', 90, 3, '2026-11-05T10:00:00Z')",
            [],
        )
        .expect("play");
    }
    let (s, report) = Store::open(&path).expect("upgrade");
    assert_eq!(
        (report.from_version, report.to_version),
        (5, latest_version())
    );
    let backup = report.backup.expect("backup before upgrade");
    assert!(backup.exists());
    assert_eq!(s.sessions().expect("sessions").len(), 1);
    assert!(s.snapshot("app.x.game").expect("get").is_some());
    assert_eq!(s.play_time("app.x.game").expect("play"), 90);
    assert_eq!(s.marketplaces().expect("list").len(), 1);

    // The backup is the untouched v5 database.
    let b = Connection::open(&backup).expect("open backup");
    let v: i64 = b
        .query_row("SELECT version FROM schema_version", [], |r| r.get(0))
        .expect("v");
    assert_eq!(v, 5);
    let has: bool = b
        .query_row(
            "SELECT EXISTS (SELECT 1 FROM sqlite_schema WHERE name = 'marketplaces')",
            [],
            |r| r.get(0),
        )
        .expect("q");
    assert!(!has);
}

#[test]
fn failed_version_6_migration_rolls_back_to_5_and_backup_is_usable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("l.db");
    {
        let conn = Connection::open(&path).expect("open raw");
        migrate(&conn, &MIGRATIONS[..5], None).expect("v5");
        conn.execute(
            "INSERT INTO activity_snapshots VALUES ('app.x.game','{}','2026-11-05T10:00:00Z')",
            [],
        )
        .expect("snapshot");
        // A clashing table makes step 6 fail after `marketplaces` and its seed would be created.
        conn.execute("CREATE TABLE installed_activities (x INTEGER)", [])
            .expect("clash");
    }
    assert!(matches!(
        Store::open(&path),
        Err(StoreError::Migration { version: 6, .. })
    ));
    let conn = Connection::open(&path).expect("reopen raw");
    let v: i64 = conn
        .query_row("SELECT max(version) FROM schema_version", [], |r| r.get(0))
        .expect("version");
    assert_eq!(v, 5, "left at the last good version");
    let n: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name IN ('marketplaces', 'activity_disabled')",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(n, 0, "the partial step was rolled back");
    drop(conn);

    let backups: Vec<_> = std::fs::read_dir(dir.path())
        .expect("ls")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "bak"))
        .collect();
    assert_eq!(backups.len(), 1, "backup written before migrating");
    let b = Connection::open(&backups[0]).expect("open backup");
    let snaps: i64 = b
        .query_row("SELECT count(*) FROM activity_snapshots", [], |r| r.get(0))
        .expect("snaps");
    assert_eq!(snaps, 1);
}

// ---- constraints and triggers ----------------------------------------------------------

fn raw_db() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("l.db");
    drop(Store::open(&path).expect("open"));
    let conn = Connection::open(&path).expect("raw");
    conn.pragma_update(None, "foreign_keys", "ON").expect("fk");
    (dir, conn)
}

fn insert_row(conn: &Connection, sha: &str, digest: &str, kind: &str) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT INTO installed_activities (activity_id, marketplace_id, source_url, source_path, commit_sha,
           manifest_version, kind, permissions, digest, size_bytes, installed_at, updated_at)
         VALUES ('a', 'lullshift', 'https://x.org/r', 'p', ?1, '1', ?3, '[]', ?2, 1, 't', 't')",
        [sha, digest, kind],
    )
}

#[test]
fn check_constraints_refuse_bad_sha_digest_and_kind() {
    let (_d, conn) = raw_db();
    insert_row(&conn, SHA_A, DIGEST, "content").expect("good row");
    conn.execute("DELETE FROM installed_activities", [])
        .expect("clear");
    for bad_sha in [
        "ABCDEF0123456789abcdef0123456789abcdef01",
        "0123456789abcdef0123456789abcdef0123456",
        "g123456789abcdef0123456789abcdef01234567",
        "",
    ] {
        assert!(
            insert_row(&conn, bad_sha, DIGEST, "app").is_err(),
            "{bad_sha}"
        );
    }
    for bad_digest in [&DIGEST[..63], &DIGEST.to_uppercase(), "zz"] {
        assert!(
            insert_row(&conn, SHA_A, bad_digest, "app").is_err(),
            "{bad_digest}"
        );
    }
    assert!(insert_row(&conn, SHA_A, DIGEST, "bundle").is_err());
    // Unknown marketplace: foreign key.
    assert!(
        conn.execute(
            "UPDATE installed_activities SET marketplace_id = 'nope'",
            []
        )
        .is_ok(),
        "no rows yet"
    );
    assert!(
        conn.execute(
            "INSERT INTO installed_activities (activity_id, marketplace_id, source_url, source_path, commit_sha,
               manifest_version, kind, permissions, digest, size_bytes, installed_at, updated_at)
             VALUES ('b', 'nope', 'https://x.org/r', 'p', ?1, '1', 'app', '[]', ?2, 1, 't', 't')",
            [SHA_A, DIGEST],
        )
        .is_err()
    );
    // Credentials in a URL, an engine on an app, permissions not an array.
    assert!(
        conn.execute(
            "UPDATE marketplaces SET url = 'https://u:tok@x.org/r' WHERE id = 'lullshift'",
            []
        )
        .is_err()
    );
    insert_row(&conn, SHA_A, DIGEST, "app").expect("row");
    assert!(
        conn.execute("UPDATE installed_activities SET engine = 'e@1'", [])
            .is_err()
    );
    assert!(
        conn.execute("UPDATE installed_activities SET permissions = '{}'", [])
            .is_err()
    );
    assert!(
        conn.execute("UPDATE installed_activities SET size_bytes = -1", [])
            .is_err()
    );
}

#[test]
fn builtin_row_cannot_be_deleted_or_marked_removed_even_in_sql() {
    let (_d, conn) = raw_db();
    assert!(
        conn.execute("DELETE FROM marketplaces WHERE id = 'lullshift'", [])
            .is_err()
    );
    assert!(
        conn.execute(
            "UPDATE marketplaces SET removed_at = 't' WHERE id = 'lullshift'",
            []
        )
        .is_err()
    );
    assert!(
        conn.execute(
            "UPDATE marketplaces SET builtin = 0 WHERE id = 'lullshift'",
            []
        )
        .is_err()
    );
    // Fetch bookkeeping on the built-in row is still allowed.
    conn.execute(
        "UPDATE marketplaces SET last_fetched_at = 't' WHERE id = 'lullshift'",
        [],
    )
    .expect("update");
}

// ---- Sprint 12: private-network marketplaces (decision 17) and migration 0011 ------------

#[test]
fn private_host_confirmation_is_recorded_and_revival_takes_the_new_answer() {
    let mut s = Store::open_in_memory().expect("store");
    let public = s
        .add_marketplace("acme", "https://github.com/acme/market", T0)
        .expect("add");
    assert!(!public.private_host, "the default is a public host");
    let private = s
        .add_marketplace_with_host("home", "https://192.168.1.10/me/market", true, T0)
        .expect("add private");
    assert!(private.private_host);
    assert!(
        s.marketplace("home")
            .expect("get")
            .expect("row")
            .private_host
    );

    // Removed while an install keeps the row, then added again without the confirmation.
    s.put_installed_activity(&install("com.home.x", "home"), T0)
        .expect("install");
    s.remove_marketplace("home", T1).expect("remove");
    let revived = s
        .add_marketplace_with_host("home", "https://192.168.1.10/me/market", false, T1)
        .expect("revive");
    assert!(!revived.private_host, "the new answer replaces the old one");
}

#[test]
fn an_existing_marketplace_can_be_confirmed_private_but_not_the_builtin_one() {
    let mut s = Store::open_in_memory().expect("store");
    s.add_marketplace("acme", "https://git.example.com/acme/market", T0)
        .expect("add");
    s.set_marketplace_private("acme", true).expect("confirm");
    assert!(
        s.marketplace("acme")
            .expect("get")
            .expect("row")
            .private_host
    );
    assert!(matches!(
        s.set_marketplace_private("lullshift", true),
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        s.set_marketplace_private("nope", true),
        Err(StoreError::NotFound)
    ));
}

#[test]
fn version_10_database_upgrades_to_11_and_marks_existing_marketplaces_public() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("l.db");
    {
        let conn = Connection::open(&path).expect("open raw");
        migrate(&conn, &MIGRATIONS[..10], None).expect("v10");
        conn.execute(
            "INSERT INTO marketplaces (id, url, added_at, builtin)
             VALUES ('acme', 'https://github.com/acme/market', '2026-11-05T10:00:00Z', 0)",
            [],
        )
        .expect("row");
    }
    let (s, report) = Store::open(&path).expect("upgrade");
    assert_eq!(report.from_version, 10);
    assert!(report.to_version >= 11);
    let backup = report.backup.expect("backup before upgrade");
    let acme = s.marketplace("acme").expect("get").expect("kept");
    assert!(!acme.private_host);
    let b = Connection::open(&backup).expect("open backup");
    let has: bool = b
        .query_row(
            "SELECT EXISTS (SELECT 1 FROM pragma_table_info('marketplaces') WHERE name = 'private_host')",
            [],
            |r| r.get(0),
        )
        .expect("q");
    assert!(!has, "the backup is the untouched v10 database");
}

#[test]
fn failed_version_11_migration_rolls_back_to_10() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("l.db");
    {
        let conn = Connection::open(&path).expect("open raw");
        migrate(&conn, &MIGRATIONS[..10], None).expect("v10");
        conn.execute("ALTER TABLE marketplaces ADD COLUMN private_host TEXT", [])
            .expect("clash");
    }
    assert!(matches!(
        Store::open(&path),
        Err(StoreError::Migration { version: 11, .. })
    ));
    let conn = Connection::open(&path).expect("reopen raw");
    let v: i64 = conn
        .query_row("SELECT max(version) FROM schema_version", [], |r| r.get(0))
        .expect("version");
    assert_eq!(v, 10, "left at the last good version");
}

#[test]
fn private_host_column_only_takes_0_or_1() {
    let (_d, conn) = raw_db();
    let bad = conn.execute(
        "INSERT INTO marketplaces (id, url, added_at, builtin, private_host)
         VALUES ('x', 'https://a.example/b', '2026-11-05T10:00:00Z', 0, 2)",
        [],
    );
    assert!(bad.is_err());
}

// ---- Sprint 12: changing a marketplace's address (decision 18.4) and migration 0012 ------

fn local_install(id: &str, market: &str, url: &str, sha: &str) -> NewInstalledActivity {
    NewInstalledActivity {
        source_url: url.to_owned(),
        commit_sha: sha.to_owned(),
        ..install(id, market)
    }
}

#[test]
fn changing_the_address_keeps_pins_marks_missing_commits_and_moves_owned_state() {
    let mut s = Store::open_in_memory().expect("store");
    let old = "https://git.example.com/acme/market";
    let new = "https://git.example.com/team/market";
    s.add_marketplace("acme", old, T0).expect("add");
    s.add_marketplace("other", "https://other.example/o/m", T0)
        .expect("add other");
    s.put_installed_activity(&local_install("com.acme.a", "acme", old, SHA_A), T0)
        .expect("a");
    s.put_installed_activity(&local_install("com.acme.b", "acme", old, SHA_B), T0)
        .expect("b");
    s.put_installed_activity(
        &local_install(
            "com.acme.ext",
            "acme",
            "https://elsewhere.example/x/y",
            SHA_A,
        ),
        T0,
    )
    .expect("external source");
    s.put_installed_activity(&local_install("com.other.c", "other", old, SHA_A), T0)
        .expect("other marketplace");
    // Kept state of an uninstalled activity from the old address.
    with_state(&mut s, "com.acme.gone");
    s.put_installed_activity(&local_install("com.acme.gone", "acme", old, SHA_A), T0)
        .expect("gone");
    s.remove_installed_activity("com.acme.gone", T0)
        .expect("uninstall keeping state");

    let found = vec!["com.acme.a".to_owned()];
    let missing = vec!["com.acme.b".to_owned()];
    s.change_marketplace_address(&AddressChange {
        id: "acme",
        new_url: new,
        private_host: false,
        commit: SHA_B,
        found: &found,
        missing: &missing,
        at: T1,
    })
    .expect("change");

    let m = s.marketplace("acme").expect("get").expect("row");
    assert_eq!(m.url, new);
    assert_eq!(m.last_fetched_commit.as_deref(), Some(SHA_B));
    assert_eq!(m.last_fetched_at, Some(T1));

    let a = s.installed_activity("com.acme.a").expect("a").expect("a");
    assert_eq!((a.source_url.as_str(), a.commit_sha.as_str()), (new, SHA_A));
    let b = s.installed_activity("com.acme.b").expect("b").expect("b");
    assert_eq!((b.source_url.as_str(), b.commit_sha.as_str()), (old, SHA_B));
    assert!(b.enabled, "a missing commit doesn't turn anything off");
    assert_eq!(
        s.install_source_notes().expect("notes"),
        [("com.acme.b".to_owned(), "notFoundAtNewAddress".to_owned())]
    );
    let ext = s.installed_activity("com.acme.ext").expect("e").expect("e");
    assert_eq!(ext.source_url, "https://elsewhere.example/x/y");
    let c = s.installed_activity("com.other.c").expect("c").expect("c");
    assert_eq!(c.source_url, old, "other marketplaces are untouched");

    // Saved progress kept from the old address still belongs to this marketplace.
    assert_eq!(
        s.saved_state_for("com.acme.gone", "acme", new)
            .expect("state"),
        SavedState::Own
    );

    // Updating the noted install clears the note.
    s.put_installed_activity(&local_install("com.acme.b", "acme", new, SHA_A), T1)
        .expect("update");
    assert!(s.install_source_notes().expect("notes").is_empty());
}

#[test]
fn changing_the_address_refuses_unknown_builtin_and_credential_urls() {
    let mut s = Store::open_in_memory().expect("store");
    s.add_marketplace("acme", "https://git.example.com/a/m", T0)
        .expect("add");
    let change = |id: &'static str, url: &'static str| AddressChange {
        id,
        new_url: url,
        private_host: false,
        commit: SHA_A,
        found: &[],
        missing: &[],
        at: T1,
    };
    assert!(matches!(
        s.change_marketplace_address(&change("lullshift", "https://x.example/a/b")),
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        s.change_marketplace_address(&change("nope", "https://x.example/a/b")),
        Err(StoreError::NotFound)
    ));
    invalid(s.change_marketplace_address(&change("acme", "https://u:pw@x.example/a/b")));
    assert_eq!(
        s.marketplace("acme").expect("get").expect("row").url,
        "https://git.example.com/a/m"
    );
}

#[test]
fn version_11_database_upgrades_to_12_and_keeps_installs_without_notes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("l.db");
    {
        let conn = Connection::open(&path).expect("open raw");
        migrate(&conn, &MIGRATIONS[..11], None).expect("v11");
        conn.execute(
            "INSERT INTO marketplaces (id, url, added_at, builtin)
             VALUES ('acme', 'https://github.com/acme/market', '2026-11-05T10:00:00Z', 0)",
            [],
        )
        .expect("row");
        conn.execute(
            &format!(
                "INSERT INTO installed_activities (activity_id, marketplace_id, source_url,
                   source_path, commit_sha, manifest_version, kind, engine, permissions, digest,
                   size_bytes, installed_at, updated_at)
                 VALUES ('com.acme.a', 'acme', 'https://github.com/acme/market', 'p', '{SHA_A}',
                   '1.0.0', 'content', 'engine.trivia@1', '[]', '{DIGEST}', 1,
                   '2026-11-05T10:00:00Z', '2026-11-05T10:00:00Z')"
            ),
            [],
        )
        .expect("install");
    }
    let (s, report) = Store::open(&path).expect("upgrade");
    assert_eq!(report.from_version, 11);
    assert!(report.backup.is_some());
    assert!(s.installed_activity("com.acme.a").expect("get").is_some());
    assert!(s.install_source_notes().expect("notes").is_empty());
}

#[test]
fn failed_version_12_migration_rolls_back_to_11() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("l.db");
    {
        let conn = Connection::open(&path).expect("open raw");
        migrate(&conn, &MIGRATIONS[..11], None).expect("v11");
        conn.execute(
            "ALTER TABLE installed_activities ADD COLUMN source_note INTEGER",
            [],
        )
        .expect("clash");
    }
    assert!(matches!(
        Store::open(&path),
        Err(StoreError::Migration { version: 12, .. })
    ));
    let conn = Connection::open(&path).expect("reopen raw");
    let v: i64 = conn
        .query_row("SELECT max(version) FROM schema_version", [], |r| r.get(0))
        .expect("version");
    assert_eq!(v, 11, "left at the last good version");
}

#[test]
fn source_note_only_takes_the_known_value() {
    let (_d, conn) = raw_db();
    conn.execute(
        "INSERT INTO marketplaces (id, url, added_at, builtin)
         VALUES ('acme', 'https://a.example/b', '2026-11-05T10:00:00Z', 0)",
        [],
    )
    .expect("market");
    let insert = |note: &str| {
        conn.execute(
            &format!(
                "INSERT INTO installed_activities (activity_id, marketplace_id, source_url,
                   source_path, commit_sha, manifest_version, kind, permissions, digest,
                   size_bytes, installed_at, updated_at, source_note)
                 VALUES ('a.{note}', 'acme', 'https://a.example/b', '', '{SHA_A}', '1.0.0',
                   'app', '[]', '{DIGEST}', 1, 't', 't', '{note}')"
            ),
            [],
        )
    };
    assert!(insert("notFoundAtNewAddress").is_ok());
    assert!(insert("whatever").is_err());
}

// ---- API: marketplaces -----------------------------------------------------------------

#[test]
fn builtin_is_listed_first_and_removal_is_refused() {
    let mut s = Store::open_in_memory().expect("store");
    s.add_marketplace("acme", "https://github.com/acme/market", T0)
        .expect("add");
    let ids: Vec<_> = s
        .marketplaces()
        .expect("list")
        .into_iter()
        .map(|m| m.id)
        .collect();
    assert_eq!(ids, ["lullshift", "acme"]);
    assert!(matches!(
        s.remove_marketplace("lullshift", T1),
        Err(StoreError::BuiltinProtected)
    ));
    assert!(s.marketplace("lullshift").expect("get").is_some());
}

#[test]
fn add_marketplace_validates_and_refuses_duplicates() {
    let mut s = Store::open_in_memory().expect("store");
    let m = s
        .add_marketplace("acme", "https://github.com/acme/market", T0)
        .expect("add");
    assert_eq!((m.id.as_str(), m.builtin, m.added_at), ("acme", false, T0));
    assert!(matches!(
        s.add_marketplace("acme", "https://github.com/acme/other", T1),
        Err(StoreError::AlreadyExists)
    ));
    invalid(s.add_marketplace("lullshift", "https://github.com/x/y", T0));
    invalid(s.add_marketplace("", "https://github.com/x/y", T0));
    invalid(s.add_marketplace(&"a".repeat(65), "https://github.com/x/y", T0));
    invalid(s.add_marketplace("Upper", "https://github.com/x/y", T0));
    invalid(s.add_marketplace("-lead", "https://github.com/x/y", T0));
    invalid(s.add_marketplace("ok", "", T0));
    invalid(s.add_marketplace("ok", "https://host/ repo", T0));
    invalid(s.add_marketplace("ok", &format!("https://h/{}", "a".repeat(2100)), T0));
    assert_eq!(s.marketplaces().expect("list").len(), 2);
}

#[test]
fn urls_with_credentials_are_refused_but_scp_form_is_fine() {
    let mut s = Store::open_in_memory().expect("store");
    // secretlint-disable-next-line -- fake credential: the test checks it is refused
    invalid(s.add_marketplace("a", "https://user:ghp_secret@github.com/x/y", T0));
    invalid(s.add_marketplace("a", "https://ghp_secret@github.com/x/y", T0));
    invalid(s.add_marketplace("a", "ssh://git:pw@host/x/y", T0));
    invalid(s.add_marketplace("a", "user:pw@host:x/y", T0));
    s.add_marketplace("a", "git@github.com:x/y.git", T0)
        .expect("scp form");
    s.add_marketplace("b", "ssh://host.example/x/y", T0)
        .expect("ssh");
    let mut bad = install("pack.one", "a");
    bad.source_url = "https://tok@host/x".to_owned();
    invalid(s.put_installed_activity(&bad, T0));
}

#[test]
fn record_fetch_stores_commit_and_time_and_validates() {
    let mut s = Store::open_in_memory().expect("store");
    s.add_marketplace("acme", "https://github.com/acme/market", T0)
        .expect("add");
    s.record_fetch("acme", SHA_A, T1).expect("fetch");
    let m = s.marketplace("acme").expect("get").expect("exists");
    assert_eq!(m.last_fetched_commit.as_deref(), Some(SHA_A));
    assert_eq!(m.last_fetched_at, Some(T1));
    invalid(s.record_fetch("acme", "main", T1));
    invalid(s.record_fetch("acme", &SHA_A.to_uppercase(), T1));
    assert!(matches!(
        s.record_fetch("nope", SHA_A, T1),
        Err(StoreError::NotFound)
    ));
    // The built-in row can record fetches of its own.
    s.record_fetch("lullshift", SHA_B, T1)
        .expect("builtin fetch");
}

#[test]
fn removing_a_marketplace_without_installs_deletes_it() {
    let mut s = Store::open_in_memory().expect("store");
    s.add_marketplace("acme", "https://github.com/acme/market", T0)
        .expect("add");
    s.remove_marketplace("acme", T1).expect("remove");
    assert!(s.marketplace("acme").expect("get").is_none());
    assert!(matches!(
        s.remove_marketplace("acme", T1),
        Err(StoreError::NotFound)
    ));
    // Id is free again.
    s.add_marketplace("acme", "https://github.com/acme/other", T1)
        .expect("re-add");
}

#[test]
fn removing_a_marketplace_keeps_its_installs_and_revival_reuses_the_row() {
    let mut s = Store::open_in_memory().expect("store");
    s.add_marketplace("acme", "https://github.com/acme/market", T0)
        .expect("add");
    s.record_fetch("acme", SHA_A, T0).expect("fetch");
    s.put_installed_activity(&install("pack.one", "acme"), T0)
        .expect("install");
    s.remove_marketplace("acme", T1).expect("remove");

    assert!(s.marketplace("acme").expect("get").is_none());
    assert_eq!(s.marketplaces().expect("list").len(), 1);
    assert!(matches!(
        s.record_fetch("acme", SHA_B, T1),
        Err(StoreError::NotFound)
    ));
    let kept = s
        .installed_activity("pack.one")
        .expect("get")
        .expect("kept");
    assert_eq!(kept.marketplace_id, "acme");

    // A different address is refused: the installs must not silently change source (M1).
    assert!(matches!(
        s.add_marketplace("acme", "https://github.com/acme/moved", T1),
        Err(StoreError::AlreadyExists)
    ));
    assert!(
        s.marketplace("acme").expect("get").is_none(),
        "still removed"
    );
    let again = s
        .add_marketplace("acme", "https://github.com/acme/market", T1)
        .expect("revive from the same address");
    assert_eq!(again.url, "https://github.com/acme/market");
    assert!(again.last_fetched_commit.is_none(), "fetch record reset");
    assert!(s.installed_activity("pack.one").expect("get").is_some());

    // Remove again, then uninstall the last activity: the hidden row goes with it.
    s.remove_marketplace("acme", T1).expect("remove");
    assert!(
        s.remove_installed_activity("pack.one", T1)
            .expect("remove install")
    );
    s.add_marketplace("acme", "https://github.com/acme/fresh", T1)
        .expect("fresh add");
}

// ---- API: installed activities ---------------------------------------------------------

#[test]
fn install_round_trips_and_lists_sorted() {
    let s = Store::open_in_memory().expect("store");
    let a = s
        .put_installed_activity(&install("pack.b", "lullshift"), T0)
        .expect("b");
    s.put_installed_activity(&install("pack.a", "lullshift"), T0)
        .expect("a");
    assert_eq!(a.activity_id, "pack.b");
    assert_eq!(a.kind, ActivityKind::Content);
    assert_eq!(a.engine.as_deref(), Some("engine.trivia@1"));
    assert_eq!(a.permissions, ["storage"]);
    assert_eq!((a.size_bytes, a.enabled), (1234, true));
    assert_eq!((a.installed_at, a.updated_at), (T0, T0));
    let ids: Vec<_> = s
        .installed_activities()
        .expect("list")
        .into_iter()
        .map(|i| i.activity_id)
        .collect();
    assert_eq!(ids, ["pack.a", "pack.b"]);
    assert!(s.installed_activity("missing").expect("get").is_none());
}

#[test]
fn put_replaces_atomically_and_keeps_install_time_and_enabled_choice() {
    let s = Store::open_in_memory().expect("store");
    s.put_installed_activity(&install("pack.one", "lullshift"), T0)
        .expect("v1");
    s.set_installed_enabled("pack.one", false, T0)
        .expect("disable");

    let mut next = install("pack.one", "lullshift");
    next.commit_sha = SHA_B.to_owned();
    next.manifest_version = "1.1.0".to_owned();
    next.permissions = vec!["storage".to_owned(), "audio".to_owned()];
    next.size_bytes = 99;
    let updated = s.put_installed_activity(&next, T1).expect("swap");
    assert_eq!(updated.commit_sha, SHA_B);
    assert_eq!(updated.manifest_version, "1.1.0");
    assert_eq!(updated.permissions.len(), 2);
    assert_eq!((updated.installed_at, updated.updated_at), (T0, T1));
    assert!(!updated.enabled, "an update does not re-enable");
    assert_eq!(s.installed_activities().expect("list").len(), 1);

    // A rejected replacement leaves the old record exactly as it was.
    let mut bad = next.clone();
    bad.commit_sha = "nope".to_owned();
    invalid(s.put_installed_activity(&bad, T1));
    assert_eq!(
        s.installed_activity("pack.one").expect("get"),
        Some(updated)
    );
}

#[test]
fn put_validates_every_field() {
    let s = Store::open_in_memory().expect("store");
    let mut cases: Vec<NewInstalledActivity> = Vec::new();
    let mut with = |f: &dyn Fn(&mut NewInstalledActivity)| {
        let mut a = install("pack.one", "lullshift");
        f(&mut a);
        cases.push(a);
    };
    with(&|a| a.commit_sha = SHA_A.to_uppercase());
    with(&|a| a.commit_sha = SHA_A[..39].to_owned());
    with(&|a| a.commit_sha = "main".to_owned());
    with(&|a| a.digest = DIGEST[..63].to_owned());
    with(&|a| a.digest = DIGEST.to_uppercase());
    with(&|a| a.activity_id = String::new());
    with(&|a| a.activity_id = "a".repeat(129));
    with(&|a| a.activity_id = "has space".to_owned());
    with(&|a| a.marketplace_id = "Bad Id".to_owned());
    with(&|a| a.kind = ActivityKind::App); // engine on an app
    with(&|a| a.engine = Some(String::new()));
    with(&|a| a.permissions = vec![String::new()]);
    with(&|a| a.manifest_version = String::new());
    with(&|a| a.source_path = "a\nb".to_owned());
    with(&|a| a.size_bytes = u64::MAX);
    for c in &cases {
        invalid(s.put_installed_activity(c, T0));
    }
    assert!(s.installed_activities().expect("list").is_empty());

    // An app without an engine is fine.
    let mut app = install("app.one", "lullshift");
    app.kind = ActivityKind::App;
    app.engine = None;
    assert_eq!(
        s.put_installed_activity(&app, T0).expect("app").kind,
        ActivityKind::App
    );

    // 128-char id is the accepted maximum.
    s.put_installed_activity(&install(&"a".repeat(128), "lullshift"), T0)
        .expect("max id");
}

#[test]
fn install_for_an_unknown_marketplace_is_not_found() {
    let s = Store::open_in_memory().expect("store");
    assert!(matches!(
        s.put_installed_activity(&install("pack.one", "ghost"), T0),
        Err(StoreError::NotFound)
    ));
}

#[test]
fn enable_disable_and_remove_installed() {
    let mut s = Store::open_in_memory().expect("store");
    s.put_installed_activity(&install("pack.one", "lullshift"), T0)
        .expect("install");
    s.set_installed_enabled("pack.one", false, T1).expect("off");
    let i = s
        .installed_activity("pack.one")
        .expect("get")
        .expect("some");
    assert!(!i.enabled);
    assert_eq!(i.updated_at, T1);
    s.set_installed_enabled("pack.one", true, T1).expect("on");
    assert!(
        s.installed_activity("pack.one")
            .expect("get")
            .expect("some")
            .enabled
    );
    assert!(matches!(
        s.set_installed_enabled("ghost", true, T1),
        Err(StoreError::NotFound)
    ));

    // Uninstall removes the record only; saved state stays for the caller to decide.
    s.save_snapshot("pack.one", "{}", T0, 100)
        .expect("snapshot");
    assert!(s.remove_installed_activity("pack.one", T1).expect("remove"));
    assert!(!s.remove_installed_activity("pack.one", T1).expect("again"));
    assert!(s.installed_activity("pack.one").expect("get").is_none());
    assert!(s.snapshot("pack.one").expect("snap").is_some());
}

// ---- API: built-in disable -------------------------------------------------------------

#[test]
fn builtin_activities_can_be_disabled_and_reenabled() {
    let s = Store::open_in_memory().expect("store");
    assert!(!s.is_builtin_disabled("app.lullshift.game.x").expect("q"));
    s.set_builtin_disabled("app.lullshift.game.x", true, T0)
        .expect("off");
    s.set_builtin_disabled("app.lullshift.game.x", true, T1)
        .expect("idempotent");
    s.set_builtin_disabled("app.lullshift.break.y", true, T0)
        .expect("off");
    assert!(s.is_builtin_disabled("app.lullshift.game.x").expect("q"));
    assert_eq!(
        s.disabled_builtins().expect("list"),
        ["app.lullshift.break.y", "app.lullshift.game.x"]
    );
    s.set_builtin_disabled("app.lullshift.game.x", false, T1)
        .expect("on");
    s.set_builtin_disabled("never.disabled", false, T1)
        .expect("noop");
    assert!(!s.is_builtin_disabled("app.lullshift.game.x").expect("q"));
    invalid(s.set_builtin_disabled("", true, T0));
    invalid(s.set_builtin_disabled(&"a".repeat(129), true, T0));
}

#[test]
fn records_survive_restart() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("l.db");
    {
        let (mut s, _) = Store::open(&path).expect("open");
        s.add_marketplace("acme", "https://github.com/acme/market", T0)
            .expect("add");
        s.put_installed_activity(&install("pack.one", "acme"), T0)
            .expect("install");
        s.set_builtin_disabled("app.x", true, T0).expect("disable");
    }
    let (s, report) = Store::open(&path).expect("reopen");
    assert!(report.backup.is_none(), "no upgrade, no backup");
    assert_eq!(s.marketplaces().expect("list").len(), 2);
    assert!(s.installed_activity("pack.one").expect("get").is_some());
    assert!(s.is_builtin_disabled("app.x").expect("q"));
}

#[test]
fn clearing_activity_state_removes_only_that_activity() {
    let mut store = Store::open_in_memory().expect("store");
    for id in ["com.example.a", "com.example.b"] {
        store
            .set_activity_value(id, "best", "42", 4096)
            .expect("value");
        store
            .save_snapshot(id, "{\"n\":1}", T0, 4096)
            .expect("snapshot");
        store.add_play_time(id, 60, true, 3600, T0).expect("play");
    }
    store.clear_activity_state("com.example.a").expect("clear");
    assert_eq!(
        store.activity_value("com.example.a", "best").expect("get"),
        None
    );
    assert_eq!(store.snapshot("com.example.a").expect("snap"), None);
    assert_eq!(store.play_time("com.example.a").expect("play"), 0);
    assert_eq!(
        store
            .activity_value("com.example.b", "best")
            .expect("get")
            .as_deref(),
        Some("42")
    );
    assert!(store.snapshot("com.example.b").expect("snap").is_some());
    assert_eq!(store.play_time("com.example.b").expect("play"), 60);
    // Idempotent, and a bad id is refused.
    store.clear_activity_state("com.example.a").expect("again");
    invalid(store.clear_activity_state(""));
}

// ---- review fixes: saved-state owner (L7) and migration 0007 ----------------------------

fn with_state(s: &mut Store, id: &str) {
    s.set_activity_value(id, "best", "42", 4096).expect("value");
    s.save_snapshot(id, "{\"n\":1}", T0, 4096)
        .expect("snapshot");
}

#[test]
fn kept_state_is_cleared_when_installed_from_a_different_source() {
    let mut s = Store::open_in_memory().expect("store");
    for m in ["acme", "other"] {
        s.add_marketplace(m, &format!("https://example.org/{m}"), T0)
            .expect("add");
    }
    s.put_installed_activity(&install("pack.one", "acme"), T0)
        .expect("install");
    with_state(&mut s, "pack.one");
    assert!(s.remove_installed_activity("pack.one", T1).expect("remove"));

    let src = "https://example.org/team/pack";
    assert_eq!(
        s.saved_state_for("pack.one", "acme", src).expect("q"),
        local_store::SavedState::Own
    );
    assert_eq!(
        s.saved_state_for("pack.one", "other", src).expect("q"),
        local_store::SavedState::Foreign
    );
    assert_eq!(
        s.saved_state_for("pack.one", "acme", "https://elsewhere.example/x")
            .expect("q"),
        local_store::SavedState::Foreign
    );
    // Asking changes nothing.
    assert!(s.snapshot("pack.one").expect("snap").is_some());

    // Claiming for a different marketplace removes the state, and only that activity's.
    with_state(&mut s, "pack.two");
    let got = s
        .claim_saved_state("pack.one", "other", src)
        .expect("claim");
    assert_eq!(got, local_store::SavedState::Foreign);
    assert!(s.snapshot("pack.one").expect("snap").is_none());
    assert_eq!(s.activity_value("pack.one", "best").expect("v"), None);
    assert!(s.snapshot("pack.two").expect("snap").is_some());
    assert_eq!(
        s.saved_state_for("pack.one", "acme", src).expect("q"),
        local_store::SavedState::None
    );
}

#[test]
fn kept_state_is_reused_by_the_same_source_and_owner_row_is_dropped() {
    let mut s = Store::open_in_memory().expect("store");
    s.add_marketplace("acme", "https://example.org/acme", T0)
        .expect("add");
    s.put_installed_activity(&install("pack.one", "acme"), T0)
        .expect("install");
    with_state(&mut s, "pack.one");
    s.remove_installed_activity("pack.one", T1).expect("remove");
    let src = "https://example.org/team/pack";
    let got = s.claim_saved_state("pack.one", "acme", src).expect("claim");
    assert_eq!(got, local_store::SavedState::Own);
    assert!(s.snapshot("pack.one").expect("snap").is_some(), "kept");
    // With the owner row gone, a later look says "unknown" rather than trusting a stale owner.
    assert_eq!(
        s.saved_state_for("pack.one", "other", src).expect("q"),
        local_store::SavedState::Unknown
    );
}

#[test]
fn state_without_an_owner_is_unknown_and_kept_and_deleting_it_leaves_no_owner() {
    let mut s = Store::open_in_memory().expect("store");
    with_state(&mut s, "pack.old");
    let src = "https://example.org/team/pack";
    assert_eq!(
        s.claim_saved_state("pack.old", "acme", src).expect("claim"),
        local_store::SavedState::Unknown
    );
    assert!(s.snapshot("pack.old").expect("snap").is_some());
    assert_eq!(
        s.saved_state_for("pack.nothing", "acme", src).expect("q"),
        local_store::SavedState::None
    );

    // Uninstall with no saved state records no owner; "delete saved state" drops it.
    s.add_marketplace("acme", "https://example.org/acme", T0)
        .expect("add");
    s.put_installed_activity(&install("pack.one", "acme"), T0)
        .expect("install");
    s.remove_installed_activity("pack.one", T1).expect("remove");
    with_state(&mut s, "pack.one");
    assert_eq!(
        s.saved_state_for("pack.one", "acme", src).expect("q"),
        local_store::SavedState::Unknown,
        "state written after the uninstall has no owner"
    );
    s.put_installed_activity(&install("pack.one", "acme"), T0)
        .expect("install");
    s.remove_installed_activity("pack.one", T1).expect("remove");
    s.clear_activity_state("pack.one").expect("clear");
    with_state(&mut s, "pack.one");
    assert_eq!(
        s.saved_state_for("pack.one", "other", src).expect("q"),
        local_store::SavedState::Unknown,
        "clearing removed the owner row"
    );
}

#[test]
fn version_6_database_upgrades_to_8_with_backup_and_keeps_state() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("l.db");
    {
        let conn = Connection::open(&path).expect("open raw");
        migrate(&conn, &MIGRATIONS[..6], None).expect("v6");
        conn.execute(
            "INSERT INTO activity_snapshots VALUES ('app.x.game','{\"m\":1}','2026-11-05T10:00:00Z')",
            [],
        )
        .expect("snapshot");
    }
    let (s, report) = Store::open(&path).expect("upgrade");
    assert_eq!(
        (report.from_version, report.to_version),
        (6, latest_version())
    );
    let backup = report.backup.expect("backup before upgrade");
    assert!(s.snapshot("app.x.game").expect("get").is_some());
    let b = Connection::open(&backup).expect("open backup");
    let has: bool = b
        .query_row(
            "SELECT EXISTS (SELECT 1 FROM sqlite_schema WHERE name = 'activity_state_owner')",
            [],
            |r| r.get(0),
        )
        .expect("q");
    assert!(!has, "the backup is the untouched v6 database");
}

#[test]
fn failed_version_7_migration_rolls_back_to_6() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("l.db");
    {
        let conn = Connection::open(&path).expect("open raw");
        migrate(&conn, &MIGRATIONS[..6], None).expect("v6");
        conn.execute("CREATE TABLE activity_state_owner (x INTEGER)", [])
            .expect("clash");
    }
    assert!(matches!(
        Store::open(&path),
        Err(StoreError::Migration { version: 7, .. })
    ));
    let conn = Connection::open(&path).expect("reopen raw");
    let v: i64 = conn
        .query_row("SELECT max(version) FROM schema_version", [], |r| r.get(0))
        .expect("version");
    assert_eq!(v, 6, "left at the last good version");
}
