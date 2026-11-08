//! Model-written sets (ADR-028, S13-T05): ids, acceptance, scheme serving, the denial cases
//! (forged id, wrong engine, tampered row, reserved namespace, other activities' saves) and
//! deletion.

use serde_json::json;
use tauri::http::{Response, StatusCode, header};

use super::*;
use crate::activities::{
    SCHEME, activity_csp, add_play_for_core, clear_snapshot_for_core, content_for_core,
    info_for_core, launch_activity, list_ids, preview_for_core, save_snapshot_for_core, serve,
    snapshot_for_core, storage_set,
};

fn quiz() -> Value {
    json!({"title": "Rust basics", "questions": [{
        "prompt": "Which is larger?", "choices": ["Two", "Three"], "answer": 1,
        "explanation": "Three is larger than two."}]})
}

fn deck() -> Value {
    json!({"title": "Calm", "cards": [{"front": "Hi"}]})
}

fn meta() -> SetMeta {
    SetMeta {
        topic: "Rust ownership".to_owned(),
        difficulty: "gentle".to_owned(),
        provider_host: "api.anthropic.com".to_owned(),
        model: Some("claude-x".to_owned()),
        origin_pack: None,
    }
}

fn core(dir: &tempfile::TempDir) -> Core {
    Core::for_tests(dir.path())
}

fn accept(core: &Core, short: &str, data: Value) -> String {
    accept_generated_set(core, short, data, &meta()).unwrap_or_else(|p| panic!("{p:?}"))
}

fn raw(core: &Core, path: &str) -> Response<Vec<u8>> {
    serve(
        Some(core),
        &tauri::http::Request::builder()
            .method("GET")
            .uri(format!("{SCHEME}://localhost{path}"))
            .body(Vec::new())
            .expect("request"),
    )
}

fn get(core: &Core, key: &str, id: &str, file: &str) -> Response<Vec<u8>> {
    raw(core, &format!("/{key}/{id}/{file}"))
}

/// A bundled app, to hold a save that must stay untouched.
fn other_app() -> String {
    crate::activities::catalog()
        .values()
        .find(|a| a.listed && matches!(a.role, crate::activities::Role::App))
        .map(|a| a.manifest.id.clone())
        .expect("a bundled app")
}

fn row_unusable(core: &Core, id: &str) -> bool {
    with_store(core, |s| s.generated_set_state(id))
        .expect("state")
        .expect("row")
        .1
}

/// Replaces a set's row by hand-edited data under the same id (what a tampered database is).
fn tamper(core: &Core, id: &str, engine: &str, data_json: &str) {
    with_store(core, |s| {
        s.delete_generated_set(id)?;
        s.insert_generated_set(&local_store::NewGeneratedSet {
            id,
            engine,
            topic: "t",
            difficulty: "gentle",
            data_json,
            created: OffsetDateTime::now_utc(),
            provider_host: "h",
            model: None,
            origin_pack: None,
        })
    })
    .expect("tamper");
}

// ---- ids ----

#[test]
fn set_ids_follow_the_format_and_the_manifest_pattern() {
    let mut seen = std::collections::BTreeSet::new();
    for (short, engine) in SETS {
        for _ in 0..50 {
            let id = new_set_id(short).expect("id");
            assert!(marketplace::manifest::valid_id(&id), "{id}");
            let rest = id.strip_prefix("lullshift.generated.").expect("prefix");
            let (s, random) = rest.split_once('.').expect("dot");
            assert_eq!(s, short);
            assert_eq!(random.len(), 12);
            assert!(
                random
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
            );
            assert_eq!(parse_set_id(&id), Some(ParsedSetId { short, engine }));
            assert!(seen.insert(id), "ids repeat");
        }
    }
    assert_eq!(seen.len(), 8 * 50);
    for bad in ["toy", "", "quiz.x", "Quiz"] {
        assert_eq!(new_set_id(bad).unwrap_err(), "unknown_engine", "{bad}");
    }
}

