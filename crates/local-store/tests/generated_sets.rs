//! Migration 21 (ADR-028 section 7): model-written sets. Upgrade with data, backup, rollback and
//! recovery, idempotence, and the whole storage API: insert, get, list, skips, mark unusable,
//! delete (one, all) with the snapshot/storage/play-time cascade, and prune.

use local_store::{
    GENERATED_ID_PREFIX, KEEP_GENERATED_SETS_PER_ENGINE, MIGRATIONS, Migration, NewGeneratedSet,
    Store, StoreError, latest_version, migrate,
};
use rusqlite::Connection;
use time::macros::datetime;
use time::{Duration, OffsetDateTime};

const T0: OffsetDateTime = datetime!(2026-11-08 10:00 UTC);
const QUIZ: &str = "lullshift.quiz@1";
const CARDS: &str = "lullshift.cards@1";
const DATA: &str = r#"{"title":"T","questions":[]}"#;

fn before_21() -> Vec<Migration> {
    MIGRATIONS
        .iter()
        .copied()
        .filter(|m| m.version < 21)
        .collect()
}

fn version(conn: &Connection) -> i64 {
    conn.query_row("SELECT max(version) FROM schema_version", [], |r| r.get(0))
        .expect("version")
}

fn has_table(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [name],
        |r| r.get(0),
    )
    .expect("pragma")
}

fn seed_old(path: &std::path::Path) {
    let conn = Connection::open(path).expect("raw");
    migrate(&conn, &before_21(), None).expect("old schema");
    conn.execute_batch(
        "INSERT INTO activity_snapshots VALUES ('app.x.game', '{\"m\":1}', '2026-11-05T10:00:00Z');
         INSERT INTO activity_storage VALUES ('app.x.game', 'k', 'v', '2026-11-05T10:00:00Z');
         INSERT INTO activity_play VALUES ('app.x.game', 30, 1, '2026-11-05T10:00:00Z');",
    )
    .expect("seed");
}

fn id(n: usize) -> String {
    format!("{GENERATED_ID_PREFIX}quiz.{n:012}")
}

fn new<'a>(id: &'a str, engine: &'a str, at: OffsetDateTime) -> NewGeneratedSet<'a> {
    NewGeneratedSet {
        id,
        engine,
        topic: "Rust ownership",
        difficulty: "gentle",
        data_json: DATA,
        created: at,
        provider_host: "api.anthropic.com",
        model: Some("m-1"),
        origin_pack: Some("lullshift.quiz.rust"),
    }
}

fn add(s: &mut Store, n: usize, engine: &str, minutes: i64) -> String {
    let i = id(n);
    s.insert_generated_set(&new(&i, engine, T0 + Duration::minutes(minutes)))
        .expect("insert");
    i
}

// ---- migration ----

#[test]
fn old_database_upgrades_with_backup_and_keeps_every_row() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("l.db");
    seed_old(&path);
    let old = version(&Connection::open(&path).expect("raw"));
    assert_eq!(old, 20);

    let (mut s, report) = Store::open(&path).expect("open");
    assert_eq!(
        (report.from_version, report.to_version),
        (20, latest_version())
    );
    assert!(latest_version() >= 21);
    assert!(s.snapshot("app.x.game").expect("snap").is_some());
    assert_eq!(
        s.activity_value("app.x.game", "k").expect("kv").as_deref(),
        Some("v")
    );
    assert_eq!(s.play_time("app.x.game").expect("play"), 30);
    assert!(s.generated_sets(None).expect("list").is_empty());
    add(&mut s, 1, QUIZ, 0);
    assert_eq!(s.generated_sets(None).expect("list").len(), 1);

    let conn = Connection::open(&path).expect("raw");
    assert!(has_table(&conn, "generated_sets") && has_table(&conn, "generated_set_skips"));
    let b = Connection::open(report.backup.expect("backup before upgrading")).expect("backup");
    assert_eq!(version(&b), 20);
    assert!(!has_table(&b, "generated_sets"));
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
}

