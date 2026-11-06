//! The built-in Git (decision 28, ADR-023): chosen only without `git` on `PATH`, public https
//! only, and the same hardening as the runner. Fetches go to the loopback test server
//! (`testserver`, `git http-backend` behind plain http on 127.0.0.1).
//!
//! The whole fixture suite also runs on the built-in Git: `whole_suite_on_the_builtin_git`
//! re-runs this test binary with `LULLSHIFT_TEST_GIT_BACKEND=builtin`, which makes every
//! `Env` use the built-in Git and every `Src::url()` a loopback https address.

use std::net::SocketAddr;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use serde_json::json;

use crate::error::{Error, GitFailureKind};
use crate::git::{Backend, Git, find_git_in};
use crate::testserver;
use crate::testutil::*;
use crate::validate::{GitRef, GitUrl};

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

#[test]
fn whole_suite_on_the_builtin_git() {
    if builtin_mode() {
        return;
    }
    let exe = std::env::current_exe().expect("test binary");
    let out = Command::new(exe)
        .args([
            "tests::",
            "--skip",
            "whole_suite_on_the_builtin_git",
            "--test-threads",
            "8",
        ])
        .env(BACKEND_VAR, "builtin")
        .output()
        .expect("run the suite again");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "the suite failed on the built-in Git:\n{stdout}\n{stderr}"
    );
    // Make sure the filter really ran the suite, not nothing.
    let passed: usize = stdout
        .lines()
        .filter_map(|l| l.strip_prefix("test result: ok. "))
        .filter_map(|l| l.split_whitespace().next()?.parse::<usize>().ok())
        .sum();
    assert!(passed >= 100, "only {passed} tests ran:\n{stdout}");
}

#[test]
fn without_git_on_path_the_builtin_git_is_used() {
    let env = Env::new();
    let empty = env.path("no-git-here");
    std::fs::create_dir_all(&empty).expect("mkdir");
    // A PATH without git, and one with only a relative entry (never searched).
    assert_eq!(find_git_in(Some(empty.clone().into_os_string())), None);
    assert_eq!(find_git_in(Some(".".into())), None);
    assert_eq!(find_git_in(None), None);
    let chosen = Git::detect(None, &env.path("app-a")).expect("built-in");
    assert_eq!(chosen.backend(), Backend::Builtin);
    // With git on PATH, installed Git always comes first.
    let found = find_git_in(std::env::var_os("PATH"));
    assert!(found.is_some(), "these tests need git on PATH");
    let chosen = Git::detect(found, &env.path("app-b")).expect("system");
    assert_eq!(chosen.backend(), Backend::System);
    assert_eq!(
        Git::new(&env.path("app-c")).expect("new").backend(),
        Backend::System
    );
}

#[test]
fn the_builtin_git_never_starts_a_process() {
    let env = Env::new();
    let git = builtin(&env);
    assert_eq!(git.executable(), Path::new(""));
    let err = git
        .run(None, &["--version"], crate::git::Class::Local, None, 1024)
        .err();
    assert_eq!(err, Some(Error::Git(GitFailureKind::GitMissing)));
}

#[test]
fn a_public_marketplace_installs_end_to_end() {
    let env = Env::new();
    let src = env.src("src");
    let commit = src.commit(&market_files());
    let git = builtin(&env);
    let cache = env.path("cache/m/repo.git");
    assert_eq!(
        git.fetch(&cache, &src.http_url(), None).expect("fetch"),
        commit
    );
    let index = git.read_index(&cache, &commit, false).expect("index");
    assert!(index.entry(ID).is_some());
    // A bare cache with no hooks, no index and no working tree.
    for absent in ["hooks", "index", "description", "info", "logs/HEAD"] {
        assert!(!cache.join(absent).exists(), "{absent} exists");
    }
    assert!(cache.join("shallow").is_file(), "depth 1");
}

