//! Security review M2 and L3: a fetch that grows past the cap is killed and leaves no cache,
//! a failed refresh keeps the good cache, and Git's own object check runs at fetch.

use serde_json::json;

use crate::error::{Error, GitFailureKind};
use crate::testutil::*;

const ID: &str = "com.example.cards";
const DIR: &str = "packs/cards";
const CAP: u64 = 1024 * 1024;

/// Bytes that do not compress, so the repository really grows by this much.
fn noise(len: usize) -> Vec<u8> {
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 24) as u8
        })
        .collect()
}

fn files(extra: Option<F>) -> Vec<F> {
    let mut files = cards_pack(DIR, ID, "1.0.0", &[]);
    files.push(index_file("acme-tools", &[(ID, json!({"path": DIR}))]));
    files.extend(extra);
    files
}

fn leftovers(parent: &std::path::Path) -> Vec<String> {
    std::fs::read_dir(parent)
        .map(|r| {
            r.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn an_oversized_first_fetch_is_killed_and_leaves_no_cache() {
    let env = Env::new();
    let src = env.src("src");
    src.commit(&files(Some(F::new("big.bin", noise(4 * 1024 * 1024)))));
    let git = env.git.clone().with_fetch_cap(CAP);
    let cache = env.path("cache/market/repo.git");

    let err = git.fetch(&cache, &src.url(), None).err();
    assert_eq!(err, Some(Error::Git(GitFailureKind::OutputTooLarge)));
    assert!(!cache.exists(), "no cache after an oversized first fetch");
    assert!(
        leftovers(&env.path("cache/market")).is_empty(),
        "no staging folder left: {:?}",
        leftovers(&env.path("cache/market"))
    );
}

#[test]
fn a_fetch_under_the_cap_still_works() {
    let env = Env::new();
    let src = env.src("src");
    let commit = src.commit(&files(Some(F::new("small.bin", noise(64 * 1024)))));
    let git = env.git.clone().with_fetch_cap(CAP);
    let cache = env.path("cache/market/repo.git");
    assert_eq!(git.fetch(&cache, &src.url(), None).expect("fetch"), commit);
    assert!(cache.join("HEAD").is_file());
    assert_eq!(leftovers(&env.path("cache/market")), ["repo.git"]);
}

#[test]
fn an_oversized_refresh_keeps_the_good_cache_and_removes_partial_files() {
    let env = Env::new();
    let src = env.src("src");
    let first = src.commit(&files(None));
    let cache = env.path("cache/market/repo.git");
    assert_eq!(
        env.git.fetch(&cache, &src.url(), None).expect("first"),
        first
    );

    src.commit(&files(Some(F::new("big.bin", noise(4 * 1024 * 1024)))));
    let git = env.git.clone().with_fetch_cap(CAP);
    let err = git.fetch(&cache, &src.url(), None).err();
    assert_eq!(err, Some(Error::Git(GitFailureKind::OutputTooLarge)));

    // The earlier commit and its index are still there and readable.
    let index = env
        .git
        .read_index(&cache, &first, false)
        .expect("old index");
    assert!(index.entry(ID).is_some());
    let pack_dir = cache.join("objects/pack");
    let temp: Vec<String> = leftovers(&pack_dir)
        .into_iter()
        .filter(|n| n.starts_with("tmp_"))
        .collect();
    assert!(temp.is_empty(), "temporary pack files left: {temp:?}");
    assert!(
        leftovers(&cache.join("objects"))
            .iter()
            .all(|n| !n.starts_with("incoming-"))
    );
    // And a later normal refresh still works.
    let third = src.commit(&files(None));
    assert_eq!(
        env.git.fetch(&cache, &src.url(), None).expect("again"),
        third
    );
}

#[test]
fn a_failed_first_fetch_leaves_no_cache() {
    let env = Env::new();
    let missing = env.src("nothing-here").url();
    let cache = env.path("cache/market/repo.git");
    assert!(env.git.fetch(&cache, &missing, None).is_err());
    assert!(!cache.exists());
    assert!(leftovers(&env.path("cache/market")).is_empty());
}

#[test]
fn gits_object_check_refuses_a_hostile_tree_at_fetch() {
    // Built with `git mktree`, which allows a `..` entry; the strict runner stops it before any
    // of our own validation (that validation is covered with `Env::lenient` in `malicious`).
    let env = Env::new();
    let src = env.src("src");
    let readme = src.blob(b"x");
    let tree = src.mktree(&[format!("100644 blob {readme}\t..")]);
    src.commit_tree(&tree);
    let cache = env.path("cache/market/repo.git");
    let err = env.git.fetch(&cache, &src.url(), None).err();
    assert_eq!(err, Some(Error::Git(GitFailureKind::Failed)));
    assert!(!cache.exists());
}
