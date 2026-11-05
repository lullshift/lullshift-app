//! Installs, external sources, refs, updates and digest checks that must work.

use serde_json::json;

use crate::Installed;
use crate::index::{EntrySource, IndexEntry};
use crate::install::{
    InstalledSummary, VerifyError, diff_update, install, install_dir, remove_install,
    verify_install,
};
use crate::pack::{Kind, PrepareRequest};
use crate::repo::MarketplaceRef;
use crate::testutil::*;
use crate::validate::{GitRef, RelPath};

const ID: &str = "com.example.facts";
const DIR: &str = "packs/facts";

fn market_repo(env: &Env, version: &str, perms: &[&str]) -> (Src, crate::Commit) {
    let src = env.src("market");
    let mut files = cards_pack(DIR, ID, version, perms);
    files.push(index_file("acme-tools", &[(ID, json!({"path": DIR}))]));
    let commit = src.commit(&files);
    (src, commit)
}

#[test]
fn installs_a_content_pack_from_the_same_repository() {
    let env = Env::new();
    let (src, commit) = market_repo(&env, "1.0.0", &[]);
    let pack = pipeline(&env, &src, ID, false).expect("prepare");
    let p = &pack.preview;
    assert_eq!(p.commit, commit.as_str());
    assert_eq!(p.marketplace, "acme-tools");
    assert_eq!(p.activity_id, ID);
    assert_eq!(p.kind, Kind::Content);
    assert_eq!(p.engine.as_deref(), Some("lullshift.cards@1"));
    assert!(p.permissions.is_empty() && p.third_party);
    assert_eq!(p.file_count, 3);
    assert_eq!(
        p.size_bytes,
        pack.files.iter().map(|f| f.bytes.len() as u64).sum::<u64>()
    );
    assert!(!p.source_url.contains('@'));

    let root = env.install_root();
    let Installed { dir, digest } = install(&root, &pack).expect("install");
    assert_eq!(dir, install_dir(&root, ID, commit.as_str()).expect("dir"));
    assert_eq!(digest, p.digest);
    assert!(dir.join("manifest.json").is_file() && dir.join("deck.json").is_file());
    verify_install(&dir, &digest).expect("verifies");
    // No temporary folder is left behind.
    let leftovers: Vec<_> = std::fs::read_dir(root.join(ID))
        .expect("ls")
        .flatten()
        .collect();
    assert_eq!(leftovers.len(), 1);
    // Installing the same thing again is a no-op.
    assert_eq!(install(&root, &pack).expect("again").dir, dir);
}

#[test]
fn a_different_copy_at_the_same_place_is_not_overwritten() {
    let env = Env::new();
    let (src, _) = market_repo(&env, "1.0.0", &[]);
    let pack = pipeline(&env, &src, ID, false).expect("prepare");
    let root = env.install_root();
    let installed = install(&root, &pack).expect("install");
    std::fs::write(installed.dir.join("deck.json"), "{}").expect("tamper");
    assert!(
        install(&root, &pack).is_err(),
        "must not silently replace or accept"
    );
    assert_eq!(
        std::fs::read(installed.dir.join("deck.json")).expect("read"),
        b"{}"
    );
}

