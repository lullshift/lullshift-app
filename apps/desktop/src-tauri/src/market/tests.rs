//! Marketplace command tests (Sprint 08). Real `git`, real repositories in temp folders (through
//! the crate's test-only `file://` support), a real store, the real bundled Calm Cards engine.
//!
//! Each test uses a fresh app folder (`Core::for_tests`), so nothing touches the user's data.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;

use serde_json::json;
use tauri::http::StatusCode;
use tempfile::TempDir;

use super::*;
use crate::activities::{self, SCHEME};

const MARKET: &str = "acme-tools";
const FACTS: &str = "com.example.facts";
const GAME: &str = "com.example.game";

struct Env {
    _tmp: TempDir,
    root: PathBuf,
    core: Arc<Core>,
}

impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().to_path_buf();
        std::fs::write(
            root.join("gitconfig"),
            "[user]\n\tname = Test\n\temail = test@example.com\n[commit]\n\tgpgsign = false\n[core]\n\tautocrlf = false\n",
        )
        .expect("gitconfig");
        let core = Arc::new(Core::for_tests(&root.join("app")));
        Self {
            _tmp: tmp,
            root,
            core,
        }
    }

    fn repo(&self, name: &str) -> Repo {
        let dir = self.root.join(name);
        std::fs::create_dir_all(&dir).expect("repo dir");
        let repo = Repo {
            dir,
            config: self.root.join("gitconfig"),
        };
        repo.git(&["init", "--quiet", "--initial-branch=main"]);
        repo
    }

    fn install_dir(&self, id: &str, commit: &str) -> PathBuf {
        self.core.app_dir.join("installed").join(id).join(commit)
    }
}

struct Repo {
    dir: PathBuf,
    config: PathBuf,
}

impl Repo {
    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.dir)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", &self.config)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .expect("run git");
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.dir.join(rel);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, text).expect("write");
    }

    fn commit(&self) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "--quiet", "-m", "test", "--allow-empty"]);
        self.git(&["rev-parse", "HEAD"])
    }

    fn url(&self) -> GitUrl {
        GitUrl::for_test_file(&self.dir)
    }
}

fn cards_manifest(id: &str, version: &str, permissions: &[&str]) -> String {
    json!({
        "schemaVersion": 1, "kind": "content", "id": id, "version": version, "name": "Facts",
        "description": "A few facts.", "category": "break", "minHostVersion": "0.1.0",
        "sdkVersion": "0.2.0", "permissions": permissions, "offline": true,
        "engine": "lullshift.cards@1", "data": "deck.json"
    })
    .to_string()
}

fn game_manifest(id: &str) -> String {
    json!({
        "schemaVersion": 1, "kind": "app", "id": id, "version": "1.0.0", "name": "Game",
        "category": "arcade", "minHostVersion": "0.1.0", "sdkVersion": "0.2.0",
        "permissions": [], "offline": true, "entrypoint": "index.html",
        "runtime": "isolated-web-v1",
        "interruptibility": {"pause": true, "resume": true, "persistState": true}
    })
    .to_string()
}

const DECK: &str =
    r#"{"title":"Facts","cards":[{"front":"Honey keeps","back":"It barely spoils."}]}"#;

fn write_facts(repo: &Repo, version: &str, permissions: &[&str]) {
    repo.write(
        "packs/facts/manifest.json",
        &cards_manifest(FACTS, version, permissions),
    );
    repo.write("packs/facts/deck.json", DECK);
    repo.write("packs/facts/README.md", "# Facts\n");
}

/// The id of a bundled content pack, which an installed activity must never shadow.
fn bundled_pack_id() -> String {
    activities::bundled_listed()
        .into_iter()
        .find(|(_, kind)| *kind == "content")
        .map(|(m, _)| m.id)
        .expect("a bundled pack")
}

fn bundled_app_id() -> String {
    activities::bundled_listed()
        .into_iter()
        .find(|(m, kind)| *kind == "app" && !m.id.contains(".test."))
        .map(|(m, _)| m.id)
        .expect("a bundled app")
}

/// A marketplace repository listing a facts pack, a game (code) and an id that collides with a
/// bundled pack.
fn standard_repo(env: &Env) -> (Repo, String) {
    let repo = env.repo("market");
    write_facts(&repo, "1.0.0", &[]);
    repo.write("packs/game/manifest.json", &game_manifest(GAME));
    repo.write("packs/game/index.html", "<!doctype html><title>g</title>");
    let shadow = bundled_pack_id();
    repo.write(
        "packs/shadow/manifest.json",
        &cards_manifest(&shadow, "1.0.0", &[]),
    );
    repo.write("packs/shadow/deck.json", DECK);
    repo.write(
        ".lullshift/marketplace.json",
        &json!({
            "schemaVersion": 1, "name": MARKET, "description": "Tools from Acme.", "owner": "Acme",
            "activities": [
                {"id": FACTS, "source": {"path": "packs/facts"}, "featured": true},
                {"id": GAME, "source": {"path": "packs/game"}},
                {"id": shadow, "source": {"path": "packs/shadow"}}
            ]
        })
        .to_string(),
    );
    let commit = repo.commit();
    (repo, commit)
}

fn added(env: &Env) -> (Repo, String) {
    let (repo, commit) = standard_repo(env);
    add(&env.core, &repo.url()).expect("add");
    (repo, commit)
}

fn installed(env: &Env) -> (Repo, String) {
    let (repo, commit) = added(env);
    let preview = preview_install(&env.core, MARKET, FACTS).expect("preview");
    install_pending(&env.core, &preview.token, false, false, Instant::now()).expect("install");
    (repo, commit)
}

fn request(env: &Env, path: &str) -> tauri::http::Response<Vec<u8>> {
    activities::serve(
        Some(&env.core),
        &tauri::http::Request::get(format!("{SCHEME}://localhost{path}"))
            .body(Vec::new())
            .expect("request"),
    )
}

fn listed_ids(env: &Env) -> Vec<String> {
    activities::list_ids(&env.core).expect("list")
}

fn manage_item(env: &Env, id: &str) -> activities::installed::ManageItem {
    env.core
        .library
        .snapshot(&env.core)
        .expect("snapshot")
        .manage
        .iter()
        .find(|m| m.activity_id == id)
        .cloned()
        .unwrap_or_else(|| panic!("{id} is not in the manage list"))
}

fn err_kind<T: std::fmt::Debug>(r: Res<T>) -> &'static str {
    r.expect_err("should fail").kind
}

// ---- add, entries, preview, install, list, serve -----------------------------------------

#[test]
fn add_entries_preview_install_then_listed_and_served() {
    let env = Env::new();
    let (repo, commit) = standard_repo(&env);

    // Add: the index name becomes the id; the built-in marketplace stays first.
    let m = add(&env.core, &repo.url()).expect("add");
    assert_eq!(m.id, MARKET);
    assert!(!m.builtin);
    assert_eq!(m.description.as_deref(), Some("Tools from Acme."));
    assert_eq!(m.owner.as_deref(), Some("Acme"));
    assert_eq!(m.last_fetched_commit.as_deref(), Some(commit.as_str()));
    assert!(m.last_fetched_at.is_some());
    let all = list(&env.core).expect("list");
    assert_eq!(all[0].id, "lullshift");
    assert!(all[0].builtin);
    assert_eq!(all[1].id, MARKET);
    assert_eq!(err_kind(add(&env.core, &repo.url())), "alreadyExists");

    // Entries come from the cache, with names read cheaply for same-repo sources.
    let entries = entries(&env.core, MARKET).expect("entries");
    let facts = entries
        .iter()
        .find(|e| e.activity_id == FACTS)
        .expect("facts");
    assert_eq!(facts.name.as_deref(), Some("Facts"));
    assert_eq!(facts.kind, Some("content"));
    assert!(facts.featured && !facts.external && facts.installed.is_none());
    assert!(!facts.third_party_code);
    let game = entries
        .iter()
        .find(|e| e.activity_id == GAME)
        .expect("game");
    assert_eq!(game.kind, Some("app"));
    assert!(game.third_party_code, "flagged so the list can say so");

    // Preview installs nothing.
    let p = preview_install(&env.core, MARKET, FACTS).expect("preview");
    assert_eq!(p.marketplace_id, MARKET);
    assert_eq!(p.activity_id, FACTS);
    assert_eq!(p.commit, commit);
    assert_eq!(p.kind, "content");
    assert_eq!(p.engine.as_deref(), Some("lullshift.cards@1"));
    assert_eq!(p.description, "A few facts.");
    assert_eq!(p.source_path.as_deref(), Some("packs/facts"));
    assert_eq!(p.file_count, 3);
    assert!(p.third_party && p.permissions.is_empty());
    assert!(p.code_risk.is_none(), "a pack carries no code warning");
    assert!(!listed_ids(&env).contains(&FACTS.to_owned()));
    assert!(!env.core.app_dir.join("installed").exists());

    // Install: listed (with its source) and served under the app CSP from the engine's files.
    install_pending(&env.core, &p.token, false, false, Instant::now()).expect("install");
    assert_eq!(
        err_kind(install_pending(
            &env.core,
            &p.token,
            false,
            false,
            Instant::now()
        )),
        "notFound",
        "a token works once"
    );
    assert!(env.install_dir(FACTS, &commit).join("deck.json").is_file());
    let infos = activities::list_activities(&env.core).expect("list");
    let info = infos.iter().find(|i| i.id() == FACTS).expect("listed");
    let source = serde_json::to_value(info).expect("json")["source"].clone();
    assert_eq!(
        source,
        json!({"marketplaceId": MARKET, "builtin": false, "commit": commit})
    );
    assert!(infos.iter().any(|i| i.id() == bundled_app_id()));
    let ok = request(&env, &format!("/{FACTS}/index.html"));
    assert_eq!(ok.status(), StatusCode::OK);
    assert_eq!(
        ok.headers()[tauri::http::header::CONTENT_SECURITY_POLICY],
        activities::activity_csp().as_str()
    );
    assert!(!ok.body().is_empty());
    assert_eq!(
        request(&env, &format!("/{FACTS}/deck.json")).status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(&env, &format!("/{FACTS}/../x/index.html")).status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        activities::content_for_core(&env.core, FACTS).expect("content"),
        serde_json::from_str::<serde_json::Value>(DECK).expect("deck")
    );
    assert_eq!(
        activities::storage_target(&env.core, FACTS, "k").unwrap_err(),
        "not_permitted"
    );

    // The entry now shows as installed, and the manage list has it.
    let again = super::entries(&env.core, MARKET).expect("entries");
    let mark = again
        .iter()
        .find(|e| e.activity_id == FACTS)
        .and_then(|e| e.installed.clone())
        .expect("installed mark");
    assert_eq!(
        (mark.commit.as_str(), mark.version.as_str(), mark.enabled),
        (commit.as_str(), "1.0.0", true)
    );
    let item = manage_item(&env, FACTS);
    assert_eq!(
        (item.kind, item.builtin, item.enabled, item.problem),
        ("content", false, true, None)
    );
    assert_eq!(item.commit.as_deref(), Some(commit.as_str()));

    // Installing the same id again is refused: use update.
    assert_eq!(
        err_kind(preview_install(&env.core, MARKET, FACTS)),
        "alreadyExists"
    );
}

