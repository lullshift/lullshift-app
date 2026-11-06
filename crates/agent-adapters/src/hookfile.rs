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

/// The characters PowerShell treats as a single quote inside a string: the ASCII apostrophe and
/// the four Unicode look-alikes (U+2018 to U+201B). Inside `'...'` each one must be doubled.
const fn is_ps_single_quote(c: char) -> bool {
    matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}')
}

/// Characters no shell quoting can carry safely, so the install is refused rather than
/// escaped: every control character (U+0000 to U+001F including NUL and newlines, U+007F, the
/// C1 range U+0080 to U+009F with NEL), the Unicode line and paragraph separators, and the
/// bidirectional controls that can make a command read differently from how it runs.
fn is_refused_char(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{2028}'
                | '\u{2029}'
                | '\u{061C}'
                | '\u{200E}'
                | '\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2066}'..='\u{2069}'
        )
}

/// `text` quoted for one argument of `shell`, or `None` if it is empty or holds a character
/// that can't be quoted safely (see [`is_refused_char`]). Everything else is escaped, not
/// refused:
///
/// - sh: single quotes make every character literal; an embedded `'` becomes `'\''` (close the
///   quote, a quoted apostrophe, reopen).
/// - PowerShell: single quotes make every character literal; an embedded quote character (the
///   ASCII apostrophe or one of U+2018 to U+201B, all of which PowerShell reads as a quote) is
///   doubled.
///
/// Unlike [`command_line`], this does not require an absolute path; it exists so the quoting
/// can be tested on arbitrary text.
pub fn quote_arg(text: &str, shell: Shell) -> Option<String> {
    if text.is_empty() || text.chars().any(is_refused_char) {
        return None;
    }
    let mut out = String::with_capacity(text.len() + 2);
    out.push('\'');
    for c in text.chars() {
        match shell {
            Shell::Sh if c == '\'' => out.push_str("'\\''"),
            Shell::PowerShell if is_ps_single_quote(c) => {
                out.push(c);
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out.push('\'');
    Some(out)
}

/// The first quoted token at the start of `s` (which begins with `'`), unescaped, and the text
/// after its closing quote. Mirrors [`quote_arg`] for the given shell.
pub fn split_quoted(s: &str, shell: Shell) -> Option<(String, &str)> {
    let body = s.strip_prefix('\'')?;
    let mut chars = body.char_indices().peekable();
    let mut out = String::new();
    while let Some((i, c)) = chars.next() {
        match shell {
            Shell::Sh if c == '\'' => {
                if body[i..].starts_with("'\\''") {
                    out.push('\'');
                    // Skip the backslash, the apostrophe and the reopening quote.
                    for _ in 0..3 {
                        chars.next();
                    }
                } else {
                    return Some((out, &body[i + 1..]));
                }
            }
            Shell::PowerShell if is_ps_single_quote(c) => {
                if chars.peek().is_some_and(|(_, n)| is_ps_single_quote(*n)) {
                    // A doubled quote is one literal quote.
                    chars.next();
                    out.push(c);
                } else {
                    return Some((out, &body[i + c.len_utf8()..]));
                }
            }
            _ => out.push(c),
        }
    }
    None
}

/// Whether the UTF-8 encoding of `text` holds a byte that Windows code page 1252 decodes to a
/// curly quote: 0x82 (U+201A), 0x91 (U+2018), 0x92 (U+2019), 0x93 (U+201C) and 0x94 (U+201D). Byte
/// 0x84 (U+201E) is left out on purpose: it is only a double quote, literal inside a
/// single-quoted string, and refusing it would refuse every path with a capital A-umlaut.
///
/// If a host passes the hook command through a legacy (ANSI) code page, a
/// character such as Cyrillic `В` (UTF-8 `D0 92`) would arrive as `Ð` plus a curly quote, and
/// PowerShell treats the single-quote look-alikes as quote characters, which would end the
/// quoted path early. UTF-8 continuation bytes are at least 0x80 and lead bytes at least 0xC2,
/// so these bytes occur only inside a non-ASCII character, never in ASCII text. The
/// PowerShell form is refused for such paths (fail closed, ADR-021).
pub fn has_codepage_quote_byte(text: &str) -> bool {
    text.bytes().any(|b| matches!(b, 0x82 | 0x91..=0x94))
}

/// A quoted absolute path. PowerShell has three more refusals:
///
/// - a trailing backslash: when PowerShell hands an argument containing spaces to a native
///   program it wraps it in double quotes, and the program's parser reads `\"` as an escaped
///   quote;
/// - a character whose UTF-8 bytes include one that a legacy code page reads as a curly quote
///   (see [`has_codepage_quote_byte`]);
/// - `[` or `]` in the program path: `& 'path'` resolves the path as a wildcard pattern, so an
///   unbalanced bracket fails to run, and a balanced one (`a[bc]`) can match a different file.
///   Quoting can't prevent that, so the install is refused. (Arguments are not affected.)
fn quote(path: &Path, shell: Shell, is_program: bool) -> Option<String> {
    let s = path.to_str()?;
    let ps_unsafe = shell == Shell::PowerShell
        && (s.ends_with('\\')
            || (is_program && s.contains(['[', ']']))
            || has_codepage_quote_byte(s));
    if !path.is_absolute() || ps_unsafe {
        return None;
    }
    quote_arg(s, shell)
}

/// The full hook command: `<exe> hook <adapter> --data-dir <dir>`. `None` if either path can't
/// be quoted safely. PowerShell needs the call operator to run a quoted path.
pub fn command_line(exe: &Path, adapter: &str, data_dir: &Path, shell: Shell) -> Option<String> {
    let (exe, dir) = (quote(exe, shell, true)?, quote(data_dir, shell, false)?);
    let lead = match shell {
        Shell::Sh => "",
        Shell::PowerShell => "& ",
    };
    Some(format!("{lead}{exe} hook {adapter} --data-dir {dir}"))
}

/// The data directory in `command` if it is exactly a command line built by [`command_line`]
/// for `adapter`: a quoted Lullshift executable (escaped for either shell), then
/// `hook <adapter> --data-dir `, then exactly one complete quoted token that ends the string.
/// Anything else, such as text after the closing quote (`...; my-notify`), is not ours and is
/// never touched. The leading `& ` tells the two forms apart.
pub fn command_data_dir(command: &str, adapter: &str) -> Option<String> {
    let (rest, shell) = match command.strip_prefix("& ") {
        Some(r) => (r, Shell::PowerShell),
        None => (command, Shell::Sh),
    };
    let (exe, tail) = split_quoted(rest, shell)?;
    let name = exe.rsplit(['/', '\\']).next().unwrap_or("");
    if !name.to_ascii_lowercase().starts_with("lullshift") {
        return None;
    }
    let dir_token = tail.strip_prefix(&format!(" hook {adapter} --data-dir "))?;
    match split_quoted(dir_token, shell)? {
        (dir, "") => Some(dir),
        _ => None,
    }
}

/// Whether `command` has the exact shape of a Lullshift hook command for `adapter`, whatever
/// data directory it names (see [`command_data_dir`]).
pub fn command_is_ours(command: &str, adapter: &str) -> bool {
    command_data_dir(command, adapter).is_some()
}

/// [`command_is_ours`] and the data directory is exactly `data_dir`. This is what install,
/// uninstall and status act on: another Lullshift build or profile (a different data
/// directory) has its own entries, which are not ours to change.
pub fn command_is_ours_in(command: &str, adapter: &str, data_dir: &str) -> bool {
    command_data_dir(command, adapter).is_some_and(|d| d == data_dir)
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
    /// The data directory in the command lines this install owns. Entries naming another one
    /// belong to another build and are left alone. `None` (not valid Unicode) matches nothing.
    pub data_dir: Option<&'a str>,
    /// Codex nests handlers in matcher groups; Cursor lists handlers directly.
    pub grouped: bool,
    /// Top-level keys the file must have; added when missing. A different existing value makes
    /// the file "invalid" (Lullshift doesn't know that format).
    pub top_keys: &'a [(&'static str, Value)],
    pub events: &'a [&'static str],
    /// The handler object for an event, or `None` if the command can't be built safely.
    pub handler: &'a dyn Fn(&str) -> Option<Value>,
}