#[test]
fn the_random_part_uses_the_whole_alphabet() {
    // 36 symbols over a few thousand characters: a stuck or biased generator would miss some.
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..400 {
        let id = new_set_id("quiz").expect("id");
        seen.extend(id.rsplit('.').next().expect("random").chars());
    }
    assert_eq!(seen.len(), 36, "{seen:?}");
}

#[test]
fn schema_pattern_for_ids_accepts_the_set_ids() {
    // The manifest schema's id pattern, as the CLI and Ajv apply it.
    let re = regex::Regex::new(r"^[a-z0-9]+(\.[a-z0-9-]+)+$").expect("regex");
    for (short, _) in SETS {
        assert!(re.is_match(&new_set_id(short).expect("id")));
    }
}

#[test]
fn every_short_name_maps_to_a_bundled_engine_with_a_usable_schema() {
    for (short, reference) in SETS {
        assert_eq!(engine_ref(short), Some(reference));
        find_engine(reference).unwrap_or_else(|e| panic!("{short}: {e:?}"));
    }
    for short in ["away", "toy", "calm-cards", "Quiz"] {
        assert_eq!(engine_ref(short), None, "{short}");
    }
}

#[test]
fn malformed_set_ids_do_not_parse() {
    for bad in [
        "",
        "lullshift.generated.",
        "lullshift.generated.quiz",
        "lullshift.generated.quiz.",
        "lullshift.generated.quiz.abc",
        "lullshift.generated.quiz.abcdefghij123",
        "lullshift.generated.quiz.ABCDEFGHIJ12",
        "lullshift.generated.quiz.abcdefghij1-",
        "lullshift.generated.quiz.abcdefghij12.x",
        "lullshift.generated.toy.abcdefghij12",
        "lullshift.generated.Quiz.abcdefghij12",
        "lullshift.generated.abcdefghij12",
        "lullshift.generatedx.quiz.abcdefghij12",
        "app.lullshift.generated.quiz.abcdefghij12",
    ] {
        assert_eq!(parse_set_id(bad), None, "{bad}");
    }
    assert!(is_set_id("lullshift.generated."));
    assert!(!is_set_id("lullshift.generatedx"));
}

// ---- acceptance ----

#[test]
fn a_checked_set_is_stored_listed_and_loads_with_its_data() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core(&dir);
    let id = accept(&core, "quiz", quiz());
    assert_eq!(content_for_core(&core, &id).as_ref(), Ok(&quiz()));

    let row = with_store(&core, |s| s.generated_set(&id))
        .expect("get")
        .expect("row");
    assert_eq!(row.engine, "lullshift.quiz@1");
    assert_eq!(row.topic, "Rust ownership");
    assert_eq!(row.model.as_deref(), Some("claude-x"));
    assert_eq!(row.provider_host, "api.anthropic.com");

    let info = info_for_core(&core, &id).expect("info");
    assert_eq!(info.name, "Rust basics");
    assert_eq!(info.category, "interactive");
    assert!(info.help.is_some(), "the engine's help");
    let g = info.generated.expect("marker");
    assert_eq!(g.engine, "lullshift.quiz@1");
    assert_eq!(g.model.as_deref(), Some("claude-x"));
    assert_eq!(g.topic, "Rust ownership");
    assert!(!g.unusable);
    assert!(!g.created_utc.is_empty());
    assert!(
        preview_for_core(&core, &id)
            .expect("preview")
            .is_some_and(|u| u.starts_with("data:image/"))
    );
    // Not in the normal list of activities.
    assert!(!list_ids(&core).expect("ids").contains(&id));
}

