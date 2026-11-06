//! Review fix L1: only an exact Lullshift hook command line for our own data directory counts as
//! ours. Temp directories only; the real home directory is never touched.

use std::fs;
use std::path::{Path, PathBuf};

use agent_adapters::claude_code::install::Status;
use agent_adapters::codex::{self, install as codex_install};
use agent_adapters::cursor::install as cursor_install;
use agent_adapters::hookfile::{Shell, command_data_dir, command_is_ours, command_is_ours_in};
use serde_json::{Value, json};

const SHELLS: [Shell; 2] = [Shell::Sh, Shell::PowerShell];

struct Env {
    dir: tempfile::TempDir,
    file: PathBuf,
    backups: PathBuf,
    exe: PathBuf,
    data: PathBuf,
}

fn env(sub: &str) -> Env {
    let dir = tempfile::tempdir().expect("tempdir");
    Env {
        file: dir.path().join(sub).join("hooks.json"),
        backups: dir.path().join("data").join("backups"),
        exe: dir
            .path()
            .join("Lullshift App")
            .join("lullshift-desktop.exe"),
        data: dir.path().join("data"),
        dir,
    }
}

fn json_of(p: &Path) -> Value {
    serde_json::from_slice(&fs::read(p).expect("read")).expect("json")
}

#[test]
fn recognition_needs_one_complete_trailing_data_dir_token() {
    for (cmd, dir) in [
        (
            "'/opt/Lullshift/lullshift-desktop' hook codex --data-dir '/d'",
            Some("/d"),
        ),
        (
            "& 'C:\\L\\lullshift-desktop.exe' hook codex --data-dir 'C:\\d'",
            Some("C:\\d"),
        ),
        (
            "'/opt/Lullshift/lullshift-desktop' hook codex --data-dir '/d'; my-notify",
            None,
        ),
        (
            "'/opt/Lullshift/lullshift-desktop' hook codex --data-dir '/d' extra",
            None,
        ),
        (
            "'/opt/Lullshift/lullshift-desktop' hook codex --data-dir '/d' '/e'",
            None,
        ),
        (
            "'/opt/Lullshift/lullshift-desktop' hook codex --data-dir /d",
            None,
        ),
        (
            "'/opt/Lullshift/lullshift-desktop' hook codex --data-dir '/d",
            None,
        ),
        (
            "'/opt/Lullshift/lullshift-desktop' hook codex --data-dir ",
            None,
        ),
    ] {
        assert_eq!(command_data_dir(cmd, "codex").as_deref(), dir, "{cmd}");
        assert_eq!(command_is_ours(cmd, "codex"), dir.is_some(), "{cmd}");
    }
    let c = "'/x/lullshift' hook codex --data-dir '/mine'";
    assert!(command_is_ours_in(c, "codex", "/mine"));
    assert!(!command_is_ours_in(c, "codex", "/other"));
    assert!(!command_is_ours_in(c, "codex", "/min"));
}

/// A user entry that starts like ours but has text after the data-dir token is theirs: it
/// survives install, repair and uninstall.
#[test]
fn cursor_entry_with_trailing_text_survives_install_and_uninstall() {
    for shell in SHELLS {
        let e = env(".cursor");
        let cmd = cursor_install::HookCommand {
            exe: e.exe.clone(),
            data_dir: e.data.clone(),
        };
        let lead = if shell == Shell::PowerShell { "& " } else { "" };
        let theirs = format!(
            "{lead}'{}' hook cursor --data-dir '{}'; my-notify",
            e.exe.display(),
            e.data.display()
        );
        let seed = json!({"version": 1, "hooks": {"stop": [{"command": theirs}]}});
        fs::create_dir_all(e.file.parent().expect("parent")).expect("mkdir");
        fs::write(&e.file, serde_json::to_vec_pretty(&seed).expect("json")).expect("write");

        let has = |v: &Value| {
            v["hooks"]["stop"]
                .as_array()
                .expect("stop")
                .iter()
                .any(|h| h["command"] == theirs.as_str())
        };
        let rec =
            cursor_install::install_for(&e.file, &e.backups, &cmd, None, shell).expect("install");
        assert!(has(&json_of(&e.file)), "survives install");
        let rec = cursor_install::install_for(&e.file, &e.backups, &cmd, Some(&rec), shell)
            .expect("repair");
        assert!(has(&json_of(&e.file)), "survives repair");
        cursor_install::uninstall(&rec, &e.data).expect("uninstall");
        let after = json_of(&e.file);
        assert!(has(&after), "survives uninstall");
        assert_eq!(after["hooks"]["stop"].as_array().expect("stop").len(), 1);
    }
}

fn codex_cmd(e: &Env, data_dir: PathBuf) -> codex_install::HookCommand {
    codex_install::HookCommand {
        exe: e.exe.clone(),
        data_dir,
    }
}

fn count_naming(v: &Value, dir: &Path) -> usize {
    let needle = format!("'{}'", dir.display());
    v["hooks"]
        .as_object()
        .expect("hooks")
        .values()
        .flat_map(|g| g.as_array().expect("groups"))
        .flat_map(|g| g["hooks"].as_array().expect("hooks"))
        .filter(|h| {
            h["command"]
                .as_str()
                .is_some_and(|c| c.contains(" hook codex ") && c.ends_with(&needle))
        })
        .count()
}

/// A second build (another data directory) has its own entries: our install, status and
/// Disconnect never change them, and its Disconnect never changes ours.
#[test]
fn entries_of_another_data_dir_survive_our_disconnect() {
    for shell in SHELLS {
        let e = env(".codex");
        let other_dir = e.dir.path().join("other-profile");
        let other = codex_cmd(&e, other_dir.clone());
        let mine = codex_cmd(&e, e.data.clone());
        let other_rec =
            codex_install::install_for(&e.file, &e.backups, &other, None, shell).expect("other");
        // The other build's entries are not ours to manage: nothing counts as installed here.
        assert_eq!(
            codex_install::status_for(&e.file, None, &mine, shell),
            Status::NotInstalled
        );
        let rec =
            codex_install::install_for(&e.file, &e.backups, &mine, None, shell).expect("install");
        assert_eq!(
            codex_install::status_for(&e.file, None, &mine, shell),
            Status::Installed
        );
        let n = codex::HOOKS.len();
        assert_eq!(count_naming(&json_of(&e.file), &e.data), n);
        assert_eq!(count_naming(&json_of(&e.file), &other_dir), n);

        codex_install::uninstall(&rec, &e.data).expect("disconnect");
        let v = json_of(&e.file);
        assert_eq!(count_naming(&v, &e.data), 0, "ours are gone");
        assert_eq!(count_naming(&v, &other_dir), n, "the other build's stay");

        codex_install::uninstall(&other_rec, &other_dir).expect("other disconnect");
        // Byte-identical to what it wrote, so it restores the original: no file at all.
        assert!(!e.file.exists());
    }
}
