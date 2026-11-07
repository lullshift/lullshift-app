//! Settling missed and silently ended sessions (owner decision 35, ADR-025). The UI contract is
//! `apps/desktop/src/sessions.ts` (`endedReason`) and `SessionTrackingSettings.tsx`.
//!
//! Two independent parts, both background work that never blocks the UI:
//!
//! 1. **Process check**, always on. The hook records the agent's process with each event
//!    (`process-probe`, first non-shell ancestor plus start key and PID namespace). At start-up
//!    and then about every 45 seconds, sessions that are not finished and hold a recorded
//!    process are checked. A session is ended with the reason `process_gone` only when all of
//!    these hold: its process is gone (or the PID now has a different start key, so it was
//!    reused) on two checks in a row; it has received nothing for [`QUIET_BEFORE_END`]; it is
//!    not waiting for the user; and the process was recorded in the app's own PID namespace
//!    (a process seen in another namespace, as in a container or Flatpak, is "unknown"). Only
//!    process *information* is read; nothing is ever signalled, and an unreadable or unknown
//!    process changes nothing. No event is written, and any later real event reopens the
//!    session (local-store).
//! 2. **Log catch-up**, off by default (a setting). At start-up, after the first process check,
//!    the Import reader (`logscan`, ADR-024: bounded windows, no links, caps, deadline, cancel)
//!    fills gaps for sessions Lullshift already tracks: the state from the log is applied only
//!    where the log is newer than Lullshift's own last word, as a marked "from log" guess. It
//!    never adds sessions (that is Import's job) and never overrides newer reported state. The
//!    shared reader parses titles and prompts in memory as part of its scan; this module keeps
//!    only the agent, session id, time and guess, and drops the rest at once.
//!
//! Errors are short codes; the outcome of the last catch-up is kept in memory for Settings.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_adapters::logscan::{self, Limits, LogsState, Report, Roots};
use local_store::ProcessKey;
use process_probe::Probe;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use time::OffsetDateTime;

use crate::Core;
use crate::context::blocking;
use crate::session_import::{one_at_a_time, state_for};

/// Stored preference key.
pub const PREFS_KEY: &str = "session_tracking_prefs";

/// How often the process check runs after start-up.
const CHECK_EVERY: Duration = Duration::from_secs(45);

// ---- settings ----------------------------------------------------------------------------

/// Settings for session tracking. Everything is off until the user turns it on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrackingPrefs {
    /// "Catch up from agent logs at startup". Off by default.
    #[serde(default)]
    pub catch_up_from_logs: bool,
}

impl TrackingPrefs {
    /// Defaults (off) for nothing stored or anything unreadable.
    pub fn from_stored(json: Option<&str>) -> Self {
        json.and_then(|j| serde_json::from_str(j).ok())
            .unwrap_or_default()
    }
}

fn load_prefs(core: &Core) -> TrackingPrefs {
    let json = core.store.lock().ok().and_then(|g| {
        g.as_ref()
            .and_then(|s| s.preference(PREFS_KEY).ok().flatten())
    });
    TrackingPrefs::from_stored(json.as_deref())
}

#[tauri::command]
pub async fn session_tracking_prefs(
    core: tauri::State<'_, Arc<Core>>,
) -> Result<TrackingPrefs, String> {
    let core = Arc::clone(&core);
    blocking(move || Ok(load_prefs(&core))).await
}

#[tauri::command]
pub async fn set_session_tracking_prefs(
    core: tauri::State<'_, Arc<Core>>,
    prefs: TrackingPrefs,
) -> Result<TrackingPrefs, String> {
    let core = Arc::clone(&core);
    blocking(move || {
        let json = serde_json::to_string(&prefs).map_err(|_| "internal")?;
        let guard = core.store.lock().map_err(|_| "internal")?;
        let store = guard.as_ref().ok_or("store_unavailable")?;
        store
            .set_preference(PREFS_KEY, &json)
            .map_err(|_| "store_unavailable")?;
        Ok(prefs)
    })
    .await
}

// ---- process check -----------------------------------------------------------------------

/// What a look at a recorded process means for its session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The same process is still running.
    Alive,
    /// No such process, or the PID now belongs to a process that started at another time (the
    /// agent exited and the number was reused).
    Gone,
    /// Could not tell: no change.
    Unknown,
}