#[test]
fn data_that_fails_the_engine_gate_is_refused_and_nothing_is_stored() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core(&dir);
    let mut with_image = quiz();
    with_image["questions"][0]["image"] = json!({"src": "images/a.png", "alt": "A picture."});
    let mut hidden = quiz();
    hidden["title"] = json!("Sneaky\u{202e}title");
    let mut unknown = quiz();
    unknown["extra"] = json!(1);
    for (name, data) in [
        ("picture", with_image),
        ("hidden character", hidden),
        ("unknown field", unknown),
        ("wrong shape", json!({"title": "T", "questions": []})),
        ("not an object", json!([1, 2])),
        ("null", Value::Null),
    ] {
        let problems = accept_generated_set(&core, "quiz", data, &meta()).expect_err(name);
        assert!(!problems.is_empty(), "{name}");
        assert!(
            problems.iter().all(|p| !p.contains("Sneaky")),
            "{name}: a problem quotes the data"
        );
    }
    assert!(
        with_store(&core, |s| s.generated_sets(None))
            .expect("list")
            .is_empty()
    );
    // The check a reply must pass first is the same gate.
    let check = engine_check("quiz");
    assert!(check(&quiz()).is_ok());
    assert!(check(&json!({"title": "T"})).is_err());
    assert!(
        engine_check("cards")(&quiz()).is_err(),
        "another engine's gate"
    );
    assert!(engine_check("cards")(&deck()).is_ok());
    assert!(engine_check("toy")(&quiz()).is_err());
    assert!(accept_generated_set(&core, "toy", quiz(), &meta()).is_err());
}

#[test]
fn oversized_data_is_refused() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core(&dir);
    let mut big = quiz();
    big["intro"] = json!("x".repeat(DATA_MAX));
    assert!(accept_generated_set(&core, "quiz", big, &meta()).is_err());
}

#[test]
fn metadata_is_cleaned_and_cut() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core(&dir);
    let m = SetMeta {
        topic: format!("  Top\u{202e}ic\n{}  ", "t".repeat(400)),
        difficulty: "\u{200b}".to_owned(),
        provider_host: "api.example.com".to_owned(),
        model: Some(format!("m{}", "x".repeat(300))),
        origin_pack: Some("not a valid id!".to_owned()),
    };
    let id = accept_generated_set(&core, "quiz", quiz(), &m).expect("accepted");
    let row = with_store(&core, |s| s.generated_set(&id))
        .expect("get")
        .expect("row");
    assert!(row.topic.starts_with("Topic"));
    assert!(row.topic.chars().count() <= 200);
    assert!(!row.topic.contains('\u{202e}') && !row.topic.contains('\n'));
    assert_eq!(row.difficulty, "unknown");
    assert_eq!(row.model.as_deref().map(|m| m.chars().count()), Some(200));
    assert_eq!(row.origin_pack, None);
    let shown = info_for_core(&core, &id)
        .expect("info")
        .generated
        .expect("g");
    assert_eq!(shown.model.map(|m| m.chars().count()), Some(40));

    // A real pack id is kept as the origin.
    let mut m = meta();
    m.origin_pack = Some("app.lullshift.content.quiz".to_owned());
    let id = accept_generated_set(&core, "quiz", quiz(), &m).expect("accepted");
    let row = with_store(&core, |s| s.generated_set(&id))
        .expect("get")
        .expect("row");
    assert_eq!(
        row.origin_pack.as_deref(),
        Some("app.lullshift.content.quiz")
    );
}

#[test]
fn accepting_prunes_to_ten_per_engine_and_keeps_sets_with_a_save() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core(&dir);
    let first = accept(&core, "quiz", quiz());
    save_snapshot_for_core(&core, &first, "{\"round\":1}").expect("save");
    let mut ids = vec![first.clone()];
    for _ in 0..12 {
        ids.push(accept(&core, "quiz", quiz()));
    }
    let other = accept(&core, "cards", deck());
    let kept: Vec<String> = with_store(&core, |s| s.generated_sets(Some("lullshift.quiz@1")))
        .expect("list")
        .into_iter()
        .map(|g| g.id)
        .collect();
    assert_eq!(kept.len(), 10, "ten, the saved one included");
    assert!(kept.contains(&first), "the set with a save is kept");
    assert!(snapshot_for_core(&core, &first).expect("snap").is_some());
    assert!(kept.contains(ids.last().expect("last")));
    assert!(!kept.contains(&ids[1]) && !kept.contains(&ids[2]) && !kept.contains(&ids[3]));
    assert!(
        with_store(&core, |s| s.generated_set(&other))
            .expect("get")
            .is_some(),
        "another engine untouched"
    );
}