#[test]
fn ssh_addresses_need_installed_git_and_touch_nothing() {
    let env = Env::new();
    let git = builtin(&env);
    for url in [
        "git@github.com:o/r.git",
        "ssh://git@git.example.com/o/r",
        "ssh://git.example.com:2222/o/r",
    ] {
        let repo = env.path("cache/ssh/repo.git");
        let parsed = GitUrl::parse(url).expect(url);
        assert_eq!(
            git.fetch(&repo, &parsed, None),
            Err(Error::Git(GitFailureKind::NeedsSystemGit)),
            "{url}"
        );
        assert!(!repo.exists(), "{url}: no cache");
    }
}

#[test]
fn a_repository_that_wants_a_sign_in_needs_installed_git() {
    for code in [401, 403] {
        let env = Env::new();
        let git = builtin(&env);
        let (key, url) = testserver::status_url(code, None);
        let repo = env.path("cache/private/repo.git");
        assert_eq!(
            git.fetch(&repo, &url, None),
            Err(Error::Git(GitFailureKind::NeedsSystemGit)),
            "{code}"
        );
        let seen = testserver::requests(&key);
        assert_eq!(
            seen.len(),
            1,
            "{code}: asked once, never retried with credentials"
        );
        assert!(!seen[0].authorization, "{code}: no credentials sent");
        assert_eq!(seen[0].method, "GET");
        assert!(
            seen[0]
                .target
                .ends_with("/info/refs?service=git-upload-pack"),
            "{}",
            seen[0].target
        );
        assert!(!repo.exists(), "{code}: no cache");
    }
}

#[test]
fn a_redirect_is_reported_as_moved_and_never_followed() {
    let env = Env::new();
    let git = builtin(&env);
    // Same host and port: the one-step move the UI offers.
    let (key, url) = testserver::status_url(
        301,
        Some("/elsewhere/repo.git/info/refs?service=git-upload-pack"),
    );
    match git.fetch(&env.path("cache/moved/repo.git"), &url, None) {
        Err(Error::Moved { to, same_host }) => {
            assert!(same_host);
            assert_eq!(
                to.as_str(),
                format!(
                    "https://{}:{}/elsewhere/repo.git",
                    testserver::HOST,
                    testserver::port()
                )
            );
            assert!(to.private_confirmed(), "same host keeps the confirmation");
        }
        other => panic!("expected moved, got {other:?}"),
    }
    assert_eq!(testserver::requests(&key).len(), 1, "not followed");

    // Another host: reported, not confirmed, not followed.
    let (key, url) = testserver::status_url(
        302,
        Some("https://git.example.net/o/r/info/refs?service=git-upload-pack"),
    );
    match git.fetch(&env.path("cache/moved2/repo.git"), &url, None) {
        Err(Error::Moved { to, same_host }) => {
            assert!(!same_host);
            assert_eq!(to.as_str(), "https://git.example.net/o/r");
            assert!(!to.private_confirmed());
        }
        other => panic!("expected moved, got {other:?}"),
    }
    assert_eq!(testserver::requests(&key).len(), 1);

    // A redirect somewhere that isn't a Git endpoint is just a failure.
    let (_, url) = testserver::status_url(302, Some("https://git.example.net/login"));
    assert_eq!(
        git.fetch(&env.path("cache/moved3/repo.git"), &url, None),
        Err(Error::Git(GitFailureKind::Failed))
    );
}

fn to_closed_port(_: &str, _: u16) -> std::io::Result<Vec<SocketAddr>> {
    Ok(vec!["127.0.0.1:1".parse().expect("addr")])
}

fn unresolvable(_: &str, _: u16) -> std::io::Result<Vec<SocketAddr>> {
    Err(std::io::ErrorKind::NotFound.into())
}

