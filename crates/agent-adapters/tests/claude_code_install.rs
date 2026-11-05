//! S02-T05 / S02-A03: install, upgrade and uninstall against real files in a temp dir.

use std::fs;
use std::path::{Path, PathBuf};

use agent_adapters::claude_code::HOOKS;
use agent_adapters::claude_code::install::{
    HookCommand, InstallError, Status, UninstallOutcome, install, status, uninstall,
};
use serde_json::Value;

/// A realistic user settings file: other keys, the user's own hooks, 4-space indent, CRLF.
const USER_SETTINGS: &str = "{\r\n    \"model\": \"opus\",\r\n    \"hooks\": {\r\n        \"Stop\": [\r\n            {\r\n                \"hooks\": [\r\n                    { \"type\": \"command\", \"command\": \"say done\" }\r\n                ]\r\n            }\r\n        ],\r\n        \"PreToolUse\": [\r\n            { \"matcher\": \"Bash\", \"hooks\": [ { \"type\": \"command\", \"command\": \"./guard.sh\" } ] }\r\n        ]\r\n    },\r\n    \"enabledPlugins\": { \"x@y\": true }\r\n}\r\n";

struct Env {
    _dir: tempfile::TempDir,
    settings: PathBuf,
    backups: PathBuf,
    cmd: HookCommand,
}

fn env(initial: Option<&str>) -> Env {
    let dir = tempfile::tempdir().expect("tempdir");
    let settings = dir.path().join(".claude").join("settings.json");
    if let Some(text) = initial {
        fs::create_dir_all(settings.parent().expect("parent")).expect("mkdir");
        fs::write(&settings, text).expect("write");
    }
    let cmd = HookCommand {
        exe: dir.path().join("Lullshift").join("lullshift-desktop.exe"),
        data_dir: dir.path().join("data"),
    };
    let backups = dir.path().join("data").join("backups");
    Env {
        settings,
        backups,
        cmd,
        _dir: dir,
    }
}

fn json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("read")).expect("json")
}

fn our_handler_count(v: &Value) -> usize {
    v["hooks"]
        .as_object()
        .into_iter()
        .flat_map(|h| h.values())
        .filter_map(Value::as_array)
        .flatten()
        .filter_map(|g| g["hooks"].as_array())
        .flatten()
        .filter(|h| h["args"][0] == "hook" && h["args"][1] == "claude-code")
        .count()
}

#[test]
fn install_preserves_user_content_and_uninstall_restores_bytes_exactly() {
    let e = env(Some(USER_SETTINGS));
    assert_eq!(status(&e.settings, &e.cmd), Status::NotInstalled);
    let rec = install(&e.settings, &e.backups, &e.cmd, None).expect("install");
    assert_eq!(status(&e.settings, &e.cmd), Status::Installed);

    let v = json(&e.settings);
    assert_eq!(v["model"], "opus");
    assert_eq!(v["enabledPlugins"]["x@y"], true);
    assert_eq!(
        v["hooks"]["Stop"][0]["hooks"][0]["command"], "say done",
        "user hook kept first"
    );
    assert_eq!(v["hooks"]["PreToolUse"][0]["matcher"], "Bash");
    assert_eq!(our_handler_count(&v), HOOKS.len());
    let keys: Vec<_> = v.as_object().expect("obj").keys().cloned().collect();
    assert_eq!(
        keys,
        ["model", "hooks", "enabledPlugins"],
        "key order preserved"
    );
    let handler = &v["hooks"]["SessionStart"][0]["hooks"][0];
    assert_eq!(handler["async"], true);
    assert_eq!(handler["args"][2], "--data-dir");
    assert_eq!(
        v["hooks"]["Notification"][0]["matcher"],
        "permission_prompt|idle_prompt|elicitation_dialog"
    );

    let backup = rec.backup_path.as_ref().expect("backup");
    assert_eq!(fs::read_to_string(backup).expect("backup"), USER_SETTINGS);
    assert!(rec.restore_exact);

    assert_eq!(
        uninstall(&rec).expect("uninstall"),
        UninstallOutcome::Restored
    );
    assert_eq!(
        fs::read_to_string(&e.settings).expect("read"),
        USER_SETTINGS,
        "byte-exact restore"
    );
    assert!(backup.exists(), "backups are never deleted");
}

