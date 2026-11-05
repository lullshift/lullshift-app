//! Lullshift desktop shell: tray lifecycle, local store and authenticated agent IPC (Sprint 01).
//!
//! Closing the window hides it; monitoring continues until "Quit" in the tray menu.

mod adapters;

use std::sync::{Arc, Mutex};

use agent_core::reducer::DEFAULT_STALE_AFTER;
use agent_core::{AgentEvent, SessionState, SessionView, view};
use agent_ipc::{Config, Running, ServeError, SinkError};
use local_store::{IngestOutcome, Store, StoreError};
use serde::Serialize;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_notification::NotificationExt;
use time::OffsetDateTime;

/// Launch argument used by autostart: start in the tray without showing the window.
const HIDDEN_ARG: &str = "--hidden";
const DB_FILE: &str = "lullshift.db";
const SESSIONS_CHANGED: &str = "sessions-changed";

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
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Dashboard {
    sessions: Vec<SessionView>,
    store: Health,
    ipc: Health,
    stale_after_seconds: i64,
    #[serde(with = "time::serde::rfc3339")]
    generated_utc: OffsetDateTime,
}

/// Current sessions with their derived (possibly unknown/stale) display state. Runs on a
/// blocking thread: it shares the store lock with event ingestion and must not stall the UI.
#[tauri::command]
async fn dashboard(core: tauri::State<'_, Arc<Core>>) -> Result<Dashboard, String> {
    let core = Arc::clone(&core);
    tauri::async_runtime::spawn_blocking(move || build_dashboard(&core))
        .await
        .map_err(|_| "internal".to_owned())
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
                Ok(records) => (
                    records
                        .iter()
                        .map(|r| view(r, core.started_utc, now, DEFAULT_STALE_AFTER))
                        .collect(),
                    core.store_health,
                ),
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
    let sink = move |event: AgentEvent| -> Result<bool, SinkError> {
        let received = OffsetDateTime::now_utc();
        let outcome = {
            let mut guard = sink_core.store.lock().map_err(|_| SinkError)?;
            let store = guard.as_mut().ok_or(SinkError)?;
            store.ingest(&event, received).map_err(|_| SinkError)?
        };
        let new = matches!(outcome, IngestOutcome::Stored { .. });
        if new {
            let _ = sink_app.emit(SESSIONS_CHANGED, ());
        }
        let notice = attention_notice(&outcome).filter(|_| match &outcome {
            IngestOutcome::Stored { record, .. } => gate.lock().is_ok_and(|mut g| {
                g.allow(&record.provider, &record.provider_session_id, received)
            }),
            _ => false,
        });
        if let Some(body) = notice {
            let _ = sink_app
                .notification()
                .builder()
                .title("Lullshift")
                .body(body)
                .show();
        }
        Ok(new)
    };
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

/// Notification text when a session has just moved into `needs_attention`, else `None`.
/// Duplicates and repeats of the same state never notify again (S02-A02). The text names
/// only the agent, never task content.
fn attention_notice(outcome: &IngestOutcome) -> Option<&'static str> {
    match outcome {
        IngestOutcome::Stored {
            state_changed: true,
            record,
        } if record.state == SessionState::NeedsAttention => Some(match record.provider.as_str() {
            "claude-code" => "A Claude Code session needs you.",
            _ => "An agent session needs you.",
        }),
        _ => None,
    }
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
    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("Lullshift")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
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
        .setup(|app| {
            let handle = app.handle().clone();
            let (store, store_health) = open_store(&handle);
            let core = Arc::new(Core {
                store: Mutex::new(store),
                store_health,
                ipc_health: Mutex::new(Health::Error("starting")),
                ipc: Mutex::new(None),
                started_utc: OffsetDateTime::now_utc(),
            });
            let ipc_health = if store_health == Health::Ok {
                start_ipc(&handle, &core)
            } else {
                // Without storage, accepting events would silently drop them.
                Health::Error("store_unavailable")
            };
            if let Ok(mut h) = core.ipc_health.lock() {
                *h = ipc_health;
            }
            app.manage(core);
            build_tray(app)?;
            if !std::env::args().any(|a| a == HIDDEN_ARG) {
                show_main(&handle);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Keep monitoring in the tray; quitting is explicit.
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            dashboard,
            adapters::claude_code_status,
            adapters::claude_code_install,
            adapters::claude_code_uninstall
        ])
        .run(tauri::generate_context!())
        .expect("error while running Lullshift");
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::SessionRecord;

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

    #[test]
    fn notifies_only_on_change_into_attention() {
        assert!(attention_notice(&stored(true, SessionState::NeedsAttention)).is_some());
        assert!(
            attention_notice(&stored(false, SessionState::NeedsAttention)).is_none(),
            "repeat"
        );
        assert!(
            attention_notice(&IngestOutcome::Duplicate).is_none(),
            "replayed eventId"
        );
        assert!(attention_notice(&stored(true, SessionState::Idle)).is_none());
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
