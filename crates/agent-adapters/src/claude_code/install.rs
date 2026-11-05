//! Consent-based Claude Code hook install/uninstall (ADR-010).
//!
//! - Only the user settings file is touched (`~/.claude/settings.json` or
//!   `$CLAUDE_CONFIG_DIR/settings.json`). Everything else in it is preserved, in order.
//! - The original bytes are backed up before the first change; backups are never deleted.
//! - Writes are atomic (synced temp file + rename, original permissions kept) and abort if the
//!   file changed since it was read. Symlinked or read-only settings files are refused.
//! - Installing again replaces Lullshift's own entries only: idempotent.
//! - Uninstall restores the original bytes exactly if nobody changed the file since Lullshift
//!   wrote it; otherwise it removes only Lullshift's entries.
//! - An unreadable or non-object settings file is never modified.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use super::ADAPTER_ID;
use super::map::HOOKS;

/// The command Claude Code runs: the Lullshift executable in hook mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookCommand {
    pub exe: PathBuf,
    pub data_dir: PathBuf,
}

impl HookCommand {
    /// The handler entry, or `None` if a path isn't valid Unicode (it couldn't be written to
    /// JSON unchanged, so the hook would point at the wrong place).
    fn handler(&self) -> Option<Value> {
        Some(json!({
            "type": "command",
            "command": self.exe.to_str()?,
            "args": ["hook", ADAPTER_ID, "--data-dir", self.data_dir.to_str()?],
            "async": true,
            "timeout": 10,
        }))
    }
}

/// What the app keeps (in SQLite) to undo an install. Contains no settings content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallRecord {
    pub settings_path: PathBuf,
    /// Copy of the file before Lullshift first changed it; `None` if the file didn't exist.
    pub backup_path: Option<PathBuf>,
    /// The backup is the user's own file (it held no Lullshift entries).
    pub restore_exact: bool,
    /// SHA-256 of the bytes Lullshift last wrote.
    pub written_sha256: String,
    /// Hook event keys that didn't exist before Lullshift added them.
    pub created_events: Vec<String>,
    /// The top-level `hooks` key didn't exist before.
    pub created_hooks_key: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    NotInstalled,
    Installed,
    /// Lullshift entries point at another executable or data directory (moved or updated app).
    NeedsUpdate,
    /// Some Lullshift entries were removed by hand.
    Partial,
    /// The settings file isn't a JSON object; Lullshift won't touch it.
    SettingsInvalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UninstallOutcome {
    /// The original file was put back byte for byte (or removed, if it didn't exist).
    Restored,
    /// Only Lullshift's entries were removed; other changes made since install are kept.
    Removed,
    NothingToRemove,
}

#[derive(Debug)]
pub enum InstallError {
    SettingsInvalid,
    /// The settings file is a symlink or read-only; Lullshift won't replace it.
    NotWritable,
    /// The app or data path isn't valid Unicode, so a hook can't point at it.
    UnsupportedPath,
    /// The file changed while Lullshift was updating it; nothing was written.
    ChangedConcurrently,
    Io(io::Error),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SettingsInvalid => f.write_str("the Claude Code settings file isn't valid JSON"),
            Self::NotWritable => f.write_str("the settings file is a symlink or read-only"),
            Self::UnsupportedPath => f.write_str("the app path isn't valid Unicode"),
            Self::ChangedConcurrently => f.write_str("the settings file changed during the update"),
            Self::Io(e) => write!(f, "file error: {}", e.kind()),
        }
    }
}

impl std::error::Error for InstallError {}

impl From<io::Error> for InstallError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// `$CLAUDE_CONFIG_DIR/settings.json`, else `~/.claude/settings.json`.
pub fn default_settings_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir).join("settings.json"));
    }
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .filter(|h| !h.is_empty())?;
    Some(PathBuf::from(home).join(".claude").join("settings.json"))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

/// A settings file as read: raw bytes (if it exists), the root object, and whether it started
/// with a UTF-8 byte order mark (kept when writing).
struct Settings {
    original: Option<Vec<u8>>,
    root: Map<String, Value>,
    bom: bool,
}

fn read_settings(path: &Path) -> Result<Settings, InstallError> {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Ok(Settings {
                original: None,
                root: Map::new(),
                bom: false,
            });
        }
        Err(e) => return Err(e.into()),
    };
    let bom = bytes.starts_with(BOM);
    let body = if bom { &bytes[BOM.len()..] } else { &bytes[..] };
    let root = if body.iter().all(u8::is_ascii_whitespace) {
        Map::new()
    } else {
        match serde_json::from_slice::<Value>(body) {
            Ok(Value::Object(root)) => root,
            _ => return Err(InstallError::SettingsInvalid),
        }
    };
    Ok(Settings {
        original: Some(bytes),
        root,
        bom,
    })
}

