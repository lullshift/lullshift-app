//! Security review follow-ups for the built-in Git (ADR-023): tree bombs, proxies, rejected
//! packs, the allocation limit.

use std::net::SocketAddr;
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::json;

use crate::error::{Error, GitFailureKind, PackError, PathError};
use crate::git::Git;
use crate::testserver;
use crate::testutil::*;
use crate::validate::Commit;

const ID: &str = "com.example.cards";
const DIR: &str = "packs/cards";

fn builtin(env: &Env) -> Git {
    Git::builtin_for_tests(&env.path("builtin-app"))
}

fn system(env: &Env) -> Git {
    Git::for_tests(&env.path("system-app")).expect("git on PATH")
}

fn market_files() -> Vec<F> {
    let mut files = cards_pack(DIR, ID, "1.0.0", &[]);
    files.push(index_file("acme-tools", &[(ID, json!({"path": DIR}))]));
    files
}

/// A tree of `width` entries all pointing at one tree, `depth` levels deep, over an empty leaf:
/// a few KB that passes fsck (trees are deduplicated) but has `width^depth` paths.
fn empty_tree(src: &Src) -> String {
    src.git(&["hash-object", "-t", "tree", "-w", "--stdin"], Some(b""))
}

fn tree_bomb(src: &Src, width: usize, depth: usize) -> Commit {
    let mut tree = empty_tree(src);
    for _ in 0..depth {
        let lines: Vec<String> = (0..width)
            .map(|i| format!("040000 tree {tree}\te{i:04}"))
            .collect();
        tree = src.mktree(&lines);
    }
    src.commit_tree(&tree)
}

fn resolved(cache: &Path, commit: &Commit) -> crate::repo::ResolvedSource {
    crate::repo::ResolvedSource {
        repo: cache.to_path_buf(),
        commit: commit.clone(),
        path: None,
        source_url: "https://example.invalid/o/r".to_owned(),
        external: false,
    }
}

fn prepare(git: &Git, cache: &Path, commit: &Commit) -> Result<crate::pack::PreparedPack, Error> {
    git.prepare_pack(
        &resolved(cache, commit),
        &crate::pack::PrepareRequest {
            marketplace: "acme-tools",
            builtin: false,
            entry_id: ID,
            engines: &[],
        },
    )
}

