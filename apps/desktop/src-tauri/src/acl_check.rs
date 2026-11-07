//! Every command the app registers must also be listed in `build.rs` (Tauri's ACL manifest) and
//! granted in at least one capability file. A command missing from either is refused at run time
//! with no compile error, and the frontend tests mock `invoke`, so only this test catches it.
//! The tray window's capability (`tray.json`) is pinned to an explicit allowlist, and each
//! capability file is scoped to its own window (ADR-026 section 6).

use std::collections::BTreeSet;

const LIB: &str = include_str!("lib.rs");
const BUILD: &str = include_str!("../build.rs");
const CAPS: &str = include_str!("../capabilities/default.json");
const TRAY_CAPS: &str = include_str!("../capabilities/tray.json");

/// The only permissions the tray window may hold. Adding one here is a deliberate review step.
const TRAY_ALLOWLIST: &[&str] = &[
    "allow-dashboard",
    "allow-activities-list",
    "allow-wait-estimates",
    "allow-ui-prefs",
    "allow-open-main",
    "allow-tray-hide",
    "core:event:allow-listen",
    "core:event:allow-unlisten",
];

fn capability(json: &str) -> (Vec<String>, Vec<String>) {
    let v: serde_json::Value = serde_json::from_str(json).expect("capability JSON");
    let list = |key: &str| -> Vec<String> {
        v[key]
            .as_array()
            .unwrap_or_else(|| panic!("{key} is a list"))
            .iter()
            .map(|p| p.as_str().expect("string").to_owned())
            .collect()
    };
    (list("windows"), list("permissions"))
}

/// Command names inside `tauri::generate_handler![...]` (the last path segment of each entry).
fn handler_commands() -> BTreeSet<String> {
    let start = LIB
        .find("generate_handler![")
        .expect("lib.rs registers commands");
    let body = &LIB[start + "generate_handler![".len()..];
    let body = &body[..body.find(']').expect("handler list closes")];
    body.split(',')
        .map(|e| {
            e.lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .collect::<String>()
        })
        .map(|e| {
            e.trim()
                .rsplit("::")
                .next()
                .unwrap_or("")
                .trim()
                .to_string()
        })
        .filter(|e| !e.is_empty())
        .collect()
}

/// Quoted names inside `AppManifest::new().commands(&[...])`.
fn manifest_commands() -> BTreeSet<String> {
    let start = BUILD.find(".commands(&[").expect("build.rs lists commands");
    let body = &BUILD[start..];
    let body = &body[..body.find("])").expect("command list closes")];
    body.lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .filter_map(|l| {
            l.trim()
                .strip_prefix('"')?
                .split('"')
                .next()
                .map(str::to_string)
        })
        .collect()
}

#[test]
fn every_registered_command_is_in_the_acl_manifest_and_granted() {
    let handlers = handler_commands();
    let manifest = manifest_commands();
    assert!(handlers.len() > 50, "parsed too few commands: {handlers:?}");

    let not_in_manifest: Vec<_> = handlers.difference(&manifest).collect();
    assert!(
        not_in_manifest.is_empty(),
        "registered but missing from build.rs: {not_in_manifest:?}"
    );
    let unregistered: Vec<_> = manifest.difference(&handlers).collect();
    assert!(
        unregistered.is_empty(),
        "listed in build.rs but not registered: {unregistered:?}"
    );

    let not_granted: Vec<_> = manifest
        .iter()
        .filter(|c| {
            let name = format!("\"allow-{}\"", c.replace('_', "-"));
            !CAPS.contains(&name) && !TRAY_CAPS.contains(&name)
        })
        .collect();
    assert!(
        not_granted.is_empty(),
        "not granted in any capability file: {not_granted:?}"
    );
}

#[test]
fn each_capability_is_scoped_to_its_own_window() {
    assert_eq!(
        capability(CAPS).0,
        ["main"],
        "default.json is for main only"
    );
    assert_eq!(
        capability(TRAY_CAPS).0,
        ["tray"],
        "tray.json is for tray only"
    );
}

#[test]
fn the_tray_capability_grants_nothing_outside_its_allowlist() {
    let (_, granted) = capability(TRAY_CAPS);
    let extra: Vec<_> = granted
        .iter()
        .filter(|p| !TRAY_ALLOWLIST.contains(&p.as_str()))
        .collect();
    assert!(
        extra.is_empty(),
        "tray.json grants more than allowed: {extra:?}"
    );
    // Nothing that reaches Companion, marketplaces, hooks, activity storage or launch, import,
    // or any write, even if the allowlist were widened by mistake.
    for p in &granted {
        for banned in [
            "companion",
            "marketplace",
            "install",
            "uninstall",
            "activity-",
            "import",
            "set-",
            "storage",
            "launch",
            "notification",
            "autostart",
            "fs:",
            "shell:",
            "dialog:",
        ] {
            assert!(!p.contains(banned), "{p} contains {banned}");
        }
    }
    // The commands the tray window may call are exactly the ones it needs.
    let (_, default_granted) = capability(CAPS);
    for p in ["allow-open-main", "allow-tray-hide"] {
        assert!(
            !default_granted.iter().any(|d| d == p),
            "{p} is for the tray window only"
        );
    }
}
