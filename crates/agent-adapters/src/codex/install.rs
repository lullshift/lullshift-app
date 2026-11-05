//! Consent-based Codex hook install/uninstall (ADR-021), on the shared engine in
//! [`crate::hookfile`].
//!
//! - Only the user-level `hooks.json` is touched: `$CODEX_HOME/hooks.json`, else
//!   `~/.codex/hooks.json`. Project files are never touched; `config.toml` is only read, for
//!   the single key `[features] hooks`.
//! - Every hook is `async: true` (it can't block or rewrite anything) except SessionEnd, which
//!   Codex always runs synchronously, so it gets an explicit 3 second timeout.
//! - Codex asks the user to review and trust new or changed hooks in `/hooks` before they run;
//!   Lullshift can't see whether that happened, so the app says so.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::ADAPTER_ID;
use super::map::HOOKS;
use crate::claude_code::install::{InstallError, Status, UninstallOutcome};
use crate::hookfile::{self, Shell, Spec};

pub use crate::hookfile::InstallRecord;

/// The command Codex runs: the Lullshift executable in hook mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookCommand {
    pub exe: PathBuf,
    pub data_dir: PathBuf,
}

/// Timeout for async hooks: the forwarder's own limit.
const TIMEOUT_SECS: u64 = 2;
/// SessionEnd runs synchronously in Codex (which allows up to 3 s); matches Cursor.
const SESSION_END_TIMEOUT_SECS: u64 = 3;

impl HookCommand {
    fn handler_for(&self, event: &str, shell: Shell) -> Option<Value> {
        let command = hookfile::command_line(&self.exe, ADAPTER_ID, &self.data_dir, shell)?;
        Some(if event == "SessionEnd" {
            json!({ "type": "command", "command": command, "timeout": SESSION_END_TIMEOUT_SECS })
        } else {
            json!({ "type": "command", "command": command, "async": true, "timeout": TIMEOUT_SECS })
        })
    }
}

fn with_spec<T>(cmd: &HookCommand, shell: Shell, f: impl FnOnce(&Spec) -> T) -> T {
    let events: Vec<&'static str> = HOOKS.iter().map(|(e, _)| *e).collect();
    let handler = |e: &str| cmd.handler_for(e, shell);
    f(&Spec {
        adapter: ADAPTER_ID,
        grouped: true,
        top_keys: &[],
        events: &events,
        handler: &handler,
    })
}

/// An absolute `$CODEX_HOME`, else `~/.codex`.
pub fn codex_home() -> Option<PathBuf> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" });
    codex_home_from(std::env::var_os("CODEX_HOME"), home)
}

fn codex_home_from(codex_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    // A relative CODEX_HOME would depend on the working directory: ignored.
    if let Some(dir) = codex_home.map(PathBuf::from).filter(|d| d.is_absolute()) {
        return Some(dir);
    }
    let home = home.filter(|h| !h.is_empty())?;
    Some(PathBuf::from(home).join(".codex"))
}

pub fn default_hooks_path() -> Option<PathBuf> {
    codex_home().map(|h| h.join("hooks.json"))
}

pub fn default_config_path() -> Option<PathBuf> {
    codex_home().map(|h| h.join("config.toml"))
}

/// Whether `config.toml` text sets `hooks = false` under `[features]` (or as the dotted key
/// `features.hooks` at the top level). Only that key is read; anything unclear counts as "not
/// disabled", so a parse problem never hides a working setup.
pub fn hooks_disabled_in(config: &str) -> bool {
    let mut section = String::new();
    for raw in config.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            name.trim().clone_into(&mut section);
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        let is_hooks_key = (section == "features" && key == "hooks")
            || (section.is_empty() && key.replace(' ', "") == "features.hooks");
        if is_hooks_key && value == "false" {
            return true;
        }
    }
    false
}