#[test]
fn failed_upgrade_rolls_back_and_a_later_open_recovers() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("l.db");
    seed_old(&path);
    {
        let conn = Connection::open(&path).expect("raw");
        // A clashing table: the step fails after generated_sets is created, inside its
        // transaction, so that table must be rolled back too.
        conn.execute_batch("CREATE TABLE generated_set_skips (x INTEGER)")
            .expect("clash");
    }
    assert!(matches!(
        Store::open(&path),
        Err(StoreError::Migration { version: 21, .. })
    ));
    let conn = Connection::open(&path).expect("reopen");
    assert_eq!(version(&conn), 20, "left at the last good version");
    assert!(!has_table(&conn, "generated_sets"), "no half-applied step");
    let n: i64 = conn
        .query_row("SELECT count(*) FROM activity_snapshots", [], |r| r.get(0))
        .expect("count");
    assert_eq!(n, 1, "saved state kept");

    conn.execute_batch("DROP TABLE generated_set_skips")
        .expect("drop clash");
    drop(conn);
    let (s, _) = Store::open(&path).expect("recovered");
    assert!(s.snapshot("app.x.game").expect("snap").is_some());
}

#[test]
fn an_older_build_refuses_a_version_21_database_and_the_backup_restores_it() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("l.db");
    seed_old(&path);
    let (s, report) = Store::open(&path).expect("open");
    drop(s);
    let conn = Connection::open(&path).expect("raw");
    assert!(matches!(
        migrate(&conn, &before_21(), None),
        Err(StoreError::TooNew {
            found: 21..,
            supported: 20
        })
    ));
    let old = Connection::open(report.backup.expect("backup")).expect("backup");
    assert_eq!(version(&old), 20);
    let again = migrate(&old, &before_21(), None).expect("old build opens the backup");
    assert_eq!(again.from_version, again.to_version);
}

#[test]
fn table_rejects_ids_outside_the_namespace_and_bad_flags() {
    let s = Store::open_in_memory().expect("open");
    drop(s);
    let conn = Connection::open_in_memory().expect("raw");
    migrate(&conn, MIGRATIONS, None).expect("schema");
    let ins = |id: &str, unusable: i64| {
        conn.execute(
            "INSERT INTO generated_sets
               (id, engine, topic, difficulty, data_json, created_utc, provider_host, unusable)
             VALUES (?1, 'lullshift.quiz@1', 't', 'gentle', '{}', '2026-11-08T10:00:00Z', 'h', ?2)",
            rusqlite::params![id, unusable],
        )
    };
    assert!(ins("app.x.game", 0).is_err());
    assert!(ins("LullShift.generated.quiz.abc", 0).is_err());
    assert!(ins("lullshift.generated.quiz.abc", 2).is_err());
    assert!(ins("lullshift.generated.quiz.abc", 0).is_ok());
}

// ---- API ----

#[test]
fn insert_get_and_list_newest_first_with_filter() {
    let mut s = Store::open_in_memory().expect("open");
    let a = add(&mut s, 1, QUIZ, 0);
    let b = add(&mut s, 2, CARDS, 5);
    let c = add(&mut s, 3, QUIZ, 10);

    let got = s.generated_set(&a).expect("get").expect("exists");
    assert_eq!(got.engine, QUIZ);
    assert_eq!(got.topic, "Rust ownership");
    assert_eq!(got.difficulty, "gentle");
    assert_eq!(got.data_json, DATA);
    assert_eq!(got.created, T0);
    assert_eq!(got.provider_host, "api.anthropic.com");
    assert_eq!(got.model.as_deref(), Some("m-1"));
    assert_eq!(got.origin_pack.as_deref(), Some("lullshift.quiz.rust"));
    assert!(!got.unusable);
    assert!(
        s.generated_set("lullshift.generated.quiz.nope")
            .expect("get")
            .is_none()
    );

    let all: Vec<_> = s
        .generated_sets(None)
        .expect("list")
        .into_iter()
        .map(|g| g.id)
        .collect();
    assert_eq!(all, vec![c.clone(), b.clone(), a.clone()]);
    let quiz: Vec<_> = s
        .generated_sets(Some(QUIZ))
        .expect("list")
        .into_iter()
        .map(|g| g.id)
        .collect();
    assert_eq!(quiz, vec![c, a]);
    let cards: Vec<_> = s
        .generated_sets(Some(CARDS))
        .expect("list")
        .into_iter()
        .map(|g| g.id)
        .collect();
    assert_eq!(cards, vec![b]);
    assert!(
        s.generated_sets(Some("lullshift.typing@1"))
            .expect("list")
            .is_empty()
    );
}

