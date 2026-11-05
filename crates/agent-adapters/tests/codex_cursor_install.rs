//! S10-T01: Codex and Cursor hook install, upgrade and uninstall against real files in temp
//! directories. Nothing here touches the real home directory.

use std::fs;
use std::path::{Path, PathBuf};

use agent_adapters::claude_code::install::{InstallError, Status, UninstallOutcome};
use agent_adapters::codex::{self, install as codex_install};
use agent_adapters::cursor::{self, install as cursor_install};
use agent_adapters::hookfile::Shell;
use serde_json::Value;

const SHELLS: [Shell; 2] = [Shell::Sh, Shell::PowerShell];

const CODEX_USER: &str = "{\r\n    \"description\": \"mine\",\r\n    \"hooks\": {\r\n        \"Stop\": [\r\n            { \"hooks\": [ { \"type\": \"command\", \"command\": \"say done\" } ] }\r\n        ],\r\n        \"PreToolUse\": [\r\n            { \"matcher\": \"Bash\", \"hooks\": [ { \"type\": \"command\", \"command\": \"./guard.sh\" } ] }\r\n        ]\r\n    }\r\n}\r\n";
const CURSOR_USER: &str = "{\n  \"version\": 1,\n  \"hooks\": {\n    \"stop\": [ { \"command\": \"./notify.sh\" } ],\n    \"beforeShellExecution\": [ { \"command\": \"./guard.sh\", \"failClosed\": true } ]\n  }\n}\n";

struct Env {
    dir: tempfile::TempDir,
    file: PathBuf,
    backups: PathBuf,
    exe: PathBuf,
    data: PathBuf,
}

fn env(sub: &str, initial: Option<&str>) -> Env {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join(sub).join("hooks.json");
    if let Some(t) = initial {
        fs::create_dir_all(file.parent().expect("parent")).expect("mkdir");
        fs::write(&file, t).expect("write");
    }
    Env {
        backups: dir.path().join("data").join("backups"),
        exe: dir
            .path()
            .join("Lullshift App")
            .join("lullshift-desktop.exe"),
        data: dir.path().join("data"),
        file,
        dir,
    }
}

fn json(p: &Path) -> Value {
    serde_json::from_slice(&fs::read(p).expect("read")).expect("json")
}

fn codex_cmd(e: &Env) -> codex_install::HookCommand {
    codex_install::HookCommand {
        exe: e.exe.clone(),
        data_dir: e.data.clone(),
    }
}
fn cursor_cmd(e: &Env) -> cursor_install::HookCommand {
    cursor_install::HookCommand {
        exe: e.exe.clone(),
        data_dir: e.data.clone(),
    }
}

// ----------------------------------------------------------------------------- Codex

fn codex_ours(v: &Value) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    for (event, groups) in v["hooks"].as_object().expect("hooks") {
        for g in groups.as_array().expect("groups") {
            for h in g["hooks"].as_array().expect("hooks") {
                if h["command"]
                    .as_str()
                    .is_some_and(|c| c.contains(" hook codex "))
                {
                    out.push((event.clone(), h.clone()));
                }
            }
        }
    }
    out
}

#[test]
fn codex_install_into_an_empty_home_and_back() {
    for shell in SHELLS {
        let e = env(".codex", None);
        let cmd = codex_cmd(&e);
        assert_eq!(
            codex_install::status_for(&e.file, None, &cmd, shell),
            Status::NotInstalled
        );
        let rec =
            codex_install::install_for(&e.file, &e.backups, &cmd, None, shell).expect("install");
        assert_eq!(
            codex_install::status_for(&e.file, None, &cmd, shell),
            Status::Installed
        );
        let v = json(&e.file);
        let ours = codex_ours(&v);
        assert_eq!(ours.len(), codex::HOOKS.len());
        let names: Vec<&str> = ours.iter().map(|(n, _)| n.as_str()).collect();
        for expected in [
            "SessionStart",
            "UserPromptSubmit",
            "PostToolUse",
            "PermissionRequest",
            "Stop",
            "SessionEnd",
        ] {
            assert!(names.contains(&expected), "{expected}");
        }
        for (event, h) in &ours {
            assert_eq!(h["type"], "command");
            if event == "SessionEnd" {
                assert!(h.get("async").is_none(), "SessionEnd is synchronous");
                assert!(h["timeout"].as_u64().is_some_and(|t| t == 3));
            } else {
                assert_eq!(h["async"], true, "{event}");
            }
            let c = h["command"].as_str().expect("command");
            assert_eq!(c.starts_with("& '"), shell == Shell::PowerShell);
        }
        assert!(rec.backup_path.is_none());
        assert_eq!(
            codex_install::uninstall(&rec).expect("uninstall"),
            UninstallOutcome::Restored
        );
        assert!(
            !e.file.exists(),
            "a file Lullshift created is removed again"
        );
    }
}