/// Reads `config_toml` (if it exists and is readable) for the `[features] hooks` key.
pub fn hooks_disabled(config_toml: &Path) -> bool {
    // Only a regular file up to 1 MiB is read (a FIFO would block; a huge file isn't a config).
    crate::claude_code::install::read_capped(config_toml)
        .ok()
        .flatten()
        .and_then(|b| String::from_utf8(b).ok())
        .is_some_and(|t| hooks_disabled_in(&t))
}

pub fn install(
    hooks_json: &Path,
    backup_dir: &Path,
    cmd: &HookCommand,
    previous: Option<&InstallRecord>,
) -> Result<InstallRecord, InstallError> {
    install_for(hooks_json, backup_dir, cmd, previous, Shell::native())
}

/// [`install`] with an explicit shell, for tests.
pub fn install_for(
    hooks_json: &Path,
    backup_dir: &Path,
    cmd: &HookCommand,
    previous: Option<&InstallRecord>,
    shell: Shell,
) -> Result<InstallRecord, InstallError> {
    with_spec(cmd, shell, |s| {
        hookfile::install(s, hooks_json, backup_dir, previous)
    })
}

pub fn uninstall(record: &InstallRecord) -> Result<UninstallOutcome, InstallError> {
    let cmd = HookCommand {
        exe: PathBuf::new(),
        data_dir: PathBuf::new(),
    };
    with_spec(&cmd, Shell::native(), |s| hookfile::uninstall(s, record))
}

/// Status of the hooks in `hooks_json`; [`Status::HooksDisabled`] when Lullshift's hooks are
/// (partly) installed but `config_toml` turns Codex hooks off (nothing would run). Nothing
/// installed stays `NotInstalled`; an unreadable hooks file is reported first.
pub fn status(hooks_json: &Path, config_toml: Option<&Path>, cmd: &HookCommand) -> Status {
    status_for(hooks_json, config_toml, cmd, Shell::native())
}

/// [`status`] with an explicit shell, for tests.
pub fn status_for(
    hooks_json: &Path,
    config_toml: Option<&Path>,
    cmd: &HookCommand,
    shell: Shell,
) -> Status {
    let s = with_spec(cmd, shell, |s| hookfile::status(s, hooks_json));
    let ours = matches!(s, Status::Installed | Status::NeedsUpdate | Status::Partial);
    if ours && config_toml.is_some_and(hooks_disabled) {
        return Status::HooksDisabled;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::{codex_home_from, hooks_disabled_in};
    use std::ffi::OsString;
    use std::path::PathBuf;

    #[test]
    fn relative_codex_home_is_ignored() {
        let home = Some(OsString::from(if cfg!(windows) {
            r"C:\Users\u"
        } else {
            "/home/u"
        }));
        let default = codex_home_from(None, home.clone());
        assert_eq!(
            codex_home_from(Some("rel/dir".into()), home.clone()),
            default
        );
        assert_eq!(codex_home_from(Some("".into()), home.clone()), default);
        let abs = std::env::temp_dir().join("codex-x");
        assert_eq!(
            codex_home_from(Some(abs.clone().into_os_string()), home),
            Some(abs)
        );
        assert_eq!(codex_home_from(Some("rel".into()), None), None::<PathBuf>);
    }

    #[test]
    fn reads_only_the_features_hooks_key() {
        assert!(hooks_disabled_in("[features]\nhooks = false\n"));
        assert!(hooks_disabled_in(
            "model = \"x\"\n[features]\n  hooks   =  false # off\n"
        ));
        assert!(hooks_disabled_in("features.hooks = false\n"));
        assert!(!hooks_disabled_in("[features]\nhooks = true\n"));
        assert!(!hooks_disabled_in("[features]\nother = false\n"));
        assert!(!hooks_disabled_in("[other]\nhooks = false\n"));
        assert!(!hooks_disabled_in("hooks = false\n"));
        assert!(!hooks_disabled_in("[features]\n[other]\nhooks = false\n"));
        assert!(!hooks_disabled_in("this is not toml ]["));
        assert!(!hooks_disabled_in(""));
    }
}
