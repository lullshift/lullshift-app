//! Guards for the release configuration (S11-T01, S11-T06, privacy audit): the updater stays
//! user-initiated and off until the owner supplies a public key, the release and npm workflows
//! are manual only and pinned, and the uninstaller hooks are wired. Text checks only; nothing is
//! built.

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
    // M1: only an in-place update keeps hooks. Every other uninstall removes them: the old
    // uninstaller of a reinstall runs before the folder page, so a cancelled or relocated
    // install must not leave hooks pointing at a deleted program.
    let pre = nsh
        .split("!macro NSIS_HOOK_PREUNINSTALL")
        .nth(1)
        .and_then(|t| t.split("!macroend").next())
        .expect("PREUNINSTALL macro");
    assert!(
        pre.contains("$UpdateMode <> 1"),
        "update mode keeps the hooks"
    );
    assert_eq!(
        pre.matches("--uninstall-hooks").count(),
        2,
        "both branches remove the hooks"
    );
    assert!(
        pre.contains("--uninstall-hooks --keep-records"),
        "the in-place (reinstall) branch keeps the install records for the restore"
    );
    assert!(
        pre.contains("$EXEDIR == $INSTDIR"),
        "the in-place test only decides whether the records are kept, never whether hooks are removed"
    );
    assert!(
        !pre.contains("$EXEDIR != $INSTDIR"),
        "the old rule (keep hooks on reinstall) must not come back"
    );
    // A reinstall restores hooks after the install finished, for recorded agents only.
    let post = nsh
        .split("!macro NSIS_HOOK_POSTINSTALL")
        .nth(1)
        .and_then(|t| t.split("!macroend").next())
        .expect("POSTINSTALL macro");
    assert!(post.contains("--repair-recorded-hooks"));
    assert!(post.contains("ExecWait"));
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

/// Manual trigger only, every action pinned to a full SHA, read-only default token.
fn assert_manual_and_pinned(name: &str, wf: &str) {
    // Triggers: exactly one, workflow_dispatch.
    let on = wf
        .split("\non:")
        .nth(1)
        .expect("on:")
        .split("\npermissions:")
        .next()
        .unwrap();
    assert!(on.contains("workflow_dispatch"), "{name}");
    for bad in [
        "push",
        "pull_request",
        "schedule",
        "workflow_run",
        "release:",
    ] {
        assert!(!on.contains(bad), "{name}: trigger `{bad}` is not allowed");
    }
    // Every action is pinned to a full commit SHA with a version comment.
    for line in wf.lines().filter(|l| l.trim_start().starts_with("uses:")) {
        let spec = line.split("uses:").nth(1).unwrap().trim();
        let (_, rest) = spec.split_once('@').expect("pinned");
        let sha = rest.split_whitespace().next().unwrap();
        assert!(
            sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()),
            "{name}: not a full SHA: {line}"
        );
        assert!(
            rest.contains("# v"),
            "{name}: missing version comment: {line}"
        );
    }
    // Default token is read-only.
    assert!(wf.contains("\npermissions:\n  contents: read\n"), "{name}");
}

