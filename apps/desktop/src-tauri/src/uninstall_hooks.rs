//! `lullshift-desktop --uninstall-hooks`: removes Lullshift's agent hooks without any UI.
//!
//! Run by the OS uninstaller (NSIS pre-uninstall hook on Windows, `prerm` in the Linux deb) so
//! Claude Code, Codex and Cursor are not left calling an executable that no longer exists
//! (ADR-010, S11-T06). It runs the same uninstall functions as **Disconnect** in the app, on the
//! settings file recorded at install time, so the original file is restored byte for byte when
//! nothing else changed and otherwise only Lullshift's own entries are removed; the user's own
//! hooks stay. It is idempotent, never creates a settings file or a database, leaves a file it
//! can't parse untouched, writes nothing to stdout or stderr and exits 0 whatever happens (an
//! uninstaller must not be blocked by it). It takes no input except one optional flag and opens
//! no socket.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use agent_adapters::claude_code::install::{self as claude, InstallError, UninstallOutcome};
use agent_adapters::hookfile::InstallRecord as HookRecord;
use agent_adapters::{claude_code, codex, cursor};
use local_store::Store;
use time::OffsetDateTime;

/// Bundle identifier: Tauri's app data directory is `<platform data dir>/<identifier>`.
const IDENTIFIER: &str = "app.lullshift.desktop";
const DB_FILE: &str = "lullshift.db";
/// The flag that selects this mode.
pub const FLAG: &str = "--uninstall-hooks";

/// What happened for one agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentResult {
    /// Same outcomes as Disconnect.
    Done(UninstallOutcome),
    /// No record and no known settings location: nothing to look at.
    Skipped,
    /// The file was left untouched; the code matches the app's error codes.
    Failed(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub claude_code: AgentResult,
    pub codex: AgentResult,
    pub cursor: AgentResult,
}

/// Where to look. [`Targets::from_env`] is the real thing; tests pass temp directories.
#[derive(Debug, Clone)]
pub struct Targets {
    pub data_dir: PathBuf,
    /// Used for an agent without a stored record.
    pub claude_default: Option<PathBuf>,
    pub codex_default: Option<PathBuf>,
    pub cursor_default: Option<PathBuf>,
}

impl Targets {
    pub fn from_env(data_dir: Option<PathBuf>) -> Option<Self> {
        Some(Self {
            data_dir: data_dir.or_else(default_data_dir)?,
            claude_default: claude::default_settings_path(),
            codex_default: codex::install::default_hooks_path(),
            cursor_default: cursor::install::default_hooks_path(),
        })
    }
}

/// The directory Tauri's `app_data_dir()` returns, without needing an app handle.
fn default_data_dir() -> Option<PathBuf> {
    let env = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let base = if cfg!(windows) {
        env("APPDATA")?
    } else if cfg!(target_os = "macos") {
        env("HOME")?.join("Library").join("Application Support")
    } else {
        env("XDG_DATA_HOME")
            .filter(|p| p.is_absolute())
            .or_else(|| env("HOME").map(|h| h.join(".local").join("share")))?
    };
    Some(base.join(IDENTIFIER))
}

/// The value of `--data-dir`, if given and absolute (a relative one would depend on the working
/// directory of the uninstaller).
fn parse_data_dir(args: impl IntoIterator<Item = OsString>) -> Option<PathBuf> {
    let mut data_dir = None;
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if a == "--data-dir" {
            data_dir = it.next().map(PathBuf::from).filter(|p| p.is_absolute());
        }
    }
    data_dir
}

/// Entry point from `main`: `args` are the arguments after the flag. Returns the process exit
/// code, always 0.
pub fn run(args: impl IntoIterator<Item = OsString>) -> i32 {
    if let Some(targets) = Targets::from_env(parse_data_dir(args)) {
        let _ = uninstall_all(&targets);
    }
    0
}

