//! Consent-based OpenCode plugin install/uninstall (ADR-022). EXPERIMENTAL, OpenCode 2.x only.
//!
//! Unlike the hook-file adapters, nothing in a user's file is merged. OpenCode 2.x loads every
//! `.js`/`.ts` file in `<global config dir>/plugins/` (source-confirmed, see ADR-022), so
//! Lullshift writes ONE file that it fully owns and touches nothing else:
//!
//! - The file is recognised by its first line. A file with that name that does not start with
//!   Lullshift's header is never read as ours, replaced or removed (install is refused).
//! - Symlinks, read-only files and non-files are refused, like the hook installers.
//! - Writes are atomic and abort if the file changed meanwhile. Installing twice changes
//!   nothing; installing over an older Lullshift version replaces the file (repair).
//! - Uninstall deletes the file only if it carries the header.
//! - The forwarder path and data directory are embedded as JavaScript string literals, escaped
//!   by [`js_string`] (ASCII only), so no path can break out of the literal.

use std::path::{Path, PathBuf};

use crate::claude_code::install::{
    InstallError, Status, UninstallOutcome, check_writable, read_capped, sha256_hex,
    write_if_unchanged,
};
use crate::hookfile::InstallRecord;

/// The plugin source with two quoted placeholders (valid JavaScript as it stands) for the
/// embedded strings.
const TEMPLATE: &str = include_str!("plugin.js");
const EXE_PLACEHOLDER: &str = "\"__LULLSHIFT_EXE__\"";
const DATA_DIR_PLACEHOLDER: &str = "\"__LULLSHIFT_DATA_DIR__\"";
/// The first bytes of every file Lullshift writes.
const HEADER: &str = "// Lullshift-owned file: lullshift-observer";
/// File name inside OpenCode's `plugins` directory.
pub const PLUGIN_FILE: &str = "lullshift-observer.js";

/// What the plugin runs: the Lullshift executable in hook mode, and the app data directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginCommand {
    pub exe: PathBuf,
    pub data_dir: PathBuf,
}

/// OpenCode's global config directory, the way OpenCode 2.x finds it: `$OPENCODE_CONFIG_DIR`,
/// else `$XDG_CONFIG_HOME/opencode`, else `~/.config/opencode` (on Windows too).
fn config_dir() -> Option<PathBuf> {
    let env = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    if let Some(dir) = env("OPENCODE_CONFIG_DIR") {
        return Some(dir);
    }
    if let Some(dir) = env("XDG_CONFIG_HOME") {
        return Some(dir.join("opencode"));
    }
    let home = env(if cfg!(windows) { "USERPROFILE" } else { "HOME" })?;
    Some(home.join(".config").join("opencode"))
}

/// `<global config dir>/plugins/lullshift-observer.js`.
pub fn default_plugin_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("plugins").join(PLUGIN_FILE))
}

/// `text` as a double-quoted JavaScript string literal, ASCII only: `"` and `\` are escaped and
/// every other non-printable or non-ASCII character becomes `\uXXXX` (UTF-16 code units).
/// Nothing in the output can end the literal or the line, whatever the input.
pub fn js_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            ' '..='~' => out.push(c),
            _ => {
                let mut units = [0u16; 2];
                for u in c.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{u:04x}"));
                }
            }
        }
    }
    out.push('"');
    out
}

/// The exact file content for `cmd`, or `None` if a path is relative, not valid Unicode or holds
/// a control character (no real path does; refused rather than escaped).
pub fn render(cmd: &PluginCommand) -> Option<String> {
    let usable =
        |p: &Path| p.is_absolute() && p.to_str().is_some_and(|s| !s.chars().any(char::is_control));
    if !usable(&cmd.exe) || !usable(&cmd.data_dir) {
        return None;
    }
    Some(
        TEMPLATE
            .replace(EXE_PLACEHOLDER, &js_string(cmd.exe.to_str()?))
            .replace(DATA_DIR_PLACEHOLDER, &js_string(cmd.data_dir.to_str()?)),
    )
}

fn is_ours(bytes: &[u8]) -> bool {
    bytes.starts_with(HEADER.as_bytes())
}

