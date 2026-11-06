//! Where a fetch may go (ADR-015 amendment, decisions 17 and 18): private hosts only after the
//! user's confirmation, and never to an address that can't be a Git server. Nothing here needs
//! a network: refused fetches never start `git`, and allowed ones aim at a closed loopback port.

use serde_json::json;

use crate::error::{Error, UrlError};
use crate::index::parse_index;
use crate::testutil::*;
use crate::validate::GitUrl;

#[test]
fn a_private_host_is_not_fetched_without_confirmation() {
    let env = Env::new();
    for url in [
        "https://localhost/o/r",
        "https://127.0.0.1:1/o/r",
        "https://192.168.1.10/o/r",
        "https://gitserver/o/r",
        "git@nas.home:o/r.git",
    ] {
        let repo = env.path("cache/private/repo.git");
        let parsed = GitUrl::parse(url).expect("private hosts parse");
        let err = env.git.fetch(&repo, &parsed, None).expect_err(url);
        assert_eq!(err, Error::Url(UrlError::PrivateNotConfirmed), "{url}");
        assert!(!repo.exists(), "{url}: nothing was created");
    }
}

#[test]
fn a_confirmed_loopback_address_reaches_git() {
    // Port 1 on loopback is closed: git runs and reports that it can't connect, which proves
    // the confirmation lets the fetch through (and only that: nothing is listening).
    let env = Env::new();
    let url = GitUrl::parse("https://127.0.0.1:1/o/r")
        .expect("parse")
        .confirm_private();
    let err = env
        .git
        .fetch(&env.path("cache/loop/repo.git"), &url, None)
        .expect_err("nothing listens on port 1");
    assert!(
        err.git_kind().is_some(),
        "a git failure, not a refusal: {err}"
    );
}

#[test]
fn git_connects_to_the_pinned_address_instead_of_resolving_the_name_again() {
    // `.invalid` never resolves (RFC 6761). With the pin, curl skips DNS and tries the pinned
    // loopback address (port 1, closed); without it, it fails to resolve. Offline-safe.
    let env = Env::new();
    let repo = env.path("cache/pin/repo.git");
    env.git.ensure_repo(&repo).expect("repo");
    let args = [
        "fetch",
        "--quiet",
        "--depth=1",
        "--end-of-options",
        "https://pin-check.invalid:1/o/r",
        "+HEAD:refs/lullshift/fetched",
    ];
    let pinned = env
        .git
        .run_fetch(
            &repo,
            &args,
            crate::git::SMALL_OUTPUT,
            Some("pin-check.invalid:1:127.0.0.1"),
        )
        .expect("ran");
    assert!(!pinned.success());
    let text = pinned.stderr_for_tests().to_ascii_lowercase();
    assert!(
        !text.contains("could not resolve"),
        "the pinned address was used: {text}"
    );
    let unpinned = env
        .git
        .run_fetch(&repo, &args, crate::git::SMALL_OUTPUT, None)
        .expect("ran");
    assert!(
        unpinned
            .stderr_for_tests()
            .to_ascii_lowercase()
            .contains("could not resolve"),
        "control: without the pin the name doesn't resolve"
    );
}

#[test]
fn a_name_that_resolves_to_loopback_is_refused_until_confirmed() {
    // `localhost` is also private by its text; the resolved check is what stops a public-looking
    // name, which `net::check_addresses` tests without DNS.
    let env = Env::new();
    let url = GitUrl::parse("https://localhost:1/o/r").expect("parse");
    assert_eq!(
        env.git
            .fetch(&env.path("cache/l/repo.git"), &url, None)
            .expect_err("refused"),
        Error::Url(UrlError::PrivateNotConfirmed)
    );
}

#[test]
fn an_index_can_never_list_a_private_source() {
    for url in [
        "https://localhost/o/r",
        "https://10.0.0.5/o/r",
        "https://gitserver/o/r",
        "git@git.corp:o/r",
    ] {
        let index = index_json("acme", &[("com.acme.x", json!({ "git": url }))]);
        let err = parse_index(index.as_bytes(), false).expect_err(url);
        assert!(
            err.to_string()
                .contains("can't be used as an activity's source"),
            "{url}: {err}"
        );
    }
}