#[test]
fn errors_carry_no_paths_urls_or_git_text() {
    let env = Env::new();
    let (repo, _) = standard_repo(&env);
    let secret = env.root.to_string_lossy().replace('\\', "/");

    // Not a marketplace.
    let bare = env.repo("plain");
    bare.write("README.md", "hi");
    bare.commit();
    let e = add(&env.core, &bare.url()).expect_err("no index");
    assert_eq!(e.kind, "invalidIndex");
    // A repository that doesn't exist.
    let missing = GitUrl::for_test_file(&env.root.join("nowhere"));
    let e2 = add(&env.core, &missing).expect_err("missing repo");
    assert!(
        ["notFound", "internal", "offline"].contains(&e2.kind),
        "{}",
        e2.kind
    );
    for e in [&e, &e2] {
        let text = e.message.replace('\\', "/");
        assert!(!text.contains(&secret), "{}", e.message);
        assert!(!text.contains("fatal"), "{}", e.message);
        assert!(!text.contains("file://"), "{}", e.message);
    }
    // The reserved name.
    repo.write(
        ".lullshift/marketplace.json",
        &json!({"schemaVersion":1,"name":"lullshift","activities":[]}).to_string(),
    );
    repo.commit();
    let reserved = add(&env.core, &repo.url()).expect_err("reserved");
    assert_eq!(reserved.kind, "invalidIndex");
    // Nothing half-added is left behind.
    assert_eq!(list(&env.core).expect("list").len(), 1);
    let staging: Vec<_> = std::fs::read_dir(markets_dir(&env.core))
        .map(|d| d.flatten().collect())
        .unwrap_or_default();
    assert!(staging.is_empty(), "{staging:?}");
}

// ---- refresh and update ------------------------------------------------------------------

#[test]
fn refresh_then_update_shows_a_permission_diff_and_keeps_the_enabled_flag() {
    let env = Env::new();
    let (repo, v1) = installed(&env);
    set_enabled(&env.core, FACTS, false).expect("disable");

    // Nothing new yet.
    let same = preview_update(&env.core, FACTS).expect("preview");
    assert!(same.unchanged);
    assert_eq!(same.from.commit, v1);
    update_pending(&env.core, &same.preview.token, false, Instant::now()).expect("no-op");
    assert_eq!(
        with_store(&env.core, |s| s.installed_activity(FACTS))
            .expect("store")
            .expect("record")
            .commit_sha,
        v1
    );

    // A new commit that asks for a permission.
    write_facts(&repo, "2.0.0", &["activity.storage"]);
    let v2 = repo.commit();
    let m = refresh(&env.core, MARKET).expect("refresh");
    assert_eq!(m.last_fetched_commit.as_deref(), Some(v2.as_str()));

    let up = preview_update(&env.core, FACTS).expect("update preview");
    assert!(!up.unchanged);
    assert_eq!(up.from.version, "1.0.0");
    assert_eq!(up.preview.version, "2.0.0");
    assert_eq!(up.preview.commit, v2);
    assert_eq!(up.added_permissions, vec!["activity.storage".to_owned()]);
    assert!(up.removed_permissions.is_empty());
    assert!(!up.kind_changed && !up.engine_changed);
    // The preview alone changed nothing.
    assert!(env.install_dir(FACTS, &v1).is_dir() && !env.install_dir(FACTS, &v2).exists());

    // An install token can't be used as an update token, and the reverse.
    assert_eq!(
        err_kind(install_pending(
            &env.core,
            &up.preview.token,
            false,
            false,
            Instant::now()
        )),
        "notFound"
    );
    let up = preview_update(&env.core, FACTS).expect("again");
    update_pending(&env.core, &up.preview.token, false, Instant::now()).expect("update");
    assert!(env.install_dir(FACTS, &v2).is_dir(), "new commit folder");
    assert!(
        !env.install_dir(FACTS, &v1).exists(),
        "old commit folder removed"
    );
    let rec = with_store(&env.core, |s| s.installed_activity(FACTS))
        .expect("store")
        .expect("record");
    assert_eq!(
        (rec.commit_sha.as_str(), rec.manifest_version.as_str()),
        (v2.as_str(), "2.0.0")
    );
    assert_eq!(rec.permissions, vec!["activity.storage".to_owned()]);
    assert!(!rec.enabled, "the enabled flag is kept");
    // It is off, so it is not listed; turning it on serves the new version.
    assert!(!listed_ids(&env).contains(&FACTS.to_owned()));
    set_enabled(&env.core, FACTS, true).expect("enable");
    assert!(listed_ids(&env).contains(&FACTS.to_owned()));
    assert!(activities::content_for_core(&env.core, FACTS).is_ok());
}

#[test]
fn refreshing_the_built_in_marketplace_needs_no_network_or_git() {
    let env = Env::new();
    let m = refresh(&env.core, "lullshift").expect("builtin");
    assert!(m.builtin && m.last_fetched_at.is_none());
    assert!(!env.core.app_dir.join("marketplaces").exists());
    // Its entries are the bundled activities, without engines.
    let e = entries(&env.core, "lullshift").expect("entries");
    assert!(e.iter().any(|e| e.activity_id == bundled_app_id()));
    assert!(e.iter().all(|e| e.installed.is_none() && !e.external));
    assert_eq!(
        err_kind(preview_install(&env.core, "lullshift", &bundled_app_id())),
        "alreadyExists"
    );
    assert_eq!(err_kind(refresh(&env.core, "nope-market")), "notFound");
    assert_eq!(err_kind(refresh(&env.core, "../x")), "notFound");
}

#[test]
fn removing_a_marketplace_keeps_its_installs_and_stops_updates() {
    let env = Env::new();
    installed(&env);
    let cache = cache_dir(&env.core, MARKET);
    assert!(cache.is_dir());
    remove(&env.core, MARKET).expect("remove");
    assert!(!cache.exists());
    assert_eq!(list(&env.core).expect("list").len(), 1);
    assert!(
        listed_ids(&env).contains(&FACTS.to_owned()),
        "installs stay"
    );
    assert_eq!(err_kind(preview_update(&env.core, FACTS)), "notFound");
    assert_eq!(err_kind(refresh(&env.core, MARKET)), "notFound");
    assert_eq!(
        err_kind(preview_install(&env.core, MARKET, GAME)),
        "notFound"
    );
}

// ---- enable and disable ------------------------------------------------------------------