#[test]
fn insert_refuses_bad_input_and_duplicates() {
    let mut s = Store::open_in_memory().expect("open");
    let good = id(1);
    s.insert_generated_set(&new(&good, QUIZ, T0))
        .expect("first");
    assert!(matches!(
        s.insert_generated_set(&new(&good, QUIZ, T0)),
        Err(StoreError::AlreadyExists)
    ));
    for bad in [
        "app.x.game",
        "lullshift.generated.",
        "lullshift.generated.Quiz.abc",
        "lullshift.generated.quiz.a b",
        "lullshift.generated.quiz.",
    ] {
        let r = s.insert_generated_set(&new(bad, QUIZ, T0));
        assert!(matches!(r, Err(StoreError::InvalidInput(_))), "{bad}");
    }
    let long = format!("{GENERATED_ID_PREFIX}{}", "a".repeat(120));
    assert!(matches!(
        s.insert_generated_set(&new(&long, QUIZ, T0)),
        Err(StoreError::InvalidInput(_))
    ));
    let i2 = id(2);
    let mut n = new(&i2, QUIZ, T0);
    n.data_json = "not json";
    assert!(matches!(
        s.insert_generated_set(&n),
        Err(StoreError::InvalidInput(_))
    ));
    let big = format!("\"{}\"", "x".repeat(300_000));
    n.data_json = &big;
    assert!(matches!(
        s.insert_generated_set(&n),
        Err(StoreError::InvalidInput(_))
    ));
    n.data_json = DATA;
    let topic = "t".repeat(201);
    n.topic = &topic;
    assert!(matches!(
        s.insert_generated_set(&n),
        Err(StoreError::InvalidInput(_))
    ));
    n.topic = "ok";
    n.engine = "";
    assert!(matches!(
        s.insert_generated_set(&n),
        Err(StoreError::InvalidInput(_))
    ));
    assert_eq!(
        s.generated_sets(None).expect("list").len(),
        1,
        "nothing partial"
    );
}

#[test]
fn debug_output_hides_topic_data_and_model() {
    let mut s = Store::open_in_memory().expect("open");
    let a = add(&mut s, 1, QUIZ, 0);
    let g = s.generated_set(&a).expect("get").expect("some");
    let text = format!("{g:?}");
    assert!(
        !text.contains("Rust ownership") && !text.contains("m-1") && !text.contains("questions")
    );
    assert!(text.contains(&a));
}

#[test]
fn mark_unusable_sticks() {
    let mut s = Store::open_in_memory().expect("open");
    let a = add(&mut s, 1, QUIZ, 0);
    assert_eq!(
        s.generated_set_state(&a).expect("state"),
        Some((QUIZ.to_owned(), false))
    );
    assert_eq!(
        s.generated_set_state("lullshift.generated.quiz.nope")
            .expect("state"),
        None
    );
    assert!(s.mark_generated_set_unusable(&a).expect("mark"));
    assert_eq!(
        s.generated_set_state(&a).expect("state"),
        Some((QUIZ.to_owned(), true))
    );
    assert!(s.generated_set(&a).expect("get").expect("some").unusable);
    assert!(
        !s.mark_generated_set_unusable("lullshift.generated.quiz.nope")
            .expect("mark")
    );
}

