//! `lullshift-desktop --repair-recorded-hooks`: puts back the agent hooks that a reinstall
//! removed, without any UI (S12 security review M1, ADR-010 amendment).
//!
//! Why this exists. When a new setup runs over an existing install it first runs the old
//! uninstaller. That happens before the user chooses the folder, so the install may still be
//! cancelled or go to a different folder. The uninstaller therefore always removes the hooks (a
//! hook pointing at a deleted program in a user-writable folder would be a persistence point)
//! and this mode restores them once the new install has finished, pointing at the new program.
//!
//! What counts as consent. Only the install record that Connect saved in the app database
//! (`lullshift.db`, table `adapter_installations`): it proves the user connected that agent. An
//! adapter without a record is never touched, and if the database was deleted nothing is
//! restored. The reinstall flow runs the uninstaller as `--uninstall-hooks --keep-records`,
//! which keeps the records and writes the marker file [`MARKER_FILE`] with the time. This mode
//! does nothing without a fresh marker (at most [`MARKER_MAX_AGE_SECS`] old):
//!
//! - no marker: nothing to do (a normal start, an in-place update that never removed hooks);
//! - a stale marker (an uninstall that was cancelled or never completed, long ago): the marker
//!   is deleted and the records are marked uninstalled, as the uninstall would have done. The
//!   hooks are not restored;
//! - a fresh marker: the marker is deleted first (one shot), then for each adapter with a
//!   record whose hooks are missing or point elsewhere the hooks are installed again for this
//!   program, with the usual backup. Already-correct hooks are not rewritten, so a second run
//!   changes nothing.
//!
//! Like `--uninstall-hooks` it has no UI, writes nothing to stdout or stderr, opens no socket,
//! never creates a database or a settings file that has no record, and exits 0 whatever
//! happens (an installer must not be blocked by it).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use agent_adapters::claude_code::install::{self as claude, InstallError, Status};
use agent_adapters::hookfile::InstallRecord as HookRecord;
use agent_adapters::{claude_code, codex, cursor, opencode};
use local_store::Store;
use serde::Serialize;
use time::OffsetDateTime;

use crate::uninstall_hooks::{HasSettingsPath, default_data_dir, parse_data_dir};

/// The flag that selects this mode.
pub const FLAG: &str = "--repair-recorded-hooks";
/// File in the app data directory that says "an in-place uninstall removed the hooks of this
/// install at this time (Unix seconds); restore them if a setup finishes soon".
pub const MARKER_FILE: &str = "hook-restore.pending";
/// How long after the uninstall the marker still counts. A setup that runs the old uninstaller
/// and then finishes takes minutes at most; a marker older than this is a cancelled uninstall.
pub const MARKER_MAX_AGE_SECS: u64 = 60 * 60;
const DB_FILE: &str = "lullshift.db";

/// What happened for one agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepairResult {
    /// No install record (never connected, disconnected, or the database was lost).
    NoRecord,
    /// The hooks are already correct for this program; nothing was written.
    AlreadyCurrent,
    /// Hooks installed again and the record updated.
    Restored,
    /// Nothing was changed (or the change was taken back); the code matches the app's error codes.
    Failed(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub claude_code: RepairResult,
    pub codex: RepairResult,
    pub cursor: RepairResult,
    pub opencode: RepairResult,
}

/// What the marker said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Run {
    /// No marker: nothing was looked at.
    NoMarker,
    /// The marker was too old (or unreadable): records marked uninstalled, nothing restored.
    Stale,
    /// A fresh marker: the per-agent results.
    Restored(Report),
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Opens the database only if it exists and is not empty: this mode must not create user data.
fn open_existing(data_dir: &Path) -> Option<Store> {
    let db = data_dir.join(DB_FILE);
    std::fs::metadata(&db)
        .is_ok_and(|m| m.is_file() && m.len() > 0)
        .then(|| Store::open(&db).ok().map(|(s, _)| s))
        .flatten()
}