#[test]
fn disabling_a_built_in_hides_it_but_keeps_its_saved_state() {
    let env = Env::new();
    let id = bundled_app_id();
    with_store(&env.core, |s| {
        s.save_snapshot(&id, "{\"n\":3}", OffsetDateTime::now_utc(), 4096)
    })
    .expect("save");
    assert!(listed_ids(&env).contains(&id));
    assert!(
        request(&env, &format!("/{id}/index.html"))
            .status()
            .is_success()
    );

    set_enabled(&env.core, &id, false).expect("disable");
    assert!(!listed_ids(&env).contains(&id));
    assert!(manage_item(&env, &id).builtin);
    assert!(!manage_item(&env, &id).enabled);
    assert_eq!(
        request(&env, &format!("/{id}/index.html")).status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        activities::content_for_core(&env.core, &id)
            .unwrap_err()
            .code(),
        "unknown_activity"
    );
    assert!(
        with_store(&env.core, |s| s.snapshot(&id))
            .expect("get")
            .is_some()
    );
    // Idempotent.
    set_enabled(&env.core, &id, false).expect("again");

    set_enabled(&env.core, &id, true).expect("enable");
    assert!(listed_ids(&env).contains(&id));
    assert!(manage_item(&env, &id).enabled);
    assert!(
        request(&env, &format!("/{id}/index.html"))
            .status()
            .is_success()
    );
    assert!(
        with_store(&env.core, |s| s.snapshot(&id))
            .expect("get")
            .is_some()
    );

    // Unknown ids and engines (which are never items of their own).
    assert_eq!(
        err_kind(set_enabled(&env.core, "com.example.nothing", false)),
        "notFound"
    );
    assert_eq!(
        err_kind(set_enabled(&env.core, "lullshift.cards", false)),
        "notFound"
    );
}

#[test]
fn disabling_an_installed_activity_stops_listing_and_serving_it() {
    let env = Env::new();
    installed(&env);
    assert!(
        request(&env, &format!("/{FACTS}/index.html"))
            .status()
            .is_success()
    );
    set_enabled(&env.core, FACTS, false).expect("disable");
    assert!(!listed_ids(&env).contains(&FACTS.to_owned()));
    assert_eq!(
        request(&env, &format!("/{FACTS}/index.html")).status(),
        StatusCode::NOT_FOUND
    );
    let item = manage_item(&env, FACTS);
    assert!(
        !item.enabled && item.problem.is_none(),
        "disabled, not broken"
    );
    assert!(
        env.install_dir(FACTS, &item.commit.expect("commit"))
            .is_dir()
    );
    set_enabled(&env.core, FACTS, true).expect("enable");
    assert!(listed_ids(&env).contains(&FACTS.to_owned()));
    assert!(
        request(&env, &format!("/{FACTS}/index.html"))
            .status()
            .is_success()
    );
}

// ---- uninstall ---------------------------------------------------------------------------

fn save_state(env: &Env, id: &str) {
    with_store(&env.core, |s| {
        s.save_snapshot(id, "{\"n\":1}", OffsetDateTime::now_utc(), 4096)?;
        s.set_activity_value(id, "best", "9", 4096)?;
        s.add_play_time(id, 30, true, 3600, OffsetDateTime::now_utc())
    })
    .expect("state");
}

#[test]
fn uninstall_keeps_saved_state_unless_asked() {
    let env = Env::new();
    let (_, commit) = installed(&env);
    save_state(&env, FACTS);
    save_state(&env, "com.example.other");

    uninstall(&env.core, FACTS, false).expect("uninstall");
    assert!(!env.install_dir(FACTS, &commit).exists());
    assert!(!env.core.app_dir.join("installed").join(FACTS).exists());
    assert!(!listed_ids(&env).contains(&FACTS.to_owned()));
    assert_eq!(
        request(&env, &format!("/{FACTS}/index.html")).status(),
        StatusCode::NOT_FOUND
    );
    assert!(
        with_store(&env.core, |s| s.snapshot(FACTS))
            .expect("get")
            .is_some()
    );
    assert_eq!(err_kind(uninstall(&env.core, FACTS, false)), "notFound");

    // Install again and uninstall with the saved state.
    let p = preview_install(&env.core, MARKET, FACTS).expect("preview");
    install_pending(&env.core, &p.token, false, false, Instant::now()).expect("install");
    uninstall(&env.core, FACTS, true).expect("uninstall");
    assert!(
        with_store(&env.core, |s| s.snapshot(FACTS))
            .expect("get")
            .is_none()
    );
    assert_eq!(
        with_store(&env.core, |s| s.activity_value(FACTS, "best")).expect("get"),
        None
    );
    assert_eq!(
        with_store(&env.core, |s| s.play_time(FACTS)).expect("get"),
        0
    );
    // Another activity's state is untouched.
    assert!(
        with_store(&env.core, |s| s.snapshot("com.example.other"))
            .expect("get")
            .is_some()
    );
}

#[test]
fn bundled_activities_cannot_be_uninstalled() {
    let env = Env::new();
    assert_eq!(
        err_kind(uninstall(&env.core, &bundled_app_id(), true)),
        "builtinProtected"
    );
    assert_eq!(
        err_kind(uninstall(&env.core, "com.example.nothing", false)),
        "notFound"
    );
    assert_eq!(err_kind(remove(&env.core, "lullshift")), "builtinProtected");
    assert_eq!(
        err_kind(preview_update(&env.core, &bundled_app_id())),
        "builtinProtected"
    );
    assert!(list(&env.core).expect("list").iter().any(|m| m.builtin));
}

// ---- tampering ---------------------------------------------------------------------------

#[test]
fn a_file_changed_after_the_check_is_refused_on_the_next_load_and_shown_damaged() {
    let env = Env::new();
    let (_, commit) = installed(&env);
    assert!(listed_ids(&env).contains(&FACTS.to_owned()));
    let original = activities::content_for_core(&env.core, FACTS).expect("content");
    // Loading twice reads and hashes twice; nothing changed, so both agree.
    assert_eq!(
        activities::content_for_core(&env.core, FACTS).expect("again"),
        original
    );

    // Someone edits the installed data while the snapshot still lists it. The very next load
    // reads the file again, sees it no longer matches, and hands the engine nothing.
    let deck = env.install_dir(FACTS, &commit).join("deck.json");
    let good = std::fs::read(&deck).expect("read");
    std::fs::write(&deck, r#"{"title":"Evil","cards":[{"front":"x"}]}"#).expect("tamper");
    let err = activities::content_for_core(&env.core, FACTS).unwrap_err();
    assert_eq!(err.code(), "damaged");

    // It is turned off and shown as damaged; nothing of it is listed or served any more.
    assert!(!listed_ids(&env).contains(&FACTS.to_owned()));
    assert_eq!(
        request(&env, &format!("/{FACTS}/index.html")).status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        activities::content_for_core(&env.core, FACTS)
            .unwrap_err()
            .code(),
        "unknown_activity"
    );
    let item = manage_item(&env, FACTS);
    assert_eq!(item.problem, Some("tampered"));
    assert!(!item.enabled);
    let rec = with_store(&env.core, |s| s.installed_activity(FACTS))
        .expect("store")
        .expect("record");
    assert!(!rec.enabled, "disabled in the store too");
    // It can't be turned back on while it differs.
    assert_eq!(err_kind(set_enabled(&env.core, FACTS, true)), "tampered");
    assert!(!listed_ids(&env).contains(&FACTS.to_owned()));

    // Once the original bytes are back it verifies again, but stays off until turned on.
    std::fs::write(&deck, good).expect("restore");
    set_enabled(&env.core, FACTS, true).expect("enable");
    assert_eq!(
        activities::content_for_core(&env.core, FACTS).expect("content"),
        original
    );
    uninstall(&env.core, FACTS, false).expect("uninstall");
    assert!(!env.install_dir(FACTS, &commit).exists());
}

/// Holds `path` so no other handle can read it, as a virus scanner can on Windows (chmod 000 on
/// Unix). `None` if this platform or user can still read it (root, for example).
fn lock_against_reading(path: &std::path::Path) -> Option<std::fs::File> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(path)
            .ok()
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o000)).ok()?;
        if std::fs::read(path).is_ok() {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644)).ok()?;
            return None;
        }
        std::fs::File::open(path.parent()?).ok()
    }
}

fn unlock(path: &std::path::Path, lock: std::fs::File) {
    drop(lock);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644)).expect("chmod");
    }
    #[cfg(not(unix))]
    let _ = path;
}

#[test]
fn a_file_that_cant_be_read_right_now_refuses_the_load_but_is_not_marked_damaged() {
    // Security review INFO-1: a locked file (antivirus, backup) is not tampering.
    let env = Env::new();
    let (_, commit) = installed(&env);
    let original = activities::content_for_core(&env.core, FACTS).expect("content");
    let deck = env.install_dir(FACTS, &commit).join("deck.json");
    let Some(lock) = lock_against_reading(&deck) else {
        eprintln!("skipped: this platform or user can read a locked file");
        return;
    };
    let err = activities::content_for_core(&env.core, FACTS).unwrap_err();
    assert_eq!(err.code(), "unreadable", "a retryable refusal");
    let rec = with_store(&env.core, |s| s.installed_activity(FACTS))
        .expect("store")
        .expect("record");
    assert!(rec.enabled, "not turned off in the store");
    // A fresh check while it is still locked shows it as unreadable, still on, not served.
    env.core.library.invalidate();
    let item = manage_item(&env, FACTS);
    assert_eq!(item.problem, Some("unreadable"));
    assert!(item.enabled);
    assert!(!listed_ids(&env).contains(&FACTS.to_owned()));
    assert_eq!(
        err_kind(set_enabled(&env.core, FACTS, true)),
        "unreadable",
        "turning on re-checks and asks for a retry"
    );

    // Once the lock is gone, the next use works again without any reinstall or toggle.
    unlock(&deck, lock);
    assert!(listed_ids(&env).contains(&FACTS.to_owned()));
    assert_eq!(manage_item(&env, FACTS).problem, None);
    assert_eq!(
        activities::content_for_core(&env.core, FACTS).expect("content"),
        original
    );
}