/// Writes the plugin file. `file` is created (with its directory) if missing; an existing file
/// must be Lullshift's own, and is left alone if it is already current.
pub fn install(file: &Path, cmd: &PluginCommand) -> Result<InstallRecord, InstallError> {
    let content = render(cmd).ok_or(InstallError::UnsupportedPath)?;
    if !file.is_absolute() {
        return Err(InstallError::UnsupportedPath);
    }
    let original = read_capped(file)?;
    if original.as_deref().is_some_and(|o| !is_ours(o)) {
        // Somebody else's file under our name: never replace it.
        return Err(InstallError::SettingsInvalid);
    }
    check_writable(file)?;
    if original.as_deref() != Some(content.as_bytes()) {
        write_if_unchanged(file, original.as_deref(), Some(content.as_bytes()))?;
    }
    Ok(InstallRecord {
        settings_path: file.to_owned(),
        backup_path: None,
        restore_exact: false,
        written_sha256: sha256_hex(content.as_bytes()),
        created_events: Vec::new(),
        created_hooks_key: false,
        created_top_keys: Vec::new(),
    })
}

/// Deletes the plugin file if it is Lullshift's. `Restored` means it was exactly what Lullshift
/// wrote; `Removed` means the user had edited it (it is still removed: it is Lullshift's file).
/// A missing file, or one without Lullshift's header, is left as it is.
pub fn uninstall(record: &InstallRecord) -> Result<UninstallOutcome, InstallError> {
    let path = &record.settings_path;
    let Some(current) = read_capped(path)? else {
        return Ok(UninstallOutcome::NothingToRemove);
    };
    if !is_ours(&current) {
        return Ok(UninstallOutcome::NothingToRemove);
    }
    write_if_unchanged(path, Some(&current), None)?;
    Ok(
        if !record.written_sha256.is_empty() && sha256_hex(&current) == record.written_sha256 {
            UninstallOutcome::Restored
        } else {
            UninstallOutcome::Removed
        },
    )
}

/// `NotInstalled` (no file), `Installed` (exactly the expected content), `NeedsUpdate` (ours but
/// different: an older version, or a moved app) or `SettingsInvalid` (a file that is not ours,
/// or not a regular readable file).
pub fn status(file: &Path, cmd: &PluginCommand) -> Status {
    let current = match read_capped(file) {
        Ok(Some(c)) => c,
        Ok(None) => return Status::NotInstalled,
        Err(_) => return Status::SettingsInvalid,
    };
    if !is_ours(&current) {
        return Status::SettingsInvalid;
    }
    match render(cmd) {
        Some(expected) if expected.as_bytes() == current.as_slice() => Status::Installed,
        _ => Status::NeedsUpdate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_strings_are_ascii_and_cannot_break_out() {
        assert_eq!(js_string("a\"b\\c"), r#""a\"b\\c""#);
        let ch = |n: u32| char::from_u32(n).expect("char");
        let esc = |hex: &str| format!("{}{}{hex}", ch(0x5c), ch(0x75));
        let quoted = |inner: String| format!("\"{inner}\"");
        let two = String::from_iter([ch(0xe9), ch(0x65e5)]);
        assert_eq!(js_string(&two), quoted(esc("00e9") + &esc("65e5")));
        assert_eq!(
            js_string(&ch(0x1F600).to_string()),
            quoted(esc("d83d") + &esc("de00"))
        );
        let tricky = format!("a{}b{}c", ch(0x0a), ch(0x2028));
        assert_eq!(
            js_string(&tricky),
            quoted(format!("a{}b{}c", esc("000a"), esc("2028")))
        );
        for s in [
            "'; process.exit(1); '",
            "\"; evil(); \"",
            "*/ evil() /*",
            "${x}",
        ] {
            let lit = js_string(s);
            assert!(lit.is_ascii() && !lit.contains('\n'));
            // Every quote inside the literal is escaped.
            let inner = &lit[1..lit.len() - 1];
            let mut chars = inner.chars();
            while let Some(c) = chars.next() {
                assert_ne!(c, '"', "{lit}");
                if c == '\\' {
                    chars.next();
                }
            }
        }
    }

    #[test]
    fn the_template_has_exactly_the_two_placeholders() {
        assert!(TEMPLATE.starts_with(HEADER));
        assert_eq!(TEMPLATE.matches(EXE_PLACEHOLDER).count(), 1);
        assert_eq!(TEMPLATE.matches(DATA_DIR_PLACEHOLDER).count(), 1);
    }

    #[test]
    fn relative_or_control_paths_are_refused() {
        let abs = std::env::temp_dir();
        let ok = PluginCommand {
            exe: abs.join("lullshift-desktop"),
            data_dir: abs.clone(),
        };
        assert!(render(&ok).is_some());
        let rel = PluginCommand {
            exe: PathBuf::from("lullshift-desktop"),
            ..ok.clone()
        };
        assert!(render(&rel).is_none());
        let ctl = PluginCommand {
            data_dir: abs.join("a\nb"),
            ..ok
        };
        assert!(render(&ctl).is_none());
    }
}
