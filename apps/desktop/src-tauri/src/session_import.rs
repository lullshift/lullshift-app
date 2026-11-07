//! Import sessions (owner decision 34, ADR-024). The UI contract is
//! `apps/desktop/src/importSessions.ts`; change both together.
//!
//! - Nothing is scanned until the user opens the dialog; the scan runs on a blocking thread.
//! - Import re-reads the logs and imports only selected sessions that are still there.
//! - An imported session is stored with a "from log" mark and its status guess. It never
//!   notifies, never wakes the tray mascot and never pauses an activity; the first real hook
//!   event clears the mark (local-store).
//! - A title, and the first prompt (stored as the captured intent, never as a title), are kept
//!   only if Task context capture is on for that agent: the consent marker file exists *and*
//!   the stored setting is on (`context::capture`). Neither is ever logged.
//! - Cancel (`import_cancel`) stops a running scan; a scan that was cancelled returns
//!   `cancelled`. A panic in an earlier scan never leaves the lock "busy" for good.
//! - Errors are short codes, never raw text.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};

use agent_adapters::forward::capture_enabled;
use agent_adapters::logscan::{self, Found, Guess, Limits, LogsState, Report, Roots};
use agent_core::SessionState;
use agent_ipc::EventContext;
use local_store::{Store, StoreError};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::Core;
use crate::context::{blocking, capture};

/// Most sessions one import may name (the scan never lists more).
pub const MAX_PICKS: usize = 500;

/// One scan or import at a time.
static RUNNING: Mutex<()> = Mutex::new(());

/// The cancel flag of the scan that is running now, if any.
static CURRENT: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

/// The one-at-a-time lock. Only "another scan is running" is `busy`; a lock left poisoned by a
/// panicking scan is taken over (it guards no data), so Import never stays busy for good.
pub(crate) fn one_at_a_time() -> Result<MutexGuard<'static, ()>, &'static str> {
    match RUNNING.try_lock() {
        Ok(g) => Ok(g),
        Err(TryLockError::Poisoned(p)) => Ok(p.into_inner()),
        Err(TryLockError::WouldBlock) => Err("busy"),
    }
}

/// A fresh cancel flag for the scan about to run, replacing (and so setting free) any old one.
fn new_cancel() -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    let mut current = CURRENT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *current = Some(Arc::clone(&flag));
    flag
}

/// Scans with a cancel flag the user can set; `Err("cancelled")` if they did.
fn cancellable_scan() -> Result<Report, &'static str> {
    let cancel = new_cancel();
    let report = logscan::scan_with(
        &Roots::from_env(),
        &Limits::default(),
        OffsetDateTime::now_utc(),
        Some(cancel),
    );
    if report.cancelled {
        return Err("cancelled");
    }
    Ok(report)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundDto {
    pub provider: String,
    pub session_id: String,
    /// The last two parts of the project folder, for the row.
    pub project: Option<String>,
    /// The whole folder as the log states it, for the tooltip. Display only.
    pub project_path: Option<String>,
    /// The agent's own title, else the first prompt (display only, never stored as a title).
    pub title: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub last_activity_utc: OffsetDateTime,
    /// `working`, `waiting`, `may_need_you` or `unknown`.
    pub guess: &'static str,
    /// Lullshift already has this session ("Already in Lullshift").
    pub tracked: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDto {
    pub provider: String,
    /// `read`, `not_found`, `unreadable` or `not_local`.
    pub state: &'static str,
    pub sessions: usize,
    /// A cap, the deadline or the entry share cut this agent's list short.
    pub incomplete: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanDto {
    pub sessions: Vec<FoundDto>,
    pub agents: Vec<AgentDto>,
    pub incomplete: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Pick {
    pub provider: String,
    pub session_id: String,
}

#[derive(Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported: usize,
    pub already_tracked: usize,
    /// Picked but no longer in the logs when re-read.
    pub not_found: usize,
}

/// The last two parts of a folder path, either separator.
pub fn short_project(path: &str) -> String {
    let parts: Vec<&str> = path.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    match parts.as_slice() {
        [] => path.to_owned(),
        [one] => (*one).to_owned(),
        [.., a, b] => format!("{a}/{b}"),
    }
}

fn logs_state(s: LogsState) -> &'static str {
    match s {
        LogsState::Read => "read",
        LogsState::NotFound => "not_found",
        LogsState::Unreadable => "unreadable",
        LogsState::NotLocal => "not_local",
    }
}

pub(crate) fn state_for(guess: Guess) -> SessionState {
    match guess {
        Guess::Working => SessionState::Working,
        Guess::Waiting => SessionState::Idle,
        Guess::MayNeedYou => SessionState::NeedsAttention,
        Guess::Unknown => SessionState::Unknown,
    }
}

pub fn to_dto(report: Report, store: Option<&Store>) -> ScanDto {
    let tracked = |f: &Found| {
        store.is_some_and(|s| {
            s.session(f.provider, &f.session_id)
                .ok()
                .flatten()
                .is_some()
        })
    };
    ScanDto {
        sessions: report
            .sessions
            .iter()
            .map(|f| FoundDto {
                provider: f.provider.to_owned(),
                session_id: f.session_id.clone(),
                project: f.project_path.as_deref().map(short_project),
                project_path: f.project_path.clone(),
                title: f.title().map(str::to_owned),
                last_activity_utc: f.last_activity,
                guess: f.guess.as_str(),
                tracked: tracked(f),
            })
            .collect(),
        agents: report
            .agents
            .iter()
            .map(|a| AgentDto {
                provider: a.provider.to_owned(),
                state: logs_state(a.state),
                sessions: a.sessions,
                incomplete: a.incomplete,
            })
            .collect(),
        incomplete: report.incomplete,
    }
}

/// Checks a selection before anything is read: known agents, plain ids, at most [`MAX_PICKS`].
pub fn validate(picks: &[Pick]) -> Result<(), &'static str> {
    if picks.len() > MAX_PICKS {
        return Err("invalid_input");
    }
    for p in picks {
        if !logscan::PROVIDERS.contains(&p.provider.as_str())
            || !logscan::is_session_id(&p.session_id)
        {
            return Err("invalid_input");
        }
    }
    Ok(())
}

