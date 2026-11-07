//! Every command the app registers must also be listed in `build.rs` (Tauri's ACL manifest) and
//! granted in `capabilities/default.json`. A command missing from either is refused at run time
//! with no compile error, and the frontend tests mock `invoke`, so only this test catches it.

use std::collections::BTreeSet;

const LIB: &str = include_str!("lib.rs");
const BUILD: &str = include_str!("../build.rs");
const CAPS: &str = include_str!("../capabilities/default.json");

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
        .filter(|c| !CAPS.contains(&format!("\"allow-{}\"", c.replace('_', "-"))))
        .collect();
    assert!(
        not_granted.is_empty(),
        "not granted in capabilities/default.json: {not_granted:?}"
    );
}