#[test]
fn skips_are_idempotent_listed_sorted_and_need_a_set() {
    let mut s = Store::open_in_memory().expect("open");
    let a = add(&mut s, 1, QUIZ, 0);
    let b = add(&mut s, 2, QUIZ, 1);
    s.add_generated_skip(&a, 3).expect("skip");
    s.add_generated_skip(&a, 1).expect("skip");
    s.add_generated_skip(&a, 3).expect("repeat");
    s.add_generated_skip(&b, 0).expect("other set");
    assert_eq!(s.generated_skips(&a).expect("skips"), vec![1, 3]);
    assert_eq!(s.generated_skips(&b).expect("skips"), vec![0]);
    assert!(matches!(
        s.add_generated_skip("lullshift.generated.quiz.nope", 0),
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        s.add_generated_skip(&a, 10_000),
        Err(StoreError::InvalidInput(_))
    ));
    assert!(
        s.generated_skips("lullshift.generated.quiz.nope")
            .expect("none")
            .is_empty()
    );
}

fn give_state(s: &mut Store, activity: &str) {
    s.save_snapshot(activity, "{\"s\":1}", T0, 1000)
        .expect("snap");
    s.set_activity_value(activity, "k", "v", 10_000)
        .expect("kv");
    s.add_play_time(activity, 60, true, 3600, T0).expect("play");
}

fn has_state(s: &Store, activity: &str) -> (bool, bool, bool) {
    (
        s.snapshot(activity).expect("snap").is_some(),
        s.activity_value(activity, "k").expect("kv").is_some(),
        s.play_time(activity).expect("play") > 0,
    )
}

#[test]
fn delete_one_removes_row_skips_snapshot_storage_and_play_time_only_for_that_id() {
    let mut s = Store::open_in_memory().expect("open");
    let a = add(&mut s, 1, QUIZ, 0);
    let b = add(&mut s, 2, QUIZ, 1);
    for who in [a.as_str(), b.as_str(), "app.x.game"] {
        give_state(&mut s, who);
    }
    s.add_generated_skip(&a, 2).expect("skip");
    s.add_generated_skip(&b, 2).expect("skip");

    assert!(s.delete_generated_set(&a).expect("delete"));
    assert!(s.generated_set(&a).expect("get").is_none());
    assert!(
        s.generated_skips(&a).expect("skips").is_empty(),
        "skips cascade"
    );
    assert_eq!(has_state(&s, &a), (false, false, false));
    assert_eq!(
        has_state(&s, &b),
        (true, true, true),
        "another set untouched"
    );
    assert_eq!(s.generated_skips(&b).expect("skips"), vec![2]);
    assert_eq!(
        has_state(&s, "app.x.game"),
        (true, true, true),
        "an installed activity untouched"
    );
    assert!(
        !s.delete_generated_set(&a).expect("again"),
        "second delete finds nothing"
    );
}

#[test]
fn delete_never_clears_state_of_an_id_outside_the_namespace() {
    let mut s = Store::open_in_memory().expect("open");
    give_state(&mut s, "app.x.game");
    assert!(!s.delete_generated_set("app.x.game").expect("delete"));
    assert_eq!(has_state(&s, "app.x.game"), (true, true, true));
}

#[test]
fn delete_clears_an_orphaned_save_of_an_already_deleted_set() {
    let mut s = Store::open_in_memory().expect("open");
    let ghost = id(9);
    give_state(&mut s, &ghost);
    assert!(!s.delete_generated_set(&ghost).expect("delete"));
    assert_eq!(has_state(&s, &ghost), (false, false, false));
}

#[test]
fn delete_all_removes_every_set_and_its_state_but_nothing_else() {
    let mut s = Store::open_in_memory().expect("open");
    let a = add(&mut s, 1, QUIZ, 0);
    let b = add(&mut s, 2, CARDS, 1);
    give_state(&mut s, &a);
    give_state(&mut s, &b);
    give_state(&mut s, "app.x.game");
    s.add_generated_skip(&a, 0).expect("skip");

    let mut gone = s.delete_all_generated_sets().expect("delete all");
    gone.sort();
    let mut want = vec![a.clone(), b.clone()];
    want.sort();
    assert_eq!(gone, want);
    assert!(s.generated_sets(None).expect("list").is_empty());
    assert_eq!(has_state(&s, &a), (false, false, false));
    assert_eq!(has_state(&s, &b), (false, false, false));
    assert_eq!(has_state(&s, "app.x.game"), (true, true, true));
    assert!(s.delete_all_generated_sets().expect("empty").is_empty());
}