#[test]
fn only_the_checked_address_is_contacted() {
    // The test host is under `.invalid` and never resolves: a fetch that works (as in the
    // test above) used the pinned address. Here the check approved a closed port, and the
    // real server, which the name could otherwise lead to, gets nothing.
    let env = Env::new();
    let src = env.src("src");
    src.commit(&market_files());
    let (key, url) = testserver::repo_url_with_key(&src.dir);
    let git = builtin(&env).with_network_for_tests(to_closed_port, Some(false));
    let err = git
        .fetch(&env.path("cache/pin/repo.git"), &url, None)
        .expect_err("closed");
    assert_eq!(err, Error::Git(GitFailureKind::Offline));
    assert!(
        testserver::requests(&key).is_empty(),
        "nothing reached the server"
    );
    assert!(!env.path("cache/pin/repo.git").exists());
}

#[test]
fn an_unresolved_name_is_offline_or_needs_installed_git_behind_a_proxy() {
    let env = Env::new();
    let url = GitUrl::parse("https://rebind.attacker.example/o/r").expect("parse");
    let offline = builtin(&env).with_network_for_tests(unresolvable, Some(false));
    assert_eq!(
        offline.fetch(&env.path("cache/u1/repo.git"), &url, None),
        Err(Error::Git(GitFailureKind::Offline))
    );
    // The built-in Git never uses a proxy, so it never fetches unpinned.
    let proxied = builtin(&env).with_network_for_tests(unresolvable, Some(true));
    assert_eq!(
        proxied.fetch(&env.path("cache/u2/repo.git"), &url, None),
        Err(Error::Git(GitFailureKind::NeedsSystemGit))
    );
    assert!(!env.path("cache/u1/repo.git").exists());
    assert!(!env.path("cache/u2/repo.git").exists());
}

#[test]
fn a_private_address_still_needs_the_confirmation() {
    let env = Env::new();
    let git = builtin(&env);
    let url = GitUrl::parse("https://127.0.0.1:1/o/r").expect("parse");
    assert_eq!(
        git.fetch(&env.path("cache/p/repo.git"), &url, None),
        Err(Error::Url(crate::error::UrlError::PrivateNotConfirmed))
    );
}

#[test]
fn the_production_builtin_git_speaks_only_https() {
    // Without the test switch the same loopback address is tried with TLS, which a plain
    // http server can't answer: the request fails instead of downgrading.
    let env = Env::new();
    let src = env.src("src");
    src.commit(&market_files());
    let git = Git::builtin(&env.path("prod-app"))
        .with_network_for_tests(testserver::resolve, Some(false));
    let err = git
        .fetch(&env.path("cache/prod/repo.git"), &src.http_url(), None)
        .expect_err("https only");
    assert!(err.git_kind().is_some(), "{err}");
    assert!(!env.path("cache/prod/repo.git").exists());
}

#[test]
fn a_slow_server_is_stopped_at_the_deadline() {
    let env = Env::new();
    let (_, url) = testserver::hang_url(Duration::from_secs(20));
    let git = builtin(&env).with_timeouts(Duration::from_millis(500), Duration::from_secs(5));
    let started = Instant::now();
    assert_eq!(
        git.fetch(&env.path("cache/slow/repo.git"), &url, None),
        Err(Error::Git(GitFailureKind::Timeout))
    );
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "returned promptly"
    );
    assert!(!env.path("cache/slow/repo.git").exists());
}

#[test]
fn a_commit_is_fetched_by_id_and_a_missing_one_is_not_found() {
    let env = Env::new();
    let src = env.src("src");
    let first = src.commit(&market_files());
    let _second = src.commit(
        &market_files()
            .into_iter()
            .chain([F::new("x.txt", "2")])
            .collect::<Vec<_>>(),
    );
    let git = builtin(&env);
    let url = src.http_url();
    let probe = env.path("cache/pin-1/repo.git");
    assert_eq!(
        git.fetch(&probe, &url, Some(&GitRef::Commit(first.clone())))
            .expect("by id"),
        first
    );
    let missing =
        crate::validate::Commit::parse("0123456789abcdef0123456789abcdef01234567").expect("sha");
    let err = git
        .fetch(
            &env.path("cache/pin-2/repo.git"),
            &url,
            Some(&GitRef::Commit(missing)),
        )
        .expect_err("missing");
    assert!(
        matches!(
            err,
            Error::Git(GitFailureKind::NotFound) | Error::NotACommit
        ),
        "{err:?}"
    );
}