#[test]
fn codex_preserves_user_hooks_backs_up_and_restores_bytes() {
    let e = env(".codex", Some(CODEX_USER));
    let cmd = codex_cmd(&e);
    let rec = codex_install::install(&e.file, &e.backups, &cmd, None).expect("install");
    let v = json(&e.file);
    assert_eq!(v["description"], "mine");
    assert_eq!(
        v["hooks"]["Stop"][0]["hooks"][0]["command"], "say done",
        "user hook stays first"
    );
    assert_eq!(v["hooks"]["PreToolUse"][0]["matcher"], "Bash");
    assert_eq!(v["hooks"]["Stop"].as_array().expect("arr").len(), 2);
    let backup = rec.backup_path.as_ref().expect("backup");
    assert_eq!(fs::read_to_string(backup).expect("backup"), CODEX_USER);
    assert_eq!(
        codex_install::uninstall(&rec).expect("uninstall"),
        UninstallOutcome::Restored
    );
    assert_eq!(fs::read_to_string(&e.file).expect("read"), CODEX_USER);
    assert!(backup.exists(), "backups are never deleted");
}

#[test]
fn codex_install_twice_changes_nothing() {
    let e = env(".codex", Some(CODEX_USER));
    let cmd = codex_cmd(&e);
    let first = codex_install::install(&e.file, &e.backups, &cmd, None).expect("install");
    let bytes = fs::read(&e.file).expect("read");
    let second = codex_install::install(&e.file, &e.backups, &cmd, Some(&first)).expect("again");
    assert_eq!(fs::read(&e.file).expect("read"), bytes);
    assert_eq!(first, second);
    assert_eq!(fs::read_dir(&e.backups).expect("dir").count(), 1);
    // Even with the record lost, installing again keeps one entry per event.
    codex_install::install(&e.file, &e.backups, &cmd, None).expect("no record");
    assert_eq!(codex_ours(&json(&e.file)).len(), codex::HOOKS.len());
}

#[test]
fn codex_uninstall_after_user_edits_removes_only_ours() {
    let e = env(".codex", Some(CODEX_USER));
    let cmd = codex_cmd(&e);
    let rec = codex_install::install(&e.file, &e.backups, &cmd, None).expect("install");
    let mut v = json(&e.file);
    v["hooks"]["Stop"]
        .as_array_mut()
        .expect("arr")
        .push(serde_json::json!({"hooks":[{"type":"command","command":"later.sh"}]}));
    fs::write(&e.file, serde_json::to_vec_pretty(&v).expect("ser")).expect("write");
    assert_eq!(
        codex_install::uninstall(&rec).expect("uninstall"),
        UninstallOutcome::Removed
    );
    let v = json(&e.file);
    assert!(codex_ours(&v).is_empty());
    assert_eq!(
        v["hooks"]["Stop"].as_array().expect("arr").len(),
        2,
        "both user hooks stay"
    );
    assert!(
        v["hooks"].get("SessionStart").is_none(),
        "events Lullshift created are removed"
    );
    assert_eq!(v["hooks"]["PreToolUse"][0]["matcher"], "Bash");
}

#[test]
fn codex_moved_app_needs_repair_and_repair_replaces_entries() {
    let e = env(".codex", Some(CODEX_USER));
    let rec = codex_install::install(&e.file, &e.backups, &codex_cmd(&e), None).expect("install");
    let moved = codex_install::HookCommand {
        exe: e.dir.path().join("Elsewhere").join("lullshift-desktop.exe"),
        data_dir: e.data.clone(),
    };
    assert_eq!(
        codex_install::status(&e.file, None, &moved),
        Status::NeedsUpdate
    );
    let rec2 = codex_install::install(&e.file, &e.backups, &moved, Some(&rec)).expect("repair");
    assert_eq!(
        codex_install::status(&e.file, None, &moved),
        Status::Installed
    );
    assert_eq!(codex_ours(&json(&e.file)).len(), codex::HOOKS.len());
    assert_eq!(
        rec2.backup_path, rec.backup_path,
        "the original backup carries over"
    );
}