pub fn verdict(recorded: ProcessKey, probe: Probe) -> Verdict {
    match probe {
        Probe::Gone => Verdict::Gone,
        Probe::Unknown => Verdict::Unknown,
        Probe::Running(now) if now.pid == recorded.pid && now.start == recorded.start => {
            Verdict::Alive
        }
        Probe::Running(_) => Verdict::Gone,
    }
}

/// A session must have been silent this long before the check may end it, so a wrapper process
/// that exited just after sending an event (or a session that is about to send the next one) is
/// never ended on a single look.
pub const QUIET_BEFORE_END: time::Duration = time::Duration::minutes(2);

/// Consecutive checks that must find the process gone before a session is ended.
pub const GONE_CHECKS_NEEDED: u32 = 2;

/// Remembers, between checks, which sessions' processes were found gone and how many times in a
/// row. Lives in memory only (a restart starts counting again).
#[derive(Debug, Default)]
pub struct GoneTracker {
    counts: HashMap<(String, String, u32, u64), u32>,
}

/// One process check: ends the sessions whose recorded process has stayed gone. `probe` looks
/// at one PID (read-only); `own_ns` is the app's own PID namespace. Returns how many sessions
/// were ended. Each PID is looked at once per run, however many sessions share it. The store
/// lock is held only to read the candidates and to write the result, never while processes are
/// looked at.
///
/// Skipped, and forgotten by the tracker: sessions waiting for the user (`needs_attention`,
/// never ended this way), sessions that received something less than [`QUIET_BEFORE_END`] ago,
/// processes recorded in another PID namespace (or in one when the app's is unknown), and
/// anything the look finds alive or can't tell.
pub fn process_check(
    core: &Core,
    probe: impl Fn(u32) -> Probe,
    own_ns: Option<u64>,
    tracker: &mut GoneTracker,
    now: OffsetDateTime,
) -> usize {
    let candidates = match core.store.lock() {
        Ok(guard) => guard
            .as_ref()
            .and_then(|s| s.process_candidates().ok())
            .unwrap_or_default(),
        Err(_) => return 0,
    };
    // Anything the store receives after this moment wins over what the look finds.
    let checked = OffsetDateTime::now_utc();
    let mut seen: HashMap<u32, Probe> = HashMap::new();
    let mut counts = HashMap::new();
    let mut due = Vec::new();
    for c in candidates {
        if c.state == agent_core::SessionState::NeedsAttention
            || now - c.last_received_utc < QUIET_BEFORE_END
            || c.process.ns.is_some_and(|ns| own_ns != Some(ns))
        {
            continue;
        }
        let p = *seen
            .entry(c.process.pid)
            .or_insert_with(|| probe(c.process.pid));
        if verdict(c.process, p) != Verdict::Gone {
            continue;
        }
        let key = (
            c.provider.clone(),
            c.provider_session_id.clone(),
            c.process.pid,
            c.process.start,
        );
        let n = tracker.counts.get(&key).copied().unwrap_or(0) + 1;
        counts.insert(key, n);
        if n >= GONE_CHECKS_NEEDED {
            due.push(c);
        }
    }
    // Only what was found gone just now is remembered: a session that came back, went quiet
    // of the list or changed process starts counting again.
    tracker.counts = counts;
    if due.is_empty() {
        return 0;
    }
    let Ok(mut guard) = core.store.lock() else {
        return 0;
    };
    let Some(store) = guard.as_mut() else {
        return 0;
    };
    let ended = OffsetDateTime::now_utc();
    due.iter()
        .filter(|c| {
            store
                .end_session_process_gone(
                    &c.provider,
                    &c.provider_session_id,
                    c.process,
                    checked,
                    ended,
                )
                .unwrap_or(false)
        })
        .count()
}

// ---- log catch-up ------------------------------------------------------------------------

/// How the last catch-up of this run went, for Settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatchUpReport {
    /// `not_run` (nothing yet in this run), `done`, `busy` (an import or scan was running),
    /// `cancelled` or `store_unavailable`.
    pub outcome: &'static str,
    #[serde(with = "time::serde::rfc3339::option")]
    pub finished_utc: Option<OffsetDateTime>,
    /// Sessions Lullshift already tracked whose state was filled in from a log.
    pub updated: usize,
    /// Agents whose logs exist but could not be read (or are not on a local drive).
    pub unreadable: Vec<String>,
    /// A cap or the deadline cut a list short: older sessions may not have been looked at.
    pub incomplete: bool,
}