// ---- scheme handler and launch ----

#[test]
fn the_engines_files_are_served_under_the_set_id_with_the_pack_rules() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core(&dir);
    let id = accept(&core, "quiz", quiz());
    let url = launch_activity(&core, &id).expect("launch");
    let path = url.split_once("localhost").expect("host").1.to_owned();
    let ok = raw(&core, &path);
    assert_eq!(ok.status(), StatusCode::OK);
    assert_eq!(
        ok.headers()[header::CONTENT_SECURITY_POLICY],
        activity_csp().as_str()
    );
    assert_eq!(ok.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    let (engine, _) = find_engine("lullshift.quiz@1").expect("engine");
    let engine_index = engine
        .dir
        .get_file(engine.dir.path().join("index.html"))
        .expect("engine index");
    assert_eq!(ok.body().as_slice(), engine_index.contents());

    let key = path
        .trim_start_matches('/')
        .split('/')
        .next()
        .expect("key")
        .to_owned();
    // A pack's pictures are never served for a set; nor files that don't exist.
    for file in ["images/a.png", "images/", "missing.js", "../x.js", "x.exe"] {
        let r = get(&core, &key, &id, file);
        assert_eq!(r.status(), StatusCode::NOT_FOUND, "{file}");
    }
    // The key serves this set only.
    let other = accept(&core, "quiz", quiz());
    assert_eq!(
        get(&core, &key, &other, "index.html").status(),
        StatusCode::FORBIDDEN
    );
    // A new launch replaces the key.
    launch_activity(&core, &id).expect("relaunch");
    assert_eq!(raw(&core, &path).status(), StatusCode::FORBIDDEN);
}

// ---- denial tests ----

#[test]
fn a_forged_set_id_that_does_not_exist_is_refused_everywhere() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core(&dir);
    let forged = "lullshift.generated.quiz.abcdefghij12";
    let garbage = "lullshift.generated.nonsense";
    for id in [forged, garbage] {
        assert_eq!(launch_activity(&core, id).unwrap_err(), "unknown_activity");
        assert_eq!(info_for_core(&core, id).unwrap_err(), "unknown_activity");
        assert_eq!(
            content_for_core(&core, id).unwrap_err().code(),
            "unknown_activity"
        );
        assert_eq!(preview_for_core(&core, id).unwrap_err(), "unknown_activity");
        assert_eq!(
            add_play_for_core(&core, id, 5, true).unwrap_err(),
            "unknown_activity"
        );
        assert_eq!(
            save_snapshot_for_core(&core, id, "{}").unwrap_err(),
            "unknown_activity"
        );
        assert!(snapshot_for_core(&core, id).is_err());
        assert!(clear_snapshot_for_core(&core, id).is_err());
        assert!(storage_set(&core, id, "k", "v").is_err());
        // Even with a live launch key for the forged id, no file is served.
        let key = core.launches.begin(id).expect("key");
        assert_eq!(
            get(&core, &key, id, "index.html").status(),
            StatusCode::NOT_FOUND
        );
    }
    assert_eq!(
        with_store(&core, |s| s.snapshot(forged)).expect("snap"),
        None
    );
    assert_eq!(with_store(&core, |s| s.play_time(forged)).expect("play"), 0);
}