/// A handler Lullshift installed: exec form `<lullshift exe> hook claude-code ...`.
fn is_ours(handler: &Value) -> bool {
    let args = handler.get("args").and_then(Value::as_array);
    let first_two = args.is_some_and(|a| {
        a.first().and_then(Value::as_str) == Some("hook")
            && a.get(1).and_then(Value::as_str) == Some(ADAPTER_ID)
    });
    let exe_is_lullshift = handler
        .get("command")
        .and_then(Value::as_str)
        .and_then(|c| Path::new(c).file_name())
        .is_some_and(|n| {
            n.to_string_lossy()
                .to_ascii_lowercase()
                .starts_with("lullshift")
        });
    first_two && exe_is_lullshift
}

/// Removes Lullshift handlers, and matcher groups left empty by that. Returns whether any were found.
fn strip_ours(hooks: &mut Map<String, Value>) -> bool {
    let mut found = false;
    for groups in hooks.values_mut().filter_map(Value::as_array_mut) {
        groups.retain_mut(|group| {
            let Some(list) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
                return true;
            };
            let before = list.len();
            list.retain(|h| !is_ours(h));
            let removed = list.len() != before;
            found |= removed;
            !(removed && list.is_empty())
        });
    }
    found
}

/// The `hooks` object; every event in it must be an array of matcher groups.
fn hooks_of(root: &Map<String, Value>) -> Result<Option<&Map<String, Value>>, InstallError> {
    match root.get("hooks") {
        None => Ok(None),
        Some(Value::Object(h)) if h.values().all(Value::is_array) => Ok(Some(h)),
        Some(_) => Err(InstallError::SettingsInvalid),
    }
}

/// Refuses settings files Lullshift shouldn't replace: symlinks (dotfile managers), non-files
/// and read-only files. Returns the current permissions to carry over; `None` if missing.
fn check_writable(path: &Path) -> Result<Option<fs::Permissions>, InstallError> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() || !m.is_file() => Err(InstallError::NotWritable),
        Ok(m) if m.permissions().readonly() => Err(InstallError::NotWritable),
        Ok(m) => Ok(Some(m.permissions())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn serialize(root: &Map<String, Value>, bom: bool) -> Result<Vec<u8>, InstallError> {
    let mut out = if bom { BOM.to_vec() } else { Vec::new() };
    serde_json::to_writer_pretty(&mut out, root).map_err(|_| InstallError::SettingsInvalid)?;
    out.push(b'\n');
    Ok(out)
}

/// Writes `bytes` (or deletes the file for `None`) only if the file still holds `expected`.
fn write_if_unchanged(
    path: &Path,
    expected: Option<&[u8]>,
    bytes: Option<&[u8]>,
) -> Result<(), InstallError> {
    let current = match fs::read(path) {
        Ok(b) => Some(b),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    if current.as_deref() != expected {
        return Err(InstallError::ChangedConcurrently);
    }
    let permissions = check_writable(path)?;
    match bytes {
        None => fs::remove_file(path).or_else(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(e)
            }
        })?,
        Some(bytes) => {
            let dir = path
                .parent()
                .ok_or_else(|| io::Error::other("no parent directory"))?;
            fs::create_dir_all(dir)?;
            // Unique, exclusively created temp file in the same directory; removed on error.
            let mut tmp = tempfile::Builder::new()
                .prefix(".settings.lullshift-")
                .tempfile_in(dir)?;
            io::Write::write_all(&mut tmp, bytes)?;
            if let Some(p) = permissions {
                tmp.as_file().set_permissions(p)?;
            }
            tmp.as_file().sync_all()?;
            tmp.persist(path).map_err(|e| e.error)?;
        }
    }
    Ok(())
}

fn write_backup(backup_dir: &Path, original: &[u8]) -> Result<PathBuf, InstallError> {
    fs::create_dir_all(backup_dir)?;
    let stamp = time::OffsetDateTime::now_utc().unix_timestamp_nanos();
    let path = backup_dir.join(format!("settings-{stamp}.json.bak"));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut f = options.open(&path)?;
    io::Write::write_all(&mut f, original)?;
    f.sync_all()?;
    Ok(path)
}

