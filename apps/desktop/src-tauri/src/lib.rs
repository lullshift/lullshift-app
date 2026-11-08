//! Lullshift desktop shell: tray lifecycle, local store and authenticated agent IPC (Sprint 01).
//!
//! Closing the window hides it; monitoring continues until "Quit" in the tray menu.

#[cfg(test)]
mod acl_check;
mod activities;
mod adapters;
mod companion_commands;
mod context;
mod hook_exe;
mod market;
mod notify;
mod packaged;
pub mod repair_hooks;
mod session_import;
mod settle;
mod tray_window;
pub mod uninstall_hooks;
mod waits;
mod webrtc;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use agent_core::reducer::DEFAULT_STALE_AFTER;
use agent_core::{AgentEvent, SessionRecord, SessionState, SessionView, view};
use agent_ipc::{Config, EventContext, Running, ServeError, SinkError};
use local_store::{IngestOutcome, Store, StoreError};
use notify::{Notice, NotificationPrefs};
use serde::Serialize;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_notification::NotificationExt;
use time::OffsetDateTime;

/// Launch argument used by autostart: start in the tray without showing the window.
const HIDDEN_ARG: &str = "--hidden";
const DB_FILE: &str = "lullshift.db";
const SESSIONS_CHANGED: &str = "sessions-changed";
/// Emitted when a session changes into `needs_attention`; payload is the provider ID.
const ATTENTION: &str = "agent-attention";
const TRAY_ID: &str = "main";

/// The tray mascot's face. Asleep: nothing needs you. Awake: at least one session is in
/// `needs_attention` and fresh. Stale or unconfirmed sessions show as unknown and never wake it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayFace {
    Asleep,
    Awake,
}

impl TrayFace {
    fn of<'a>(sessions: impl IntoIterator<Item = &'a SessionView>) -> Self {
        if sessions
            .into_iter()
            .any(|s| s.state == SessionState::NeedsAttention)
        {
            Self::Awake
        } else {
            Self::Asleep
        }
    }

    /// macOS gets one-color template images that follow the menu bar's light/dark mode;
    /// Windows and Linux get the teal version, which reads on both light and dark taskbars.
    fn image(self) -> Option<Image<'static>> {
        #[cfg(target_os = "macos")]
        let bytes: &[u8] = match self {
            Self::Asleep => include_bytes!("../icons/tray/tray-asleep-template.png"),
            Self::Awake => include_bytes!("../icons/tray/tray-awake-template.png"),
        };
        #[cfg(not(target_os = "macos"))]
        let bytes: &[u8] = match self {
            Self::Asleep => include_bytes!("../icons/tray/tray-asleep.png"),
            Self::Awake => include_bytes!("../icons/tray/tray-awake.png"),
        };
        Image::from_bytes(bytes).ok()
    }

    fn tooltip(self) -> &'static str {
        match self {
            Self::Asleep => "Lullshift",
            Self::Awake => "Lullshift: an agent needs you",
        }
    }
}