#[test]
fn a_same_length_change_is_caught_by_the_hash_not_only_the_size() {
    let env = Env::new();
    let (_, commit) = installed(&env);
    let deck = env.install_dir(FACTS, &commit).join("deck.json");
    let mut bytes = std::fs::read(&deck).expect("read");
    let at = bytes
        .iter()
        .position(|b| b.is_ascii_lowercase())
        .expect("letter");
    bytes[at] = if bytes[at] == b'a' { b'b' } else { b'a' };
    std::fs::write(&deck, &bytes).expect("tamper");
    assert_eq!(
        activities::content_for_core(&env.core, FACTS)
            .unwrap_err()
            .code(),
        "damaged"
    );
    assert_eq!(manage_item(&env, FACTS).problem, Some("tampered"));
}

#[test]
fn an_extra_or_missing_file_is_also_tampering() {
    let env = Env::new();
    let (_, commit) = installed(&env);
    let dir = env.install_dir(FACTS, &commit);
    std::fs::write(dir.join("extra.md"), "x").expect("extra");
    env.core.library.invalidate();
    assert_eq!(manage_item(&env, FACTS).problem, Some("tampered"));
    std::fs::remove_file(dir.join("extra.md")).expect("rm");
    std::fs::remove_file(dir.join("README.md")).expect("rm");
    env.core.library.invalidate();
    assert_eq!(manage_item(&env, FACTS).problem, Some("tampered"));
}

#[test]
fn records_this_release_cannot_run_are_not_served() {
    let env = Env::new();
    added(&env);
    let bundled = bundled_pack_id();
    let digest = "0".repeat(64);
    let sha = "1".repeat(40);
    let record = |id: &str, kind| NewInstalledActivity {
        activity_id: id.to_owned(),
        marketplace_id: MARKET.to_owned(),
        source_url: "example.org/x".to_owned(),
        source_path: String::new(),
        commit_sha: sha.clone(),
        manifest_version: "1.0.0".to_owned(),
        kind,
        engine: None,
        permissions: Vec::new(),
        digest: digest.clone(),
        size_bytes: 1,
        code_consent: None,
    };
    // A hand-made third-party code record without consent, and one that reuses a bundled id.
    with_store(&env.core, |s| {
        s.put_installed_activity(&record(GAME, ActivityKind::App), OffsetDateTime::now_utc())?;
        s.put_installed_activity(
            &record(&bundled, ActivityKind::Content),
            OffsetDateTime::now_utc(),
        )
    })
    .expect("records");
    let ids = listed_ids(&env);
    assert!(!ids.contains(&GAME.to_owned()));
    assert_eq!(
        ids.iter().filter(|i| **i == bundled).count(),
        1,
        "bundled wins, once"
    );
    assert_eq!(manage_item(&env, GAME).problem, Some("tampered"));
    assert!(!manage_item(&env, GAME).enabled);
    assert_eq!(
        request(&env, &format!("/{GAME}/index.html")).status(),
        StatusCode::NOT_FOUND
    );
}

// ---- busy, blocked code, shadowing, tokens -----------------------------------------------

#[test]
fn a_second_operation_on_the_same_marketplace_is_busy() {
    let env = Env::new();
    installed(&env);
    {
        let _running = begin(&env.core, MARKET).expect("first");
        assert_eq!(err_kind(refresh(&env.core, MARKET)), "busy");
        assert_eq!(err_kind(preview_install(&env.core, MARKET, GAME)), "busy");
        assert_eq!(err_kind(preview_update(&env.core, FACTS)), "busy");
        assert_eq!(err_kind(uninstall(&env.core, FACTS, false)), "busy");
        assert_eq!(err_kind(remove(&env.core, MARKET)), "busy");
        assert_eq!(err_kind(begin(&env.core, MARKET).map(|_| ())), "busy");
        // Another marketplace id is not blocked.
        assert!(begin(&env.core, "other-market").is_ok());
    }
    // Released when the first finishes.
    refresh(&env.core, MARKET).expect("free again");
}

// ---- third-party code with consent (owner decision 27.5) ----------------------------------

const GAME_JS: &str = "document.title = 'g';";

/// Gives the standard repository's game a script and a stylesheet, and commits.
fn write_game(repo: &Repo, body: &str) -> String {
    repo.write("packs/game/manifest.json", &game_manifest(GAME));
    repo.write(
        "packs/game/index.html",
        &format!(
            "<!doctype html><title>g</title><link rel=stylesheet href=style.css><script src=main.js></script>{body}"
        ),
    );
    repo.write("packs/game/main.js", GAME_JS);
    repo.write("packs/game/style.css", "body{margin:0}");
    repo.commit()
}

/// The standard repository with a richer game, added.
fn added_with_game(env: &Env) -> (Repo, String) {
    let (repo, _) = standard_repo(env);
    let commit = write_game(&repo, "");
    add(&env.core, &repo.url()).expect("add");
    (repo, commit)
}

fn installed_game(env: &Env) -> (Repo, String) {
    let (repo, commit) = added_with_game(env);
    let p = preview_install(&env.core, MARKET, GAME).expect("preview");
    install_pending(&env.core, &p.token, false, true, Instant::now()).expect("install");
    (repo, commit)
}

#[test]
fn third_party_code_is_reviewed_with_a_risk_note_for_this_computer() {
    let env = Env::new();
    added_with_game(&env);
    let p = preview_install(&env.core, MARKET, GAME).expect("third-party code is reviewable");
    assert_eq!(p.kind, "app");
    assert!(p.third_party);
    let risk = p.code_risk.clone().expect("a code warning");
    let json = serde_json::to_value(&p).expect("json");
    let expected_platform = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    assert_eq!(json["codeRisk"]["platform"], expected_platform);
    assert_eq!(
        json["codeRisk"]["status"],
        serde_json::to_value(marketplace::code_risk_status()).expect("status")
    );
    assert_eq!(risk.status, marketplace::code_risk_status());
    // The review installs nothing.
    assert!(!env.core.app_dir.join("installed").exists());
}

#[test]
fn third_party_code_is_refused_without_consent_and_the_token_is_used_up() {
    let env = Env::new();
    added_with_game(&env);
    let p = preview_install(&env.core, MARKET, GAME).expect("preview");
    let e =
        install_pending(&env.core, &p.token, false, false, Instant::now()).expect_err("no consent");
    assert_eq!(e.kind, "codeRiskNotAccepted");
    assert!(
        e.message
            .contains("I understand and want to install this program")
    );
    assert!(
        !env.core.app_dir.join("installed").exists(),
        "nothing written"
    );
    assert!(
        with_store(&env.core, |s| s.installed_activity(GAME))
            .expect("store")
            .is_none(),
        "nothing recorded"
    );
    assert!(!listed_ids(&env).contains(&GAME.to_owned()));
    // The refused review can't be replayed with consent added afterwards.
    assert_eq!(
        err_kind(install_pending(
            &env.core,
            &p.token,
            false,
            true,
            Instant::now()
        )),
        "notFound"
    );
}

