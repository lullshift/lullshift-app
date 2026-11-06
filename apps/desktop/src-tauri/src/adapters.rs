//! Claude Code hook integration commands (ADR-010). Install and uninstall run only from an
//! explicit user action in the UI, which shows the exact file and events first. Install is then
//! confirmed again in a native dialog owned by Rust, so a script running in a webview can't
//! change the user's agent settings on its own (S03-T07).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use agent_adapters::claude_code::install::{
    self, HookCommand, InstallError, InstallRecord, Status, UninstallOutcome,
};
use agent_adapters::claude_code::{ADAPTER_ID, HOOKS};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use time::OffsetDateTime;

use crate::Core;

pub(crate) mod more;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterView {
    status: Status,
    settings_path: Option<String>,
    events: Vec<&'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallView {
    outcome: UninstallOutcome,
    view: AdapterView,
    /// How many open sessions stopped being tracked because the tool was disconnected
    /// (S05-T10). Zero when there were none.
    sessions_disconnected: usize,
}

struct Ctx {
    settings: PathBuf,
    backups: PathBuf,
    cmd: HookCommand,
}

/// Install and uninstall change a user file and the record together; never run two at once
/// (the commands can be invoked concurrently even when the UI disables its buttons).
static CHANGE: Mutex<()> = Mutex::new(());

/// The settings file to act on: the one recorded at install time if there is a record (the
/// app's environment, e.g. `CLAUDE_CONFIG_DIR`, may differ from launch to launch), else the
/// current default.
fn ctx(app: &AppHandle, recorded: Option<&InstallRecord>) -> Result<Ctx, String> {
    let data_dir = app.path().app_data_dir().map_err(|_| "no_data_dir")?;
    let settings = match recorded {
        Some(r) => r.settings_path.clone(),
        None => install::default_settings_path().ok_or("no_settings_path")?,
    };
    let exe = crate::hook_exe::hook_exe().map_err(|_| "no_executable_path")?;
    Ok(Ctx {
        settings,
        backups: data_dir.join("adapters").join(ADAPTER_ID).join("backups"),
        cmd: HookCommand { exe, data_dir },
    })
}

fn view(ctx: Option<&Ctx>) -> AdapterView {
    AdapterView {
        status: ctx.map_or(Status::SettingsInvalid, |c| {
            install::status(&c.settings, &c.cmd)
        }),
        settings_path: ctx.map(|c| c.settings.to_string_lossy().into_owned()),
        events: HOOKS.iter().map(|(e, _)| *e).collect(),
    }
}

fn error_code(e: &InstallError) -> String {
    match e {
        InstallError::SettingsInvalid => "settings_invalid",
        InstallError::NotWritable => "settings_not_writable",
        InstallError::UnsupportedPath => "unsupported_path",
        InstallError::ChangedConcurrently => "changed_concurrently",
        InstallError::Io(_) => "file_error",
    }
    .to_owned()
}

fn stored_record_of<R: serde::de::DeserializeOwned>(
    core: &Core,
    id: &str,
) -> Result<Option<R>, String> {
    let guard = core.store.lock().map_err(|_| "internal")?;
    let store = guard.as_ref().ok_or("store_unavailable")?;
    let json = store.adapter_install(id).map_err(|_| "store_unavailable")?;
    Ok(json.and_then(|j| serde_json::from_str(&j).ok()))
}

fn stored_record(core: &Core) -> Result<Option<InstallRecord>, String> {
    stored_record_of(core, ADAPTER_ID)
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|_| "internal".to_owned())?
}

#[tauri::command]
pub async fn claude_code_status(
    app: AppHandle,
    core: tauri::State<'_, Arc<Core>>,
) -> Result<AdapterView, String> {
    let core = Arc::clone(&core);
    blocking(move || {
        let recorded = stored_record(&core).ok().flatten();
        Ok(view(ctx(&app, recorded.as_ref()).ok().as_ref()))
    })
    .await
}

fn save_record_of<R: Serialize>(core: &Core, id: &str, record: &R) -> Result<(), String> {
    let json = serde_json::to_string(record).map_err(|_| "internal")?;
    let guard = core.store.lock().map_err(|_| "internal")?;
    let store = guard.as_ref().ok_or("store_unavailable")?;
    store
        .save_adapter_install(id, &json, OffsetDateTime::now_utc())
        .map_err(|_| "store_unavailable".to_owned())
}

