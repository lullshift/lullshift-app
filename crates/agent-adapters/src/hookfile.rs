//! Shared install engine for hook files that hold a command line (Codex `hooks.json`, Cursor
//! `hooks.json`), plus the shell quoting for that command line (ADR-021).
//!
//! The rules match the Claude Code installer (ADR-010): consent happens in the app before this
//! is called; the original bytes are backed up first; writes are atomic and abort on a
//! concurrent change; symlinked or read-only files are refused; an unparseable file is never
//! modified; installing twice changes nothing; uninstall restores the original bytes exactly
//! or removes only Lullshift's own entries.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::claude_code::install::{
    InstallError, Settings, Status, UninstallOutcome, check_writable, read_capped, read_settings,
    serialize, sha256_hex, write_backup, write_if_unchanged,
};

// ---------------------------------------------------------------------------------------------
// Command line quoting
// ---------------------------------------------------------------------------------------------

/// Which shell runs the hook command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    /// POSIX `sh`: macOS and Linux.
    Sh,
    /// PowerShell: how Cursor runs hook commands on Windows.
    PowerShell,
}

impl Shell {
    pub const fn native() -> Self {
        if cfg!(windows) {
            Self::PowerShell
        } else {
            Self::Sh
        }
    }
}

/// A path may go into a command line only if every character is a letter, a digit or one of a
/// short list of harmless punctuation marks. Single quotes, double quotes, backticks, `$`, `&`,
/// `;`, `|`, `<`, `>`, `%`, `!`, `#`, newlines and all other control or quote-like characters
/// are refused rather than escaped: the path is app-controlled, so refusing is cheap and safe.
fn path_is_quotable(path: &str) -> bool {
    !path.is_empty()
        && path.chars().all(|c| {
            c.is_alphanumeric()
                || matches!(
                    c,
                    ' ' | '_'
                        | '-'
                        | '.'
                        | '/'
                        | '\\'
                        | ':'
                        | '('
                        | ')'
                        | '@'
                        | '+'
                        | '='
                        | ','
                        | '~'
                )
        })
}

/// `'<path>'` for both shells: inside single quotes sh and PowerShell take every character
/// literally, and the allow-list above leaves no way to close the quote.
fn quote(path: &Path) -> Option<String> {
    let s = path.to_str()?;
    (path_is_quotable(s) && path.is_absolute()).then(|| format!("'{s}'"))
}

/// The full hook command: `<exe> hook <adapter> --data-dir <dir>`. `None` if either path can't
/// be quoted safely. PowerShell needs the call operator to run a quoted path.
pub fn command_line(exe: &Path, adapter: &str, data_dir: &Path, shell: Shell) -> Option<String> {
    let (exe, dir) = (quote(exe)?, quote(data_dir)?);
    let lead = match shell {
        Shell::Sh => "",
        Shell::PowerShell => "& ",
    };
    Some(format!("{lead}{exe} hook {adapter} --data-dir {dir}"))
}

/// Whether `command` is a command line built by [`command_line`] for `adapter`: a quoted
/// Lullshift executable followed by `hook <adapter> --data-dir`.
pub fn command_is_ours(command: &str, adapter: &str) -> bool {
    let rest = command.strip_prefix("& ").unwrap_or(command);
    let Some(rest) = rest.strip_prefix('\'') else {
        return false;
    };
    let Some((exe, tail)) = rest.split_once('\'') else {
        return false;
    };
    let name = exe.rsplit(['/', '\\']).next().unwrap_or("");
    name.to_ascii_lowercase().starts_with("lullshift")
        && tail.starts_with(&format!(" hook {adapter} --data-dir "))
}

// ---------------------------------------------------------------------------------------------
// Install engine
// ---------------------------------------------------------------------------------------------

/// What the app keeps (in SQLite) to undo an install. Contains no file content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallRecord {
    pub settings_path: PathBuf,
    pub backup_path: Option<PathBuf>,
    pub restore_exact: bool,
    pub written_sha256: String,
    pub created_events: Vec<String>,
    pub created_hooks_key: bool,
    /// Top-level keys (such as Cursor's `version`) that Lullshift added.
    #[serde(default)]
    pub created_top_keys: Vec<String>,
}

/// Describes one agent's hook file.
pub(crate) struct Spec<'a> {
    /// Adapter id in the hook command (`codex`, `cursor`) and the backup file name stem.
    pub adapter: &'static str,
    /// Codex nests handlers in matcher groups; Cursor lists handlers directly.
    pub grouped: bool,
    /// Top-level keys the file must have; added when missing. A different existing value makes
    /// the file "invalid" (Lullshift doesn't know that format).
    pub top_keys: &'a [(&'static str, Value)],
    pub events: &'a [&'static str],
    /// The handler object for an event, or `None` if the command can't be built safely.
    pub handler: &'a dyn Fn(&str) -> Option<Value>,
}

fn is_ours(handler: &Value, adapter: &str) -> bool {
    handler
        .get("command")
        .and_then(Value::as_str)
        .is_some_and(|c| command_is_ours(c, adapter))
}