const ADAPTERS: [&str; 4] = [
    claude_code::ADAPTER_ID,
    codex::ADAPTER_ID,
    cursor::ADAPTER_ID,
    opencode::ADAPTER_ID,
];

/// Called by the uninstaller after it kept the records: writes the marker if any adapter still
/// has a record (otherwise there is nothing to restore later). Best effort.
pub(crate) fn write_marker_if_records(store: &Store, data_dir: &Path) {
    let any = ADAPTERS
        .iter()
        .any(|id| store.adapter_install(id).ok().flatten().is_some());
    if any {
        let _ = std::fs::write(data_dir.join(MARKER_FILE), unix_now().to_string());
    }
}

/// Entry point from `main`: `args` are the arguments after the flag. Always returns 0.
pub fn run(args: impl IntoIterator<Item = OsString>) -> i32 {
    let data_dir = parse_data_dir(args).or_else(default_data_dir);
    if let (Some(data_dir), Ok(exe)) = (data_dir, crate::hook_exe::hook_exe()) {
        let _ = repair_all(&data_dir, &exe, unix_now());
    }
    0
}

/// See the module docs. `now` is Unix seconds; `exe` is the program the hooks must call.
pub fn repair_all(data_dir: &Path, exe: &Path, now: u64) -> Run {
    let marker = data_dir.join(MARKER_FILE);
    let Ok(text) = std::fs::read_to_string(&marker) else {
        return Run::NoMarker;
    };
    // One shot, whatever happens next: a crash must not leave a marker that restores hooks later.
    let _ = std::fs::remove_file(&marker);
    let mut store = open_existing(data_dir);
    let fresh = text
        .trim()
        .parse::<u64>()
        .ok()
        // A time in the future (clock changed) is not trusted either.
        .is_some_and(|t| t <= now && now - t <= MARKER_MAX_AGE_SECS);
    if !fresh {
        if let Some(s) = store.as_mut() {
            for id in ADAPTERS {
                let _ = s.mark_adapter_uninstalled(id);
            }
        }
        return Run::Stale;
    }
    let Some(store) = store else {
        return Run::Restored(Report {
            claude_code: RepairResult::NoRecord,
            codex: RepairResult::NoRecord,
            cursor: RepairResult::NoRecord,
            opencode: RepairResult::NoRecord,
        });
    };
    let data = data_dir.to_path_buf();
    let exe = exe.to_path_buf();

    let claude_code = {
        let cmd = claude::HookCommand {
            exe: exe.clone(),
            data_dir: data.clone(),
        };
        one(
            &store,
            claude_code::ADAPTER_ID,
            &data,
            |f: &Path| claude::status(f, &cmd) == Status::Installed,
            |f, backups| claude::install(f, backups, &cmd, None),
            |r: &claude::InstallRecord| {
                let _ = claude::uninstall(r);
            },
        )
    };
    let codex = {
        let cmd = codex::install::HookCommand {
            exe: exe.clone(),
            data_dir: data.clone(),
        };
        one(
            &store,
            codex::ADAPTER_ID,
            &data,
            |f: &Path| codex::install::status(f, None, &cmd) == Status::Installed,
            |f, backups| codex::install::install(f, backups, &cmd, None),
            |r: &HookRecord| {
                let _ = codex::install::uninstall(r, &data);
            },
        )
    };
    let cursor = {
        let cmd = cursor::install::HookCommand {
            exe: exe.clone(),
            data_dir: data.clone(),
        };
        one(
            &store,
            cursor::ADAPTER_ID,
            &data,
            |f: &Path| cursor::install::status(f, &cmd) == Status::Installed,
            |f, backups| cursor::install::install(f, backups, &cmd, None),
            |r: &HookRecord| {
                let _ = cursor::install::uninstall(r, &data);
            },
        )
    };
    let opencode = {
        let cmd = opencode::install::PluginCommand {
            exe,
            data_dir: data.clone(),
        };
        one(
            &store,
            opencode::ADAPTER_ID,
            &data,
            |f: &Path| opencode::install::status(f, &cmd) == Status::Installed,
            |f, _backups| opencode::install::install(f, &cmd),
            |r: &HookRecord| {
                let _ = opencode::install::uninstall(r);
            },
        )
    };
    Run::Restored(Report {
        claude_code,
        codex,
        cursor,
        opencode,
    })
}