fn save_record(core: &Core, record: &InstallRecord) -> Result<(), String> {
    save_record_of(core, ADAPTER_ID, record)
}

/// Installs or updates the hooks. Called only after the user confirmed the consent dialog.
#[tauri::command]
pub async fn claude_code_install(
    app: AppHandle,
    core: tauri::State<'_, Arc<Core>>,
) -> Result<AdapterView, String> {
    let core = Arc::clone(&core);
    blocking(move || {
        let _change = CHANGE.lock().map_err(|_| "internal")?;
        // The store must be readable before anything is changed.
        let previous = stored_record(&core)?;
        let c = ctx(&app, previous.as_ref())?;
        if !confirm_install(&app, &c) {
            return Err("cancelled".to_owned());
        }
        let record = install::install(&c.settings, &c.backups, &c.cmd, previous.as_ref())
            .map_err(|e| error_code(&e))?;
        if let Err(code) = save_record(&core, &record) {
            // Without a record the install can't be undone reliably: take it back out.
            return Err(match install::uninstall(&record) {
                Ok(_) => code,
                Err(_) => "rollback_failed".to_owned(),
            });
        }
        Ok(view(Some(&c)))
    })
    .await
}

/// Native confirmation naming the exact file. Blocks this worker thread until the user answers;
/// closing the dialog counts as "no".
fn confirm_install(app: &AppHandle, c: &Ctx) -> bool {
    let mut dialog = app
        .dialog()
        .message(install_prompt(&c.settings))
        .title("Connect Claude Code?")
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Connect".to_owned(),
            "Cancel".to_owned(),
        ));
    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.parent(&window);
    }
    dialog.blocking_show()
}

fn install_prompt(settings: &std::path::Path) -> String {
    format!(
        "Lullshift will add its hooks to this Claude Code settings file:

{}

         A backup is saved first, and disconnecting removes the hooks again.",
        settings.display()
    )
}

/// Marks the adapter uninstalled and ends tracking of its open sessions. A store problem is
/// not an uninstall failure: the hooks are already gone. Returns the number of sessions.
fn disconnect_sessions(core: &Core, id: &str) -> usize {
    let Ok(mut guard) = core.store.lock() else {
        return 0;
    };
    let Some(store) = guard.as_mut() else {
        return 0;
    };
    let _ = store.mark_adapter_uninstalled(id);
    store
        .disconnect_provider(id, OffsetDateTime::now_utc())
        .unwrap_or(0)
}

#[tauri::command]
pub async fn claude_code_uninstall(
    app: AppHandle,
    core: tauri::State<'_, Arc<Core>>,
) -> Result<UninstallView, String> {
    let core = Arc::clone(&core);
    blocking(move || {
        let _change = CHANGE.lock().map_err(|_| "internal")?;
        let recorded = stored_record(&core)?;
        let c = ctx(&app, recorded.as_ref())?;
        // Without a record (database lost), remove only Lullshift's own entries.
        let record = recorded.unwrap_or_else(|| InstallRecord {
            settings_path: c.settings.clone(),
            backup_path: None,
            restore_exact: false,
            written_sha256: String::new(),
            created_events: HOOKS.iter().map(|(e, _)| (*e).to_owned()).collect(),
            created_hooks_key: false,
        });
        let outcome = install::uninstall(&record).map_err(|e| error_code(&e))?;
        // Disconnecting ends tracking of the tool's open sessions at once, in one transaction.
        let sessions_disconnected = disconnect_sessions(&core, ADAPTER_ID);
        if sessions_disconnected > 0 {
            let _ = app.emit(crate::SESSIONS_CHANGED, ());
        }
        // The mascot may have been awake for a session that is no longer tracked.
        if let Some(face) = crate::current_face(&core) {
            crate::set_tray_face(&app, &core, face);
        }
        Ok(UninstallView {
            outcome,
            view: view(Some(&c)),
            sessions_disconnected,
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::install_prompt;

    #[test]
    fn install_prompt_names_the_exact_file() {
        let path = std::path::Path::new("C:/Users/me/.claude/settings.json");
        let text = install_prompt(path);
        assert!(text.contains("C:/Users/me/.claude/settings.json"));
        assert!(text.contains("backup"));
    }
}
