//! Consent-based Cursor hook install/uninstall (ADR-021), on the shared engine in
//! [`crate::hookfile`].
//!
//! - Only the user-level `~/.cursor/hooks.json` is touched, with `"version": 1`. A file with
//!   another `version` is refused.
//! - Only non-blocking events are registered, each with a short timeout. Cursor has no async
//!   option, so the forwarder writes nothing to stdout and always exits 0.
//! - The command line is built only from the app's own executable and data directory, quoted
//!   for PowerShell (Windows) or sh; paths that can't be quoted safely are refused.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::ADAPTER_ID;
use super::map::HOOKS;
use crate::claude_code::install::{InstallError, Status, UninstallOutcome};
use crate::hookfile::{self, Shell, Spec};

pub use crate::hookfile::InstallRecord;

/// The command Cursor runs: the Lullshift executable in hook mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookCommand {
    pub exe: PathBuf,
    pub data_dir: PathBuf,
}

/// Seconds. Above the forwarder's own 2 second limit, well below Cursor's default.
const TIMEOUT_SECS: u64 = 3;

impl HookCommand {
    fn handler_for(&self, shell: Shell) -> Option<Value> {
        let command = hookfile::command_line(&self.exe, ADAPTER_ID, &self.data_dir, shell)?;
        Some(json!({ "command": command, "timeout": TIMEOUT_SECS }))
    }
}

fn with_spec<T>(cmd: &HookCommand, shell: Shell, f: impl FnOnce(&Spec) -> T) -> T {
    let events: Vec<&'static str> = HOOKS.iter().map(|(e, _)| *e).collect();
    let handler = |_: &str| cmd.handler_for(shell);
    f(&Spec {
        adapter: ADAPTER_ID,
        data_dir: cmd.data_dir.to_str(),
        grouped: false,
        top_keys: &[("version", json!(1))],
        events: &events,
        handler: &handler,
    })
}

/// `~/.cursor/hooks.json`.
pub fn default_hooks_path() -> Option<PathBuf> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .filter(|h| !h.is_empty())?;
    Some(PathBuf::from(home).join(".cursor").join("hooks.json"))
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

/// Removes Lullshift's entries that name `data_dir` (this app's data directory); entries of
/// another Lullshift build, and the user's own, stay.
pub fn uninstall(
    record: &InstallRecord,
    data_dir: &Path,
) -> Result<UninstallOutcome, InstallError> {
    let cmd = HookCommand {
        exe: PathBuf::new(),
        data_dir: data_dir.to_owned(),
    };
    with_spec(&cmd, Shell::native(), |s| hookfile::uninstall(s, record))
}

pub fn status(hooks_json: &Path, cmd: &HookCommand) -> Status {
    status_for(hooks_json, cmd, Shell::native())
}

/// [`status`] with an explicit shell, for tests.
pub fn status_for(hooks_json: &Path, cmd: &HookCommand, shell: Shell) -> Status {
    with_spec(cmd, shell, |s| hookfile::status(s, hooks_json))
}
