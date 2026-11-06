//! Guards for the release configuration (S11-T01, S11-T06, privacy audit): the updater stays
//! user-initiated and off until the owner supplies a public key, the release workflow is manual
//! only and pinned, and the uninstaller hooks are wired. Text checks only; nothing is built.

use std::fs;
use std::path::{Path, PathBuf};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(p: &Path) -> String {
    fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn conf() -> serde_json::Value {
    serde_json::from_str(&read(&manifest_dir().join("tauri.conf.json"))).unwrap()
}

fn pubkey_configured(conf: &serde_json::Value) -> bool {
    conf["plugins"]["updater"]["pubkey"]
        .as_str()
        .is_some_and(|k| !k.trim().is_empty())
}

#[test]
fn updater_is_unregistered_unless_a_pubkey_is_configured() {
    let conf = conf();
    let lib = read(&manifest_dir().join("src").join("lib.rs"));
    let cargo = read(&manifest_dir().join("Cargo.toml"));
    let registered = lib.contains("tauri_plugin_updater") || cargo.contains("tauri-plugin-updater");
    let artifacts = conf["bundle"]["createUpdaterArtifacts"]
        .as_bool()
        .unwrap_or(false);
    if !pubkey_configured(&conf) {
        assert!(!registered, "updater plugin registered without a pubkey");
        assert!(!artifacts, "updater artifacts enabled without a pubkey");
        assert!(
            conf["plugins"].get("updater").is_none(),
            "plugins.updater present without a pubkey"
        );
    }
}

#[test]
fn updater_never_checks_in_the_background() {
    // Any key that suggests an automatic check is refused; checks are a manual action only.
    let conf = conf();
    let text = conf["plugins"]["updater"].to_string().to_lowercase();
    for bad in [
        "checkonstart",
        "checkonlaunch",
        "autocheck",
        "interval",
        "background",
        "dialog",
    ] {
        assert!(
            !text.contains(bad),
            "updater setting `{bad}` is not allowed"
        );
    }
    // No startup or timer call in the Rust shell.
    let lib = read(&manifest_dir().join("src").join("lib.rs"));
    assert!(!lib.contains("updater().check") && !lib.contains(".check_for_updates"));
}

#[test]
fn uninstaller_hooks_are_wired() {
    let conf = conf();
    let nsis = &conf["bundle"]["windows"]["nsis"];
    let hooks = nsis["installerHooks"].as_str().expect("installerHooks");
    let nsh = read(&manifest_dir().join(hooks));
    assert!(nsh.contains("NSIS_HOOK_PREUNINSTALL"));
    assert!(nsh.contains("--uninstall-hooks"));
    assert!(
        nsh.contains("$UpdateMode"),
        "hooks must survive an in-place update"
    );
    assert!(
        nsh.contains("$EXEDIR != $INSTDIR"),
        "hooks must survive a reinstall (the old uninstaller runs in place, S12-T05)"
    );
    assert_eq!(nsis["installMode"], "currentUser");

    let prerm = conf["bundle"]["linux"]["deb"]["preRemoveScript"]
        .as_str()
        .expect("preRemoveScript");
    let script = read(&manifest_dir().join(prerm));
    assert!(script.starts_with("#!/bin/sh"));
    assert!(script.contains("--uninstall-hooks"));
    assert!(script.contains("remove)"), "only on removal, not upgrade");
    assert!(script.trim_end().ends_with("exit 0"));
    assert!(
        script.contains("runuser"),
        "must act as the user, not as root"
    );
}

#[test]
fn release_workflow_is_manual_pinned_and_least_privilege() {
    let root = manifest_dir().join("..").join("..").join("..");
    let wf = read(&root.join(".github").join("workflows").join("release.yml"));
    // Triggers: exactly one, workflow_dispatch.
    let on = wf
        .split("\non:")
        .nth(1)
        .expect("on:")
        .split("\npermissions:")
        .next()
        .unwrap();
    assert!(on.contains("workflow_dispatch"));
    for bad in [
        "push",
        "pull_request",
        "schedule",
        "workflow_run",
        "release:",
    ] {
        assert!(!on.contains(bad), "trigger `{bad}` is not allowed");
    }
    // Only one workflow file may exist (nothing else may run on push).
    let count = fs::read_dir(root.join(".github").join("workflows"))
        .unwrap()
        .count();
    assert_eq!(count, 1, "no other workflow without a new decision");
    // Every action is pinned to a full commit SHA with a version comment.
    for line in wf.lines().filter(|l| l.trim_start().starts_with("uses:")) {
        let spec = line.split("uses:").nth(1).unwrap().trim();
        let (_, rest) = spec.split_once('@').expect("pinned");
        let sha = rest.split_whitespace().next().unwrap();
        assert!(
            sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()),
            "not a full SHA: {line}"
        );
        assert!(rest.contains("# v"), "missing version comment: {line}");
    }
    // Default token is read-only; write permissions appear only in the release job.
    assert!(wf.contains("\npermissions:\n  contents: read\n"));
    assert_eq!(wf.matches("contents: write").count(), 1);
    // The draft and its attestation come only from main, and the documented verification pins
    // the signer workflow and source ref (otherwise any branch's build would verify).
    let draft_job = wf
        .split("\n  draft-release:")
        .nth(1)
        .expect("draft-release job");
    assert!(draft_job.contains("if: github.ref == 'refs/heads/main'"));
    let releasing = read(&root.join("docs").join("RELEASING.md"));
    assert!(releasing.contains("--source-ref refs/heads/main"));
    assert!(
        releasing
            .contains("--signer-workflow lullshift/lullshift-app/.github/workflows/release.yml")
    );
    assert!(wf.contains("--draft"));
    assert!(!wf.contains("--prerelease") && !wf.contains("--latest"));
}