#[test]
fn a_set_whose_stored_engine_differs_from_its_id_is_refused_and_marked() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core(&dir);
    // A quiz id whose row says it is a cards deck (a hand-edited database).
    let id = "lullshift.generated.quiz.abcdefghij12";
    tamper(
        &core,
        id,
        "lullshift.cards@1",
        r#"{"title":"T","cards":[{"front":"Hi"}]}"#,
    );
    assert_eq!(launch_activity(&core, id).unwrap_err(), "unknown_activity");
    let key = core.launches.begin(id).expect("key");
    assert_eq!(
        get(&core, &key, id, "index.html").status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        content_for_core(&core, id).unwrap_err().code(),
        "unknown_activity"
    );
    assert!(row_unusable(&core, id), "marked unusable");
    // Listed (so it can be deleted) as unusable, never as a playable quiz or deck.
    let rows = list_generated(&core, None).expect("list");
    let g = rows[0].generated.as_ref().expect("marker");
    assert!(g.unusable);
    assert_eq!(g.engine, "lullshift.quiz@1");
    assert_eq!(rows[0].name, FALLBACK_NAME);
    assert!(delete_generated_set(&core, id).expect("delete"));
}

#[test]
fn a_tampered_row_fails_the_recheck_is_marked_unusable_and_is_not_served() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core(&dir);
    let q = r#"{"prompt":"P?","choices":["a","b"],"answer":0,"explanation":"E"}"#;
    let cases: [(&str, String); 5] = [
        (
            "unknown field",
            format!(r#"{{"title":"T","questions":[{q}],"x":1}}"#),
        ),
        (
            "answer out of range",
            r#"{"title":"T","questions":[{"prompt":"P?","choices":["a","b"],"answer":5,"explanation":"E"}]}"#.to_owned(),
        ),
        (
            "hidden character",
            format!("{{\"title\":\"T\\u202e\",\"questions\":[{q}]}}"),
        ),
        (
            "picture",
            r#"{"title":"T","questions":[{"prompt":"P?","choices":["a","b"],"answer":0,"explanation":"E","image":{"src":"images/a.png","alt":"A."}}]}"#.to_owned(),
        ),
        ("not an object", "[1,2,3]".to_owned()),
    ];
    for (name, tampered) in cases {
        let id = accept(&core, "quiz", quiz());
        tamper(&core, &id, "lullshift.quiz@1", &tampered);
        // A key issued while the row still looked fine.
        let key = core.launches.begin(&id).expect("key");
        assert_eq!(
            get(&core, &key, &id, "index.html").status(),
            StatusCode::OK,
            "{name}: engine files are static until the data check fails"
        );

        let err = content_for_core(&core, &id).expect_err(name);
        assert_eq!(err.code(), "generated_unusable", "{name}");
        assert!(!err.detail.contains("202e") && !err.detail.is_empty());
        assert!(row_unusable(&core, &id), "{name}: marked");
        // Now nothing is served, launched or described as playable.
        assert_eq!(
            get(&core, &key, &id, "index.html").status(),
            StatusCode::NOT_FOUND,
            "{name}"
        );
        assert_eq!(
            launch_activity(&core, &id).unwrap_err(),
            "generated_unusable",
            "{name}"
        );
        assert_eq!(
            content_for_core(&core, &id).unwrap_err().code(),
            "generated_unusable",
            "{name}: stays refused"
        );
        assert_eq!(
            info_for_core(&core, &id).unwrap_err(),
            "generated_unusable",
            "{name}"
        );
        // Still listed, marked, so the user can delete it; the tampered title is not shown.
        let row = list_generated(&core, Some("quiz"))
            .expect("list")
            .into_iter()
            .find(|i| i.id == id)
            .expect("listed");
        assert!(row.generated.expect("g").unusable);
        assert_eq!(row.name, FALLBACK_NAME, "{name}");
        assert!(delete_generated_set(&core, &id).expect("delete"));
    }
}

#[test]
fn a_title_that_fails_the_plain_text_rule_is_not_shown() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core(&dir);
    let id = accept(&core, "cards", deck());
    tamper(
        &core,
        &id,
        "lullshift.cards@1",
        "{\"title\":\"Evil\\u202etitle\",\"cards\":[{\"front\":\"Hi\"}]}",
    );
    let rows = list_generated(&core, None).expect("list");
    assert_eq!(rows[0].name, FALLBACK_NAME);
}