#[test]
fn third_party_code_installed_with_consent_runs_exactly_like_bundled_code() {
    let env = Env::new();
    let (_repo, commit) = installed_game(&env);

    // The consent is recorded with the commit it was given for.
    let rec = with_store(&env.core, |s| s.installed_activity(GAME))
        .expect("store")
        .expect("record");
    let consent = rec.code_consent.clone().expect("consent recorded");
    assert_eq!(consent.commit, commit);
    assert_eq!(rec.commit_sha, commit);
    assert!(rec.enabled);

    // Listed with its source, as an app (no engine).
    let infos = activities::list_activities(&env.core).expect("list");
    let info = infos.iter().find(|i| i.id() == GAME).expect("listed");
    let json = serde_json::to_value(info).expect("json");
    assert_eq!(
        json["source"],
        json!({"marketplaceId": MARKET, "builtin": false, "commit": commit})
    );
    assert_eq!(json["engine"], serde_json::Value::Null);
    let item = manage_item(&env, GAME);
    assert_eq!(
        (item.kind, item.builtin, item.enabled, item.problem),
        ("app", false, true, None)
    );

    // Every file is served with the same headers as a bundled app's, CSP included.
    let bundled = request(&env, &format!("/{}/index.html", bundled_app_id()));
    assert_eq!(bundled.status(), StatusCode::OK);
    for file in ["index.html", "main.js", "style.css", "manifest.json"] {
        let r = request(&env, &format!("/{GAME}/{file}"));
        assert_eq!(r.status(), StatusCode::OK, "{file}");
        for (name, value) in bundled.headers() {
            if name == tauri::http::header::CONTENT_TYPE {
                continue;
            }
            assert_eq!(r.headers().get(name), Some(value), "{file}: {name}");
        }
        assert_eq!(
            r.headers().len(),
            bundled.headers().len(),
            "{file}: no extra headers"
        );
        assert_eq!(
            r.headers()[tauri::http::header::CONTENT_SECURITY_POLICY],
            activities::activity_csp().as_str()
        );
    }
    assert_eq!(
        request(&env, &format!("/{GAME}/main.js")).body(),
        GAME_JS.as_bytes()
    );
    assert_eq!(
        request(&env, &format!("/{GAME}/main.js")).headers()[tauri::http::header::CONTENT_TYPE],
        "text/javascript; charset=utf-8"
    );
    // Path rules are the same: no escaping the folder, no unlisted files.
    for bad in [
        "../x/index.html",
        "missing.js",
        ".hidden.js",
        "main.js%00.html",
    ] {
        assert_eq!(
            request(&env, &format!("/{GAME}/{bad}")).status(),
            StatusCode::NOT_FOUND,
            "{bad}"
        );
    }
    // It has no storage permission, so the host refuses storage on its behalf.
    assert_eq!(
        activities::storage_target(&env.core, GAME, "k").unwrap_err(),
        "not_permitted"
    );
    // An app gets no pack content.
    assert_eq!(
        activities::content_for_core(&env.core, GAME).expect("content"),
        serde_json::Value::Null
    );
}

#[test]
fn installed_third_party_code_changed_on_disk_is_never_served() {
    let env = Env::new();
    let (_repo, commit) = installed_game(&env);
    assert_eq!(
        request(&env, &format!("/{GAME}/main.js")).status(),
        StatusCode::OK
    );
    // Same length, different bytes: caught by the re-hash on this load.
    let evil: String = GAME_JS.chars().map(|_| 'x').collect();
    std::fs::write(env.install_dir(GAME, &commit).join("main.js"), evil).expect("swap");
    assert_eq!(
        request(&env, &format!("/{GAME}/main.js")).status(),
        StatusCode::NOT_FOUND
    );
    env.core.library.invalidate();
    assert_eq!(manage_item(&env, GAME).problem, Some("tampered"));
    assert!(!listed_ids(&env).contains(&GAME.to_owned()));
}

#[test]
fn an_update_that_changes_third_party_code_needs_consent_again() {
    let env = Env::new();
    let (repo, v1) = installed_game(&env);
    let first = with_store(&env.core, |s| s.installed_activity(GAME))
        .expect("store")
        .expect("record")
        .code_consent
        .expect("consent");

    // Nothing new: no consent is needed to confirm "no update".
    let same = preview_update(&env.core, GAME).expect("preview");
    assert!(same.unchanged);
    assert!(same.preview.code_risk.is_some(), "still flagged as code");
    update_pending(&env.core, &same.preview.token, false, Instant::now()).expect("no-op");

    // New code at a new commit.
    let v2 = write_game(&repo, "<p>v2</p>");
    refresh(&env.core, MARKET).expect("refresh");
    let up = preview_update(&env.core, GAME).expect("update preview");
    assert!(!up.unchanged);
    assert_eq!(up.preview.commit, v2);
    assert!(
        up.preview.code_risk.is_some(),
        "the update review warns again"
    );

    // Without consent: refused, nothing changes, the old commit keeps running.
    assert_eq!(
        err_kind(update_pending(
            &env.core,
            &up.preview.token,
            false,
            Instant::now()
        )),
        "codeRiskNotAccepted"
    );
    let rec = with_store(&env.core, |s| s.installed_activity(GAME))
        .expect("store")
        .expect("record");
    assert_eq!(rec.commit_sha, v1);
    assert_eq!(rec.code_consent.as_ref(), Some(&first));
    assert!(env.install_dir(GAME, &v1).is_dir() && !env.install_dir(GAME, &v2).exists());
    assert_eq!(
        request(&env, &format!("/{GAME}/index.html")).status(),
        StatusCode::OK
    );

    // With consent: updated, and the record holds the consent for the new commit.
    let up = preview_update(&env.core, GAME).expect("again");
    update_pending(&env.core, &up.preview.token, true, Instant::now()).expect("update");
    let rec = with_store(&env.core, |s| s.installed_activity(GAME))
        .expect("store")
        .expect("record");
    assert_eq!(rec.commit_sha, v2);
    let consent = rec.code_consent.expect("new consent");
    assert_eq!(consent.commit, v2);
    assert!(!env.install_dir(GAME, &v1).exists(), "old folder removed");
    let page = request(&env, &format!("/{GAME}/index.html"));
    assert_eq!(page.status(), StatusCode::OK);
    assert!(String::from_utf8_lossy(page.body()).contains("v2"));
}

#[test]
fn consent_is_ignored_for_content_packs() {
    // A pack has no code; the flag changes nothing either way.
    let env = Env::new();
    added(&env);
    let p = preview_install(&env.core, MARKET, FACTS).expect("preview");
    install_pending(&env.core, &p.token, false, true, Instant::now()).expect("install");
    let rec = with_store(&env.core, |s| s.installed_activity(FACTS))
        .expect("store")
        .expect("record");
    assert!(rec.code_consent.is_none(), "nothing recorded for a pack");
}

#[test]
fn installs_cannot_shadow_bundled_ids() {
    let env = Env::new();
    added(&env);
    let shadow = bundled_pack_id();
    assert_eq!(
        err_kind(preview_install(&env.core, MARKET, &shadow)),
        "alreadyExists"
    );
    assert!(!env.core.app_dir.join("installed").exists());
}

#[test]
fn tokens_expire_and_expired_reviews_install_nothing() {
    let env = Env::new();
    added(&env);
    let p = preview_install(&env.core, MARKET, FACTS).expect("preview");
    let later = Instant::now() + TOKEN_TTL + Duration::from_secs(1);
    assert_eq!(
        err_kind(install_pending(&env.core, &p.token, false, false, later)),
        "notFound"
    );
    assert!(!env.core.app_dir.join("installed").exists());
    // The expired token is gone for good, and a made-up one never works.
    assert_eq!(
        err_kind(install_pending(
            &env.core,
            &p.token,
            false,
            false,
            Instant::now()
        )),
        "notFound"
    );
    assert_eq!(
        err_kind(install_pending(
            &env.core,
            "00",
            false,
            false,
            Instant::now()
        )),
        "notFound"
    );
    // Only a few reviews are held at once.
    for _ in 0..(MAX_PENDING + 3) {
        preview_install(&env.core, MARKET, FACTS).expect("preview");
    }
    assert_eq!(
        env.core.market.pending.lock().expect("lock").len(),
        MAX_PENDING
    );
}

#[test]
fn the_new_commands_are_granted_only_to_the_main_window() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities/default.json");
    let cap: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir).expect("read")).expect("json");
    let perms: Vec<&str> = cap["permissions"]
        .as_array()
        .expect("array")
        .iter()
        .filter_map(|p| p.as_str())
        .collect();
    for p in [
        "allow-marketplace-add",
        "allow-marketplace-install",
        "allow-marketplace-update",
        "allow-activity-uninstall",
        "allow-activity-set-enabled",
        "allow-activities-manage-list",
    ] {
        assert!(perms.contains(&p), "{p}");
    }
    assert_eq!(cap["windows"], json!(["main"]));
}

// ---- security review fixes (M1, L4, L5, L7) -----------------------------------------------

/// A second repository that claims a marketplace name and lists its own FACTS pack.
fn lookalike_repo(env: &Env, name: &str, market_name: &str) -> Repo {
    let repo = env.repo(name);
    write_facts(&repo, "9.0.0", &["activity.storage"]);
    repo.write(
        ".lullshift/marketplace.json",
        &json!({
            "schemaVersion": 1, "name": market_name,
            "activities": [{"id": FACTS, "source": {"path": "packs/facts"}}]
        })
        .to_string(),
    );
    repo.commit();
    repo
}

#[test]
fn re_adding_a_removed_name_from_another_address_is_refused() {
    let env = Env::new();
    let (original, v1) = installed(&env);
    remove(&env.core, MARKET).expect("remove");

    let other = lookalike_repo(&env, "lookalike", MARKET);
    let e = add(&env.core, &other.url()).expect_err("refused");
    assert_eq!(e.kind, "alreadyExists");
    assert!(
        e.message.contains("different address"),
        "a clear message: {}",
        e.message
    );
    // Nothing changed: still removed, install untouched, still no updates from anywhere.
    assert_eq!(list(&env.core).expect("list").len(), 1);
    assert_eq!(err_kind(preview_update(&env.core, FACTS)), "notFound");
    let rec = with_store(&env.core, |s| s.installed_activity(FACTS))
        .expect("store")
        .expect("record");
    assert_eq!(rec.commit_sha, v1);
    assert!(
        !markets_dir(&env.core).join(MARKET).exists(),
        "no cache was kept for the refused source"
    );

    // The original address is still welcome, and updates then come from it.
    add(&env.core, &original.url()).expect("same address");
    assert!(preview_update(&env.core, FACTS).expect("update").unchanged);
}