#[test]
fn installs_a_pack_from_another_repository_at_a_ref() {
    let env = Env::new();
    // The marketplace repo only holds the index; the pack lives elsewhere.
    let market = env.src("market");
    market.commit(&[index_file("acme-tools", &[(ID, json!({"path": "unused"}))])]);

    let ext = env.src("ext");
    let v1 = ext.commit(&cards_pack("facts", ID, "1.0.0", &[]));
    ext.git(&["tag", "v1"], None);
    let v2 = ext.commit(&cards_pack("facts", ID, "2.0.0", &["activity.storage"]));
    assert_ne!(v1, v2);

    let cache = env.path("cache/market/repo.git");
    let mc = env.git.fetch(&cache, &market.url(), None).expect("market");
    let url = market.url();
    let ext_root = env.path("cache/external");
    let m = MarketplaceRef {
        repo: &cache,
        commit: &mc,
        url: &url,
        external_cache_root: &ext_root,
    };
    let prepare = |git_ref: Option<GitRef>| {
        // The index parser refuses file:// (as it must), so the entry is built by hand.
        let entry = IndexEntry {
            id: ID.to_owned(),
            source: EntrySource::Git {
                url: ext.url(),
                git_ref,
                path: Some(RelPath::parse("facts").expect("path")),
            },
            featured: false,
        };
        let resolved = env.git.resolve_entry(&m, &entry)?;
        assert!(resolved.external);
        env.git.prepare_pack(
            &resolved,
            &PrepareRequest {
                marketplace: "acme-tools",
                builtin: false,
                entry_id: ID,
                engines: &[cards_engine()],
            },
        )
    };
    // The default branch: newest commit.
    let head = prepare(None).expect("head");
    assert_eq!(head.preview.commit, v2.as_str());
    assert_eq!(head.preview.version, "2.0.0");
    // A tag pins the older commit.
    let tagged = prepare(Some(GitRef::parse("v1").expect("ref"))).expect("tag");
    assert_eq!(tagged.preview.commit, v1.as_str());
    assert_eq!(tagged.preview.version, "1.0.0");
    // A branch name, and a full commit id.
    let branch = prepare(Some(GitRef::parse("main").expect("ref"))).expect("branch");
    assert_eq!(branch.preview.commit, v2.as_str());
    let by_sha = prepare(Some(GitRef::parse(v1.as_str()).expect("ref"))).expect("sha");
    assert_eq!(by_sha.preview.commit, v1.as_str());
    install(&env.install_root(), &tagged).expect("install the pinned one");
}

#[test]
fn updating_shows_a_permission_diff_and_swaps_atomically() {
    let env = Env::new();
    let (src, c1) = market_repo(&env, "1.0.0", &[]);
    let old_pack = pipeline(&env, &src, ID, false).expect("v1");
    let root = env.install_root();
    let old = install(&root, &old_pack).expect("install v1");
    let installed = InstalledSummary::from(&old_pack.preview);

    // The author publishes a new version that asks for a permission.
    let mut files = cards_pack(DIR, ID, "1.1.0", &["activity.storage"]);
    files.push(index_file("acme-tools", &[(ID, json!({"path": DIR}))]));
    let c2 = src.commit(&files);
    assert_ne!(c1, c2);

    // Refresh (explicit) and prepare. Nothing about the installed copy changes yet.
    let new_pack = pipeline(&env, &src, ID, false).expect("v1.1");
    verify_install(&old.dir, &old.digest).expect("old copy untouched");
    let diff = diff_update(&installed, &new_pack.preview);
    assert_eq!(
        (diff.old_version.as_str(), diff.new_version.as_str()),
        ("1.0.0", "1.1.0")
    );
    assert_eq!(
        (diff.old_commit.as_str(), diff.new_commit.as_str()),
        (c1.as_str(), c2.as_str())
    );
    assert_eq!(diff.added_permissions, ["activity.storage"]);
    assert!(
        diff.removed_permissions.is_empty()
            && diff.kind_change.is_none()
            && diff.engine_change.is_none()
    );
    assert!(diff.needs_attention && !diff.unchanged);

    // On confirmation: new commit folder first, then the old one goes.
    let new = install(&root, &new_pack).expect("install v1.1");
    assert_ne!(new.dir, old.dir);
    assert!(old.dir.exists() && new.dir.exists());
    remove_install(&root, ID, c1.as_str()).expect("remove old");
    assert!(!old.dir.exists());
    verify_install(&new.dir, &new.digest).expect("new copy verifies");
}