#[test]
fn installed_activities_cannot_use_a_generated_id() {
    use marketplace::manifest::{is_reserved_generated_id, parse_manifest};
    let id = "lullshift.generated.quiz.abcdefghij12";
    assert!(is_reserved_generated_id(id));
    for manifest in [
        json!({"schemaVersion":1,"kind":"app","id":id,"version":"1.0.0","name":"X",
            "category":"arcade","minHostVersion":"0.1.0","sdkVersion":"0.1.0",
            "permissions":[],"offline":true,"entrypoint":"index.html",
            "runtime":"isolated-web-v1",
            "interruptibility":{"pause":true,"resume":true,"persistState":true}}),
        json!({"schemaVersion":1,"kind":"content","id":id,"version":"1.0.0","name":"X",
            "category":"break","minHostVersion":"0.1.0","sdkVersion":"0.1.0",
            "permissions":[],"offline":true,"engine":"lullshift.quiz@1","data":"d.json"}),
    ] {
        assert!(parse_manifest(&manifest.to_string()).is_none());
    }
    // Nothing bundled uses the namespace: not in the parsed catalog, not in any manifest file on
    // disk, not in the built-in marketplace index.
    assert!(
        crate::activities::catalog()
            .keys()
            .all(|k| !is_reserved_generated_id(k))
    );
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let mut scanned = 0;
    let mut stack = vec![root.join("activities")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().is_some_and(|n| n == "manifest.json") {
                scanned += 1;
                let text = std::fs::read_to_string(&path).expect("manifest");
                assert!(
                    !text.contains(GENERATED_ID_PREFIX),
                    "{} uses the reserved namespace",
                    path.display()
                );
            }
        }
    }
    assert!(scanned > 10, "scanned {scanned} manifests");
    let index =
        std::fs::read_to_string(root.join(".lullshift/marketplace.json")).expect("built-in index");
    assert!(!index.contains(GENERATED_ID_PREFIX));
}

#[test]
fn a_sets_snapshot_play_time_and_storage_never_touch_another_activity() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core(&dir);
    let app = other_app();
    let a = accept(&core, "quiz", quiz());
    let b = accept(&core, "quiz", quiz());
    save_snapshot_for_core(&core, &app, "{\"who\":\"app\"}").expect("app save");
    add_play_for_core(&core, &app, 100, true).expect("app play");

    save_snapshot_for_core(&core, &a, "{\"who\":\"a\"}").expect("a save");
    add_play_for_core(&core, &a, 30, true).expect("a play");
    let snap = |id: &str| {
        snapshot_for_core(&core, id)
            .expect("get")
            .map(|s| s.snapshot)
    };
    assert_eq!(snap(&a).as_deref(), Some("{\"who\":\"a\"}"));
    assert_eq!(snap(&b), None, "the other set has no save");
    assert_eq!(snap(&app).as_deref(), Some("{\"who\":\"app\"}"));
    // A bad or oversize snapshot for a set is refused as for any activity.
    for bad in ["null", "not json"] {
        assert_eq!(
            save_snapshot_for_core(&core, &a, bad).unwrap_err(),
            "invalid_snapshot"
        );
    }
    assert_eq!(
        save_snapshot_for_core(&core, &a, &"1".repeat(crate::activities::SNAPSHOT_MAX + 1))
            .unwrap_err(),
        "quota_exceeded"
    );
    assert_eq!(snap(&a).as_deref(), Some("{\"who\":\"a\"}"), "kept");

    clear_snapshot_for_core(&core, &a).expect("clear a");
    assert_eq!(snap(&a), None);
    assert_eq!(
        snap(&app).as_deref(),
        Some("{\"who\":\"app\"}"),
        "the app's save stays"
    );
    assert_eq!(with_store(&core, |s| s.play_time(&app)).expect("play"), 100);
    assert_eq!(with_store(&core, |s| s.play_time(&a)).expect("play"), 30);

    // Storage: a set may use it only if its engine declares it (none of the bundled engines
    // do), and a refusal writes nothing.
    assert_eq!(
        storage_set(&core, &a, "k", "v").unwrap_err(),
        "not_permitted"
    );
    assert_eq!(
        with_store(&core, |s| s.activity_value(&a, "k")).expect("kv"),
        None
    );

    let rows = list_generated(&core, Some("lullshift.quiz@1")).expect("list");
    let listed = rows.iter().find(|i| i.id == a).expect("a");
    assert_eq!(listed.played_seconds, 30);
    assert_eq!(
        listed.storage,
        has_storage(find_engine("lullshift.quiz@1").expect("engine").0)
    );
}