/// Removes the hooks of every adapter. Each adapter is independent: a failure in one never
/// stops the others.
pub fn uninstall_all(t: &Targets) -> Report {
    // Only open a database that exists and is not empty: an uninstall must not create user
    // data, and SQLite would give an empty file a full schema.
    let db = t.data_dir.join(DB_FILE);
    let usable = std::fs::metadata(&db).is_ok_and(|m| m.is_file() && m.len() > 0);
    let mut store = usable
        .then(|| Store::open(&db).ok().map(|(s, _)| s))
        .flatten();
    let claude_code = one(
        &mut store,
        claude_code::ADAPTER_ID,
        t.claude_default.as_deref(),
        claude_code::HOOKS.iter().map(|(e, _)| *e).collect(),
        |settings, events, record: Option<claude::InstallRecord>| {
            let record = record.unwrap_or_else(|| claude::InstallRecord {
                settings_path: settings.to_path_buf(),
                backup_path: None,
                restore_exact: false,
                written_sha256: String::new(),
                created_events: events,
                created_hooks_key: false,
            });
            claude::uninstall(&record)
        },
    );
    let codex = one(
        &mut store,
        codex::ADAPTER_ID,
        t.codex_default.as_deref(),
        codex::HOOKS.iter().map(|(e, _)| *e).collect(),
        |settings, events, record| {
            codex::install::uninstall(&hook_record(settings, events, record))
        },
    );
    let cursor = one(
        &mut store,
        cursor::ADAPTER_ID,
        t.cursor_default.as_deref(),
        cursor::HOOKS.iter().map(|(e, _)| *e).collect(),
        |settings, events, record| {
            cursor::install::uninstall(&hook_record(settings, events, record))
        },
    );
    Report {
        claude_code,
        codex,
        cursor,
    }
}

/// Without a record (database lost, or never connected) only Lullshift's own entries are removed.
fn hook_record(settings: &Path, events: Vec<String>, record: Option<HookRecord>) -> HookRecord {
    record.unwrap_or_else(|| HookRecord {
        settings_path: settings.to_path_buf(),
        backup_path: None,
        restore_exact: false,
        written_sha256: String::new(),
        created_events: events,
        created_hooks_key: false,
        created_top_keys: Vec::new(),
    })
}

fn one<R: serde::de::DeserializeOwned + HasSettingsPath>(
    store: &mut Option<Store>,
    id: &str,
    default: Option<&Path>,
    events: Vec<&'static str>,
    undo: impl FnOnce(&Path, Vec<String>, Option<R>) -> Result<UninstallOutcome, InstallError>,
) -> AgentResult {
    let record: Option<R> = store
        .as_ref()
        .and_then(|s| s.adapter_install(id).ok().flatten())
        .and_then(|j| serde_json::from_str(&j).ok());
    let settings = match (&record, default) {
        (Some(r), _) => r.settings_path().to_path_buf(),
        (None, Some(d)) => d.to_path_buf(),
        (None, None) => return AgentResult::Skipped,
    };
    let events = events.into_iter().map(str::to_owned).collect();
    match undo(&settings, events, record) {
        Ok(outcome) => {
            if let Some(s) = store.as_mut() {
                // As in Disconnect: a store problem doesn't undo a finished uninstall.
                let _ = s.mark_adapter_uninstalled(id);
                let _ = s.disconnect_provider(id, OffsetDateTime::now_utc());
            }
            AgentResult::Done(outcome)
        }
        Err(e) => AgentResult::Failed(match e {
            InstallError::SettingsInvalid => "settings_invalid",
            InstallError::NotWritable => "settings_not_writable",
            InstallError::UnsupportedPath => "unsupported_path",
            InstallError::ChangedConcurrently => "changed_concurrently",
            InstallError::Io(_) => "file_error",
        }),
    }
}