#[test]
fn an_update_from_another_source_is_flagged() {
    let env = Env::new();
    installed(&env);
    let up = preview_update(&env.core, FACTS).expect("preview");
    assert!(up.source_changed.is_none(), "same source: nothing to flag");

    // The recorded source differs from where the index now points (as after a moved entry).
    let rec = with_store(&env.core, |s| s.installed_activity(FACTS))
        .expect("store")
        .expect("record");
    with_store(&env.core, |s| {
        s.put_installed_activity(
            &NewInstalledActivity {
                activity_id: rec.activity_id.clone(),
                marketplace_id: rec.marketplace_id.clone(),
                source_url: "https://old.example/team/facts".into(),
                source_path: rec.source_path.clone(),
                commit_sha: rec.commit_sha.clone(),
                manifest_version: rec.manifest_version.clone(),
                kind: rec.kind,
                engine: rec.engine.clone(),
                permissions: rec.permissions.clone(),
                digest: rec.digest.clone(),
                size_bytes: rec.size_bytes,
                code_consent: rec.code_consent.clone(),
            },
            OffsetDateTime::now_utc(),
        )
    })
    .expect("rewrite record");
    let up = preview_update(&env.core, FACTS).expect("preview");
    let changed = up.source_changed.clone().expect("flagged");
    assert_eq!(changed.old, "https://old.example/team/facts");
    assert_ne!(changed.new, changed.old);
    let json = serde_json::to_value(&up).expect("json");
    assert_eq!(
        json["sourceChanged"]["old"],
        "https://old.example/team/facts"
    );
    assert!(json["sourceChanged"]["new"].is_string());
}

#[test]
fn concurrent_installs_of_one_id_from_different_marketplaces_are_busy() {
    let env = Env::new();
    added(&env);
    let other = lookalike_repo(&env, "other", "other-tools");
    add(&env.core, &other.url()).expect("second marketplace");
    let first = preview_install(&env.core, MARKET, FACTS).expect("preview one");
    let second = preview_install(&env.core, "other-tools", FACTS).expect("preview two");
    {
        // The first install is running: the id is locked for every marketplace.
        let _running = begin(&env.core, &activity_key(FACTS)).expect("lock");
        assert_eq!(
            err_kind(install_pending(
                &env.core,
                &second.token,
                false,
                false,
                Instant::now()
            )),
            "busy"
        );
    }
    install_pending(&env.core, &first.token, false, false, Instant::now()).expect("first wins");
    assert_eq!(
        err_kind(preview_install(&env.core, "other-tools", FACTS)),
        "alreadyExists"
    );
    let update = preview_update(&env.core, FACTS).expect("update preview");
    {
        let _running = begin(&env.core, &activity_key(FACTS)).expect("lock");
        assert_eq!(
            err_kind(update_pending(
                &env.core,
                &update.preview.token,
                false,
                Instant::now()
            )),
            "busy"
        );
        assert_eq!(err_kind(uninstall(&env.core, FACTS, false)), "busy");
    }
    uninstall(&env.core, FACTS, false).expect("free again");
}

#[test]
fn saved_state_is_announced_and_reused_only_by_the_same_source() {
    let env = Env::new();
    installed(&env);
    save_state(&env, FACTS);
    save_state(&env, "com.example.other");
    uninstall(&env.core, FACTS, false).expect("uninstall, keep state");

    // Same marketplace and source: the review says progress will be used, and it is.
    let again = preview_install(&env.core, MARKET, FACTS).expect("preview");
    assert!(again.has_saved_state);
    assert_eq!(
        serde_json::to_value(&again).expect("json")["hasSavedState"],
        true
    );
    install_pending(&env.core, &again.token, false, false, Instant::now()).expect("install");
    assert!(
        with_store(&env.core, |s| s.snapshot(FACTS))
            .expect("get")
            .is_some()
    );

    // Uninstall again, then install the same id from a different marketplace: the old state is
    // cleared (and the review does not promise it); other activities keep theirs.
    uninstall(&env.core, FACTS, false).expect("uninstall, keep state");
    let other = lookalike_repo(&env, "other", "other-tools");
    add(&env.core, &other.url()).expect("second marketplace");
    let foreign = preview_install(&env.core, "other-tools", FACTS).expect("preview");
    assert!(!foreign.has_saved_state);
    assert_eq!(
        serde_json::to_value(&foreign).expect("json")["savedState"],
        "foreign",
        "the review asks"
    );
    assert!(
        with_store(&env.core, |s| s.snapshot(FACTS))
            .expect("get")
            .is_some(),
        "a preview deletes nothing"
    );
    install_pending(&env.core, &foreign.token, false, false, Instant::now()).expect("install");
    assert!(
        with_store(&env.core, |s| s.snapshot(FACTS))
            .expect("get")
            .is_none()
    );
    assert_eq!(
        with_store(&env.core, |s| s.activity_value(FACTS, "best")).expect("get"),
        None
    );
    assert_eq!(
        with_store(&env.core, |s| s.play_time(FACTS)).expect("get"),
        0
    );
    assert!(
        with_store(&env.core, |s| s.snapshot("com.example.other"))
            .expect("get")
            .is_some()
    );
}

#[test]
fn state_of_unknown_origin_is_announced_in_the_review() {
    let env = Env::new();
    added(&env);
    let clean = preview_install(&env.core, MARKET, FACTS).expect("preview");
    assert!(!clean.has_saved_state);
    assert_eq!(
        serde_json::to_value(&clean).expect("json")["savedState"],
        "none"
    );
    save_state(&env, FACTS);
    let with_state = preview_install(&env.core, MARKET, FACTS).expect("preview");
    assert!(with_state.has_saved_state);
    // State older than the owner record is kept and used as before; nothing is asked.
    assert_eq!(
        serde_json::to_value(&with_state).expect("json")["savedState"],
        "unknown"
    );
    install_pending(&env.core, &with_state.token, false, false, Instant::now()).expect("install");
    assert!(
        with_store(&env.core, |s| s.snapshot(FACTS))
            .expect("get")
            .is_some()
    );
}

#[test]
fn progress_from_another_source_is_kept_only_when_the_user_chooses_to() {
    let env = Env::new();
    installed(&env);
    save_state(&env, FACTS);
    uninstall(&env.core, FACTS, false).expect("uninstall, keep state");
    let other = lookalike_repo(&env, "other", "other-tools");
    add(&env.core, &other.url()).expect("second marketplace");

    let foreign = preview_install(&env.core, "other-tools", FACTS).expect("preview");
    assert_eq!(
        serde_json::to_value(&foreign).expect("json")["savedState"],
        "foreign"
    );
    install_pending(&env.core, &foreign.token, true, false, Instant::now()).expect("install, keep");
    assert!(
        with_store(&env.core, |s| s.snapshot(FACTS))
            .expect("get")
            .is_some(),
        "kept"
    );
    assert_eq!(
        with_store(&env.core, |s| s.activity_value(FACTS, "best"))
            .expect("get")
            .as_deref(),
        Some("9")
    );
    // The new install owns it now: reinstalling from the same place reuses it without asking.
    uninstall(&env.core, FACTS, false).expect("uninstall, keep state");
    let again = preview_install(&env.core, "other-tools", FACTS).expect("preview");
    assert_eq!(
        serde_json::to_value(&again).expect("json")["savedState"],
        "own"
    );
}

#[test]
fn a_preview_image_swapped_after_the_check_is_never_shown() {
    let env = Env::new();
    let repo = env.repo("market");
    write_facts(&repo, "1.0.0", &[]);
    let mut manifest: serde_json::Value =
        serde_json::from_str(&cards_manifest(FACTS, "1.0.0", &[])).expect("json");
    manifest["preview"] = json!("preview.png");
    repo.write("packs/facts/manifest.json", &manifest.to_string());
    repo.write("packs/facts/preview.png", "ORIGINAL-IMAGE");
    repo.write(
        ".lullshift/marketplace.json",
        &json!({
            "schemaVersion": 1, "name": MARKET,
            "activities": [{"id": FACTS, "source": {"path": "packs/facts"}}]
        })
        .to_string(),
    );
    let commit = repo.commit();
    add(&env.core, &repo.url()).expect("add");
    let p = preview_install(&env.core, MARKET, FACTS).expect("preview");
    install_pending(&env.core, &p.token, false, false, Instant::now()).expect("install");

    let served = |env: &Env| {
        env.core
            .library
            .snapshot(&env.core)
            .expect("snapshot")
            .served
            .get(FACTS)
            .map(|s| s.preview_bytes(activities::PREVIEW_MAX))
    };
    assert_eq!(
        served(&env),
        Some(Ok(Some(b"ORIGINAL-IMAGE".to_vec()))),
        "read and hashed now"
    );

    // The file is swapped (same length) after the snapshot's check: the next read hashes it
    // again and refuses it.
    std::fs::write(
        env.install_dir(FACTS, &commit).join("preview.png"),
        "SWAPPED-IMAGE!",
    )
    .expect("swap");
    assert!(matches!(served(&env), Some(Err(_))));

    // The command that shows it marks the activity damaged and returns no image.
    assert_eq!(
        activities::preview_for_core(&env.core, FACTS).expect("preview"),
        None
    );
    assert_eq!(served(&env), None, "no longer served");
    let item = manage_item(&env, FACTS);
    assert_eq!(item.problem, Some("tampered"));
    assert!(!item.enabled);
}