#[test]
fn only_the_two_manual_workflows_exist() {
    let root = manifest_dir().join("..").join("..").join("..");
    // Nothing may run on push: only these manual workflows, each from its own owner decision
    // (release: ADR-004 amendment; npm: decision 29, ADR-017 amendment).
    let mut names: Vec<String> = fs::read_dir(root.join(".github").join("workflows"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(
        names,
        ["publish-npm.yml", "release.yml"],
        "no other workflow without a new decision"
    );
}

fn npm_workflow() -> String {
    let root = manifest_dir().join("..").join("..").join("..");
    read(
        &root
            .join(".github")
            .join("workflows")
            .join("publish-npm.yml"),
    )
}

/// The text of one job (from `  <name>:` to the next job or the end).
fn job<'a>(wf: &'a str, name: &str) -> &'a str {
    let jobs = wf.split("\njobs:\n").nth(1).expect("jobs:");
    let start = jobs
        .find(&format!("  {name}:\n"))
        .unwrap_or_else(|| panic!("job {name}"));
    let rest = &jobs[start + name.len() + 4..];
    // The next job starts with exactly two spaces of indent and a name.
    let end = rest
        .match_indices("\n  ")
        .map(|(i, _)| i)
        .find(|&i| {
            let after = &rest[i + 3..];
            after
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
        })
        .unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn npm_workflow_is_manual_gated_and_token_scoped() {
    let wf = npm_workflow();
    assert_manual_and_pinned("publish-npm.yml", &wf);
    let build = job(&wf, "build");
    let publish = job(&wf, "publish");
    // Main only (defence in depth; the environment's branch policy is the control).
    assert!(build.contains("if: github.ref == 'refs/heads/main'"));
    assert!(publish.contains("if: github.ref == 'refs/heads/main'"));
    // Nothing writes to the repository.
    assert!(!wf.contains("contents: write"));
    // The token appears once, in the publish step's env.
    assert_eq!(wf.matches("secrets.").count(), 1);
    assert_eq!(wf.matches("NODE_AUTH_TOKEN: ${{").count(), 1);
    let publish_step = publish
        .split("- name: Publish\n")
        .nth(1)
        .expect("Publish step");
    assert!(publish_step.contains("NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}"));
    // Inputs and job outputs reach the shell only through env, never interpolated into a script.
    for line in wf
        .lines()
        .filter(|l| l.contains("${{ inputs.") || l.contains("${{ needs."))
    {
        let key = line.trim_start().split(':').next().unwrap();
        assert!(
            key.chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'),
            "inputs and outputs only via env: {line}"
        );
    }
}

#[test]
fn npm_id_token_and_environment_are_only_in_the_publish_job() {
    // Medium 1: dev dependencies run in the build job, so it must not be able to mint an OIDC
    // token (npm trusted publishing) or reach the `npm` environment's secret.
    let wf = npm_workflow();
    let build = job(&wf, "build");
    let publish = job(&wf, "publish");
    let keys = |key: &str| {
        wf.lines()
            .filter(|l| l.trim_start().starts_with(key))
            .count()
    };
    assert_eq!(keys("id-token:"), 1, "id-token appears once");
    assert_eq!(keys("environment:"), 1, "one environment");
    assert!(publish.contains("environment: npm"));
    assert!(publish.contains("permissions:\n      contents: read\n      id-token: write"));
    assert!(publish.contains("needs: build"));
    assert!(!build.contains("id-token") && !build.contains("environment:"));
    assert!(!build.contains("secrets."));
    assert!(build.contains("permissions:\n      contents: read\n"));
    // Top-level default stays read-only (no id-token at the workflow level).
    let top = wf.split("\njobs:\n").next().unwrap();
    assert!(!top.contains("id-token:"));
}

#[test]
fn npm_publish_job_uploads_the_checked_tarballs_and_runs_nothing_from_the_repo() {
    // Medium 2: the uploaded bytes are the ones build packed, checked and smoke-tested.
    let wf = npm_workflow();
    let build = job(&wf, "build");
    let publish = job(&wf, "publish");
    assert!(build.contains("node scripts/check-npm-packages.mjs --keep"));
    assert!(build.contains("actions/upload-artifact@"));
    assert!(build.contains("sha512sums: ${{ steps.sums.outputs.sha512sums }}"));
    assert!(
        build.contains("lullshift simulate"),
        "smoke test before upload"
    );
    for bad in [
        "actions/checkout",
        "pnpm",
        "corepack",
        "npm install",
        "npm ci",
        "run test",
        "node scripts/",
        "npx ",
        "cache:",
    ] {
        assert!(
            !publish.contains(bad),
            "publish job must not run `{bad}` (no checkout, no install, no repo scripts)"
        );
    }
    assert!(publish.contains("actions/download-artifact@"));
    assert!(publish.contains("SHA512SUMS: ${{ needs.build.outputs.sha512sums }}"));
    assert!(publish.contains("sha512sum --check --strict"));
    // Every npm publish publishes a .tgz path, with provenance, scripts off, tag from env.
    let publish_lines: Vec<&str> = publish
        .lines()
        .filter(|l| l.contains("npm publish"))
        .collect();
    assert_eq!(publish_lines.len(), 1, "{publish_lines:?}");
    assert_eq!(
        publish_lines[0].trim(),
        "npm publish \"${args[@]}\" \"$tgz\""
    );
    assert!(publish.contains("tgz=\"$(find \"$dir\" -maxdepth 1 -name \"lullshift-$short-*.tgz\""));
    assert!(publish.contains("*.tgz) ;;"));
    assert!(
        publish
            .contains("args=(--access public --provenance --ignore-scripts --tag \"$DIST_TAG\")")
    );
    assert!(publish.contains("if [ \"$DRY_RUN\" = true ]; then args+=(--dry-run); fi"));
    // Low 3: a version already on npm is skipped only if npm holds the same bytes.
    assert!(publish.contains("dist.integrity"));
    // npm >= 11.5.1 for trusted publishing; the publish job's Node matches .node-version.
    assert!(publish.contains("printf '%s\\n' 11.5.1 \"$v\" | sort -V -C"));
    let root = manifest_dir().join("..").join("..").join("..");
    let node = read(&root.join(".node-version")).trim().to_string();
    assert!(publish.contains(&format!("node-version: \"{node}\"")));
    // pnpm no longer publishes, so its git-check override is gone too.
    assert!(!wf.contains("pnpm publish") && !wf.contains("--no-git-checks"));
}

#[test]
fn release_workflow_is_manual_pinned_and_least_privilege() {
    let root = manifest_dir().join("..").join("..").join("..");
    let wf = read(&root.join(".github").join("workflows").join("release.yml"));
    assert_manual_and_pinned("release.yml", &wf);
    // Write permissions appear only in the release job.
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

#[test]
fn npm_docs_name_the_real_controls() {
    // Medium 3: the branch `if:` and the guard are defence in depth; the owner must configure the
    // environment's branch policy and bind the trusted publisher to the `npm` environment.
    let root = manifest_dir().join("..").join("..").join("..");
    let releasing = read(&root.join("docs").join("RELEASING.md"));
    for needle in [
        "deployment branch policy that allows `main` only",
        "configured **with environment `npm`**",
        "for Lullshift it is **mandatory**",
        "Selected branches and tags",
        "confirm GitHub refuses the `npm` deployment",
        "branch ruleset",
    ] {
        assert!(releasing.contains(needle), "RELEASING.md: {needle}");
    }
    let adr = read(
        &root
            .join("docs")
            .join("adr")
            .join("ADR-017-creator-cli-and-simulator.md"),
    );
    assert!(adr.contains("configured **with environment `npm`**"));
    assert!(adr.contains("defence in depth only"));
}