impl CatchUpReport {
    fn of(outcome: &'static str, now: OffsetDateTime) -> Self {
        Self {
            outcome,
            finished_utc: Some(now),
            updated: 0,
            unreadable: Vec::new(),
            incomplete: false,
        }
    }
}

static LAST_CATCH_UP: Mutex<Option<CatchUpReport>> = Mutex::new(None);

fn remember(report: &CatchUpReport) {
    *LAST_CATCH_UP
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(report.clone());
}

#[tauri::command]
pub fn catch_up_status() -> CatchUpReport {
    LAST_CATCH_UP
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
        .unwrap_or(CatchUpReport {
            outcome: "not_run",
            finished_utc: None,
            updated: 0,
            unreadable: Vec::new(),
            incomplete: false,
        })
}

/// Applies a scan to the sessions Lullshift already tracks. Takes no lock of its own (see
/// [`catch_up`]). Never creates a session: `apply_log_hint` only updates existing rows.
pub fn apply_report(core: &Core, report: &Report, now: OffsetDateTime) -> CatchUpReport {
    if report.cancelled {
        return CatchUpReport::of("cancelled", now);
    }
    let Ok(mut guard) = core.store.lock() else {
        return CatchUpReport::of("store_unavailable", now);
    };
    let Some(store) = guard.as_mut() else {
        return CatchUpReport::of("store_unavailable", now);
    };
    let updated = report
        .sessions
        .iter()
        .filter(|f| {
            store
                .apply_log_hint(
                    f.provider,
                    &f.session_id,
                    state_for(f.guess),
                    f.last_activity,
                    now,
                )
                .unwrap_or(false)
        })
        .count();
    CatchUpReport {
        outcome: "done",
        finished_utc: Some(now),
        updated,
        unreadable: report
            .agents
            .iter()
            .filter(|a| matches!(a.state, LogsState::Unreadable | LogsState::NotLocal))
            .map(|a| a.provider.to_owned())
            .collect(),
        incomplete: report.incomplete,
    }
}

/// The opt-in catch-up: one scan with the Import reader, applied to tracked sessions. Shares
/// Import's one-at-a-time lock, so it is `busy` (and does nothing) if an import is running.
pub fn catch_up(core: &Core, now: OffsetDateTime, scan: impl FnOnce() -> Report) -> CatchUpReport {
    let result = match one_at_a_time() {
        Ok(_one) => apply_report(core, &scan(), now),
        Err(_) => CatchUpReport::of("busy", now),
    };
    remember(&result);
    result
}

fn scan_logs() -> Report {
    logscan::scan_with(
        &Roots::from_env(),
        &Limits::default(),
        OffsetDateTime::now_utc(),
        // Its own flag: Import's Cancel button never stops this scan, and no UI waits on it.
        Some(Arc::new(AtomicBool::new(false))),
    )
}

// ---- background worker -------------------------------------------------------------------

/// Tells the UI and the tray that sessions changed.
fn refresh(app: &AppHandle, core: &Core) {
    let _ = app.emit(crate::SESSIONS_CHANGED, ());
    if let Some(face) = crate::current_face(core) {
        crate::set_tray_face(app, core, face);
    }
}