#[test]
fn prune_removes_the_oldest_beyond_ten_per_engine_and_clears_their_state() {
    assert_eq!(KEEP_GENERATED_SETS_PER_ENGINE, 10);
    let mut s = Store::open_in_memory().expect("open");
    let quiz: Vec<String> = (0..13)
        .map(|n| add(&mut s, n, QUIZ, i64::try_from(n).unwrap()))
        .collect();
    let card = add(&mut s, 100, CARDS, 0);
    give_state(&mut s, &quiz[0]);
    // A save-less set's snapshot row does not exist, but its play time and storage do.
    s.snapshot(&quiz[0]).expect("ok");
    s.clear_snapshot(&quiz[0]).expect("clear");

    let removed = s
        .prune_generated_sets(QUIZ, KEEP_GENERATED_SETS_PER_ENGINE)
        .expect("prune");
    assert_eq!(
        removed,
        quiz[..3].to_vec(),
        "the three oldest, oldest first"
    );
    let left: Vec<_> = s
        .generated_sets(Some(QUIZ))
        .expect("list")
        .into_iter()
        .map(|g| g.id)
        .collect();
    assert_eq!(left.len(), 10);
    assert!(!left.contains(&quiz[0]) && left.contains(&quiz[3]) && left.contains(&quiz[12]));
    assert_eq!(
        has_state(&s, &quiz[0]),
        (false, false, false),
        "state of a pruned set is gone"
    );
    assert!(
        s.generated_set(&card).expect("get").is_some(),
        "another engine untouched"
    );
    assert!(s.prune_generated_sets(QUIZ, 10).expect("again").is_empty());
}

#[test]
fn prune_keeps_sets_with_a_saved_snapshot_even_beyond_the_limit() {
    let mut s = Store::open_in_memory().expect("open");
    let ids: Vec<String> = (0..12)
        .map(|n| add(&mut s, n, QUIZ, i64::try_from(n).unwrap()))
        .collect();
    // The two oldest and one in the middle have a save.
    for i in [0, 1, 6] {
        s.save_snapshot(&ids[i], "{\"s\":1}", T0, 1000)
            .expect("snap");
        s.add_play_time(&ids[i], 10, true, 3600, T0).expect("play");
    }
    let removed = s.prune_generated_sets(QUIZ, 10).expect("prune");
    // 12 sets, keep 10: two removed, the oldest without a save (ids 2 and 3).
    assert_eq!(removed, vec![ids[2].clone(), ids[3].clone()]);
    for i in [0, 1, 6] {
        assert!(
            s.generated_set(&ids[i]).expect("get").is_some(),
            "saved set {i} kept"
        );
        assert!(s.snapshot(&ids[i]).expect("snap").is_some());
        assert!(s.play_time(&ids[i]).expect("play") > 0);
    }

    // When every set has a save nothing can go, even far over the limit.
    let mut all = Store::open_in_memory().expect("open");
    let ids: Vec<String> = (0..12)
        .map(|n| add(&mut all, n, QUIZ, i64::try_from(n).unwrap()))
        .collect();
    for i in &ids {
        all.save_snapshot(i, "{}", T0, 1000).expect("snap");
    }
    assert!(
        all.prune_generated_sets(QUIZ, 10)
            .expect("prune")
            .is_empty()
    );
    assert_eq!(all.generated_sets(None).expect("list").len(), 12);
}

#[test]
fn prune_counts_unusable_sets_and_removes_them_when_old_enough() {
    let mut s = Store::open_in_memory().expect("open");
    let ids: Vec<String> = (0..4)
        .map(|n| add(&mut s, n, QUIZ, i64::try_from(n).unwrap()))
        .collect();
    s.mark_generated_set_unusable(&ids[0]).expect("mark");
    let removed = s.prune_generated_sets(QUIZ, 3).expect("prune");
    assert_eq!(removed, vec![ids[0].clone()]);
}
