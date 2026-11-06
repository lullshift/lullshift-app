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