// ---- pictures in installed packs (S12-T11) ------------------------------------------------

const PICTURES: &str = "com.example.pictures";
const HEXAGON: &[u8] = include_bytes!(
    "../../../../../activities/content/quiz/quiz-shapes-and-sides/images/hexagon.png"
);

fn pictures_repo(env: &Env, name: &str, image: &[u8]) -> Repo {
    let repo = env.repo(name);
    repo.write(
        "packs/pictures/manifest.json",
        &json!({
            "schemaVersion": 1, "kind": "content", "id": PICTURES, "version": "1.0.0",
            "name": "Pictures", "description": "One picture.", "category": "interactive",
            "minHostVersion": "0.1.0", "sdkVersion": "0.2.0", "permissions": [], "offline": true,
            "engine": "lullshift.quiz@1", "data": "quiz.json", "images": ["images/shape.png"]
        })
        .to_string(),
    );
    repo.write(
        "packs/pictures/quiz.json",
        &json!({"title": "Pictures", "questions": [{
            "prompt": "What shape is this?",
            "image": {"src": "images/shape.png", "alt": "A flat shape with six sides."},
            "choices": ["Hexagon", "Square"], "answer": 0,
            "explanation": "A hexagon has six sides."
        }]})
        .to_string(),
    );
    let path = repo.dir.join("packs/pictures/images/shape.png");
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(path, image).expect("image");
    repo.write(
        ".lullshift/marketplace.json",
        &json!({
            "schemaVersion": 1, "name": MARKET,
            "activities": [{"id": PICTURES, "source": {"path": "packs/pictures"}}]
        })
        .to_string(),
    );
    repo.commit();
    repo
}

#[test]
fn an_installed_pack_serves_its_listed_pictures_and_nothing_else() {
    let env = Env::new();
    let repo = pictures_repo(&env, "pictures", HEXAGON);
    add(&env.core, &repo.url()).expect("add");
    let p = preview_install(&env.core, MARKET, PICTURES).expect("preview");
    install_pending(&env.core, &p.token, false, false, Instant::now()).expect("install");
    assert!(listed_ids(&env).contains(&PICTURES.to_owned()));
    activities::content_for_core(&env.core, PICTURES).expect("content");

    let ok = request(&env, &format!("/{PICTURES}/images/shape.png"));
    assert_eq!(ok.status(), StatusCode::OK);
    assert_eq!(ok.headers()[tauri::http::header::CONTENT_TYPE], "image/png");
    assert_eq!(
        ok.headers()[tauri::http::header::X_CONTENT_TYPE_OPTIONS],
        "nosniff"
    );
    assert_eq!(
        ok.headers()[tauri::http::header::CONTENT_SECURITY_POLICY],
        activities::activity_csp().as_str()
    );
    assert_eq!(ok.body().as_slice(), HEXAGON);
    for path in [
        "images/other.png",
        "images/shape.webp",
        "quiz.json",
        "images/../quiz.json",
    ] {
        assert_eq!(
            request(&env, &format!("/{PICTURES}/{path}")).status(),
            StatusCode::NOT_FOUND,
            "{path}"
        );
    }
}

#[test]
fn a_picture_changed_on_disk_after_the_check_is_refused_and_the_pack_shown_damaged() {
    // Decision 18.5: a listed picture is read and hashed again on every request.
    let env = Env::new();
    let repo = pictures_repo(&env, "pictures", HEXAGON);
    add(&env.core, &repo.url()).expect("add");
    let p = preview_install(&env.core, MARKET, PICTURES).expect("preview");
    install_pending(&env.core, &p.token, false, false, Instant::now()).expect("install");
    let path = format!("/{PICTURES}/images/shape.png");
    assert_eq!(request(&env, &path).status(), StatusCode::OK);

    // Same length, one byte changed, after the snapshot's check.
    let rec = with_store(&env.core, |s| s.installed_activity(PICTURES))
        .expect("store")
        .expect("record");
    let file = env
        .install_dir(PICTURES, &rec.commit_sha)
        .join("images/shape.png");
    let mut bytes = std::fs::read(&file).expect("read");
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;
    std::fs::write(&file, &bytes).expect("tamper");

    let refused = request(&env, &path);
    assert_eq!(refused.status(), StatusCode::NOT_FOUND);
    assert_ne!(refused.body().as_slice(), bytes.as_slice());
    let item = manage_item(&env, PICTURES);
    assert_eq!(item.problem, Some("tampered"));
    assert!(!item.enabled);
    assert!(!listed_ids(&env).contains(&PICTURES.to_owned()));
}

#[test]
fn a_pack_with_a_disguised_or_animated_picture_is_refused() {
    // An SVG with a script, renamed to .png.
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#;
    // A real PNG with an animation control chunk (acTL) slipped in after the header.
    let actl = [0u8, 0, 0, 1, 0, 0, 0, 0];
    let mut chunk = b"acTL".to_vec();
    chunk.extend_from_slice(&actl);
    let mut apng = HEXAGON[..33].to_vec();
    apng.extend_from_slice(&u32::try_from(actl.len()).expect("len").to_be_bytes());
    apng.extend_from_slice(&chunk);
    apng.extend_from_slice(&crc32(&chunk).to_be_bytes());
    apng.extend_from_slice(&HEXAGON[33..]);
    for (name, bytes) in [("svg", svg.as_slice()), ("apng", apng.as_slice())] {
        let env = Env::new();
        let repo = pictures_repo(&env, name, bytes);
        add(&env.core, &repo.url()).expect("add");
        // The preview may already refuse; if not, the install must.
        let refused = match preview_install(&env.core, MARKET, PICTURES) {
            Err(e) => e,
            Ok(p) => {
                install_pending(&env.core, &p.token, false, false, Instant::now()).expect_err(name)
            }
        };
        assert_eq!(refused.kind, "invalidPack", "{name}");
        assert!(
            refused.message.contains("picture isn't allowed"),
            "{name}: {}",
            refused.message
        );
        assert!(!listed_ids(&env).contains(&PICTURES.to_owned()), "{name}");
        assert!(
            !env.core.app_dir.join("installed").join(PICTURES).exists(),
            "{name}"
        );
    }
}

/// CRC-32 (IEEE), enough to build a well-formed chunk in a test.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

// ---- Sprint 12: private-network marketplaces (decision 17) -------------------------------

#[test]
fn a_private_address_needs_the_confirmation_before_anything_is_fetched() {
    let env = Env::new();
    for text in [
        "https://192.168.1.10/me/market",
        "https://localhost/me/market",
        "https://gitserver/me/market",
        "git@nas.home:me/market.git",
    ] {
        let url = user_url(text, false).expect("private addresses parse");
        let err = add(&env.core, &url).expect_err(text);
        assert_eq!(err.kind, "privateHost", "{text}");
        assert!(
            err.message.contains("on my own computer or network"),
            "{text}"
        );
    }
    assert_eq!(list(&env.core).expect("list").len(), 1, "nothing was added");
    assert!(!env.core.app_dir.join("marketplaces").join(MARKET).exists());

    assert!(
        user_url("https://192.168.1.10/me/market", true)
            .expect("parse")
            .private_confirmed()
    );
    // The confirmation never relaxes anything else.
    for (text, kind) in [
        ("https://user:pw@10.0.0.1/o/r", "invalidUrl"),
        ("http://10.0.0.1/o/r", "invalidUrl"),
        ("https://169.254.169.254/o/r", "invalidUrl"),
        ("https://10.0.0.1/o/r%2e", "invalidUrl"),
    ] {
        assert_eq!(err_kind(user_url(text, true)), kind, "{text}");
    }
}

#[test]
fn a_confirmed_marketplace_records_it_and_later_fetches_carry_it() {
    let env = Env::new();
    let (repo, _) = standard_repo(&env);
    let m = add(&env.core, &repo.url().confirm_private()).expect("add");
    assert!(m.private_host);
    let rec = active_market(&env.core, MARKET).expect("record");
    assert!(rec.private_host);
    assert!(
        git_url_of(&env.core, &rec)
            .expect("url")
            .private_confirmed()
    );
    refresh(&env.core, MARKET).expect("refresh with the stored confirmation");

    // A refresh can add the confirmation (a name that now resolves inward, decision 18.8);
    // it is stored once that fetch succeeded.
    let env2 = Env::new();
    let (repo2, _) = standard_repo(&env2);
    add(&env2.core, &repo2.url()).expect("add");
    assert!(!active_market(&env2.core, MARKET).expect("rec").private_host);
    let m = refresh_confirming(&env2.core, MARKET, true).expect("refresh");
    assert!(m.private_host);
    drop(repo2);

    // An unconfirmed marketplace never gets the flag from its cache sidecar.
    let env = Env::new();
    let (repo, _) = standard_repo(&env);
    let m = add(&env.core, &repo.url()).expect("add");
    assert!(!m.private_host);
    let rec = active_market(&env.core, MARKET).expect("record");
    assert!(
        !git_url_of(&env.core, &rec)
            .expect("url")
            .private_confirmed()
    );
}