#[test]
fn codex_refuses_unparseable_and_wrongly_shaped_files() {
    for bad in [
        "{ not json",
        "[1,2]",
        "\"x\"",
        "{\"hooks\": 5}",
        "{\"hooks\": {\"Stop\": {}}}",
    ] {
        let e = env(".codex", Some(bad));
        let cmd = codex_cmd(&e);
        assert!(matches!(
            codex_install::install(&e.file, &e.backups, &cmd, None),
            Err(InstallError::SettingsInvalid)
        ));
        assert_eq!(fs::read_to_string(&e.file).expect("read"), bad, "untouched");
        assert!(
            !e.backups.exists(),
            "no backup for a file that wasn't touched"
        );
        assert_eq!(
            codex_install::status(&e.file, None, &cmd),
            Status::SettingsInvalid
        );
    }
}

#[test]
fn codex_refuses_paths_that_cannot_be_quoted() {
    let e = env(".codex", None);
    let cmd = codex_install::HookCommand {
        exe: e.dir.path().join("it's").join("lullshift.exe"),
        data_dir: e.data.clone(),
    };
    assert!(matches!(
        codex_install::install(&e.file, &e.backups, &cmd, None),
        Err(InstallError::UnsupportedPath)
    ));
    assert!(!e.file.exists());
}

#[test]
fn codex_status_reports_hooks_turned_off_in_config() {
    let e = env(".codex", None);
    let cmd = codex_cmd(&e);
    let config = e.file.with_file_name("config.toml");
    fs::create_dir_all(config.parent().expect("parent")).expect("mkdir");
    fs::write(&config, "[features]\nhooks = false\n").expect("write");
    let before = fs::read(&config).expect("read");
    assert_eq!(
        codex_install::status(&e.file, Some(&config), &cmd),
        Status::NotInstalled,
        "nothing installed: not masked by the config"
    );
    let rec = codex_install::install(&e.file, &e.backups, &cmd, None).expect("install");
    assert_eq!(
        codex_install::status(&e.file, Some(&config), &cmd),
        Status::HooksDisabled
    );
    assert_eq!(
        fs::read(&config).expect("read"),
        before,
        "config.toml is never edited"
    );
    fs::write(&config, "[features]\nhooks = true\n").expect("write");
    assert_eq!(
        codex_install::status(&e.file, Some(&config), &cmd),
        Status::Installed
    );
    fs::remove_file(&config).expect("rm");
    assert_eq!(
        codex_install::status(&e.file, Some(&config), &cmd),
        Status::Installed
    );
    codex_install::uninstall(&rec).expect("uninstall");
}

// ----------------------------------------------------------------------------- Cursor

fn cursor_ours(v: &Value) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    for (event, list) in v["hooks"].as_object().expect("hooks") {
        for h in list.as_array().expect("list") {
            if h["command"]
                .as_str()
                .is_some_and(|c| c.contains(" hook cursor "))
            {
                out.push((event.clone(), h.clone()));
            }
        }
    }
    out
}

#[test]
fn cursor_install_into_an_empty_home_registers_only_non_blocking_events() {
    for shell in SHELLS {
        let e = env(".cursor", None);
        let cmd = cursor_cmd(&e);
        let rec =
            cursor_install::install_for(&e.file, &e.backups, &cmd, None, shell).expect("install");
        assert_eq!(
            cursor_install::status_for(&e.file, &cmd, shell),
            Status::Installed
        );
        let v = json(&e.file);
        assert_eq!(v["version"], 1);
        let keys: Vec<_> = v.as_object().expect("obj").keys().cloned().collect();
        assert_eq!(keys, ["version", "hooks"]);
        let mut names: Vec<String> = v["hooks"].as_object().expect("h").keys().cloned().collect();
        names.sort();
        assert_eq!(
            names,
            [
                "afterAgentResponse",
                "beforeSubmitPrompt",
                "postToolUse",
                "sessionEnd",
                "sessionStart",
                "stop"
            ]
        );
        for (_, h) in cursor_ours(&v) {
            assert!(h["timeout"].as_u64().is_some_and(|t| (1..=5).contains(&t)));
            assert!(h.get("failClosed").is_none(), "never fail closed");
            assert_eq!(
                h["command"].as_str().expect("c").starts_with("& '"),
                shell == Shell::PowerShell
            );
        }
        assert_eq!(
            cursor_install::uninstall(&rec).expect("uninstall"),
            UninstallOutcome::Restored
        );
        assert!(!e.file.exists());
    }
}