/// `git --git-dir <repo> fsck --full` with installed Git, outside the runner.
fn system_fsck(repo: &Path) -> bool {
    Command::new("git")
        .arg("--git-dir")
        .arg(repo)
        .args(["fsck", "--full", "--no-progress"])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git fsck")
        .status
        .success()
}

#[test]
fn a_cache_written_by_the_builtin_git_works_with_installed_git_and_back() {
    let env = Env::new();
    let src = env.src("src");
    let first = src.commit(&market_files());
    let cache = env.path("cache/m/repo.git");
    let gix = builtin(&env);
    assert_eq!(
        gix.fetch(&cache, &src.http_url(), None).expect("gix"),
        first
    );
    assert!(
        system_fsck(&cache),
        "installed Git accepts the built-in Git's cache"
    );

    // After Git is installed, installed Git reads and refreshes the same cache.
    let sys = system(&env);
    let index = sys.read_index(&cache, &first, false).expect("system reads");
    assert!(index.entry(ID).is_some());
    let second = src.commit(
        &market_files()
            .into_iter()
            .chain([F::new("y.txt", "y")])
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        sys.fetch(&cache, &GitUrl::for_test_file(&src.dir), None)
            .expect("system refresh"),
        second
    );
    // And the built-in Git reads what installed Git added, and the older pinned commit.
    for c in [&first, &second] {
        let idx = gix.read_index(&cache, c, false).expect("gix reads");
        assert!(idx.entry(ID).is_some());
    }
    let listed_sys = sys
        .list_tree(
            &cache,
            &second,
            Some(&crate::validate::RelPath::parse(DIR).expect("p")),
        )
        .expect("sys list");
    let listed_gix = gix
        .list_tree(
            &cache,
            &second,
            Some(&crate::validate::RelPath::parse(DIR).expect("p")),
        )
        .expect("gix list");
    assert_eq!(
        listed_sys, listed_gix,
        "both list the same tree identically"
    );
    let wanted: Vec<(String, u64)> = listed_sys.iter().map(|e| (e.oid.clone(), e.size)).collect();
    assert_eq!(
        sys.read_blobs(&cache, &wanted).expect("sys blobs"),
        gix.read_blobs(&cache, &wanted).expect("gix blobs")
    );
}

#[test]
fn a_cache_written_by_installed_git_is_refreshed_by_the_builtin_git() {
    let env = Env::new();
    let src = env.src("src");
    let first = src.commit(&market_files());
    let cache = env.path("cache/m/repo.git");
    let sys = system(&env);
    assert_eq!(
        sys.fetch(&cache, &GitUrl::for_test_file(&src.dir), None)
            .expect("system"),
        first
    );
    // Git is uninstalled: the built-in Git keeps using the same cache.
    let gix = builtin(&env);
    let second = src.commit(
        &market_files()
            .into_iter()
            .chain([F::new("z.txt", "z")])
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        gix.fetch(&cache, &src.http_url(), None)
            .expect("gix refresh"),
        second
    );
    for c in [&first, &second] {
        assert!(
            gix.read_index(&cache, c, false)
                .expect("read")
                .entry(ID)
                .is_some()
        );
    }
    assert!(system_fsck(&cache));
}

#[test]
fn refs_are_checked_by_the_builtin_git_too() {
    let env = Env::new();
    let git = builtin(&env);
    for bad in ["a..b", "a/.b", "a.lock", "a//b", "@{x}"] {
        if let Ok(r) = GitRef::parse(bad) {
            assert!(git.check_ref_format(&r).is_err(), "{bad}");
        }
    }
    for good in ["main", "v1.2.3", "release/2026"] {
        let r = GitRef::parse(good).expect(good);
        assert!(git.check_ref_format(&r).is_ok(), "{good}");
    }
}