/// Removes Lullshift handlers (and matcher groups left empty by that). Returns whether any were found.
fn strip_ours(hooks: &mut Map<String, Value>, spec: &Spec) -> bool {
    let mut found = false;
    for entries in hooks.values_mut().filter_map(Value::as_array_mut) {
        if spec.grouped {
            entries.retain_mut(|group| {
                let Some(list) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
                    return true;
                };
                let before = list.len();
                list.retain(|h| !is_ours(h, spec.adapter));
                let removed = list.len() != before;
                found |= removed;
                !(removed && list.is_empty())
            });
        } else {
            let before = entries.len();
            entries.retain(|h| !is_ours(h, spec.adapter));
            found |= entries.len() != before;
        }
    }
    found
}

fn hooks_of(root: &Map<String, Value>) -> Result<Option<&Map<String, Value>>, InstallError> {
    match root.get("hooks") {
        None => Ok(None),
        Some(Value::Object(h)) if h.values().all(Value::is_array) => Ok(Some(h)),
        Some(_) => Err(InstallError::SettingsInvalid),
    }
}

fn check_top_keys(root: &Map<String, Value>, spec: &Spec) -> Result<(), InstallError> {
    for (k, v) in spec.top_keys {
        if root.get(*k).is_some_and(|existing| existing != v) {
            return Err(InstallError::SettingsInvalid);
        }
    }
    Ok(())
}

pub(crate) fn install(
    spec: &Spec,
    file: &Path,
    backup_dir: &Path,
    previous: Option<&InstallRecord>,
) -> Result<InstallRecord, InstallError> {
    let handlers: Vec<(&str, Value)> = spec
        .events
        .iter()
        .map(|e| (spec.handler)(e).map(|h| (*e, h)))
        .collect::<Option<_>>()
        .ok_or(InstallError::UnsupportedPath)?;
    let Settings {
        original,
        mut root,
        bom,
    } = read_settings(file)?;
    hooks_of(&root)?;
    check_top_keys(&root, spec)?;
    check_writable(file)?;
    let previous = previous.filter(|p| p.settings_path == file);
    let mut created_hooks_key =
        previous.map_or(!root.contains_key("hooks"), |p| p.created_hooks_key);
    let mut created_events = previous
        .map(|p| p.created_events.clone())
        .unwrap_or_default();
    let mut created_top_keys = previous
        .map(|p| p.created_top_keys.clone())
        .unwrap_or_default();

    for (k, v) in spec.top_keys {
        if !root.contains_key(*k) {
            root.insert((*k).to_owned(), v.clone());
            if !created_top_keys.iter().any(|c| c == k) {
                created_top_keys.push((*k).to_owned());
            }
        }
    }
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or(InstallError::SettingsInvalid)?;
    let had_ours = strip_ours(hooks, spec);
    if had_ours && previous.is_none() {
        hooks.retain(|event, v| {
            !(spec.events.contains(&event.as_str()) && v.as_array().is_some_and(Vec::is_empty))
        });
        created_hooks_key |= hooks.is_empty();
    }
    for (event, handler) in handlers {
        if !hooks.contains_key(event) && !created_events.iter().any(|e| e == event) {
            created_events.push(event.to_owned());
        }
        let entries = hooks
            .entry(event)
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or(InstallError::SettingsInvalid)?;
        if spec.grouped {
            let mut group = Map::new();
            group.insert("hooks".into(), Value::Array(vec![handler]));
            entries.push(Value::Object(group));
        } else {
            entries.push(handler);
        }
    }
    let bytes = serialize(&root, bom)?;

    let (backup_path, restore_exact) = match previous {
        Some(p) => (
            p.backup_path.clone(),
            p.restore_exact
                && original.as_deref().map(sha256_hex).as_deref()
                    == Some(p.written_sha256.as_str()),
        ),
        None => match &original {
            Some(o) => (Some(write_backup(backup_dir, spec.adapter, o)?), !had_ours),
            None => (None, true),
        },
    };
    if original.as_deref() != Some(bytes.as_slice()) {
        write_if_unchanged(file, original.as_deref(), Some(&bytes))?;
    }
    Ok(InstallRecord {
        settings_path: file.to_owned(),
        backup_path,
        restore_exact,
        written_sha256: sha256_hex(&bytes),
        created_events,
        created_hooks_key,
        created_top_keys,
    })
}