/// One agent: needs a stored record; installs again unless the hooks are already current.
fn one<R: serde::de::DeserializeOwned + Serialize + HasSettingsPath>(
    store: &Store,
    id: &str,
    data_dir: &Path,
    is_current: impl Fn(&Path) -> bool,
    apply: impl Fn(&Path, &Path) -> Result<R, InstallError>,
    undo: impl Fn(&R),
) -> RepairResult {
    let record: Option<R> = store
        .adapter_install(id)
        .ok()
        .flatten()
        .and_then(|j| serde_json::from_str(&j).ok());
    let Some(record) = record else {
        return RepairResult::NoRecord;
    };
    let settings: PathBuf = record.settings_path().to_path_buf();
    if is_current(&settings) {
        return RepairResult::AlreadyCurrent;
    }
    let backups = data_dir.join("adapters").join(id).join("backups");
    match apply(&settings, &backups) {
        Ok(new_record) => {
            let saved = serde_json::to_string(&new_record).ok().is_some_and(|j| {
                store
                    .save_adapter_install(id, &j, OffsetDateTime::now_utc())
                    .is_ok()
            });
            if saved {
                RepairResult::Restored
            } else {
                // Without a record the install can't be undone reliably: take it back out.
                undo(&new_record);
                RepairResult::Failed("store_unavailable")
            }
        }
        Err(e) => RepairResult::Failed(match e {
            InstallError::SettingsInvalid => "settings_invalid",
            InstallError::NotWritable => "settings_not_writable",
            InstallError::UnsupportedPath => "unsupported_path",
            InstallError::ChangedConcurrently => "changed_concurrently",
            InstallError::Io(_) => "file_error",
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uninstall_hooks::{self, AgentResult, Targets};
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
        fn data(&self) -> PathBuf {
            self.root.join("data")
        }
        fn old_exe(&self) -> PathBuf {
            self.root.join("old-app").join("lullshift-desktop.exe")
        }
        fn new_exe(&self) -> PathBuf {
            self.root.join("new dir").join("lullshift-desktop.exe")
        }
        fn targets(&self, keep: bool) -> Targets {
            Targets {
                data_dir: self.data(),
                claude_default: Some(self.root.join("claude").join("settings.json")),
                codex_default: Some(self.root.join("codex").join("hooks.json")),
                cursor_default: Some(self.root.join("cursor").join("hooks.json")),
                opencode_default: Some(
                    self.root
                        .join("opencode")
                        .join("plugins")
                        .join(opencode::install::PLUGIN_FILE),
                ),
                keep_records: keep,
            }
        }
        fn write(&self, rel: &str, v: &Value) -> PathBuf {
            let p = self.root.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, serde_json::to_vec_pretty(v).unwrap()).unwrap();
            p
        }
        fn backups(&self, id: &str) -> PathBuf {
            self.data().join("adapters").join(id).join("backups")
        }
        fn store(&self) -> Store {
            Store::open(&self.data().join(DB_FILE)).unwrap().0
        }
    }

    fn save<R: Serialize>(store: &Store, id: &str, r: &R) {
        store
            .save_adapter_install(
                id,
                &serde_json::to_string(r).unwrap(),
                OffsetDateTime::now_utc(),
            )
            .unwrap();
    }

    /// Connects the agents named in `which` (claude-code, codex, cursor, opencode) for the old
    /// program, with some user content, and records them. Returns the settings files.
    fn connect(env: &Env, which: &[&str]) -> Vec<PathBuf> {
        let store = env.store();
        let exe = env.old_exe();
        let data = env.data();
        let mut files = Vec::new();
        if which.contains(&"claude-code") {
            let f = env.write("claude/settings.json", &json!({"theme": "dark"}));
            let cmd = claude::HookCommand {
                exe: exe.clone(),
                data_dir: data.clone(),
            };
            let r = claude::install(&f, &env.backups("claude-code"), &cmd, None).unwrap();
            save(&store, "claude-code", &r);
            files.push(f);
        }
        if which.contains(&"codex") {
            let f = env.write(
                "codex/hooks.json",
                &json!({"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "mine"}]}]}}),
            );
            let cmd = codex::install::HookCommand {
                exe: exe.clone(),
                data_dir: data.clone(),
            };
            let r = codex::install::install(&f, &env.backups("codex"), &cmd, None).unwrap();
            save(&store, "codex", &r);
            files.push(f);
        }
        if which.contains(&"cursor") {
            let f = env.write(
                "cursor/hooks.json",
                &json!({"version": 1, "hooks": {"stop": [{"command": "mine"}]}}),
            );
            let cmd = cursor::install::HookCommand {
                exe: exe.clone(),
                data_dir: data.clone(),
            };
            let r = cursor::install::install(&f, &env.backups("cursor"), &cmd, None).unwrap();
            save(&store, "cursor", &r);
            files.push(f);
        }
        if which.contains(&"opencode") {
            let f = env.targets(false).opencode_default.unwrap();
            let cmd = opencode::install::PluginCommand {
                exe,
                data_dir: data,
            };
            let r = opencode::install::install(&f, &cmd).unwrap();
            save(&store, "opencode", &r);
            files.push(f);
        }
        files
    }

    fn mentions(p: &Path, exe: &Path) -> bool {
        let text = fs::read_to_string(p).unwrap_or_default();
        // JSON escapes backslashes, so look for the file name and its folder separately.
        let folder = exe.parent().unwrap().file_name().unwrap().to_string_lossy();
        text.contains(&*folder) && text.contains("lullshift-desktop")
    }

    fn has_ours(p: &Path) -> bool {
        fs::read_to_string(p)
            .map(|t| t.contains("lullshift-desktop") || t.contains("Lullshift-owned"))
            .unwrap_or(false)
    }

    /// The reinstall sequence: the old uninstaller runs in place (keep records), the new setup
    /// finishes, and the repair runs for the new program.
    fn reinstall(env: &Env, at: u64) -> Run {
        uninstall_hooks::uninstall_all(&env.targets(true));
        repair_all(&env.data(), &env.new_exe(), at)
    }

    #[test]
    fn uninstall_then_cancel_leaves_no_hooks() {
        let env = Env::new();
        let files = connect(&env, &["claude-code", "codex", "cursor", "opencode"]);
        assert!(files.iter().all(|f| has_ours(f)));
        // The user chose Uninstall on the reinstall page, then cancelled before the install:
        // only the in-place uninstaller ran.
        let report = uninstall_hooks::uninstall_all(&env.targets(true));
        assert!(matches!(report.codex, AgentResult::Done(_)));
        for f in &files {
            assert!(!has_ours(f), "{f:?}");
        }
        // The records are kept, with a marker, so a setup that does finish can restore.
        assert!(env.data().join(MARKER_FILE).is_file());
        assert!(env.store().adapter_install("cursor").unwrap().is_some());
        // Nothing but the uninstaller ran: no hook points anywhere.
    }

    #[test]
    fn a_reinstall_restores_hooks_for_the_new_program_for_recorded_agents_only() {
        let env = Env::new();
        // Only Claude Code and Cursor were connected.
        let files = connect(&env, &["claude-code", "cursor"]);
        let Run::Restored(report) = reinstall(&env, unix_now()) else {
            panic!("expected a restore");
        };
        assert_eq!(report.claude_code, RepairResult::Restored);
        assert_eq!(report.cursor, RepairResult::Restored);
        assert_eq!(report.codex, RepairResult::NoRecord);
        assert_eq!(report.opencode, RepairResult::NoRecord);
        for f in &files {
            assert!(
                mentions(f, &env.new_exe()),
                "{f:?} points at the new folder"
            );
            assert!(
                !mentions(f, &env.old_exe()),
                "{f:?} no longer points at the old one"
            );
        }
        // The user's own hooks and settings are intact.
        let cursor = fs::read_to_string(&files[1]).unwrap();
        assert!(cursor.contains("\"mine\""));
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(&files[0]).unwrap()).unwrap()["theme"],
            "dark"
        );
        // Never for an adapter without a record: its default file was not created.
        assert!(!env.root.join("codex").exists());
        assert!(!env.root.join("opencode").exists());
        // One shot: the marker is gone, and a second run does nothing.
        assert!(!env.data().join(MARKER_FILE).exists());
        let before: Vec<_> = files.iter().map(|f| fs::read(f).unwrap()).collect();
        assert_eq!(
            repair_all(&env.data(), &env.new_exe(), unix_now()),
            Run::NoMarker
        );
        let after: Vec<_> = files.iter().map(|f| fs::read(f).unwrap()).collect();
        assert_eq!(before, after);
    }

    #[test]
    fn restores_all_four_adapters_and_the_records_point_at_the_new_program() {
        let env = Env::new();
        let files = connect(&env, &["claude-code", "codex", "cursor", "opencode"]);
        let Run::Restored(report) = reinstall(&env, unix_now()) else {
            panic!("expected a restore");
        };
        assert_eq!(
            report,
            Report {
                claude_code: RepairResult::Restored,
                codex: RepairResult::Restored,
                cursor: RepairResult::Restored,
                opencode: RepairResult::Restored,
            }
        );
        for f in &files {
            assert!(mentions(f, &env.new_exe()), "{f:?}");
        }
        // The records still work: a later Disconnect (uninstall by record) removes the hooks and
        // restores the originals.
        let report = uninstall_hooks::uninstall_all(&env.targets(false));
        assert!(matches!(report.claude_code, AgentResult::Done(_)));
        for f in &files {
            assert!(!has_ours(f), "{f:?}");
        }
        for id in ["claude-code", "codex", "cursor", "opencode"] {
            assert!(env.store().adapter_install(id).unwrap().is_none());
        }
    }

    #[test]
    fn already_current_hooks_are_not_rewritten() {
        let env = Env::new();
        let files = connect(&env, &["cursor"]);
        // The marker exists but the hooks were never removed (nothing to restore for the new
        // program either: use the old program as the new one).
        fs::write(env.data().join(MARKER_FILE), unix_now().to_string()).unwrap();
        let before = fs::read(&files[0]).unwrap();
        let run = repair_all(&env.data(), &env.old_exe(), unix_now());
        let Run::Restored(report) = run else {
            panic!("fresh marker")
        };
        assert_eq!(report.cursor, RepairResult::AlreadyCurrent);
        assert_eq!(fs::read(&files[0]).unwrap(), before);
    }

    #[test]
    fn without_a_marker_nothing_happens_even_with_records() {
        let env = Env::new();
        let files = connect(&env, &["claude-code", "codex"]);
        // Hooks removed by hand; no marker: this mode must not put them back.
        for f in &files {
            fs::write(f, b"{}").unwrap();
        }
        assert_eq!(
            repair_all(&env.data(), &env.new_exe(), unix_now()),
            Run::NoMarker
        );
        for f in &files {
            assert_eq!(fs::read(f).unwrap(), b"{}");
        }
    }

    #[test]
    fn a_deleted_database_restores_nothing() {
        let env = Env::new();
        let files = connect(&env, &["claude-code", "cursor"]);
        uninstall_hooks::uninstall_all(&env.targets(true));
        // The record is what keeps consent: without the database there is none.
        for ext in ["", "-wal", "-shm"] {
            let _ = fs::remove_file(env.data().join(format!("{DB_FILE}{ext}")));
        }
        let Run::Restored(report) = repair_all(&env.data(), &env.new_exe(), unix_now()) else {
            panic!("fresh marker");
        };
        assert_eq!(report.claude_code, RepairResult::NoRecord);
        assert_eq!(report.cursor, RepairResult::NoRecord);
        for f in &files {
            assert!(!has_ours(f));
        }
        assert!(!env.data().join(DB_FILE).exists(), "no database is created");
    }

    #[test]
    fn a_disconnected_agent_stays_disconnected() {
        let env = Env::new();
        let files = connect(&env, &["claude-code", "cursor"]);
        // The user pressed Disconnect for Cursor before the reinstall.
        env.store().mark_adapter_uninstalled("cursor").unwrap();
        fs::write(&files[1], b"{\"version\":1}").unwrap();
        let Run::Restored(report) = reinstall(&env, unix_now()) else {
            panic!("fresh marker");
        };
        assert_eq!(report.claude_code, RepairResult::Restored);
        assert_eq!(report.cursor, RepairResult::NoRecord);
        assert!(!has_ours(&files[1]));
    }

    #[test]
    fn a_stale_marker_restores_nothing_and_finishes_the_uninstall() {
        let env = Env::new();
        let files = connect(&env, &["claude-code", "codex"]);
        uninstall_hooks::uninstall_all(&env.targets(true));
        let later = unix_now() + MARKER_MAX_AGE_SECS + 1;
        assert_eq!(repair_all(&env.data(), &env.new_exe(), later), Run::Stale);
        for f in &files {
            assert!(!has_ours(f));
        }
        assert!(!env.data().join(MARKER_FILE).exists());
        assert!(
            env.store()
                .adapter_install("claude-code")
                .unwrap()
                .is_none()
        );
        assert!(env.store().adapter_install("codex").unwrap().is_none());
        // Garbage and future markers are stale too.
        for text in ["", "soon", "-1", &(unix_now() + 100_000).to_string()] {
            let env = Env::new();
            connect(&env, &["cursor"]);
            fs::write(env.data().join(MARKER_FILE), text).unwrap();
            assert_eq!(
                repair_all(&env.data(), &env.new_exe(), unix_now()),
                Run::Stale,
                "{text:?}"
            );
        }
    }

    #[test]
    fn a_plain_uninstall_deletes_the_records_and_writes_no_marker() {
        let env = Env::new();
        connect(&env, &["claude-code", "cursor"]);
        uninstall_hooks::uninstall_all(&env.targets(false));
        assert!(!env.data().join(MARKER_FILE).exists());
        assert!(
            env.store()
                .adapter_install("claude-code")
                .unwrap()
                .is_none()
        );
        assert_eq!(
            repair_all(&env.data(), &env.new_exe(), unix_now()),
            Run::NoMarker
        );
    }

    #[test]
    fn no_marker_is_written_when_there_are_no_records() {
        let env = Env::new();
        // A database exists but nothing was ever connected.
        drop(env.store());
        uninstall_hooks::uninstall_all(&env.targets(true));
        assert!(!env.data().join(MARKER_FILE).exists());
    }

    #[test]
    fn a_file_that_does_not_parse_is_left_alone_and_others_still_restore() {
        let env = Env::new();
        let files = connect(&env, &["claude-code", "cursor"]);
        uninstall_hooks::uninstall_all(&env.targets(true));
        fs::write(&files[0], b"{ not json").unwrap();
        let Run::Restored(report) = repair_all(&env.data(), &env.new_exe(), unix_now()) else {
            panic!("fresh marker");
        };
        assert_eq!(report.claude_code, RepairResult::Failed("settings_invalid"));
        assert_eq!(fs::read(&files[0]).unwrap(), b"{ not json");
        assert_eq!(report.cursor, RepairResult::Restored);
    }

    #[test]
    fn data_dir_flag_is_shared_with_the_uninstall_mode() {
        let abs = std::env::temp_dir();
        let args = [OsString::from("--data-dir"), abs.clone().into_os_string()];
        assert_eq!(parse_data_dir(args), Some(abs));
    }
}