trait HasSettingsPath {
    fn settings_path(&self) -> &Path;
}
impl HasSettingsPath for claude::InstallRecord {
    fn settings_path(&self) -> &Path {
        &self.settings_path
    }
}
impl HasSettingsPath for HookRecord {
    fn settings_path(&self) -> &Path {
        &self.settings_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::fs;

    /// Everything lives in one temp directory; the real home directories are never read.
    struct Env {
        _dir: tempfile::TempDir,
        root: PathBuf,
    }

    impl Env {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().to_path_buf();
            fs::create_dir_all(root.join("data")).unwrap();
            Self { _dir: dir, root }
        }
        fn targets(&self) -> Targets {
            Targets {
                data_dir: self.root.join("data"),
                claude_default: Some(self.root.join("claude").join("settings.json")),
                codex_default: Some(self.root.join("codex").join("hooks.json")),
                cursor_default: Some(self.root.join("cursor").join("hooks.json")),
            }
        }
        fn exe(&self) -> PathBuf {
            self.root.join("app").join("lullshift-desktop.exe")
        }
        fn write(&self, rel: &str, v: &Value) -> PathBuf {
            let p = self.root.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, serde_json::to_vec_pretty(v).unwrap()).unwrap();
            p
        }
        fn backups(&self, id: &str) -> PathBuf {
            self.root.join("data").join("adapters").join(id)
        }
    }

    fn read(p: &Path) -> Value {
        serde_json::from_slice(&fs::read(p).unwrap()).unwrap()
    }

    fn user_claude() -> Value {
        json!({
            "theme": "dark",
            "hooks": { "Stop": [ { "hooks": [ { "type": "command", "command": "my-own-hook" } ] } ] }
        })
    }

    /// Installs all three adapters into temp files and records them in a temp database.
    fn connect_all(env: &Env) -> (PathBuf, PathBuf, PathBuf) {
        let t = env.targets();
        let (store, _) = Store::open(&t.data_dir.join(DB_FILE)).unwrap();
        let now = OffsetDateTime::now_utc();

        let claude_path = env.write("claude/settings.json", &user_claude());
        let rec = claude::install(
            &claude_path,
            &env.backups("claude-code"),
            &claude::HookCommand {
                exe: env.exe(),
                data_dir: t.data_dir.clone(),
            },
            None,
        )
        .unwrap();
        store
            .save_adapter_install("claude-code", &serde_json::to_string(&rec).unwrap(), now)
            .unwrap();

        let codex_path = env.write(
            "codex/hooks.json",
            &json!({"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "mine"}]}]}}),
        );
        let rec = codex::install::install(
            &codex_path,
            &env.backups("codex"),
            &codex::install::HookCommand {
                exe: env.exe(),
                data_dir: t.data_dir.clone(),
            },
            None,
        )
        .unwrap();
        store
            .save_adapter_install("codex", &serde_json::to_string(&rec).unwrap(), now)
            .unwrap();

        let cursor_path = env.write(
            "cursor/hooks.json",
            &json!({"version": 1, "hooks": {"stop": [{"command": "mine"}]}}),
        );
        let rec = cursor::install::install(
            &cursor_path,
            &env.backups("cursor"),
            &cursor::install::HookCommand {
                exe: env.exe(),
                data_dir: t.data_dir.clone(),
            },
            None,
        )
        .unwrap();
        store
            .save_adapter_install("cursor", &serde_json::to_string(&rec).unwrap(), now)
            .unwrap();
        (claude_path, codex_path, cursor_path)
    }

    fn has_ours(p: &Path) -> bool {
        fs::read_to_string(p).unwrap().contains("lullshift-desktop")
    }

    #[test]
    fn restores_originals_and_is_idempotent() {
        let env = Env::new();
        let original = serde_json::to_vec_pretty(&user_claude()).unwrap();
        let (c, x, u) = connect_all(&env);
        assert!(has_ours(&c) && has_ours(&x) && has_ours(&u));

        let report = uninstall_all(&env.targets());
        assert_eq!(
            report.claude_code,
            AgentResult::Done(UninstallOutcome::Restored)
        );
        assert_eq!(fs::read(&c).unwrap(), original);
        assert!(matches!(report.codex, AgentResult::Done(_)));
        assert!(matches!(report.cursor, AgentResult::Done(_)));
        for p in [&c, &x, &u] {
            assert!(!has_ours(p));
        }
        // The user's own hooks are still there.
        assert!(fs::read_to_string(&c).unwrap().contains("my-own-hook"));
        assert!(fs::read_to_string(&x).unwrap().contains("mine"));
        assert!(fs::read_to_string(&u).unwrap().contains("mine"));

        // A second run changes nothing and reports nothing to remove.
        let before: Vec<_> = [&c, &x, &u].iter().map(|p| fs::read(p).unwrap()).collect();
        let again = uninstall_all(&env.targets());
        assert_eq!(
            again.claude_code,
            AgentResult::Done(UninstallOutcome::NothingToRemove)
        );
        let after: Vec<_> = [&c, &x, &u].iter().map(|p| fs::read(p).unwrap()).collect();
        assert_eq!(before, after);
    }

    #[test]
    fn keeps_users_later_edits_and_own_hooks() {
        let env = Env::new();
        let (c, ..) = connect_all(&env);
        // Edited after connecting: not byte-identical, so no exact restore.
        let mut v = read(&c);
        v["model"] = json!("opus");
        fs::write(&c, serde_json::to_vec_pretty(&v).unwrap()).unwrap();

        let report = uninstall_all(&env.targets());
        assert_eq!(
            report.claude_code,
            AgentResult::Done(UninstallOutcome::Removed)
        );
        let after = read(&c);
        assert_eq!(after["model"], "opus");
        assert_eq!(after["theme"], "dark");
        assert!(!has_ours(&c));
        assert!(fs::read_to_string(&c).unwrap().contains("my-own-hook"));
    }

    #[test]
    fn works_without_a_database_and_creates_nothing() {
        let env = Env::new();
        let t = env.targets();
        let report = uninstall_all(&t);
        assert_eq!(
            report.claude_code,
            AgentResult::Done(UninstallOutcome::NothingToRemove)
        );
        assert!(!t.data_dir.join(DB_FILE).exists());
        assert!(!env.root.join("claude").exists());
        assert!(!env.root.join("codex").exists());
        assert!(!env.root.join("cursor").exists());
    }

    #[test]
    fn leaves_an_empty_database_file_empty() {
        let env = Env::new();
        let t = env.targets();
        std::fs::create_dir_all(&t.data_dir).unwrap();
        std::fs::write(t.data_dir.join(DB_FILE), b"").unwrap();
        let _ = uninstall_all(&t);
        assert_eq!(
            std::fs::metadata(t.data_dir.join(DB_FILE)).unwrap().len(),
            0
        );
        let others: Vec<_> = std::fs::read_dir(&t.data_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name())
            .filter(|n| n != DB_FILE)
            .collect();
        assert!(others.is_empty(), "uninstall created {others:?}");
    }

    #[test]
    fn without_a_record_removes_only_our_entries() {
        let env = Env::new();
        let (c, x, u) = connect_all(&env);
        fs::remove_file(env.targets().data_dir.join(DB_FILE)).unwrap();
        let report = uninstall_all(&env.targets());
        assert!(matches!(report.claude_code, AgentResult::Done(_)));
        for p in [&c, &x, &u] {
            assert!(!has_ours(p));
        }
        assert!(fs::read_to_string(&c).unwrap().contains("my-own-hook"));
        assert!(fs::read_to_string(&u).unwrap().contains("mine"));
    }

    #[test]
    fn unparsable_file_is_left_untouched_and_others_still_run() {
        let env = Env::new();
        let (c, x, _u) = connect_all(&env);
        fs::write(&c, b"{ not json").unwrap();
        let report = uninstall_all(&env.targets());
        assert_eq!(report.claude_code, AgentResult::Failed("settings_invalid"));
        assert_eq!(fs::read(&c).unwrap(), b"{ not json");
        assert!(!has_ours(&x));
    }

    #[test]
    fn marks_adapters_uninstalled_in_the_database() {
        let env = Env::new();
        connect_all(&env);
        uninstall_all(&env.targets());
        let (store, _) = Store::open(&env.targets().data_dir.join(DB_FILE)).unwrap();
        for id in ["claude-code", "codex", "cursor"] {
            assert!(store.adapter_install(id).unwrap().is_none());
        }
    }

    #[test]
    fn data_dir_flag_must_be_absolute() {
        let abs = std::env::temp_dir();
        let args = [OsString::from("--data-dir"), abs.clone().into_os_string()];
        assert_eq!(parse_data_dir(args), Some(abs));
        let rel = [OsString::from("--data-dir"), OsString::from("rel/dir")];
        assert_eq!(parse_data_dir(rel), None);
        assert_eq!(parse_data_dir([OsString::from("--other")]), None);
    }
}