#[test]
fn cursor_preserves_user_hooks_backs_up_and_restores_bytes() {
    let e = env(".cursor", Some(CURSOR_USER));
    let cmd = cursor_cmd(&e);
    let rec = cursor_install::install(&e.file, &e.backups, &cmd, None).expect("install");
    let v = json(&e.file);
    assert_eq!(v["hooks"]["stop"][0]["command"], "./notify.sh");
    assert_eq!(v["hooks"]["stop"].as_array().expect("a").len(), 2);
    assert_eq!(v["hooks"]["beforeShellExecution"][0]["failClosed"], true);
    assert!(
        v["hooks"].get("preToolUse").is_none() && v["hooks"].get("beforeMCPExecution").is_none()
    );
    assert_eq!(
        fs::read_to_string(rec.backup_path.as_ref().expect("backup")).expect("b"),
        CURSOR_USER
    );
    assert_eq!(
        cursor_install::uninstall(&rec).expect("u"),
        UninstallOutcome::Restored
    );
    assert_eq!(fs::read_to_string(&e.file).expect("read"), CURSOR_USER);
}

#[test]
fn cursor_install_twice_and_uninstall_after_edits() {
    let e = env(".cursor", Some(CURSOR_USER));
    let cmd = cursor_cmd(&e);
    let first = cursor_install::install(&e.file, &e.backups, &cmd, None).expect("install");
    let bytes = fs::read(&e.file).expect("read");
    let second = cursor_install::install(&e.file, &e.backups, &cmd, Some(&first)).expect("again");
    assert_eq!((fs::read(&e.file).expect("read"), &first), (bytes, &second));
    assert_eq!(cursor_ours(&json(&e.file)).len(), cursor::HOOKS.len());

    let mut v = json(&e.file);
    v["extra"] = Value::Bool(true);
    fs::write(&e.file, serde_json::to_vec_pretty(&v).expect("ser")).expect("write");
    assert_eq!(
        cursor_install::uninstall(&first).expect("u"),
        UninstallOutcome::Removed
    );
    let v = json(&e.file);
    assert!(cursor_ours(&v).is_empty());
    assert_eq!(v["extra"], true);
    assert_eq!(v["version"], 1);
    assert_eq!(v["hooks"]["stop"].as_array().expect("a").len(), 1);
    assert!(v["hooks"].get("sessionStart").is_none());
}

#[test]
fn cursor_without_a_record_still_removes_only_ours_and_created_version_goes() {
    let e = env(".cursor", None);
    let cmd = cursor_cmd(&e);
    let mut rec = cursor_install::install(&e.file, &e.backups, &cmd, None).expect("install");
    // Someone added an unrelated key, so the exact restore no longer applies.
    let mut v = json(&e.file);
    v["other"] = Value::Bool(true);
    fs::write(&e.file, serde_json::to_vec(&v).expect("ser")).expect("write");
    rec.restore_exact = false;
    assert_eq!(
        cursor_install::uninstall(&rec).expect("u"),
        UninstallOutcome::Removed
    );
    let v = json(&e.file);
    assert_eq!(
        v,
        serde_json::json!({"other": true}),
        "hooks and the version key Lullshift added are gone"
    );
}

#[test]
fn cursor_refuses_unparseable_unknown_version_and_unquotable_paths() {
    for bad in [
        "{ nope",
        "[]",
        "{\"version\": 2, \"hooks\": {}}",
        "{\"version\":1,\"hooks\":{\"stop\":{}}}",
    ] {
        let e = env(".cursor", Some(bad));
        let cmd = cursor_cmd(&e);
        assert!(
            matches!(
                cursor_install::install(&e.file, &e.backups, &cmd, None),
                Err(InstallError::SettingsInvalid)
            ),
            "{bad}"
        );
        assert_eq!(fs::read_to_string(&e.file).expect("r"), bad);
        assert_eq!(
            cursor_install::status(&e.file, &cmd),
            Status::SettingsInvalid
        );
    }
    for odd in ["a$b", "a`b", "a;b", "a\"b", "a&b"] {
        let e = env(".cursor", None);
        let cmd = cursor_install::HookCommand {
            exe: e.dir.path().join(odd).join("lullshift.exe"),
            data_dir: e.data.clone(),
        };
        for shell in SHELLS {
            assert!(
                matches!(
                    cursor_install::install_for(&e.file, &e.backups, &cmd, None, shell),
                    Err(InstallError::UnsupportedPath)
                ),
                "{odd}"
            );
        }
        assert!(!e.file.exists());
    }
}