pub(crate) fn uninstall(
    spec: &Spec,
    record: &InstallRecord,
) -> Result<UninstallOutcome, InstallError> {
    let path = &record.settings_path;
    let current = read_capped(path)?;
    let Some(current) = current else {
        return Ok(UninstallOutcome::NothingToRemove);
    };
    if record.restore_exact && sha256_hex(&current) == record.written_sha256 {
        let original = match &record.backup_path {
            Some(b) => fs::read(b).ok().map(Some),
            None => Some(None),
        };
        if let Some(original) = original {
            write_if_unchanged(path, Some(&current), original.as_deref())?;
            return Ok(UninstallOutcome::Restored);
        }
    }
    let Settings { mut root, bom, .. } = read_settings(path)?;
    let Some(Value::Object(hooks)) = root.get_mut("hooks") else {
        return Ok(UninstallOutcome::NothingToRemove);
    };
    if !strip_ours(hooks, spec) {
        return Ok(UninstallOutcome::NothingToRemove);
    }
    for event in &record.created_events {
        if hooks
            .get(event)
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
        {
            hooks.remove(event);
        }
    }
    if record.created_hooks_key && hooks.is_empty() {
        root.remove("hooks");
        for k in &record.created_top_keys {
            root.remove(k);
        }
    }
    let bytes = serialize(&root, bom)?;
    write_if_unchanged(path, Some(&current), Some(&bytes))?;
    Ok(UninstallOutcome::Removed)
}

pub(crate) fn status(spec: &Spec, file: &Path) -> Status {
    let Ok(Settings { root, .. }) = read_settings(file) else {
        return Status::SettingsInvalid;
    };
    if check_top_keys(&root, spec).is_err() {
        return Status::SettingsInvalid;
    }
    let hooks = match hooks_of(&root) {
        Ok(Some(h)) => h,
        Ok(None) => return Status::NotInstalled,
        Err(_) => return Status::SettingsInvalid,
    };
    let mut present = 0;
    let mut stale = false;
    for event in spec.events {
        let expected = (spec.handler)(event);
        let entries = hooks
            .get(*event)
            .and_then(Value::as_array)
            .into_iter()
            .flatten();
        let ours: Vec<&Value> = if spec.grouped {
            entries
                .filter_map(|g| g.get("hooks").and_then(Value::as_array))
                .flatten()
                .filter(|h| is_ours(h, spec.adapter))
                .collect()
        } else {
            entries.filter(|h| is_ours(h, spec.adapter)).collect()
        };
        if !ours.is_empty() {
            present += 1;
            stale |= ours.len() != 1 || ours.iter().any(|h| Some(*h) != expected.as_ref());
        }
    }
    match present {
        0 => Status::NotInstalled,
        _ if stale => Status::NeedsUpdate,
        n if n == spec.events.len() => Status::Installed,
        _ => Status::Partial,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[cfg(windows)]
    const ABS: &str = "C:\\Program Files\\Lullshift\\lullshift-desktop.exe";
    #[cfg(not(windows))]
    const ABS: &str = "/opt/Lullshift/lullshift-desktop";
    #[cfg(windows)]
    const DIR: &str = "C:\\Users\\me\\AppData\\Roaming\\app";
    #[cfg(not(windows))]
    const DIR: &str = "/home/me/.local/share/app";

    #[test]
    fn quotes_for_sh_and_powershell() {
        let sh = command_line(&p(ABS), "cursor", &p(DIR), Shell::Sh).expect("sh");
        assert_eq!(sh, format!("'{ABS}' hook cursor --data-dir '{DIR}'"));
        let ps = command_line(&p(ABS), "cursor", &p(DIR), Shell::PowerShell).expect("ps");
        assert_eq!(ps, format!("& '{ABS}' hook cursor --data-dir '{DIR}'"));
        assert!(command_is_ours(&sh, "cursor") && command_is_ours(&ps, "cursor"));
        assert!(!command_is_ours(&sh, "codex"));
    }

    #[test]
    fn refuses_paths_that_cannot_be_quoted_safely() {
        for bad in [
            "it's",
            "a\"b",
            "a`b",
            "a$(x)",
            "a$b",
            "a;b",
            "a&b",
            "a|b",
            "a<b",
            "a>b",
            "a%PATH%b",
            "a!b",
            "a#b",
            "a\nb",
            "a\u{2019}b",
            "a\u{202e}b",
            "a\0b",
            "a*b",
            "a{b}",
            "a[b]",
        ] {
            let dir = p(&format!("{DIR}{}{bad}", std::path::MAIN_SEPARATOR));
            assert!(
                command_line(&p(ABS), "cursor", &dir, Shell::Sh).is_none(),
                "{bad:?}"
            );
            assert!(
                command_line(&dir, "cursor", &p(DIR), Shell::PowerShell).is_none(),
                "{bad:?}"
            );
        }
        assert!(command_line(&p("relative/x"), "cursor", &p(DIR), Shell::Sh).is_none());
        assert!(command_line(&p(""), "cursor", &p(DIR), Shell::Sh).is_none());
    }

    #[test]
    fn foreign_commands_are_not_ours() {
        for c in [
            "./format.sh",
            "'/bin/evil' hook cursor --data-dir '/x'",
            "'/opt/lullshift/x' hook claude-code --data-dir '/x'",
            "lullshift hook cursor --data-dir x",
            "'unterminated hook cursor --data-dir ",
        ] {
            assert!(!command_is_ours(c, "cursor"), "{c}");
        }
    }
}