// ---- deletion ----

#[test]
fn deleting_a_set_removes_its_state_ends_its_launch_and_spares_everything_else() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core(&dir);
    let app = other_app();
    let a = accept(&core, "quiz", quiz());
    let b = accept(&core, "cards", deck());
    for id in [&a, &b, &app] {
        save_snapshot_for_core(&core, id, "{\"s\":1}").expect("save");
        add_play_for_core(&core, id, 10, true).expect("play");
    }
    let url = launch_activity(&core, &a).expect("launch");
    let path = url.split_once("localhost").expect("host").1.to_owned();
    assert_eq!(raw(&core, &path).status(), StatusCode::OK);

    assert!(delete_generated_set(&core, &a).expect("delete"));
    assert_eq!(
        raw(&core, &path).status(),
        StatusCode::FORBIDDEN,
        "key revoked"
    );
    assert!(
        with_store(&core, |s| s.generated_set(&a))
            .expect("get")
            .is_none()
    );
    assert_eq!(with_store(&core, |s| s.snapshot(&a)).expect("snap"), None);
    assert_eq!(with_store(&core, |s| s.play_time(&a)).expect("play"), 0);
    assert_eq!(info_for_core(&core, &a).unwrap_err(), "unknown_activity");
    assert!(snapshot_for_core(&core, &b).expect("b").is_some());
    assert!(snapshot_for_core(&core, &app).expect("app").is_some());
    assert!(!delete_generated_set(&core, &a).expect("again"));

    // Only ids in the namespace can be deleted this way.
    assert_eq!(
        delete_generated_set(&core, &app).unwrap_err(),
        "unknown_activity"
    );
    assert!(snapshot_for_core(&core, &app).expect("app").is_some());

    assert_eq!(delete_all_generated_sets(&core).expect("all"), 1);
    assert!(list_generated(&core, None).expect("list").is_empty());
    assert!(snapshot_for_core(&core, &app).expect("app").is_some());
    assert_eq!(with_store(&core, |s| s.play_time(&app)).expect("play"), 10);
    assert_eq!(delete_all_generated_sets(&core).expect("none left"), 0);
}

#[test]
fn listing_filters_by_engine_short_name_or_reference_newest_first() {
    let dir = tempfile::tempdir().expect("dir");
    let core = core(&dir);
    let q1 = accept(&core, "quiz", quiz());
    let q2 = accept(&core, "quiz", quiz());
    let c = accept(&core, "cards", deck());
    let ids = |engine: Option<&str>| -> Vec<String> {
        list_generated(&core, engine)
            .expect("list")
            .into_iter()
            .map(|i| i.id)
            .collect()
    };
    assert_eq!(ids(Some("quiz")), vec![q2.clone(), q1.clone()]);
    assert_eq!(ids(Some("lullshift.quiz@1")), vec![q2, q1]);
    assert_eq!(ids(Some("cards")), vec![c]);
    assert_eq!(ids(None).len(), 3);
    assert!(ids(Some("lullshift.typing@1")).is_empty());
    assert_eq!(
        list_generated(&core, Some("toy")).unwrap_err(),
        "unknown_engine"
    );
}