#[test]
fn uninstall_removes_the_folder() {
    let env = Env::new();
    let (src, c) = market_repo(&env, "1.0.0", &[]);
    let root = env.install_root();
    let i = install(&root, &pipeline(&env, &src, ID, false).expect("prepare")).expect("install");
    remove_install(&root, ID, c.as_str()).expect("remove");
    assert!(!i.dir.exists() && !root.join(ID).exists());
    remove_install(&root, ID, c.as_str()).expect("removing twice is fine");
}

#[test]
fn the_digest_catches_tampering() {
    let env = Env::new();
    let (src, _) = market_repo(&env, "1.0.0", &[]);
    let pack = pipeline(&env, &src, ID, false).expect("prepare");
    let installed = install(&env.install_root(), &pack).expect("install");
    let (dir, digest) = (installed.dir, installed.digest);
    verify_install(&dir, &digest).expect("clean");

    // changed content
    let deck = dir.join("deck.json");
    let original = std::fs::read(&deck).expect("read");
    std::fs::write(&deck, b"{\"title\":\"x\",\"cards\":[{\"front\":\"evil\"}]}").expect("tamper");
    assert_eq!(verify_install(&dir, &digest), Err(VerifyError::Mismatch));
    std::fs::write(&deck, &original).expect("restore");
    verify_install(&dir, &digest).expect("restored");

    // one byte appended
    let mut longer = original.clone();
    longer.push(b' ');
    std::fs::write(&deck, longer).expect("tamper");
    assert_eq!(verify_install(&dir, &digest), Err(VerifyError::Mismatch));
    std::fs::write(&deck, &original).expect("restore");

    // an added file, and a nested added script
    std::fs::write(dir.join("extra.txt"), b"x").expect("add");
    assert_eq!(verify_install(&dir, &digest), Err(VerifyError::Mismatch));
    std::fs::remove_file(dir.join("extra.txt")).expect("rm");
    std::fs::create_dir_all(dir.join("sub")).expect("mkdir");
    std::fs::write(dir.join("sub/run.js"), b"x").expect("add");
    assert_eq!(verify_install(&dir, &digest), Err(VerifyError::Mismatch));
    std::fs::remove_dir_all(dir.join("sub")).expect("rm");

    // a removed file
    std::fs::remove_file(dir.join("README.md")).expect("rm");
    assert_eq!(verify_install(&dir, &digest), Err(VerifyError::Mismatch));
    std::fs::write(dir.join("README.md"), b"# Facts\n").expect("restore");
    verify_install(&dir, &digest).expect("restored");

    // a renamed file
    std::fs::rename(dir.join("README.md"), dir.join("readme2.md")).expect("rename");
    assert_eq!(verify_install(&dir, &digest), Err(VerifyError::Mismatch));
    std::fs::rename(dir.join("readme2.md"), dir.join("README.md")).expect("rename back");

    // the wrong digest, a missing folder
    assert_eq!(
        verify_install(&dir, &"0".repeat(64)),
        Err(VerifyError::Mismatch)
    );
    std::fs::remove_dir_all(&dir).expect("rm");
    assert_eq!(verify_install(&dir, &digest), Err(VerifyError::Unreadable));
}

#[cfg(unix)]
#[test]
fn a_link_planted_in_an_install_is_unreadable() {
    let env = Env::new();
    let (src, _) = market_repo(&env, "1.0.0", &[]);
    let pack = pipeline(&env, &src, ID, false).expect("prepare");
    let installed = install(&env.install_root(), &pack).expect("install");
    std::os::unix::fs::symlink("/etc/passwd", installed.dir.join("link.txt")).expect("link");
    assert_eq!(
        verify_install(&installed.dir, &installed.digest),
        Err(VerifyError::Unreadable)
    );
}