fn is_ours(handler: &Value, spec: &Spec) -> bool {
    let Some(dir) = spec.data_dir else {
        return false;
    };
    handler
        .get("command")
        .and_then(Value::as_str)
        .is_some_and(|c| command_is_ours_in(c, spec.adapter, dir))
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
                list.retain(|h| !is_ours(h, spec));
                let removed = list.len() != before;
                found |= removed;
                !(removed && list.is_empty())
            });
        } else {
            let before = entries.len();
            entries.retain(|h| !is_ours(h, spec));
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
                .filter(|h| is_ours(h, spec))
                .collect()
        } else {
            entries.filter(|h| is_ours(h, spec)).collect()
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

    /// Small deterministic generator, so a failure reproduces from the seed.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    /// Characters shells and path parsers treat specially, quote look-alikes and non-ASCII text.
    const NASTY: &[char] = &[
        '\'', '"', '`', '$', ';', '&', '|', '<', '>', '(', ')', '{', '}', '[', ']', '*', '?', '!',
        '#', '%', '^', '~', '=', '+', ',', '@', ' ', '\\', '/', ':', '-', '_', '.', '\u{2018}',
        '\u{2019}', '\u{201A}', '\u{201B}', '\u{201C}', '\u{201D}', '\u{FF07}', 'é', 'ß', '日',
        '本', '😀', '\u{0301}', '\u{200D}', '\u{00A0}', 'a', 'b', 'Z', '0', '9',
    ];

    /// A random string made mostly of special characters; never empty, never refused.
    pub(crate) fn random_text(rng_state: &mut u64) -> String {
        let mut rng = Rng(*rng_state);
        let len = 1 + rng.below(24);
        let s: String = (0..len).map(|_| NASTY[rng.below(NASTY.len())]).collect();
        *rng_state = rng.next();
        s
    }

    #[test]
    fn quoting_round_trips_for_many_random_texts() {
        let mut state = 0x9E37_79B9_7F4A_7C15_u64;
        for _ in 0..5000 {
            let text = random_text(&mut state);
            for shell in [Shell::Sh, Shell::PowerShell] {
                let q = quote_arg(&text, shell).expect("quotable");
                let (back, rest) = split_quoted(&q, shell).expect("parses");
                assert_eq!(back, text, "{shell:?} {q}");
                assert_eq!(rest, "", "{shell:?} {q}");
                // Nothing can follow the closing quote: appended text stays outside.
                let with_tail = format!("{q} tail");
                let (_, rest) = split_quoted(&with_tail, shell).expect("parses");
                assert_eq!(rest, " tail");
            }
        }
    }

    #[test]
    fn escapes_per_shell() {
        assert_eq!(quote_arg("it's", Shell::Sh).as_deref(), Some("'it'\\''s'"));
        assert_eq!(
            quote_arg("it's", Shell::PowerShell).as_deref(),
            Some("'it''s'")
        );
        for q in ['\u{2018}', '\u{2019}', '\u{201A}', '\u{201B}'] {
            let got = quote_arg(&format!("a{q}b"), Shell::PowerShell).expect("q");
            assert_eq!(got, format!("'a{q}{q}b'"));
            // sh has no such look-alikes: they stay literal.
            assert_eq!(
                quote_arg(&format!("a{q}b"), Shell::Sh).expect("q"),
                format!("'a{q}b'")
            );
        }
        assert_eq!(
            quote_arg("$(id)`x`;&|", Shell::Sh).as_deref(),
            Some("'$(id)`x`;&|'")
        );
    }

    #[test]
    fn control_and_separator_characters_are_refused() {
        let mut bad: Vec<char> = (0u32..0x20).filter_map(char::from_u32).collect();
        bad.extend((0x7Fu32..=0x9F).filter_map(char::from_u32));
        bad.extend(['\u{2028}', '\u{2029}', '\u{202E}', '\u{2066}', '\u{200F}']);
        for c in bad {
            for shell in [Shell::Sh, Shell::PowerShell] {
                assert!(
                    quote_arg(&format!("a{c}b"), shell).is_none(),
                    "{:?}",
                    c as u32
                );
                let dir = p(&format!("{DIR}{}a{c}b", std::path::MAIN_SEPARATOR));
                assert!(command_line(&p(ABS), "cursor", &dir, shell).is_none());
                assert!(command_line(&dir, "cursor", &p(DIR), shell).is_none());
            }
        }
        assert!(quote_arg("", Shell::Sh).is_none());
        assert!(command_line(&p("relative/x"), "cursor", &p(DIR), Shell::Sh).is_none());
        assert!(command_line(&p(""), "cursor", &p(DIR), Shell::Sh).is_none());
    }

    #[test]
    fn awkward_but_legal_paths_are_quoted_and_still_recognised_as_ours() {
        for odd in [
            "it's here",
            "a$b",
            "a`b",
            "a;b",
            "a&b",
            "a|b",
            "a%PATH%b",
            "a!b",
            "a\u{2019}b",
            "a\u{201B}b",
            "日本 😀",
            "a'; rm -rf ~; echo '",
        ] {
            let exe = p(&format!(
                "{DIR}{}{odd}{}lullshift-desktop",
                std::path::MAIN_SEPARATOR,
                std::path::MAIN_SEPARATOR
            ));
            for shell in [Shell::Sh, Shell::PowerShell] {
                let c = command_line(&exe, "cursor", &p(DIR), shell).expect("quoted");
                assert!(command_is_ours(&c, "cursor"), "{c}");
                assert!(!command_is_ours(&c, "codex"));
            }
        }
        // PowerShell reads a program path as a wildcard pattern, so brackets are refused there
        // (for the program only, and only for PowerShell).
        let bracket = p(&format!(
            "{DIR}{}a[bc]{0}lullshift",
            std::path::MAIN_SEPARATOR
        ));
        assert!(command_line(&bracket, "cursor", &p(DIR), Shell::PowerShell).is_none());
        assert!(command_line(&bracket, "cursor", &p(DIR), Shell::Sh).is_some());
        assert!(command_line(&p(ABS), "cursor", &bracket, Shell::PowerShell).is_some());
        // PowerShell hands a trailing backslash to native programs as an escaped quote.
        assert!(
            command_line(
                &p(ABS),
                "cursor",
                &p(&format!("{DIR}\\")),
                Shell::PowerShell
            )
            .is_none()
        );
    }

    #[test]
    fn powershell_refuses_paths_a_legacy_code_page_would_turn_into_quotes() {
        // UTF-8 bytes 0x82 and 0x91 to 0x94 decode to curly quotes in code page 1252.
        // Cyrillic V is D0 92; the others are picked for their second byte.
        for bad in [
            '\u{0402}', '\u{0411}', '\u{0412}', '\u{0413}', '\u{0414}', '\u{0452}', '\u{00d2}',
        ] {
            let bytes = bad.to_string().into_bytes();
            assert!(
                bytes.iter().any(|b| matches!(b, 0x82 | 0x91..=0x94)),
                "{bad:?} {bytes:x?}"
            );
            let dir = p(&format!("{DIR}{}x{bad}", std::path::MAIN_SEPARATOR));
            assert!(has_codepage_quote_byte(&bad.to_string()));
            // The PowerShell form is refused for both the program and the data directory...
            assert!(command_line(&p(ABS), "cursor", &dir, Shell::PowerShell).is_none());
            assert!(command_line(&dir, "cursor", &p(DIR), Shell::PowerShell).is_none());
            // ...and sh, which has no such quotes, is not affected.
            assert!(command_line(&p(ABS), "cursor", &dir, Shell::Sh).is_some());
        }
        // Other non-ASCII text is fine, in both shells.
        for ok in [
            "\u{c4}\u{e9}\u{fc}",
            "\u{65e5}\u{672c}",
            "\u{424}\u{43f}",
            "\u{1F600}",
        ] {
            assert!(!has_codepage_quote_byte(ok), "{ok}");
            let dir = p(&format!("{DIR}{}{ok}", std::path::MAIN_SEPARATOR));
            assert!(command_line(&p(ABS), "cursor", &dir, Shell::PowerShell).is_some());
        }
        assert!(!has_codepage_quote_byte("plain ASCII 'quotes' \"here\""));
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