#[test]
fn install_is_idempotent() {
    let e = env(Some(USER_SETTINGS));
    let first = install(&e.settings, &e.backups, &e.cmd, None).expect("install");
    let bytes = fs::read(&e.settings).expect("read");
    let second = install(&e.settings, &e.backups, &e.cmd, Some(&first)).expect("again");
    assert_eq!(fs::read(&e.settings).expect("read"), bytes);
    assert_eq!(first, second);
    assert_eq!(
        fs::read_dir(&e.backups).expect("dir").count(),
        1,
        "one backup only"
    );
}

#[test]
fn missing_settings_file_is_created_and_removed_again() {
    let e = env(None);
    let rec = install(&e.settings, &e.backups, &e.cmd, None).expect("install");
    assert_eq!(rec.backup_path, None);
    assert_eq!(our_handler_count(&json(&e.settings)), HOOKS.len());
    assert_eq!(
        uninstall(&rec).expect("uninstall"),
        UninstallOutcome::Restored
    );
    assert!(!e.settings.exists());
}

#[test]
fn edits_after_install_survive_uninstall() {
    let e = env(Some(USER_SETTINGS));
    let rec = install(&e.settings, &e.backups, &e.cmd, None).expect("install");
    // The user (or Claude Code's /config) changes the file afterwards.
    let mut v = json(&e.settings);
    v["theme"] = "dark".into();
    v["hooks"]["Stop"]
        .as_array_mut()
        .expect("arr")
        .push(serde_json::json!({"hooks":[{"type":"command","command":"later"}]}));
    fs::write(&e.settings, serde_json::to_vec_pretty(&v).expect("json")).expect("write");

    assert_eq!(
        uninstall(&rec).expect("uninstall"),
        UninstallOutcome::Removed
    );
    let after = json(&e.settings);
    assert_eq!(our_handler_count(&after), 0);
    assert_eq!(after["theme"], "dark");
    let mut expected: Value = serde_json::from_str(USER_SETTINGS).expect("json");
    expected["theme"] = "dark".into();
    expected["hooks"]["Stop"]
        .as_array_mut()
        .expect("arr")
        .push(serde_json::json!({"hooks":[{"type":"command","command":"later"}]}));
    assert_eq!(
        after, expected,
        "hook configuration equals the user's, plus their later edits"
    );
    for (event, _) in HOOKS {
        if !["Stop"].contains(event) {
            assert!(
                after["hooks"].get(*event).is_none(),
                "{event} key Lullshift created is gone"
            );
        }
    }
}

#[test]
fn invalid_settings_are_never_touched() {
    for bad in [
        "{ not json",
        "[1,2]",
        r#"{"hooks": "nope"}"#,
        r#"{"hooks": {"Stop": {}}}"#,
    ] {
        let e = env(Some(bad));
        let r = install(&e.settings, &e.backups, &e.cmd, None);
        assert!(matches!(r, Err(InstallError::SettingsInvalid)), "{bad}");
        assert_eq!(fs::read_to_string(&e.settings).expect("read"), bad);
        assert!(!e.backups.exists(), "no backup for a refused install");
    }
    let e = env(Some("{ not json"));
    assert_eq!(status(&e.settings, &e.cmd), Status::SettingsInvalid);
}

#[test]
fn moved_app_is_detected_and_upgrade_keeps_one_entry_per_event() {
    let e = env(Some(USER_SETTINGS));
    let rec = install(&e.settings, &e.backups, &e.cmd, None).expect("install");
    let moved = HookCommand {
        exe: e.cmd.exe.with_file_name("lullshift-desktop-new.exe"),
        ..e.cmd.clone()
    };
    assert_eq!(status(&e.settings, &moved), Status::NeedsUpdate);
    let rec2 = install(&e.settings, &e.backups, &moved, Some(&rec)).expect("upgrade");
    assert_eq!(status(&e.settings, &moved), Status::Installed);
    assert_eq!(our_handler_count(&json(&e.settings)), HOOKS.len());
    assert_eq!(
        rec2.backup_path, rec.backup_path,
        "original backup carried over"
    );
    assert_eq!(
        uninstall(&rec2).expect("uninstall"),
        UninstallOutcome::Restored
    );
    assert_eq!(
        fs::read_to_string(&e.settings).expect("read"),
        USER_SETTINGS
    );
}