/// Imports the picked sessions that `report` (a fresh re-read) still holds. The agent's title,
/// and the first prompt as the captured intent, are stored only with Task context consent for
/// that agent (marker file and stored setting).
pub fn import_found(
    store: &mut Store,
    data_dir: &Path,
    report: &Report,
    picks: &[Pick],
    now: OffsetDateTime,
) -> Result<ImportResult, StoreError> {
    let mut out = ImportResult::default();
    for pick in picks {
        let Some(found) = report
            .sessions
            .iter()
            .find(|f| f.provider == pick.provider && f.session_id == pick.session_id)
        else {
            out.not_found += 1;
            continue;
        };
        if !store.import_session(
            found.provider,
            &found.session_id,
            state_for(found.guess),
            found.last_activity,
            now,
        )? {
            out.already_tracked += 1;
            continue;
        }
        out.imported += 1;
        if (found.agent_title.is_some() || found.first_prompt.is_some())
            && capture_enabled(data_dir, found.provider)
        {
            // The first prompt is what the user asked, so it is the captured intent, labelled
            // as such everywhere; only the agent's own title is a title (security review L6).
            let ctx = EventContext {
                intent: found.first_prompt.clone(),
                ask: None,
                title: found.agent_title.clone(),
            };
            // Context that can't be stored never undoes the import.
            let _ = capture(store, found.provider, &found.session_id, &ctx, now);
        }
    }
    Ok(out)
}

#[tauri::command]
pub async fn import_scan(core: tauri::State<'_, Arc<Core>>) -> Result<ScanDto, String> {
    let core = Arc::clone(&core);
    blocking(move || {
        let _one = one_at_a_time()?;
        let report = cancellable_scan()?;
        let guard = core.store.lock().map_err(|_| "internal")?;
        Ok(to_dto(report, guard.as_ref()))
    })
    .await
}