#[test]
fn the_two_adapters_and_claude_code_do_not_remove_each_other() {
    let e = env(".codex", None);
    let codex_rec =
        codex_install::install(&e.file, &e.backups, &codex_cmd(&e), None).expect("codex");
    // The same file name holding Cursor hooks is a different agent; a Cursor uninstall on a
    // Codex file finds nothing of its own.
    let fake = cursor_install::InstallRecord {
        settings_path: e.file.clone(),
        backup_path: None,
        restore_exact: false,
        written_sha256: String::new(),
        created_events: vec![],
        created_hooks_key: false,
        created_top_keys: vec![],
    };
    assert_eq!(
        cursor_install::uninstall(&fake).expect("u"),
        UninstallOutcome::NothingToRemove
    );
    assert_eq!(codex_ours(&json(&e.file)).len(), codex::HOOKS.len());
    codex_install::uninstall(&codex_rec).expect("u");
}

#[test]
fn duplicate_keys_and_huge_integers_are_refused_untouched() {
    for bad in [
        r#"{"hooks": {"Stop": []}, "hooks": {"Stop": [{"hooks": [{"type":"command","command":"x"}]}]}}"#,
        r#"{"hooks": {"Stop": [{"hooks": [], "hooks": []}]}}"#,
        r#"{"hooks": {}, "big": 123456789012345678901234567890}"#,
        r#"{"hooks": {}, "big": -9223372036854775809}"#,
    ] {
        let e = env(".codex", Some(bad));
        let cmd = codex_cmd(&e);
        assert!(matches!(
            codex_install::install(&e.file, &e.backups, &cmd, None),
            Err(InstallError::SettingsInvalid)
        ));
        assert_eq!(fs::read_to_string(&e.file).expect("read"), bad);
        assert_eq!(
            codex_install::status(&e.file, None, &cmd),
            Status::SettingsInvalid
        );
    }
    // Large-but-fitting integers and ordinary floats are fine.
    let ok = r#"{"a": 18446744073709551615, "b": -9223372036854775808, "c": 1.5, "hooks": {}}"#;
    let e = env(".codex", Some(ok));
    let cmd = codex_cmd(&e);
    codex_install::install(&e.file, &e.backups, &cmd, None).expect("install");
    let text = fs::read_to_string(&e.file).expect("read");
    assert!(text.contains("18446744073709551615") && text.contains("-9223372036854775808"));
}

#[test]
fn oversized_hooks_and_config_files_are_not_read() {
    let big = format!("{{\"pad\": \"{}\"}}", "x".repeat(1024 * 1024 + 10));
    let e = env(".codex", Some(&big));
    let cmd = codex_cmd(&e);
    assert_eq!(
        codex_install::status(&e.file, None, &cmd),
        Status::SettingsInvalid
    );
    assert!(matches!(
        codex_install::install(&e.file, &e.backups, &cmd, None),
        Err(InstallError::SettingsInvalid)
    ));
    let config = e.file.with_file_name("config.toml");
    fs::write(
        &config,
        format!(
            "[features]
hooks = false
#{}",
            "x".repeat(1024 * 1024)
        ),
    )
    .expect("write");
    assert!(!codex_install::hooks_disabled(&config));
}

#[test]
fn non_regular_files_are_refused_not_read() {
    // Same is_file gate that keeps a FIFO from blocking; a directory exercises it everywhere.
    let e = env(".codex", None);
    fs::create_dir_all(&e.file).expect("mkdir as hooks.json");
    let cmd = codex_cmd(&e);
    assert_eq!(
        codex_install::status(&e.file, None, &cmd),
        Status::SettingsInvalid
    );
    let config = e.file.with_file_name("config.toml");
    fs::create_dir_all(&config).expect("mkdir as config.toml");
    assert!(!codex_install::hooks_disabled(&config));
}
