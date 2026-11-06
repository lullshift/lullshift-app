//! Nothing a repository contains may cause code to run: hooks, `.gitattributes` filters, LFS,
//! and option injection through refs. Each test has a control that proves the trap would fire
//! if the hardening were missing.

use std::path::Path;
use std::process::Command;

use serde_json::json;

use crate::error::RefError;
use crate::testutil::*;
use crate::validate::GitRef;

const ID: &str = "com.evil.pack";
const DIR: &str = "packs/x";

fn slash(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// A shell script that creates `marker`.
fn trap(marker: &Path) -> String {
    format!("#!/bin/sh\ntouch \"{}\"\n", slash(marker))
}

/// A filter command that creates `marker` and passes data through.
fn filter_cmd(marker: &Path) -> String {
    format!("sh -c \"touch '{}'; cat\"", slash(marker))
}

fn plain_git(env: &Env, repo: &Path, args: &[&str]) -> bool {
    // The built-in Git has no executable; installed Git then plays the user's later `git`.
    let exe = env.git.executable();
    let exe = if exe.as_os_str().is_empty() {
        Path::new("git")
    } else {
        exe
    };
    Command::new(exe)
        .arg("--git-dir")
        .arg(repo)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("run git")
        .status
        .success()
}

fn pipeline_with_cache(
    env: &Env,
    src: &Src,
    cache: &Path,
) -> Result<crate::pack::PreparedPack, crate::Error> {
    use crate::repo::MarketplaceRef;
    let commit = env.git.fetch(cache, &src.url(), None)?;
    let index = env.git.read_index(cache, &commit, false)?;
    let entry = index.entry(ID).expect("entry").clone();
    let url = src.url();
    let ext = env.path("cache/external");
    let market = MarketplaceRef {
        repo: cache,
        commit: &commit,
        url: &url,
        external_cache_root: &ext,
    };
    let resolved = env.git.resolve_entry(&market, &entry)?;
    env.git.prepare_pack(
        &resolved,
        &crate::pack::PrepareRequest {
            marketplace: &index.name,
            builtin: false,
            entry_id: ID,
            engines: &[cards_engine()],
        },
    )
}

#[test]
fn hooks_in_the_cache_repository_never_run() {
    let env = Env::new();
    let src = env.src("src");
    let mut files = cards_pack(DIR, ID, "1.0.0", &[]);
    files.push(index_file("acme-tools", &[(ID, json!({"path": DIR}))]));
    let first = src.commit(&files);

    let cache = env.path("cache/market/repo.git");
    let marker = env.path("hook-ran.marker");
    let hooks = env.path("evil-hooks");
    std::fs::create_dir_all(&hooks).expect("mkdir");
    for name in [
        "reference-transaction",
        "post-checkout",
        "post-merge",
        "pre-auto-gc",
        "post-update",
    ] {
        std::fs::write(hooks.join(name), trap(&marker)).expect("hook");
    }
    env.git
        .fetch(&cache, &src.url(), None)
        .expect("first fetch creates the cache");
    // Hostile local configuration: the cache itself points git at a hooks folder.
    assert!(plain_git(
        &env,
        &cache,
        &["config", "core.hooksPath", &slash(&hooks)]
    ));
    assert!(plain_git(
        &env,
        &cache,
        &["config", "core.fsmonitor", "touch-never"]
    ));

    // Control: an unhardened git does run the hook.
    assert!(plain_git(
        &env,
        &cache,
        &["update-ref", "refs/heads/control", first.as_str()]
    ));
    assert!(
        marker.exists(),
        "control: the trap hook should fire with plain git"
    );
    std::fs::remove_file(&marker).expect("rm marker");

    // Our runner: fetch (which updates refs), index read and a full prepare.
    let second = src.commit(&files);
    assert_ne!(first, second);
    let pack = pipeline(&env, &src, ID, false).expect("install path still works");
    assert_eq!(pack.preview.file_count, 3);
    assert!(!marker.exists(), "a hook ran during fetch or read");
}

#[test]
fn hooks_and_scripts_in_the_repository_never_run() {
    let env = Env::lenient();
    let src = env.src("src");
    let marker = env.path("repo-hook-ran.marker");
    let mut files = cards_pack(DIR, ID, "1.0.0", &[]);
    files.push(index_file("acme-tools", &[(ID, json!({"path": DIR}))]));
    // Everything a repository might hope Git runs, tracked in the tree.
    for p in [
        ".githooks/post-checkout",
        ".githooks/pre-commit",
        "hooks/post-checkout",
        ".husky/pre-commit",
        "post-checkout",
        "install.sh",
    ] {
        files.push(F::new(p, trap(&marker)));
    }
    files.push(F::new(
        ".gitmodules",
        "[submodule \"x\"]\n\tpath = packs/x/sub\n\turl = ../evil\n\tupdate = !touch evil\n",
    ));
    src.commit(&files);
    // And real hooks in the source repository's own .git/hooks.
    std::fs::write(src.dir.join(".git/hooks/post-checkout"), trap(&marker)).expect("hook");
    std::fs::write(src.dir.join(".git/hooks/pre-upload-pack"), trap(&marker)).expect("hook");

    let pack = pipeline(&env, &src, ID, false).expect("the pack itself is fine");
    assert_eq!(
        pack.preview.file_count, 3,
        "files outside the pack folder are ignored"
    );
    assert!(!marker.exists(), "something in the repository ran");
}

#[test]
fn gitattributes_filters_never_run() {
    let env = Env::new();
    let src = env.src("src");
    let marker = env.path("filter-ran.marker");
    let mut files = cards_pack(DIR, ID, "1.0.0", &[]);
    files.push(index_file("acme-tools", &[(ID, json!({"path": DIR}))]));
    files.push(F::new(
        ".gitattributes",
        "* filter=pwn\n*.json filter=pwn\n",
    ));
    src.commit(&files);

    // The attacker can't set a filter command from inside the repository, but a local config
    // might define one with that name. Put one in the cache, and make it apply to everything,
    // so we can see whether it would run.
    let cache = env.path("cache/market/repo.git");
    let commit = env.git.fetch(&cache, &src.url(), None).expect("fetch");
    let cmd = filter_cmd(&marker);
    assert!(plain_git(
        &env,
        &cache,
        &["config", "filter.pwn.smudge", &cmd]
    ));
    assert!(plain_git(
        &env,
        &cache,
        &["config", "filter.pwn.clean", &cmd]
    ));
    std::fs::create_dir_all(cache.join("info")).expect("info");
    std::fs::write(cache.join("info/attributes"), "* filter=pwn\n").expect("attributes");

    // Control: asking git to apply filters does run it.
    let blob = format!("{commit}:packs/x/deck.json");
    assert!(
        plain_git(&env, &cache, &["cat-file", "--filters", &blob]),
        "control command failed"
    );
    assert!(
        marker.exists(),
        "control: the filter should fire when filters are requested"
    );
    std::fs::remove_file(&marker).expect("rm marker");

    // Our read path never applies filters: bytes are exactly what is in the blob.
    let pack = pipeline_with_cache(&env, &src, &cache).expect("raw read");
    let deck = pack
        .files
        .iter()
        .find(|f| f.path == "deck.json")
        .expect("deck");
    assert_eq!(deck.bytes, files[1].bytes);
    assert!(!marker.exists(), "a filter ran while reading");
}

#[test]
fn a_pack_may_not_carry_gitattributes() {
    let env = Env::new();
    let mut files = cards_pack(DIR, ID, "1.0.0", &[]);
    files.push(F::new(
        "packs/x/.gitattributes",
        "* filter=pwn text eol=crlf ident\n",
    ));
    let err = local_pipeline(&env, files, ID, DIR).expect_err("refused");
    assert!(
        matches!(err, crate::Error::Pack(crate::PackError::BadPath(..))),
        "{err}"
    );
}

#[test]
fn lfs_filter_configuration_does_not_run() {
    let env = Env::new();
    let src = env.src("src");
    let marker = env.path("lfs-ran.marker");
    let mut files = cards_pack(DIR, ID, "1.0.0", &[]);
    files.push(index_file("acme-tools", &[(ID, json!({"path": DIR}))]));
    files.push(F::new(
        ".gitattributes",
        "*.json filter=lfs diff=lfs merge=lfs -text\n",
    ));
    src.commit(&files);
    let cache = env.path("cache/market/repo.git");
    env.git.fetch(&cache, &src.url(), None).expect("fetch");
    let cmd = filter_cmd(&marker);
    assert!(plain_git(
        &env,
        &cache,
        &["config", "filter.lfs.smudge", &cmd]
    ));
    assert!(plain_git(
        &env,
        &cache,
        &["config", "filter.lfs.process", &cmd]
    ));
    std::fs::create_dir_all(cache.join("info")).expect("info");
    std::fs::write(cache.join("info/attributes"), "*.json filter=lfs\n").expect("attributes");
    let pack = pipeline_with_cache(&env, &src, &cache).expect("fine");
    assert_eq!(pack.preview.file_count, 3);
    assert!(!marker.exists(), "an lfs filter ran");
}

#[test]
fn a_ref_that_looks_like_an_option_never_reaches_git() {
    let dir = tempfile::tempdir().expect("tmp");
    let marker = dir.path().join("ran.marker");
    let shell = format!("sh -c touch-{}", slash(&marker));
    for bad in [
        format!("--upload-pack={shell}"),
        "--upload-pack=touch".to_owned(),
        "-u".to_owned(),
        "--exec=touch".to_owned(),
        "-oProxyCommand=touch".to_owned(),
        "--output=/tmp/x".to_owned(),
    ] {
        let err = GitRef::parse(&bad).expect_err("refused");
        assert_eq!(err, RefError::LooksLikeOption, "{bad}");
    }
    assert!(!marker.exists());
}

#[test]
fn urls_that_look_like_options_never_reach_git() {
    for bad in [
        "--upload-pack=touch",
        "-oProxyCommand=touch",
        "ssh://-oProxyCommand=touch/x",
        "ext::sh -c touch",
        "--help",
    ] {
        assert!(crate::validate::GitUrl::parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn production_runner_cannot_use_file_urls() {
    // A runner built the way the app builds it (no test constructor) has file:// switched off
    // twice (GIT_ALLOW_PROTOCOL and protocol.allow=never). This proves the hardening blocks a
    // transport for real.
    let env = Env::new();
    let src = env.src("src");
    src.commit(&cards_pack(DIR, ID, "1.0.0", &[]));
    let prod = crate::Git::new(&env.path("prod-app")).expect("git");
    let err = prod
        .fetch(&env.path("cache/prod/repo.git"), &src.url(), None)
        .expect_err("file:// is not an allowed transport");
    assert!(err.git_kind().is_some(), "{err}");
    assert!(
        !env.path("cache/prod/repo.git/refs/lullshift/fetched")
            .exists()
    );
}