#[tauri::command]
pub async fn import_sessions(
    app: tauri::AppHandle,
    core: tauri::State<'_, Arc<Core>>,
    picks: Vec<Pick>,
) -> Result<ImportResult, String> {
    validate(&picks)?;
    let core = Arc::clone(&core);
    let result = blocking(move || {
        let _one = one_at_a_time()?;
        if picks.is_empty() {
            return Ok(ImportResult::default());
        }
        // Re-read: only sessions still in the logs, as they are now, are imported.
        let report = cancellable_scan()?;
        let mut guard = core.store.lock().map_err(|_| "internal")?;
        let store = guard.as_mut().ok_or("store_unavailable")?;
        import_found(
            store,
            &core.app_dir,
            &report,
            &picks,
            OffsetDateTime::now_utc(),
        )
        .map_err(|_| "store_unavailable".to_owned())
    })
    .await?;
    if result.imported > 0 {
        use tauri::Emitter;
        let _ = app.emit(crate::SESSIONS_CHANGED, ());
    }
    Ok(result)
}

/// Stops the scan that is running, if any (the dialog's Cancel or close). Harmless otherwise.
#[tauri::command]
pub fn import_cancel() {
    let current = CURRENT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(flag) = current.as_ref() {
        flag.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{CONTEXT_PREFS_KEY, ContextPrefs, create_markers};

    const T0: OffsetDateTime = time::macros::datetime!(2026-11-07 10:00:00 UTC);
    const ID: &str = "11111111-2222-4333-8444-555555555555";

    fn found(guess: Guess, title: Option<&str>) -> Found {
        Found {
            provider: "claude-code",
            session_id: ID.to_owned(),
            project_path: Some(r"D:\work\shop\api".to_owned()),
            agent_title: title.map(str::to_owned),
            first_prompt: None,
            last_activity: T0 - time::Duration::minutes(1),
            guess,
        }
    }

    fn report(sessions: Vec<Found>) -> Report {
        Report {
            sessions,
            agents: Vec::new(),
            incomplete: false,
            cancelled: false,
        }
    }

    fn pick(provider: &str, id: &str) -> Pick {
        Pick {
            provider: provider.to_owned(),
            session_id: id.to_owned(),
        }
    }

    fn consent(store: &Store, dir: &Path, on: bool) {
        let mut prefs = ContextPrefs::default();
        prefs.capture.insert("claude-code".into(), on);
        store
            .set_preference(
                CONTEXT_PREFS_KEY,
                &serde_json::to_string(&prefs).expect("json"),
            )
            .expect("prefs");
        if on {
            create_markers(dir, &prefs).expect("marker");
        }
    }

    #[test]
    fn projects_show_the_last_two_folders() {
        assert_eq!(short_project(r"D:\work\shop\api"), "shop/api");
        assert_eq!(short_project("/home/u/shop/"), "u/shop");
        assert_eq!(short_project("d-work-shop"), "d-work-shop");
        assert_eq!(short_project("/"), "/");
    }

    #[test]
    fn selections_are_checked_before_anything_is_read() {
        assert!(validate(&[pick("claude-code", ID), pick("opencode", "ses_1")]).is_ok());
        assert!(validate(&[pick("smoke", ID)]).is_err());
        assert!(validate(&[pick("claude-code", "../x")]).is_err());
        assert!(validate(&vec![pick("cursor", ID); MAX_PICKS + 1]).is_err());
    }

    #[test]
    fn import_stores_the_guess_marked_and_skips_tracked_and_vanished() {
        let dir = tempfile::tempdir().expect("tmp");
        let mut store = Store::open_in_memory().expect("store");
        let r = report(vec![found(Guess::MayNeedYou, Some("Login fix"))]);
        let out = import_found(
            &mut store,
            dir.path(),
            &r,
            &[pick("claude-code", ID), pick("cursor", ID)],
            T0,
        )
        .expect("import");
        assert_eq!(
            out,
            ImportResult {
                imported: 1,
                already_tracked: 0,
                not_found: 1
            }
        );
        let rec = store
            .session("claude-code", ID)
            .expect("read")
            .expect("row");
        assert_eq!(rec.state, SessionState::NeedsAttention);
        assert!(
            store
                .imported_sessions()
                .expect("marked")
                .contains_key(&("claude-code".to_owned(), ID.to_owned()))
        );
        // Imported again: already there, nothing changes.
        let again = import_found(&mut store, dir.path(), &r, &[pick("claude-code", ID)], T0)
            .expect("import");
        assert_eq!(again.already_tracked, 1);
        // The import produced no ingest outcome, so there is no notice to send, and the tray
        // ignores the session (see `face_for`).
        assert_eq!(store.event_count().expect("count"), 0);
    }

    #[test]
    fn title_is_kept_only_with_task_context_on() {
        let dir = tempfile::tempdir().expect("tmp");
        let mut store = Store::open_in_memory().expect("store");
        consent(&store, dir.path(), false);
        let r = report(vec![found(Guess::Waiting, Some("Login fix"))]);
        import_found(&mut store, dir.path(), &r, &[pick("claude-code", ID)], T0).expect("import");
        let ctx = store.context("claude-code", ID).expect("read");
        assert!(
            ctx.and_then(|c| c.agent_title).is_none(),
            "capture off: no title saved"
        );

        let dir = tempfile::tempdir().expect("tmp");
        let mut store = Store::open_in_memory().expect("store");
        consent(&store, dir.path(), true);
        import_found(&mut store, dir.path(), &r, &[pick("claude-code", ID)], T0).expect("import");
        let title = store
            .context("claude-code", ID)
            .expect("read")
            .and_then(|c| c.agent_title)
            .expect("title");
        assert_eq!(title.text, "Login fix");
    }

    #[test]
    fn the_setting_alone_or_the_marker_alone_is_not_consent() {
        let r = report(vec![found(Guess::Waiting, Some("Login fix"))]);
        // Setting on, marker missing.
        let dir = tempfile::tempdir().expect("tmp");
        let mut store = Store::open_in_memory().expect("store");
        consent(&store, dir.path(), true);
        std::fs::remove_dir_all(dir.path().join(agent_adapters::forward::CAPTURE_DIR))
            .expect("remove marker");
        import_found(&mut store, dir.path(), &r, &[pick("claude-code", ID)], T0).expect("import");
        assert!(
            store
                .context("claude-code", ID)
                .expect("read")
                .and_then(|c| c.agent_title)
                .is_none()
        );
        // Marker present, setting off.
        let dir = tempfile::tempdir().expect("tmp");
        let mut store = Store::open_in_memory().expect("store");
        let mut on = ContextPrefs::default();
        on.capture.insert("claude-code".into(), true);
        create_markers(dir.path(), &on).expect("marker");
        import_found(&mut store, dir.path(), &r, &[pick("claude-code", ID)], T0).expect("import");
        assert!(
            store
                .context("claude-code", ID)
                .expect("read")
                .and_then(|c| c.agent_title)
                .is_none()
        );
    }

    #[test]
    fn a_first_prompt_is_stored_as_the_captured_intent_never_as_a_title() {
        let dir = tempfile::tempdir().expect("tmp");
        let mut store = Store::open_in_memory().expect("store");
        consent(&store, dir.path(), true);
        let r = report(vec![Found {
            first_prompt: Some("Fix the login bug".into()),
            ..found(Guess::Waiting, None)
        }]);
        import_found(&mut store, dir.path(), &r, &[pick("claude-code", ID)], T0).expect("import");
        let ctx = store
            .context("claude-code", ID)
            .expect("read")
            .expect("context");
        assert!(ctx.agent_title.is_none());
        assert_eq!(
            ctx.captured_intent.expect("intent").text,
            "Fix the login bug"
        );
    }

    #[test]
    fn a_poisoned_lock_is_taken_over_and_cancel_is_harmless_when_idle() {
        let _ = std::thread::spawn(|| {
            let _g = RUNNING.lock();
            panic!("a scan panicked");
        })
        .join();
        assert!(one_at_a_time().is_ok(), "not busy forever");
        import_cancel();
        let flag = new_cancel();
        assert!(!flag.load(Ordering::Relaxed));
        import_cancel();
        assert!(flag.load(Ordering::Relaxed));
    }

    #[test]
    fn scan_results_mark_sessions_lullshift_already_has() {
        let mut store = Store::open_in_memory().expect("store");
        store
            .import_session("claude-code", ID, SessionState::Idle, T0, T0)
            .expect("import");
        let other = Found {
            session_id: "22222222-2222-4333-8444-555555555555".into(),
            ..found(Guess::Working, None)
        };
        let dto = to_dto(
            report(vec![found(Guess::Waiting, None), other]),
            Some(&store),
        );
        assert!(dto.sessions[0].tracked);
        assert!(!dto.sessions[1].tracked);
        assert_eq!(dto.sessions[0].project.as_deref(), Some("shop/api"));
        assert_eq!(dto.sessions[0].guess, "waiting");
    }
}