#[test]
fn a_tree_bomb_is_refused_quickly_by_the_builtin_git_and_cleanly_by_installed_git() {
    let env = Env::new();
    let src = env.src("bomb");
    let commit = tree_bomb(&src, 1000, 5);
    let cache = env.path("cache/bomb/repo.git");
    let gix = builtin(&env);
    // The bomb is tiny and passes the object check (fsck deduplicates trees).
    assert_eq!(
        gix.fetch(&cache, &src.http_url(), None).expect("fetch"),
        commit
    );

    let started = Instant::now();
    let err = prepare(&gix, &cache, &commit).expect_err("a tree bomb");
    assert!(
        matches!(
            err,
            Error::Pack(PackError::TooManyFiles | PackError::BadPath(..))
        ),
        "{err:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "refused in {:?}",
        started.elapsed()
    );

    // Installed Git on the same cache: killed by the local timeout, never hangs.
    let sys = system(&env).with_timeouts(Duration::from_secs(60), Duration::from_secs(1));
    let started = Instant::now();
    let err = prepare(&sys, &cache, &commit).expect_err("a tree bomb");
    assert!(
        matches!(
            err,
            Error::Git(GitFailureKind::Timeout | GitFailureKind::OutputTooLarge) | Error::Pack(_)
        ),
        "{err:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(8));
}

#[test]
fn a_deep_tree_is_refused_by_the_builtin_git_during_traversal() {
    let env = Env::new();
    let src = env.src("deep");
    let mut tree = empty_tree(&src);
    for _ in 0..(crate::validate::PATH_PARTS_MAX + 3) {
        tree = src.mktree(&[format!("040000 tree {tree}\td")]);
    }
    let commit = src.commit_tree(&tree);
    let cache = env.path("cache/deep/repo.git");
    let gix = builtin(&env);
    gix.fetch(&cache, &src.http_url(), None).expect("fetch");
    let err = gix.list_tree(&cache, &commit, None).expect_err("too deep");
    assert!(
        matches!(err, Error::Pack(PackError::BadPath(_, PathError::TooDeep))),
        "{err:?}"
    );
}

#[test]
fn local_reads_have_a_deadline() {
    let env = Env::new();
    let src = env.src("src");
    let commit = src.commit(&market_files());
    let cache = env.path("cache/d/repo.git");
    builtin(&env)
        .fetch(&cache, &src.http_url(), None)
        .expect("fetch");
    let expired = builtin(&env).with_timeouts(Duration::from_secs(60), Duration::ZERO);
    assert_eq!(
        expired.list_tree(&cache, &commit, None),
        Err(Error::Git(GitFailureKind::Timeout))
    );
}

fn must_not_resolve(_: &str, _: u16) -> std::io::Result<Vec<SocketAddr>> {
    panic!("the name was looked up locally");
}

#[test]
fn a_proxy_sends_the_builtin_git_to_installed_git_before_any_lookup_or_request() {
    let env = Env::new();
    let src = env.src("src");
    src.commit(&market_files());
    let (key, url) = testserver::repo_url_with_key(&src.dir);
    // The test host resolves locally; a configured proxy must still win.
    let git = builtin(&env).with_network_for_tests(testserver::resolve, Some(true));
    let cache = env.path("cache/proxy/repo.git");
    assert_eq!(
        git.fetch(&cache, &url, None),
        Err(Error::Git(GitFailureKind::NeedsSystemGit))
    );
    assert!(testserver::requests(&key).is_empty(), "no direct request");
    assert!(!cache.exists());

    // And no name lookup at all.
    let git = builtin(&env).with_network_for_tests(must_not_resolve, Some(true));
    assert_eq!(
        git.fetch(&cache, &url, None),
        Err(Error::Git(GitFailureKind::NeedsSystemGit))
    );
}

fn pack_listing(cache: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(cache.join("objects").join("pack"))
        .expect("pack folder")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_refused_refresh_leaves_the_pack_folder_unchanged() {
    // The rejected pack is deleted after the repository handle is dropped, so the delete works
    // on Windows too (the handle may still map the files).
    let env = Env::new();
    let src = env.src("src");
    let first = src.commit(&market_files());
    let cache = env.path("cache/refuse/repo.git");
    let gix = builtin(&env);
    assert_eq!(
        gix.fetch(&cache, &src.http_url(), None).expect("first"),
        first
    );
    let before = pack_listing(&cache);
    assert!(!before.is_empty());

    // A tree fsck refuses: a `.git` entry (built with mktree, which `update-index` would not).
    let blob = src.blob(b"x");
    let tree = src.mktree(&[format!("100644 blob {blob}\t.git")]);
    src.commit_tree(&tree);
    let err = gix
        .fetch(&cache, &src.http_url(), None)
        .expect_err("refused");
    assert_eq!(err, Error::Git(GitFailureKind::Failed));
    assert_eq!(pack_listing(&cache), before, "no rejected pack left");
    // The older commit is still readable.
    assert!(gix.read_index(&cache, &first, false).is_ok());
}

#[test]
fn an_object_past_the_allocation_limit_fails_cleanly() {
    // 71 MiB of zeros built from a 1 MiB base: after `repack` it is a tiny delta that expands
    // past the 64 MiB limit.
    let env = Env::new();
    let src = env.src("src");
    let base = vec![0u8; 1024 * 1024];
    let mut big = base.clone();
    big.extend(vec![0u8; 70 * 1024 * 1024]);
    let mut files = market_files();
    files.push(F::new("a.bin", &base));
    files.push(F::new("b.bin", &big));
    src.commit(&files);
    src.git(
        &["repack", "-a", "-d", "-f", "--depth=50", "--window=50"],
        None,
    );
    let cache = env.path("cache/alloc/repo.git");
    let gix = builtin(&env);
    let started = Instant::now();
    let fetched = gix.fetch(&cache, &src.http_url(), None);
    // The fetch refuses the oversized object (71 MiB is well under the 200 MiB fetch cap, so
    // the allocation limit did it) and leaves no cache behind.
    assert_eq!(fetched, Err(Error::Git(GitFailureKind::Failed)));
    assert!(!cache.exists());
    assert!(started.elapsed() < Duration::from_secs(30));
}