#[test]
fn hand_removed_entries_show_as_partial() {
    let e = env(Some(USER_SETTINGS));
    install(&e.settings, &e.backups, &e.cmd, None).expect("install");
    let mut v = json(&e.settings);
    v["hooks"]
        .as_object_mut()
        .expect("obj")
        .remove("SessionEnd");
    fs::write(&e.settings, serde_json::to_vec(&v).expect("json")).expect("write");
    assert_eq!(status(&e.settings, &e.cmd), Status::Partial);
}

#[test]
fn reinstall_without_record_never_claims_exact_restore() {
    let e = env(Some(USER_SETTINGS));
    install(&e.settings, &e.backups, &e.cmd, None).expect("install");
    // The app's database was lost; install again from scratch.
    let rec = install(&e.settings, &e.backups, &e.cmd, None).expect("install again");
    assert!(!rec.restore_exact, "backup holds Lullshift entries");
    assert_eq!(our_handler_count(&json(&e.settings)), HOOKS.len());
    assert_eq!(
        uninstall(&rec).expect("uninstall"),
        UninstallOutcome::Removed
    );
    assert_eq!(
        json(&e.settings),
        serde_json::from_str::<Value>(USER_SETTINGS).expect("json")
    );
}

#[test]
fn foreign_hooks_with_similar_args_are_not_ours() {
    let other = r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"/usr/bin/other","args":["hook","claude-code"]}]}]}}"#;
    let e = env(Some(other));
    assert_eq!(status(&e.settings, &e.cmd), Status::NotInstalled);
    let rec = install(&e.settings, &e.backups, &e.cmd, None).expect("install");
    assert_eq!(
        uninstall(&rec).expect("uninstall"),
        UninstallOutcome::Restored
    );
    assert_eq!(fs::read_to_string(&e.settings).expect("read"), other);
}

/// Review H1: repairing after the user edited the file must not let uninstall roll the edit back.
#[test]
fn repair_after_user_edit_keeps_the_edit_on_uninstall() {
    let e = env(Some(USER_SETTINGS));
    let first = install(&e.settings, &e.backups, &e.cmd, None).expect("install");
    let mut v = json(&e.settings);
    v["env"] = serde_json::json!({ "KEEP": "me" });
    fs::write(&e.settings, serde_json::to_vec_pretty(&v).expect("ser")).expect("edit");

    let moved = HookCommand {
        exe: e.cmd.exe.with_file_name("lullshift-desktop-new.exe"),
        data_dir: e.cmd.data_dir.clone(),
    };
    let second = install(&e.settings, &e.backups, &moved, Some(&first)).expect("repair");
    assert!(
        !second.restore_exact,
        "the backup no longer matches what the user has"
    );
    assert_eq!(
        uninstall(&second).expect("uninstall"),
        UninstallOutcome::Removed
    );
    let after = json(&e.settings);
    assert_eq!(after["env"]["KEEP"], "me");
    assert_eq!(our_handler_count(&after), 0);
}

/// Repairing with no edits in between keeps the exact restore.
#[test]
fn repair_without_edits_still_restores_exactly() {
    let e = env(Some(USER_SETTINGS));
    let first = install(&e.settings, &e.backups, &e.cmd, None).expect("install");
    let moved = HookCommand {
        exe: e.cmd.exe.with_file_name("lullshift-desktop-new.exe"),
        data_dir: e.cmd.data_dir.clone(),
    };
    let second = install(&e.settings, &e.backups, &moved, Some(&first)).expect("repair");
    assert_eq!(
        uninstall(&second).expect("uninstall"),
        UninstallOutcome::Restored
    );
    assert_eq!(
        fs::read_to_string(&e.settings).expect("read"),
        USER_SETTINGS
    );
}