// ---- Sprint 12: Change address (decision 18.4) --------------------------------------------

/// A full copy of `repo` at `name` (same history, so the same commits exist there).
fn copy_of(env: &Env, repo: &Repo, name: &str) -> Repo {
    let dir = env.root.join(name);
    let out = Command::new("git")
        .args(["clone", "--quiet", "--no-hardlinks"])
        .arg(&repo.dir)
        .arg(&dir)
        .env("GIT_CONFIG_GLOBAL", &repo.config)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("clone");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    Repo {
        dir,
        config: repo.config.clone(),
    }
}

#[test]
fn change_address_keeps_pins_and_progress_and_moves_installs_whose_commit_is_there() {
    let env = Env::new();
    let (repo, commit) = installed(&env);
    with_store(&env.core, |s| {
        s.set_activity_value(FACTS, "best", "7", 4096)
    })
    .expect("progress");
    let moved = copy_of(&env, &repo, "moved");
    let changed = change_address(&env.core, MARKET, &moved.url()).expect("change");
    assert!(changed.not_found.is_empty());
    assert_eq!(changed.marketplace.url, moved.url().display());

    let rec = with_store(&env.core, |s| s.installed_activity(FACTS))
        .expect("store")
        .expect("still installed");
    assert_eq!(rec.commit_sha, commit, "the pin is unchanged");
    assert_eq!(rec.source_url, moved.url().display());
    assert_eq!(
        with_store(&env.core, |s| s.activity_value(FACTS, "best")).expect("value"),
        Some("7".to_owned()),
        "saved progress is untouched"
    );
    assert!(listed_ids(&env).contains(&FACTS.to_owned()));
    assert_eq!(manage_item(&env, FACTS).source_note, None);
    // The next Refresh uses the new address, and so do entries.
    refresh(&env.core, MARKET).expect("refresh from the new address");
    assert!(
        entries(&env.core, MARKET)
            .expect("entries")
            .iter()
            .any(|e| e.activity_id == FACTS)
    );
    // The old repository can disappear.
    std::fs::remove_dir_all(&repo.dir).expect("delete old");
    refresh(&env.core, MARKET).expect("still refreshes");
}

#[test]
fn progress_kept_from_an_uninstalled_activity_is_asked_about_after_a_change_of_address() {
    // Security review MEDIUM-2: the new address only shares the marketplace's name. Kept
    // progress of an activity that isn't installed stays the old address's, so installing it
    // from the new address asks (decision 18.6) instead of silently handing it over.
    let env = Env::new();
    let (repo, _) = installed(&env);
    save_state(&env, FACTS);
    uninstall(&env.core, FACTS, false).expect("uninstall, keep state");
    let moved = copy_of(&env, &repo, "moved");
    change_address(&env.core, MARKET, &moved.url()).expect("change");
    let review = preview_install(&env.core, MARKET, FACTS).expect("preview");
    assert_eq!(
        serde_json::to_value(&review).expect("json")["savedState"],
        "foreign",
        "the review asks"
    );
    install_pending(&env.core, &review.token, false, false, Instant::now()).expect("install");
    assert!(
        with_store(&env.core, |s| s.snapshot(FACTS))
            .expect("get")
            .is_none(),
        "deleted by default"
    );
}

#[test]
fn a_pinned_commit_missing_at_the_new_address_leaves_the_install_as_is_with_a_note() {
    let env = Env::new();
    let (_repo, commit) = installed(&env);
    // Same marketplace name, unrelated history: the pinned commit isn't there.
    let (fresh, _) = {
        let r = env.repo("fresh");
        write_facts(&r, "1.0.1", &[]);
        r.write(
            ".lullshift/marketplace.json",
            &json!({
                "schemaVersion": 1, "name": MARKET,
                "activities": [{"id": FACTS, "source": {"path": "packs/facts"}}]
            })
            .to_string(),
        );
        let c = r.commit();
        (r, c)
    };
    let changed = change_address(&env.core, MARKET, &fresh.url()).expect("change");
    assert_eq!(changed.not_found, [FACTS]);
    let rec = with_store(&env.core, |s| s.installed_activity(FACTS))
        .expect("store")
        .expect("kept");
    assert_eq!(rec.commit_sha, commit);
    assert_ne!(
        rec.source_url,
        fresh.url().display(),
        "its source is still the old one"
    );
    assert_eq!(
        manage_item(&env, FACTS).source_note,
        Some("notFoundAtNewAddress")
    );
    assert!(
        listed_ids(&env).contains(&FACTS.to_owned()),
        "it still runs"
    );
    // An update from the new address clears the note.
    let p = preview_update(&env.core, FACTS).expect("update preview");
    update_pending(&env.core, &p.preview.token, false, Instant::now()).expect("update");
    assert_eq!(manage_item(&env, FACTS).source_note, None);
}

#[test]
fn change_address_refuses_another_marketplace_the_same_address_and_the_builtin_one() {
    let env = Env::new();
    let (repo, commit) = installed(&env);
    let other = env.repo("other");
    other.write(
        ".lullshift/marketplace.json",
        &json!({"schemaVersion": 1, "name": "someone-else", "activities": []}).to_string(),
    );
    other.commit();
    let err = change_address(&env.core, MARKET, &other.url()).expect_err("other name");
    assert_eq!(err.kind, "invalidIndex");
    let rec = active_market(&env.core, MARKET).expect("rec");
    assert_eq!(rec.url, repo.url().display(), "nothing changed");
    assert_eq!(rec.last_fetched_commit.as_deref(), Some(commit.as_str()));
    assert!(
        !std::fs::read_dir(env.core.app_dir.join("marketplaces"))
            .expect("dir")
            .flatten()
            .any(|e| e.file_name().to_string_lossy().starts_with(".changing-")),
        "no staging folder is left behind"
    );

    assert_eq!(
        err_kind(change_address(&env.core, MARKET, &repo.url())),
        "invalidUrl"
    );
    assert_eq!(
        err_kind(change_address(&env.core, BUILTIN_ID, &other.url())),
        "builtinProtected"
    );
    assert_eq!(
        err_kind(change_address(&env.core, "missing", &other.url())),
        "notFound"
    );
}

#[test]
fn adding_a_known_name_from_another_address_points_at_change_address() {
    let env = Env::new();
    let (repo, _) = added(&env);
    let copy = copy_of(&env, &repo, "copy");
    let err = add(&env.core, &copy.url()).expect_err("same name");
    assert_eq!(err.kind, "alreadyExists");
    assert!(err.message.contains("Change address"), "{}", err.message);
}

// ---- Sprint 12: redirects are reported, never followed (decision 18.1) -------------------

#[test]
fn a_redirect_reaches_the_ui_as_moved_with_the_checked_address() {
    let to = GitUrl::parse("https://github.com/new/market").expect("url");
    let same = CommandError::from(Error::Moved {
        to: Box::new(to.clone()),
        same_host: true,
    });
    assert_eq!(same.kind, "moved");
    assert!(same.message.contains("https://github.com/new/market"));
    assert!(same.message.contains("same server"));
    let json = serde_json::to_value(&same).expect("json");
    assert_eq!(
        json["moved"],
        json!({"url": "https://github.com/new/market", "sameHost": true})
    );

    let other = CommandError::from(Error::Moved {
        to: Box::new(to),
        same_host: false,
    });
    assert!(other.message.contains("different server"));
    assert_eq!(
        serde_json::to_value(&other).expect("json")["moved"]["sameHost"],
        json!(false)
    );

    // Every other error has no `moved` field at all.
    let plain = serde_json::to_value(CommandError::from(Error::Git(GitFailureKind::NotFound)))
        .expect("json");
    assert!(plain.get("moved").is_none());
}

/// Decision 28 (ADR-023): the built-in Git's "install Git" answer is its own kind, distinct
/// from `gitMissing`, with fixed text and no link; and the backend in use is reported.
#[test]
fn needs_git_is_a_distinct_error_and_the_backend_is_reported() {
    let e = CommandError::from(Error::Git(GitFailureKind::NeedsSystemGit));
    assert_eq!(e.kind, "needsGit");
    assert!(
        e.message
            .starts_with("Lullshift's built-in Git couldn't read this marketplace.")
    );
    assert!(!e.message.contains("://"), "{}", e.message);
    let missing = CommandError::from(Error::Git(GitFailureKind::GitMissing));
    assert_ne!(missing.kind, e.kind);

    let env = Env::new();
    let dto = git_backend(&env.core).expect("backend");
    // Tests run with git on PATH, and installed Git always comes first.
    assert_eq!(dto.backend, "system");
    assert_eq!(
        serde_json::to_value(&dto).expect("json"),
        json!({ "backend": "system" })
    );
}