/// Starts the background worker: a process check now, the opt-in catch-up, then a process check
/// every [`CHECK_EVERY`]. The setting is read once, at start-up. A session is ended after two
/// checks in a row, so the first end can come about 45 seconds after start-up.
pub fn spawn(app: AppHandle, core: Arc<Core>) {
    let started = std::thread::Builder::new()
        .name("settle".into())
        .spawn(move || {
            let own_ns = process_probe::pid_namespace();
            let mut tracker = GoneTracker::default();
            let check = |tracker: &mut GoneTracker| {
                process_check(
                    &core,
                    process_probe::check,
                    own_ns,
                    tracker,
                    OffsetDateTime::now_utc(),
                )
            };
            let catch_up_on = load_prefs(&core).catch_up_from_logs;
            if check(&mut tracker) > 0 {
                refresh(&app, &core);
            }
            if catch_up_on && catch_up(&core, OffsetDateTime::now_utc(), scan_logs).updated > 0 {
                refresh(&app, &core);
            }
            loop {
                std::thread::sleep(CHECK_EVERY);
                if check(&mut tracker) > 0 {
                    refresh(&app, &core);
                }
            }
        });
    if started.is_err() {
        // Once, with no task data: without the worker, sessions are not settled this run.
        eprintln!("lullshift: the session settling worker could not start");
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use agent_adapters::logscan::{AgentLogs, Found, Guess};
    use agent_core::{AgentEvent, SessionState, parse_event};
    use process_probe::ProcessId;

    use super::*;

    const P: &str = "claude-code";

    fn ev(n: u32, kind: &str, session: &str) -> AgentEvent {
        parse_event(&format!(
            r#"{{"schemaVersion":1,"eventId":"00000000-0000-4000-8000-{n:012}","provider":"{P}","providerSessionId":"{session}","eventType":"{kind}","timestampUtc":"{}"}}"#,
            now().format(&time::format_description::well_known::Rfc3339).expect("fmt")
        ))
        .expect("event")
    }

    fn now() -> OffsetDateTime {
        // Fixed second so event timestamps never run ahead of the receipt time.
        OffsetDateTime::now_utc().replace_nanosecond(0).expect("ns") - time::Duration::minutes(10)
    }

    fn core() -> Core {
        Core::for_tests(std::path::Path::new("."))
    }

    fn pk(pid: u32, start: u64) -> ProcessKey {
        ProcessKey::new(pid, start)
    }

    /// Two checks in a row, as the worker makes them, the app in no namespace.
    fn check_twice(core: &Core, probe: impl Fn(u32) -> Probe) -> usize {
        let mut tracker = GoneTracker::default();
        let first = process_check(core, &probe, None, &mut tracker, OffsetDateTime::now_utc());
        first + process_check(core, &probe, None, &mut tracker, OffsetDateTime::now_utc())
    }

    fn running(pid: u32, start: u64) -> Probe {
        Probe::Running(ProcessId { pid, start })
    }

    fn track(core: &Core, n: u32, session: &str, kind: &str, process: Option<ProcessKey>) {
        track_at(core, n, session, kind, process, now());
    }

    fn track_at(
        core: &Core,
        n: u32,
        session: &str,
        kind: &str,
        process: Option<ProcessKey>,
        received: OffsetDateTime,
    ) {
        let mut guard = core.store.lock().expect("lock");
        guard
            .as_mut()
            .expect("store")
            .ingest_with_process(&ev(n, kind, session), received, process)
            .expect("ingest");
    }

    fn state_of(core: &Core, session: &str) -> SessionState {
        let guard = core.store.lock().expect("lock");
        guard
            .as_ref()
            .expect("store")
            .session(P, session)
            .expect("read")
            .expect("row")
            .state
    }

    #[test]
    fn the_catch_up_setting_is_off_by_default_and_bad_data_reads_as_off() {
        assert!(!TrackingPrefs::default().catch_up_from_logs);
        assert!(!TrackingPrefs::from_stored(None).catch_up_from_logs);
        for bad in [
            "",
            "{",
            "[]",
            r#"{"catchUpFromLogs":"yes"}"#,
            r#"{"other":1}"#,
        ] {
            assert_eq!(
                TrackingPrefs::from_stored(Some(bad)),
                TrackingPrefs::default()
            );
        }
        let on = TrackingPrefs {
            catch_up_from_logs: true,
        };
        let json = serde_json::to_string(&on).expect("json");
        assert_eq!(json, r#"{"catchUpFromLogs":true}"#);
        assert_eq!(TrackingPrefs::from_stored(Some(&json)), on);
        assert_eq!(
            TrackingPrefs::from_stored(Some("{}")),
            TrackingPrefs::default(),
            "a missing field is off"
        );
        // A fresh store has nothing saved: off.
        assert!(!load_prefs(&core()).catch_up_from_logs);
    }

    #[test]
    fn the_setting_round_trips_through_the_store() {
        let core = core();
        let on = TrackingPrefs {
            catch_up_from_logs: true,
        };
        {
            let guard = core.store.lock().expect("lock");
            guard
                .as_ref()
                .expect("store")
                .set_preference(PREFS_KEY, &serde_json::to_string(&on).expect("json"))
                .expect("save");
        }
        assert!(load_prefs(&core).catch_up_from_logs);
    }

    #[test]
    fn verdicts_tell_a_reused_pid_from_the_same_process() {
        let rec = pk(100, 5);
        assert_eq!(verdict(rec, running(100, 5)), Verdict::Alive);
        // The number now belongs to a process that started later: the agent is gone.
        assert_eq!(verdict(rec, running(100, 6)), Verdict::Gone);
        assert_eq!(verdict(rec, Probe::Gone), Verdict::Gone);
        assert_eq!(verdict(rec, Probe::Unknown), Verdict::Unknown);
    }

    #[test]
    fn a_gone_process_ends_the_session_and_everything_else_is_left_alone() {
        let core = core();
        track(&core, 1, "gone", "working", Some(pk(100, 5)));
        track(&core, 2, "alive", "working", Some(pk(200, 5)));
        track(&core, 3, "reused", "working", Some(pk(300, 5)));
        track(&core, 4, "unknown", "working", Some(pk(400, 5)));
        track(&core, 5, "no-pid", "working", None);
        track(&core, 6, "done", "completed", Some(pk(500, 5)));
        track(&core, 7, "waiting", "needs_attention", Some(pk(600, 5)));
        track_at(
            &core,
            8,
            "just-now",
            "working",
            Some(pk(700, 5)),
            OffsetDateTime::now_utc() - time::Duration::seconds(30),
        );
        let probe = |pid: u32| match pid {
            100 | 500 | 600 | 700 => Probe::Gone,
            200 => running(200, 5),
            300 => running(300, 99),
            _ => Probe::Unknown,
        };
        let events_before = core
            .store
            .lock()
            .expect("lock")
            .as_ref()
            .expect("store")
            .event_count()
            .expect("count");
        assert_eq!(check_twice(&core, probe), 2);
        assert_eq!(state_of(&core, "gone"), SessionState::Ended);
        assert_eq!(state_of(&core, "reused"), SessionState::Ended);
        assert_eq!(state_of(&core, "alive"), SessionState::Working);
        assert_eq!(state_of(&core, "unknown"), SessionState::Working);
        assert_eq!(state_of(&core, "no-pid"), SessionState::Working);
        assert_eq!(
            state_of(&core, "done"),
            SessionState::Completed,
            "a finished session is a fact"
        );
        assert_eq!(
            state_of(&core, "waiting"),
            SessionState::NeedsAttention,
            "a session waiting for you is never ended this way"
        );
        assert_eq!(
            state_of(&core, "just-now"),
            SessionState::Working,
            "it was heard from under two minutes ago"
        );
        let guard = core.store.lock().expect("lock");
        let store = guard.as_ref().expect("store");
        assert_eq!(store.event_count().expect("count"), events_before);
        let reasons = store.ended_reasons().expect("reasons");
        assert_eq!(reasons.len(), 2);
        assert!(reasons.contains_key(&(P.to_owned(), "gone".to_owned())));
        drop(guard);
        // Nothing left to end: another run is a no-op.
        assert_eq!(check_twice(&core, probe), 0);
    }

    #[test]
    fn one_gone_look_never_ends_a_session_and_coming_back_resets_the_count() {
        let core = core();
        track(&core, 1, "a", "working", Some(pk(100, 5)));
        let mut tracker = GoneTracker::default();
        let now = OffsetDateTime::now_utc();
        assert_eq!(
            process_check(&core, |_| Probe::Gone, None, &mut tracker, now),
            0
        );
        assert_eq!(
            state_of(&core, "a"),
            SessionState::Working,
            "one look is not enough"
        );
        // Alive in between: the count starts over.
        assert_eq!(
            process_check(&core, |_| running(100, 5), None, &mut tracker, now),
            0
        );
        assert_eq!(
            process_check(&core, |_| Probe::Gone, None, &mut tracker, now),
            0
        );
        assert_eq!(state_of(&core, "a"), SessionState::Working);
        // Unknown in between also starts over.
        assert_eq!(
            process_check(&core, |_| Probe::Unknown, None, &mut tracker, now),
            0
        );
        assert_eq!(
            process_check(&core, |_| Probe::Gone, None, &mut tracker, now),
            0
        );
        assert_eq!(state_of(&core, "a"), SessionState::Working);
        // Two in a row.
        assert_eq!(
            process_check(&core, |_| Probe::Gone, None, &mut tracker, now),
            1
        );
        assert_eq!(state_of(&core, "a"), SessionState::Ended);
    }

    #[test]
    fn a_process_from_another_pid_namespace_is_unknown_and_never_ends_a_session() {
        let foreign = ProcessKey {
            ns: Some(4_026_532_000),
            ..pk(100, 5)
        };
        let same = ProcessKey {
            ns: Some(4_026_531_836),
            ..pk(200, 5)
        };
        // A fresh store per case, so one case's ends never leak into the next.
        for (own_ns, same_ends) in [(Some(4_026_531_836), true), (None, false)] {
            let core = core();
            track(&core, 1, "foreign", "working", Some(foreign));
            track(&core, 2, "same", "working", Some(same));
            let mut tracker = GoneTracker::default();
            let now = OffsetDateTime::now_utc();
            process_check(&core, |_| Probe::Gone, own_ns, &mut tracker, now);
            process_check(&core, |_| Probe::Gone, own_ns, &mut tracker, now);
            assert_eq!(
                state_of(&core, "foreign"),
                SessionState::Working,
                "{own_ns:?}"
            );
            let expected = if same_ends {
                SessionState::Ended
            } else {
                // The namespace is recorded but the app's own is unknown: no change.
                SessionState::Working
            };
            assert_eq!(state_of(&core, "same"), expected, "{own_ns:?}");
        }
    }

    #[test]
    fn a_shared_process_is_looked_at_once_per_run() {
        let core = core();
        for (n, id) in [(1, "a"), (2, "b"), (3, "c")] {
            track(&core, n, id, "working", Some(pk(100, 5)));
        }
        let looks = Cell::new(0);
        let mut tracker = GoneTracker::default();
        let now = OffsetDateTime::now_utc();
        let run = |tracker: &mut GoneTracker| {
            process_check(
                &core,
                |_| {
                    looks.set(looks.get() + 1);
                    Probe::Gone
                },
                None,
                tracker,
                now,
            )
        };
        assert_eq!((run(&mut tracker), looks.get()), (0, 1));
        assert_eq!((run(&mut tracker), looks.get()), (3, 2));
    }

    #[test]
    fn nothing_to_check_looks_at_nothing() {
        let core = core();
        let looks = Cell::new(0);
        assert_eq!(
            check_twice(&core, |_| {
                looks.set(looks.get() + 1);
                Probe::Gone
            }),
            0
        );
        assert_eq!(looks.get(), 0);
    }

    #[test]
    fn a_real_event_after_process_gone_reopens_the_session() {
        let core = core();
        track(&core, 1, "a", "working", Some(pk(100, 5)));
        assert_eq!(check_twice(&core, |_| Probe::Gone), 1);
        assert_eq!(state_of(&core, "a"), SessionState::Ended);
        // The agent was alive after all (a launcher that exited): its next event reopens it.
        let mut guard = core.store.lock().expect("lock");
        let later = OffsetDateTime::now_utc() + time::Duration::seconds(2);
        let event = parse_event(&format!(
            r#"{{"schemaVersion":1,"eventId":"00000000-0000-4000-8000-{:012}","provider":"{P}","providerSessionId":"a","eventType":"working","timestampUtc":"{}"}}"#,
            9,
            later
                .format(&time::format_description::well_known::Rfc3339)
                .expect("fmt")
        ))
        .expect("event");
        guard
            .as_mut()
            .expect("store")
            .ingest_with_process(&event, later, Some(pk(700, 8)))
            .expect("ingest");
        let rec = guard
            .as_ref()
            .expect("store")
            .session(P, "a")
            .expect("read")
            .expect("row");
        assert_eq!(rec.state, SessionState::Working);
        assert!(
            guard
                .as_ref()
                .expect("store")
                .ended_reasons()
                .expect("reasons")
                .is_empty()
        );
    }

    #[test]
    fn a_store_that_cannot_be_read_ends_nothing() {
        let core = core();
        *core.store.lock().expect("lock") = None;
        assert_eq!(check_twice(&core, |_| Probe::Gone), 0);
        let report = apply_report(&core, &Report::default(), now());
        assert_eq!(report.outcome, "store_unavailable");
    }

    // ---- catch-up ----

    fn found(id: &str, guess: Guess, last: OffsetDateTime) -> Found {
        Found {
            provider: P,
            session_id: id.to_owned(),
            project_path: Some("D:/secret/project".to_owned()),
            agent_title: Some("a private title".to_owned()),
            first_prompt: Some("a private prompt".to_owned()),
            last_activity: last,
            guess,
        }
    }

    fn report(sessions: Vec<Found>) -> Report {
        Report {
            sessions,
            agents: vec![
                AgentLogs {
                    provider: P,
                    state: LogsState::Read,
                    sessions: 0,
                    incomplete: false,
                },
                AgentLogs {
                    provider: "codex",
                    state: LogsState::Unreadable,
                    sessions: 0,
                    incomplete: false,
                },
                AgentLogs {
                    provider: "cursor",
                    state: LogsState::NotFound,
                    sessions: 0,
                    incomplete: false,
                },
            ],
            incomplete: true,
            cancelled: false,
        }
    }

    #[test]
    fn catch_up_fills_tracked_sessions_only_and_labels_them_as_a_guess() {
        let core = core();
        track(&core, 1, "tracked", "working", None);
        let log = OffsetDateTime::now_utc() - time::Duration::seconds(30);
        let out = apply_report(
            &core,
            &report(vec![
                found("tracked", Guess::Waiting, log),
                found("not-tracked", Guess::Working, log),
            ]),
            OffsetDateTime::now_utc(),
        );
        assert_eq!(out.outcome, "done");
        assert_eq!(out.updated, 1);
        assert_eq!(out.unreadable, ["codex"], "NotFound is not a failure");
        assert!(out.incomplete);
        let guard = core.store.lock().expect("lock");
        let store = guard.as_ref().expect("store");
        let rec = store.session(P, "tracked").expect("read").expect("row");
        assert_eq!(rec.state, SessionState::Idle);
        assert!(
            store
                .imported_sessions()
                .expect("marked")
                .contains_key(&(P.to_owned(), "tracked".to_owned())),
            "shown as a guess from the log"
        );
        assert!(
            store.session(P, "not-tracked").expect("read").is_none(),
            "catch-up never adds sessions"
        );
        assert_eq!(store.sessions().expect("sessions").len(), 1);
        // No text from the log was kept anywhere.
        assert!(
            store.context(P, "tracked").expect("context").is_none(),
            "no title or prompt is stored"
        );
    }

    #[test]
    fn catch_up_never_overrides_newer_reported_state() {
        let core = core();
        track(&core, 1, "a", "needs_attention", None);
        // The log was last written well before Lullshift's last event.
        let stale = now() - time::Duration::minutes(20);
        let out = apply_report(
            &core,
            &report(vec![found("a", Guess::Working, stale)]),
            OffsetDateTime::now_utc(),
        );
        assert_eq!(out.updated, 0);
        assert_eq!(state_of(&core, "a"), SessionState::NeedsAttention);
        let guard = core.store.lock().expect("lock");
        assert!(
            guard
                .as_ref()
                .expect("store")
                .imported_sessions()
                .expect("marked")
                .is_empty()
        );
    }

    #[test]
    fn catch_up_leaves_unknown_guesses_finished_and_gone_sessions_alone() {
        let core = core();
        track(&core, 1, "live", "working", None);
        track(&core, 2, "done", "completed", None);
        track(&core, 3, "gone", "working", Some(pk(100, 5)));
        assert_eq!(check_twice(&core, |_| Probe::Gone), 1);
        let log = OffsetDateTime::now_utc() - time::Duration::seconds(5);
        let out = apply_report(
            &core,
            &report(vec![
                found("live", Guess::Unknown, log),
                found("done", Guess::Waiting, log),
                found("gone", Guess::Working, log),
            ]),
            OffsetDateTime::now_utc(),
        );
        assert_eq!(out.updated, 0);
        assert_eq!(state_of(&core, "live"), SessionState::Working);
        assert_eq!(state_of(&core, "done"), SessionState::Completed);
        assert_eq!(state_of(&core, "gone"), SessionState::Ended);
    }

    #[test]
    fn a_cancelled_scan_changes_nothing() {
        let core = core();
        track(&core, 1, "a", "working", None);
        let mut r = report(vec![found(
            "a",
            Guess::Waiting,
            OffsetDateTime::now_utc() - time::Duration::seconds(5),
        )]);
        r.cancelled = true;
        assert_eq!(
            apply_report(&core, &r, OffsetDateTime::now_utc()).outcome,
            "cancelled"
        );
        assert_eq!(state_of(&core, "a"), SessionState::Working);
    }

    #[test]
    fn the_catch_up_outcome_is_remembered_for_settings() {
        let core = core();
        let out = catch_up(&core, OffsetDateTime::now_utc(), || report(vec![]));
        // Either it ran, or an import happened to be running in another test: both are shown.
        assert!(matches!(out.outcome, "done" | "busy"));
        assert_eq!(catch_up_status().outcome, out.outcome);
        assert!(catch_up_status().finished_utc.is_some());
    }
}