/// Review M4: a deleted backup falls back to removing Lullshift's entries.
#[test]
fn missing_backup_falls_back_to_removal() {
    let e = env(Some(USER_SETTINGS));
    let record = install(&e.settings, &e.backups, &e.cmd, None).expect("install");
    fs::remove_file(record.backup_path.as_ref().expect("backup")).expect("delete backup");
    assert_eq!(
        uninstall(&record).expect("uninstall"),
        UninstallOutcome::Removed
    );
    let after = json(&e.settings);
    assert_eq!(our_handler_count(&after), 0);
    assert_eq!(after["model"], "opus");
}

/// Review L1: a UTF-8 byte order mark is accepted, kept, and restored exactly.
#[test]
fn byte_order_mark_is_kept() {
    let with_bom = format!("\u{feff}{USER_SETTINGS}");
    let e = env(Some(&with_bom));
    let record = install(&e.settings, &e.backups, &e.cmd, None).expect("install");
    assert!(
        fs::read(&e.settings)
            .expect("read")
            .starts_with(&[0xEF, 0xBB, 0xBF])
    );
    assert_eq!(status(&e.settings, &e.cmd), Status::Installed);
    assert_eq!(
        uninstall(&record).expect("uninstall"),
        UninstallOutcome::Restored
    );
    assert_eq!(fs::read_to_string(&e.settings).expect("read"), with_bom);
}

/// Review L3: a hook event that isn't an array is reported as invalid, not "not installed".
#[test]
fn non_array_hook_event_is_invalid() {
    let text = r#"{"hooks":{"Stop":{}}}"#;
    let e = env(Some(text));
    assert_eq!(status(&e.settings, &e.cmd), Status::SettingsInvalid);
    assert!(matches!(
        install(&e.settings, &e.backups, &e.cmd, None),
        Err(InstallError::SettingsInvalid)
    ));
    assert_eq!(fs::read_to_string(&e.settings).expect("read"), text);
}

/// Review M1: read-only settings files are refused and left alone.
#[test]
fn read_only_settings_are_refused() {
    let e = env(Some(USER_SETTINGS));
    let mut p = fs::metadata(&e.settings).expect("meta").permissions();
    p.set_readonly(true);
    fs::set_permissions(&e.settings, p).expect("chmod");
    assert!(matches!(
        install(&e.settings, &e.backups, &e.cmd, None),
        Err(InstallError::NotWritable)
    ));
    assert_eq!(
        fs::read_to_string(&e.settings).expect("read"),
        USER_SETTINGS
    );
    let mut p = fs::metadata(&e.settings).expect("meta").permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    p.set_readonly(false);
    fs::set_permissions(&e.settings, p).expect("chmod back");
}

/// Review M1: permissions survive install and uninstall; symlinked settings are refused.
#[cfg(unix)]
#[test]
fn unix_permissions_are_kept_and_symlinks_refused() {
    use std::os::unix::fs::PermissionsExt;
    let e = env(Some(USER_SETTINGS));
    fs::set_permissions(&e.settings, fs::Permissions::from_mode(0o600)).expect("chmod");
    let record = install(&e.settings, &e.backups, &e.cmd, None).expect("install");
    let mode = |p: &Path| fs::metadata(p).expect("meta").permissions().mode() & 0o777;
    assert_eq!(mode(&e.settings), 0o600);
    assert_eq!(mode(record.backup_path.as_ref().expect("backup")), 0o600);
    uninstall(&record).expect("uninstall");
    assert_eq!(mode(&e.settings), 0o600);

    let real = e.settings.with_file_name("real.json");
    fs::rename(&e.settings, &real).expect("move");
    std::os::unix::fs::symlink(&real, &e.settings).expect("symlink");
    assert!(matches!(
        install(&e.settings, &e.backups, &e.cmd, None),
        Err(InstallError::NotWritable)
    ));
    assert!(
        fs::symlink_metadata(&e.settings)
            .expect("meta")
            .file_type()
            .is_symlink()
    );
}