/// Installs or updates Lullshift's hooks in `settings`. Pass the stored record from an earlier
/// install so the original backup and "created" bookkeeping carry over.
pub fn install(
    settings: &Path,
    backup_dir: &Path,
    cmd: &HookCommand,
    previous: Option<&InstallRecord>,
) -> Result<InstallRecord, InstallError> {
    let handler = cmd.handler().ok_or(InstallError::UnsupportedPath)?;
    let Settings {
        original,
        mut root,
        bom,
    } = read_settings(settings)?;
    hooks_of(&root)?;
    check_writable(settings)?;
    let previous = previous.filter(|p| p.settings_path == settings);
    let mut created_hooks_key =
        previous.map_or(!root.contains_key("hooks"), |p| p.created_hooks_key);
    let mut created_events = previous
        .map(|p| p.created_events.clone())
        .unwrap_or_default();

    let hooks = root
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or(InstallError::SettingsInvalid)?;
    let had_ours = strip_ours(hooks);
    if had_ours && previous.is_none() {
        // Leftovers of an install whose record was lost: event keys that held only Lullshift
        // entries count as Lullshift's, so uninstall can remove them again.
        hooks.retain(|event, v| {
            !(HOOKS.iter().any(|(e, _)| e == event) && v.as_array().is_some_and(Vec::is_empty))
        });
        created_hooks_key |= hooks.is_empty();
    }
    for (event, matcher) in HOOKS {
        if !hooks.contains_key(*event) && !created_events.iter().any(|e| e == event) {
            created_events.push((*event).to_owned());
        }
        let groups = hooks
            .entry(*event)
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or(InstallError::SettingsInvalid)?;
        let mut group = Map::new();
        if let Some(m) = matcher {
            group.insert("matcher".into(), Value::String((*m).to_owned()));
        }
        group.insert("hooks".into(), Value::Array(vec![handler.clone()]));
        groups.push(Value::Object(group));
    }
    let bytes = serialize(&root, bom)?;

    let (backup_path, restore_exact) = match previous {
        // The exact restore stays valid only while the file still holds what Lullshift last
        // wrote. If anyone changed it since, restoring the old backup would undo those changes,
        // so uninstall must then remove only Lullshift's entries.
        Some(p) => (
            p.backup_path.clone(),
            p.restore_exact
                && original.as_deref().map(sha256_hex).as_deref()
                    == Some(p.written_sha256.as_str()),
        ),
        None => match &original {
            Some(o) => (Some(write_backup(backup_dir, o)?), !had_ours),
            None => (None, true),
        },
    };
    if original.as_deref() != Some(bytes.as_slice()) {
        write_if_unchanged(settings, original.as_deref(), Some(&bytes))?;
    }
    Ok(InstallRecord {
        settings_path: settings.to_owned(),
        backup_path,
        restore_exact,
        written_sha256: sha256_hex(&bytes),
        created_events,
        created_hooks_key,
    })
}

/// Removes Lullshift's hooks, restoring the original file exactly when possible.
pub fn uninstall(record: &InstallRecord) -> Result<UninstallOutcome, InstallError> {
    let path = &record.settings_path;
    let current = match fs::read(path) {
        Ok(b) => Some(b),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    let Some(current) = current else {
        return Ok(UninstallOutcome::NothingToRemove);
    };
    if record.restore_exact && sha256_hex(&current) == record.written_sha256 {
        // A missing or unreadable backup falls back to removing Lullshift's entries.
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
    if !strip_ours(hooks) {
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
    }
    let bytes = serialize(&root, bom)?;
    write_if_unchanged(path, Some(&current), Some(&bytes))?;
    Ok(UninstallOutcome::Removed)
}

/// Whether Lullshift's hooks are present and current in `settings`.
pub fn status(settings: &Path, cmd: &HookCommand) -> Status {
    let Ok(Settings { root, .. }) = read_settings(settings) else {
        return Status::SettingsInvalid;
    };
    let hooks = match hooks_of(&root) {
        Ok(Some(h)) => h,
        Ok(None) => return Status::NotInstalled,
        Err(_) => return Status::SettingsInvalid,
    };
    let expected = cmd.handler();
    let mut present = 0;
    let mut stale = false;
    for (event, _) in HOOKS {
        let ours: Vec<&Value> = hooks
            .get(*event)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|g| g.get("hooks").and_then(Value::as_array))
            .flatten()
            .filter(|h| is_ours(h))
            .collect();
        if !ours.is_empty() {
            present += 1;
            stale |= ours.iter().any(|h| Some(*h) != expected.as_ref());
        }
    }
    match present {
        0 => Status::NotInstalled,
        _ if stale => Status::NeedsUpdate,
        n if n == HOOKS.len() => Status::Installed,
        _ => Status::Partial,
    }
}