/// Health of one subsystem as shown in the UI. Codes are fixed strings, never raw errors.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(tag = "status", content = "code", rename_all = "snake_case")]
enum Health {
    Ok,
    Error(&'static str),
}

struct Core {
    store: Mutex<Option<Store>>,
    store_health: Health,
    ipc_health: Mutex<Health>,
    ipc: Mutex<Option<Running>>,
    started_utc: OffsetDateTime,
    tray_face: Mutex<Option<TrayFace>>,
    /// `<app data>`: the marketplace cache (`marketplaces/`) and installs (`installed/`) live here.
    app_dir: PathBuf,
    /// What the activity library shows and serves, and the install-verification cache.
    library: activities::installed::Library,
    /// Live launch keys and request budgets of the `lullact` scheme (ADR-012).
    launches: activities::launch::Launches,
    /// Storage write budget per activity (ADR-012).
    storage_writes: activities::launch::WriteBudget,
    /// Marketplace operations in flight and pending install reviews.
    market: market::MarketState,
    /// Companion (BYOK): keychain, HTTP limits and in-flight sends.
    companion: companion_commands::CompanionState,
}

#[cfg(test)]
impl Core {
    /// A core over an in-memory store and `app_dir`, for tests.
    fn for_tests(app_dir: &std::path::Path) -> Self {
        Self {
            store: Mutex::new(Some(Store::open_in_memory().expect("store"))),
            store_health: Health::Ok,
            ipc_health: Mutex::new(Health::Error("starting")),
            ipc: Mutex::new(None),
            started_utc: OffsetDateTime::now_utc(),
            tray_face: Mutex::new(None),
            app_dir: app_dir.to_path_buf(),
            library: activities::installed::Library::default(),
            launches: activities::launch::Launches::default(),
            storage_writes: activities::launch::WriteBudget::default(),
            market: market::MarketState::default(),
            companion: companion_commands::CompanionState::with(
                std::sync::Arc::new(::companion::MemoryKeyStore::new()),
                ::companion::HttpConfig::default(),
            ),
        }
    }
}

/// Shows `face` in the tray unless it is already showing. Failures leave the previous icon.
fn set_tray_face(app: &AppHandle, core: &Core, face: TrayFace) {
    let Ok(mut current) = core.tray_face.lock() else {
        return;
    };
    if *current == Some(face) {
        return;
    }
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    if let Some(image) = face.image() {
        if tray.set_icon(Some(image)).is_err() {
            return;
        }
        #[cfg(target_os = "macos")]
        let _ = tray.set_icon_as_template(true);
    }
    let _ = tray.set_tooltip(Some(face.tooltip()));
    *current = Some(face);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Dashboard {
    sessions: Vec<context::DashboardSession>,
    store: Health,
    ipc: Health,
    stale_after_seconds: i64,
    #[serde(with = "time::serde::rfc3339")]
    generated_utc: OffsetDateTime,
}

/// Current sessions with their derived (possibly unknown/stale) display state. Runs on a
/// blocking thread: it shares the store lock with event ingestion and must not stall the UI.
#[tauri::command]
async fn dashboard(app: AppHandle, core: tauri::State<'_, Arc<Core>>) -> Result<Dashboard, String> {
    let core = Arc::clone(&core);
    tauri::async_runtime::spawn_blocking(move || {
        let dashboard = build_dashboard(&core);
        // Refreshing here also lets a needs-you session that has since gone stale put the
        // mascot back to sleep without waiting for the next event.
        if dashboard.store == Health::Ok {
            set_tray_face(
                &app,
                &core,
                TrayFace::of(
                    dashboard
                        .sessions
                        .iter()
                        .filter(|s| s.disconnected_utc.is_none() && s.imported_utc.is_none())
                        .map(|s| &s.view),
                ),
            );
        }
        dashboard
    })
    .await
    .map_err(|_| "internal".to_owned())
}

/// Tray face for the stored sessions right now, or `None` if the store can't be read.
fn current_face(core: &Core) -> Option<TrayFace> {
    let now = OffsetDateTime::now_utc();
    let guard = core.store.lock().ok()?;
    let store = guard.as_ref()?;
    let records = store.sessions().ok()?;
    let mut untracked = store.disconnected_sessions().unwrap_or_default();
    // Imported sessions hold only a guess from a log and never wake it either (ADR-024).
    untracked.extend(store.imported_sessions().unwrap_or_default());
    Some(face_for(&records, &untracked, core.started_utc, now))
}

/// The mascot's face for these sessions. Sessions in `disconnected` (a disconnected tool,
/// S05-T10, or imported from a log and not heard from since, ADR-024) never wake it.
fn face_for(
    records: &[SessionRecord],
    disconnected: &std::collections::HashMap<(String, String), OffsetDateTime>,
    started: OffsetDateTime,
    now: OffsetDateTime,
) -> TrayFace {
    let views: Vec<SessionView> = records
        .iter()
        .filter(|r| {
            !disconnected.contains_key(&(r.provider.clone(), r.provider_session_id.clone()))
        })
        .map(|r| view(r, started, now, DEFAULT_STALE_AFTER))
        .collect();
    TrayFace::of(&views)
}

fn build_dashboard(core: &Core) -> Dashboard {
    let now = OffsetDateTime::now_utc();
    let mut ipc = core
        .ipc_health
        .lock()
        .map_or(Health::Error("internal"), |h| *h);
    if ipc == Health::Ok {
        let accepting = core
            .ipc
            .lock()
            .is_ok_and(|r| r.as_ref().is_some_and(Running::is_accepting));
        if !accepting {
            ipc = Health::Error("stopped");
        }
    }
    let (sessions, store) = match core.store.lock() {
        Ok(guard) => match guard.as_ref() {
            Some(s) => match s.sessions() {
                Ok(records) => {
                    // A failure here only hides the review flag; the dashboard still works.
                    let lookups = context::DashboardLookups::load(s);
                    (
                        records
                            .iter()
                            .map(|r| {
                                let v = view(r, core.started_utc, now, DEFAULT_STALE_AFTER);
                                context::dashboard_session(v, r, &lookups)
                            })
                            .collect(),
                        core.store_health,
                    )
                }
                Err(_) => (Vec::new(), Health::Error("read_failed")),
            },
            None => (Vec::new(), core.store_health),
        },
        Err(_) => (Vec::new(), Health::Error("internal")),
    };
    Dashboard {
        sessions,
        store,
        ipc,
        stale_after_seconds: DEFAULT_STALE_AFTER.whole_seconds(),
        generated_utc: now,
    }
}

fn open_store(app: &AppHandle) -> (Option<Store>, Health) {
    let Ok(dir) = app.path().app_data_dir() else {
        return (None, Health::Error("no_data_dir"));
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return (None, Health::Error("no_data_dir"));
    }
    match Store::open(&dir.join(DB_FILE)) {
        Ok((store, _report)) => (Some(store), Health::Ok),
        Err(StoreError::TooNew { .. }) => (None, Health::Error("too_new")),
        Err(StoreError::Migration { .. }) => (None, Health::Error("migration_failed")),
        Err(_) => (None, Health::Error("unavailable")),
    }
}

fn start_ipc(app: &AppHandle, core: &Arc<Core>) -> Health {
    let Ok(dir) = app.path().app_data_dir() else {
        return Health::Error("no_data_dir");
    };
    let sink_core = Arc::clone(core);
    let sink_app = app.clone();
    let gate = std::sync::Mutex::new(AttentionGate::default());
    let sink = agent_ipc::WithProcess(
        move |event: AgentEvent,
              task_text: Option<EventContext>,
              process: Option<agent_ipc::ProcessRef>|
              -> Result<bool, SinkError> {
            let received = OffsetDateTime::now_utc();
            let process = process.map(|p| local_store::ProcessKey {
                pid: p.pid,
                start: p.start,
                ns: p.ns,
            });
            let outcome = {
                let mut guard = sink_core.store.lock().map_err(|_| SinkError)?;
                let store = guard.as_mut().ok_or(SinkError)?;
                context::ingest_with_process(store, &event, task_text, process, received)
                    .map_err(|_| SinkError)?
            };
            let new = matches!(outcome, IngestOutcome::Stored { .. });
            if new {
                let _ = sink_app.emit(SESSIONS_CHANGED, ());
                if let Some(face) = current_face(&sink_core) {
                    set_tray_face(&sink_app, &sink_core, face);
                }
            }
            if let Some((notice, record)) = change_notice(&outcome) {
                if notice == Notice::NeedsYou {
                    // The activity host pauses on this whatever the notification settings say.
                    let _ = sink_app.emit(ATTENTION, &record.provider);
                }
                let first = notice != Notice::NeedsYou
                    || gate.lock().is_ok_and(|mut g| {
                        g.allow(&record.provider, &record.provider_session_id, received)
                    });
                let prefs = load_prefs(&sink_core);
                if first && notify::should_notify(&prefs, notice, local_minutes()) {
                    let _ = sink_app
                        .notification()
                        .builder()
                        .title("Lullshift")
                        .body(notice.text(&record.provider))
                        .show();
                }
            }
            Ok(new)
        },
    );
    let result = tauri::async_runtime::block_on(async move {
        agent_ipc::serve(&dir, Arc::new(sink), Config::default())
    });
    match result {
        Ok(running) => {
            if let Ok(mut slot) = core.ipc.lock() {
                *slot = Some(running);
            }
            Health::Ok
        }
        Err(ServeError::AddressInUse) => Health::Error("address_in_use"),
        Err(ServeError::Io(_)) => Health::Error("unavailable"),
    }
}

/// The notice for a session that has just changed state, if that change is one worth telling
/// the user about. Duplicates and repeats of the same state never notify again (S02-A02).
fn change_notice(outcome: &IngestOutcome) -> Option<(Notice, &SessionRecord)> {
    match outcome {
        IngestOutcome::Stored {
            state_changed: true,
            record,
        } => Notice::for_change(record.state).map(|n| (n, record)),
        _ => None,
    }
}

/// Saved notification preferences; defaults if none are saved or the store is unavailable.
fn load_prefs(core: &Core) -> NotificationPrefs {
    let json = core.store.lock().ok().and_then(|g| {
        g.as_ref()
            .and_then(|s| s.preference(notify::PREFS_KEY).ok().flatten())
    });
    NotificationPrefs::from_stored(json.as_deref())
}

/// Local wall-clock minute of the day, for quiet hours.
fn local_minutes() -> u16 {
    use chrono::Timelike;
    let now = chrono::Local::now();
    u16::try_from(now.hour() * 60 + now.minute()).unwrap_or(0)
}

#[tauri::command]
async fn notification_prefs(
    core: tauri::State<'_, Arc<Core>>,
) -> Result<NotificationPrefs, String> {
    let core = Arc::clone(&core);
    tauri::async_runtime::spawn_blocking(move || load_prefs(&core))
        .await
        .map_err(|_| "internal".to_owned())
}

#[tauri::command]
async fn set_notification_prefs(
    core: tauri::State<'_, Arc<Core>>,
    prefs: NotificationPrefs,
) -> Result<NotificationPrefs, String> {
    prefs.validate()?;
    let core = Arc::clone(&core);
    tauri::async_runtime::spawn_blocking(move || {
        let json = serde_json::to_string(&prefs).map_err(|_| "internal")?;
        let guard = core.store.lock().map_err(|_| "internal")?;
        let store = guard.as_ref().ok_or("store_unavailable")?;
        store
            .set_preference(notify::PREFS_KEY, &json)
            .map_err(|_| "store_unavailable")?;
        Ok(prefs)
    })
    .await
    .map_err(|_| "internal".to_owned())?
}

/// Hooks run as separate async processes, so a `working` event can land between a permission
/// request and Claude Code's own permission notification a few seconds later. Both would move
/// the session into `needs_attention`; this keeps that to one notification per session within
/// [`AttentionGate::WINDOW`] (S02-A02). The dashboard still shows every state change.
#[derive(Default)]
struct AttentionGate {
    last: std::collections::HashMap<(String, String), OffsetDateTime>,
}

impl AttentionGate {
    const WINDOW: time::Duration = time::Duration::seconds(15);

    fn allow(&mut self, provider: &str, session: &str, now: OffsetDateTime) -> bool {
        self.last.retain(|_, t| now - *t < Self::WINDOW);
        let key = (provider.to_owned(), session.to_owned());
        if self.last.contains_key(&key) {
            return false;
        }
        self.last.insert(key, now);
        true
    }
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show Lullshift", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Lullshift", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(TrayFace::Asleep.tooltip())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // Left click: the quick window or the main window (Settings > Appearance). Some Linux
            // tray backends send no click events; the menu above is the way in there.
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                position,
                rect,
                ..
            } = event
            {
                tray_window::on_left_click(tray.app_handle(), position, rect);
            }
        });
    if let Some(icon) = TrayFace::Asleep
        .image()
        .or_else(|| app.default_window_icon().cloned())
    {
        tray = tray.icon(icon);
    }
    #[cfg(target_os = "macos")]
    {
        tray = tray.icon_as_template(true);
    }
    tray.build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be first: a second launch focuses the running instance and exits.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app)
        }))
        // Off until the user turns it on in the app.
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .arg(HIDDEN_ARG)
                .build(),
        )
        .plugin(tauri_plugin_notification::init())
        // Used from Rust only (install confirmation); no capability grants its JS commands.
        .plugin(tauri_plugin_dialog::init())
        // Bundled activities, framed by the UI in a sandboxed iframe (ADR-012).
        // Asynchronous: serving may read and hash a file (up to 5 MiB), which must not run on the
        // webview's thread. Floods are limited by the per-activity budget in `serve`.
        .register_asynchronous_uri_scheme_protocol(activities::SCHEME, |ctx, request, responder| {
            // Only the main window frames activities; the tray window never does.
            if ctx.webview_label() != "main" {
                responder.respond(activities::forbidden());
                return;
            }
            let core: Option<Arc<Core>> = ctx
                .app_handle()
                .try_state::<Arc<Core>>()
                .map(|s| Arc::clone(&s));
            tauri::async_runtime::spawn_blocking(move || {
                responder.respond(activities::serve(core.as_deref(), &request));
            });
        })
        .setup(|app| {
            // Before anything can open an activity: WebRTC off in the webview (ADR-012).
            for label in ["main", tray_window::LABEL] {
                if let Some(w) = app.get_webview_window(label) {
                    webrtc::disable(&w);
                }
            }
            let handle = app.handle().clone();
            let (store, store_health) = open_store(&handle);
            let core = Arc::new(Core {
                store: Mutex::new(store),
                store_health,
                ipc_health: Mutex::new(Health::Error("starting")),
                ipc: Mutex::new(None),
                started_utc: OffsetDateTime::now_utc(),
                tray_face: Mutex::new(None),
                app_dir: handle.path().app_data_dir().unwrap_or_default(),
                library: activities::installed::Library::default(),
                launches: activities::launch::Launches::default(),
                storage_writes: activities::launch::WriteBudget::default(),
                market: market::MarketState::default(),
                companion: companion_commands::CompanionState::new(),
            });
            tray_window::set_quick(context::read_ui_prefs(&core).tray_click);
            companion_commands::recover_pending(&core);
            let ipc_health = if store_health == Health::Ok {
                start_ipc(&handle, &core)
            } else {
                // Without storage, accepting events would silently drop them.
                Health::Error("store_unavailable")
            };
            if let Ok(mut h) = core.ipc_health.lock() {
                *h = ipc_health;
            }
            build_tray(app)?;
            if let Some(face) = current_face(&core) {
                set_tray_face(&handle, &core, face);
            }
            if let Ok(dir) = handle.path().app_data_dir() {
                context::reconcile_markers(&core, &dir);
            }
            context::spawn_retention(Arc::clone(&core));
            settle::spawn(handle.clone(), Arc::clone(&core));
            app.manage(core);
            if !std::env::args().any(|a| a == HIDDEN_ARG) {
                show_main(&handle);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    // Keep monitoring in the tray; quitting is explicit.
                    api.prevent_close();
                    let _ = window.hide();
                }
                // The quick window closes when you click away from it.
                WindowEvent::Focused(false) => tray_window::on_blur(window),
                // ... and when the main window takes focus.
                WindowEvent::Focused(true) => tray_window::on_focus(window),
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            dashboard,
            waits::wait_estimates,
            tray_window::open_main,
            tray_window::tray_hide,
            adapters::claude_code_status,
            adapters::claude_code_install,
            adapters::claude_code_uninstall,
            adapters::more::codex_status,
            adapters::more::codex_install,
            adapters::more::codex_uninstall,
            adapters::more::cursor_status,
            adapters::more::cursor_install,
            adapters::more::cursor_uninstall,
            adapters::opencode::opencode_status,
            adapters::opencode::opencode_install,
            adapters::opencode::opencode_uninstall,
            packaged::packaged_context,
            packaged::connection_health,
            packaged::connection_fix_stale,
            notification_prefs,
            set_notification_prefs,
            settle::session_tracking_prefs,
            settle::set_session_tracking_prefs,
            settle::catch_up_status,
            activities::activities_list,
            activities::activities_manage_list,
            activities::activity_info,
            activities::activity_launch,
            activities::activity_launch_end,
            activities::activity_storage_get,
            activities::activity_storage_set,
            activities::activity_storage_remove,
            activities::activity_snapshot,
            activities::activity_save_snapshot,
            activities::activity_clear_snapshot,
            activities::activity_preview,
            activities::activity_add_play,
            activities::activity_content,
            market::marketplace_list,
            market::marketplace_git_backend,
            market::marketplace_add,
            market::marketplace_remove,
            market::marketplace_refresh,
            market::marketplace_change_address,
            market::marketplace_entries,
            market::marketplace_preview_install,
            market::marketplace_install,
            market::marketplace_preview_update,
            market::marketplace_update,
            market::activity_uninstall,
            market::activity_set_enabled,
            context::session_detail,
            context::notes_overview,
            context::notes_open_count,
            context::set_session_notes,
            context::set_session_intent,
            context::set_session_title,
            context::add_session_item,
            context::update_session_item,
            context::delete_session_item,
            context::move_session_item,
            context::link_sessions,
            context::unlink_sessions,
            context::mark_reviewed,
            context::clear_captured_context,
            context::clear_all_captured_context,
            context::export_session,
            context::delete_session,
            context::context_prefs,
            context::set_context_prefs,
            context::ui_prefs,
            context::set_ui_prefs,
            companion_commands::companion_profiles,
            companion_commands::companion_save_profile,
            companion_commands::companion_delete_profile,
            companion_commands::companion_set_key,
            companion_commands::companion_clear_key,
            companion_commands::companion_test,
            companion_commands::companion_add_model,
            companion_commands::companion_remove_model,
            companion_commands::companion_set_default_model,
            companion_commands::companion_open_link,
            companion_commands::companion_model_catalog,
            companion_commands::companion_catalog_models,
            companion_commands::companion_threads,
            companion_commands::companion_new_thread,
            companion_commands::companion_rename_thread,
            companion_commands::companion_delete_thread,
            companion_commands::companion_messages,
            companion_commands::companion_preview_context,
            companion_commands::companion_send,
            companion_commands::companion_retry,
            companion_commands::companion_cancel,
            companion_commands::companion_usage,
            companion_commands::companion_prefs,
            companion_commands::companion_set_tools_enabled,
            companion_commands::companion_tool_decision,
            // Import sessions (decision 34).
            session_import::import_scan,
            session_import::import_sessions,
            session_import::import_cancel
        ])
        .run(tauri::generate_context!())
        .expect("error while running Lullshift");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every capability file shipped with the app (S03-T07, ADR-010).
    fn capabilities() -> Vec<(String, serde_json::Value)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities");
        std::fs::read_dir(dir)
            .expect("capabilities dir")
            .map(|e| e.expect("entry").path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .map(|p| {
                let text = std::fs::read_to_string(&p).expect("read capability");
                (
                    p.display().to_string(),
                    serde_json::from_str(&text).expect("capability JSON"),
                )
            })
            .collect()
    }

    #[test]
    fn only_the_main_window_may_change_agent_settings() {
        const SENSITIVE: &[&str] = &[
            "allow-companion-set-tools-enabled",
            "allow-companion-tool-decision",
            "allow-companion-set-key",
            "allow-companion-clear-key",
            "allow-companion-send",
            "allow-companion-save-profile",
            "allow-companion-delete-profile",
            "allow-companion-add-model",
            "allow-companion-remove-model",
            "allow-companion-set-default-model",
            "allow-companion-open-link",
            "allow-claude-code-install",
            "allow-claude-code-uninstall",
            "allow-codex-install",
            "allow-codex-uninstall",
            "allow-cursor-install",
            "allow-cursor-uninstall",
            "allow-opencode-install",
            "allow-opencode-uninstall",
            "allow-marketplace-add",
            "allow-marketplace-remove",
            "allow-marketplace-change-address",
            "allow-marketplace-install",
            "allow-marketplace-update",
            "allow-activity-uninstall",
            "allow-activity-set-enabled",
            "allow-connection-fix-stale",
        ];
        let mut granted = 0;
        for (file, cap) in capabilities() {
            let perms = cap["permissions"].as_array().expect("permissions array");
            let has = perms
                .iter()
                .filter_map(|p| p.as_str().or_else(|| p["identifier"].as_str()))
                .any(|p| SENSITIVE.contains(&p));
            if !has {
                continue;
            }
            granted += 1;
            assert_eq!(
                cap["windows"],
                serde_json::json!(["main"]),
                "{file}: windows"
            );
            assert!(
                cap.get("webviews").is_none(),
                "{file}: must not target webviews"
            );
            assert!(
                cap.get("remote").is_none(),
                "{file}: must not allow remote origins"
            );
        }
        assert!(granted > 0, "the main window still needs these permissions");
    }

    #[test]
    fn no_capability_exposes_dialogs_to_scripts() {
        for (file, cap) in capabilities() {
            let text = cap["permissions"].to_string();
            assert!(!text.contains("dialog:"), "{file}: dialogs are Rust-only");
        }
    }

    fn stored(changed: bool, state: SessionState) -> IngestOutcome {
        let t = OffsetDateTime::UNIX_EPOCH;
        IngestOutcome::Stored {
            state_changed: changed,
            record: SessionRecord {
                provider: "claude-code".into(),
                provider_session_id: "s".into(),
                state,
                state_timestamp_utc: Some(t),
                last_seen_utc: t,
                last_received_utc: t,
            },
        }
    }

    fn session(state: SessionState) -> SessionView {
        let t = OffsetDateTime::UNIX_EPOCH;
        SessionView {
            provider: "claude-code".into(),
            provider_session_id: "s".into(),
            state,
            reason: agent_core::ViewReason::Observed,
            last_known_state: state,
            last_seen_utc: t,
            last_received_utc: t,
        }
    }

    #[test]
    fn disconnected_sessions_never_wake_the_mascot() {
        let t = OffsetDateTime::now_utc();
        let record = |id: &str| SessionRecord {
            provider: "claude-code".into(),
            provider_session_id: id.into(),
            state: SessionState::NeedsAttention,
            state_timestamp_utc: Some(t),
            last_seen_utc: t,
            last_received_utc: t,
        };
        let started = t - time::Duration::minutes(1);
        let records = [record("a"), record("b")];
        let mut gone = std::collections::HashMap::new();
        assert_eq!(face_for(&records, &gone, started, t), TrayFace::Awake);
        gone.insert(("claude-code".to_owned(), "a".to_owned()), t);
        assert_eq!(
            face_for(&records, &gone, started, t),
            TrayFace::Awake,
            "b still needs you"
        );
        gone.insert(("claude-code".to_owned(), "b".to_owned()), t);
        assert_eq!(face_for(&records, &gone, started, t), TrayFace::Asleep);
    }

    #[test]
    fn imported_sessions_never_wake_the_mascot() {
        let t = OffsetDateTime::now_utc();
        let mut store = Store::open_in_memory().expect("store");
        store
            .import_session("claude-code", "a", SessionState::NeedsAttention, t, t)
            .expect("import");
        let records = store.sessions().expect("sessions");
        let mut untracked = store.disconnected_sessions().expect("disconnected");
        untracked.extend(store.imported_sessions().expect("imported"));
        let started = t - time::Duration::minutes(1);
        assert_eq!(
            face_for(&records, &untracked, started, t),
            TrayFace::Asleep,
            "a guessed 'may need you' is not a needs-you"
        );
        // The first real hook event takes over: now it may wake the mascot (and notify).
        let event = agent_core::parse_event(
            r#"{"schemaVersion":1,"eventId":"00000000-0000-4000-8000-000000000001","provider":"claude-code","providerSessionId":"a","eventType":"needs_attention","timestampUtc":"2026-11-07T10:00:00Z"}"#,
        )
        .expect("event");
        let outcome = store.ingest(&event, t).expect("ingest");
        assert_eq!(
            change_notice(&outcome).map(|(n, _)| n),
            Some(Notice::NeedsYou)
        );
        let records = store.sessions().expect("sessions");
        assert!(store.imported_sessions().expect("imported").is_empty());
        assert_eq!(
            face_for(&records, &Default::default(), started, t),
            TrayFace::Awake
        );
    }

    #[test]
    fn tray_wakes_only_while_a_session_needs_you() {
        assert_eq!(TrayFace::of(&[]), TrayFace::Asleep);
        assert_eq!(
            TrayFace::of(&[session(SessionState::Working), session(SessionState::Idle)]),
            TrayFace::Asleep
        );
        assert_eq!(
            TrayFace::of(&[
                session(SessionState::Working),
                session(SessionState::NeedsAttention)
            ]),
            TrayFace::Awake
        );
        // A stale needs-you session is shown as unknown and must not keep the mascot awake.
        assert_eq!(
            TrayFace::of(&[session(SessionState::Unknown)]),
            TrayFace::Asleep
        );
    }

    #[test]
    fn bundled_tray_images_decode() {
        for face in [TrayFace::Asleep, TrayFace::Awake] {
            let image = face.image().expect("tray PNG decodes");
            assert_eq!((image.width(), image.height()), (32, 32));
        }
    }

    #[test]
    fn notifies_only_on_change_into_a_notable_state() {
        let kind = |o: &IngestOutcome| change_notice(o).map(|(n, _)| n);
        assert_eq!(
            kind(&stored(true, SessionState::NeedsAttention)),
            Some(Notice::NeedsYou)
        );
        assert_eq!(
            kind(&stored(true, SessionState::Failed)),
            Some(Notice::Failed)
        );
        assert_eq!(
            kind(&stored(false, SessionState::NeedsAttention)),
            None,
            "repeat"
        );
        assert_eq!(kind(&IngestOutcome::Duplicate), None, "replayed eventId");
        // A turn ending leaves the session idle: never shown as completion (S03-A02).
        assert_eq!(kind(&stored(true, SessionState::Idle)), None);
    }

    #[test]
    fn one_notification_per_session_within_the_window() {
        let t = time::macros::datetime!(2026-11-05 10:00:00 UTC);
        let s = time::Duration::seconds;
        let mut gate = AttentionGate::default();
        assert!(gate.allow("claude-code", "a", t), "permission request");
        assert!(
            !gate.allow("claude-code", "a", t + s(6)),
            "permission notification after a reordered working event"
        );
        assert!(gate.allow("claude-code", "b", t + s(6)), "other session");
        assert!(gate.allow("claude-code", "a", t + s(30)), "a later prompt");
    }
}