#[test]
fn the_digest_covers_files_only() {
    let a = Env::new();
    let b = Env::new();
    let (sa, _) = market_repo(&a, "1.0.0", &[]);
    let (sb, _) = market_repo(&b, "1.0.0", &[]);
    let pa = pipeline(&a, &sa, ID, false).expect("a");
    let pb = pipeline(&b, &sb, ID, false).expect("b");
    // Different commits (timestamps), same files.
    assert_eq!(pa.preview.digest, pb.preview.digest);
    assert_eq!(pa.preview.digest.len(), 64);
}

#[test]
fn installs_a_real_bundled_content_pack() {
    use std::path::PathBuf;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../activities/content/calm-cards/wonder-facts");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&root).expect("ls") {
        let entry = entry.expect("entry");
        files.push(F::new(
            &format!("pack/{}", entry.file_name().to_string_lossy()),
            std::fs::read(entry.path()).expect("read"),
        ));
    }
    let env = Env::new();
    let src = env.src("src");
    files.push(index_file(
        "acme-tools",
        &[(
            "app.lullshift.content.wonder-facts",
            json!({"path": "pack"}),
        )],
    ));
    src.commit(&files);
    let pack = pipeline(&env, &src, "app.lullshift.content.wonder-facts", false)
        .expect("real pack validates");
    assert_eq!(pack.preview.name, "Wonder Facts");
    install(&env.install_root(), &pack).expect("install");
}

#[test]
fn an_unwritable_install_root_reports_an_error() {
    let env = Env::new();
    let (src, _) = market_repo(&env, "1.0.0", &[]);
    let pack = pipeline(&env, &src, ID, false).expect("prepare");
    // A file where the install root should be.
    let blocker = env.path("blocked");
    std::fs::write(&blocker, b"x").expect("file");
    let err = install(&blocker, &pack).expect_err("fails");
    assert!(matches!(err, crate::Error::Io { .. }), "{err}");
}

#[test]
fn manifests_are_listed_in_bulk_and_bad_ones_are_none() {
    let env = Env::new();
    let src = env.src("market");
    let mut files = cards_pack("packs/a", "com.example.a", "1.0.0", &[]);
    files.extend(cards_pack("packs/b", "com.example.b", "2.0.0", &[]));
    // A manifest this host rejects, and a folder with no manifest.
    files.push(F::new("packs/bad/manifest.json", "{\"kind\":\"nope\"}"));
    files.push(F::new("packs/empty/README.md", "x"));
    files.push(F::new("packs/linked/manifest.json", "target").mode("120000"));
    let commit = src.commit(&files);
    let cache = env.path("cache/market/repo.git");
    env.git.fetch(&cache, &src.url(), None).expect("fetch");
    let paths: Vec<RelPath> = [
        "packs/a",
        "packs/bad",
        "packs/b",
        "packs/empty",
        "packs/linked",
        "packs/gone",
    ]
    .iter()
    .map(|p| RelPath::parse(p).expect("path"))
    .collect();
    let refs: Vec<&RelPath> = paths.iter().collect();
    let got = env
        .git
        .read_manifests(&cache, &commit, &refs)
        .expect("read");
    let ids: Vec<Option<&str>> = got
        .iter()
        .map(|m| m.as_ref().map(|m| m.id.as_str()))
        .collect();
    assert_eq!(
        ids,
        vec![
            Some("com.example.a"),
            None,
            Some("com.example.b"),
            None,
            None,
            None
        ]
    );
    // More than one chunk of paths still lines up.
    let many: Vec<RelPath> = (0..120)
        .map(|i| RelPath::parse(if i % 2 == 0 { "packs/a" } else { "packs/gone" }).expect("p"))
        .collect();
    let refs: Vec<&RelPath> = many.iter().collect();
    let got = env
        .git
        .read_manifests(&cache, &commit, &refs)
        .expect("read");
    assert_eq!(got.len(), 120);
    assert!(
        got.iter()
            .enumerate()
            .all(|(i, m)| m.is_some() == (i % 2 == 0))
    );
}
