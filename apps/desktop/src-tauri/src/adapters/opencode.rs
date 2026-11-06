//! OpenCode plugin integration commands (EXPERIMENTAL, OpenCode 2.x only; ADR-022). Same rules as
//! the hook adapters (ADR-010, ADR-021): install and uninstall run only from an explicit action
//! in the `main` window, and install is confirmed again in a native dialog owned by Rust.
//!
//! Lullshift writes one plugin file it owns, nothing else. The status is honest about what it
//! cannot know: a correct file only means "installed"; `event_seen` turns true only after the app
//! has received an OpenCode event that arrived after the file was written.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use agent_adapters::claude_code::install::{InstallError, Status, UninstallOutcome};
use agent_adapters::hookfile::InstallRecord;
use agent_adapters::opencode::install::{self, PluginCommand};
use agent_adapters::opencode::{ADAPTER_ID, EVENTS};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use time::OffsetDateTime;

use super::{
    AdapterView, CHANGE, UninstallView, blocking, disconnect_sessions, error_code, save_record_of,
    stored_record_of,
};
use crate::Core;

struct Ctx {
    file: PathBuf,
    cmd: PluginCommand,
}

/// The plugin file recorded at install time if there is a record (OpenCode's config location can
/// change with the environment), else the current default.
fn ctx(app: &AppHandle, recorded: Option<&InstallRecord>) -> Result<Ctx, String> {
    let data_dir = app.path().app_data_dir().map_err(|_| "no_data_dir")?;
    let file = match recorded {
        Some(r) => r.settings_path.clone(),
        None => install::default_plugin_path().ok_or("no_settings_path")?,
    };
    let exe = crate::hook_exe::hook_exe().map_err(|_| "no_executable_path")?;
    Ok(Ctx {
        file,
        cmd: PluginCommand { exe, data_dir },
    })
}

/// Whether an OpenCode event reached the app after the plugin file was last written. `None`
/// when that can't be told (no store, no file time): shown as unknown, never as connected.
fn event_seen(core: &Core, file: &Path) -> Option<bool> {
    let written: OffsetDateTime = std::fs::metadata(file)
        .and_then(|m| m.modified())
        .ok()
        .map(|t: SystemTime| t.into())?;
    let guard = core.store.lock().ok()?;
    let store = guard.as_ref()?;
    let last = store.last_received_from(ADAPTER_ID).ok()?;
    Some(last.is_some_and(|l| l >= written))
}

fn view(core: &Core, c: Option<&Ctx>) -> AdapterView {
    let status = c.map_or(Status::SettingsInvalid, |c| {
        install::status(&c.file, &c.cmd)
    });
    let seen = match (c, status) {
        (Some(c), Status::Installed) => Some(event_seen(core, &c.file).unwrap_or(false)),
        _ => None,
    };
    AdapterView {
        status,
        settings_path: c.map(|c| c.file.to_string_lossy().into_owned()),
        events: EVENTS.iter().map(|(e, _)| *e).collect(),
        experimental: true,
        event_seen: seen,
    }
}

fn install_prompt(file: &Path) -> String {
    format!(
        "EXPERIMENTAL: this works only with OpenCode 2.x.\n\nLullshift will create this one plugin file, which it owns:\n\n{}\n\nNothing else is changed, and an existing file that Lullshift didn't write is never replaced. OpenCode loads the plugin the next time it starts. Disconnecting deletes the file.",
        file.display()
    )
}

fn confirm_install(app: &AppHandle, c: &Ctx) -> bool {
    let mut dialog = app
        .dialog()
        .message(install_prompt(&c.file))
        .title("Connect OpenCode (experimental)?")
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

fn fallback_record(file: &Path) -> InstallRecord {
    InstallRecord {
        settings_path: file.to_path_buf(),
        backup_path: None,
        restore_exact: false,
        written_sha256: String::new(),
        created_events: Vec::new(),
        created_hooks_key: false,
        created_top_keys: Vec::new(),
    }
}

#[tauri::command]
pub async fn opencode_status(
    app: AppHandle,
    core: tauri::State<'_, Arc<Core>>,
) -> Result<AdapterView, String> {
    let core = Arc::clone(&core);
    blocking(move || {
        let recorded: Option<InstallRecord> = stored_record_of(&core, ADAPTER_ID).ok().flatten();
        Ok(view(&core, ctx(&app, recorded.as_ref()).ok().as_ref()))
    })
    .await
}

#[tauri::command]
pub async fn opencode_install(
    app: AppHandle,
    core: tauri::State<'_, Arc<Core>>,
) -> Result<AdapterView, String> {
    let core = Arc::clone(&core);
    blocking(move || {
        let _change = CHANGE.lock().map_err(|_| "internal")?;
        // The store must be readable before anything is changed.
        let previous: Option<InstallRecord> = stored_record_of(&core, ADAPTER_ID)?;
        let c = ctx(&app, previous.as_ref())?;
        if !confirm_install(&app, &c) {
            return Err("cancelled".to_owned());
        }
        let record = install::install(&c.file, &c.cmd).map_err(|e| error_code(&e))?;
        if let Err(code) = save_record_of(&core, ADAPTER_ID, &record) {
            // Without a record the install can't be undone reliably: take it back out.
            return Err(match install::uninstall(&record) {
                Ok(_) => code,
                Err(_) => "rollback_failed".to_owned(),
            });
        }
        Ok(view(&core, Some(&c)))
    })
    .await
}

#[tauri::command]
pub async fn opencode_uninstall(
    app: AppHandle,
    core: tauri::State<'_, Arc<Core>>,
) -> Result<UninstallView, String> {
    let core = Arc::clone(&core);
    blocking(move || {
        let _change = CHANGE.lock().map_err(|_| "internal")?;
        let recorded: Option<InstallRecord> = stored_record_of(&core, ADAPTER_ID)?;
        let c = ctx(&app, recorded.as_ref())?;
        // Without a record (database lost), the file is still removed if it is Lullshift's.
        let record = recorded.unwrap_or_else(|| fallback_record(&c.file));
        let outcome: UninstallOutcome =
            install::uninstall(&record).map_err(|e: InstallError| error_code(&e))?;
        let sessions_disconnected = disconnect_sessions(&core, ADAPTER_ID);
        if sessions_disconnected > 0 {
            let _ = app.emit(crate::SESSIONS_CHANGED, ());
        }
        if let Some(face) = crate::current_face(&core) {
            crate::set_tray_face(&app, &core, face);
        }
        Ok(UninstallView {
            outcome,
            view: view(&core, Some(&c)),
            sessions_disconnected,
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_names_the_file_and_the_experiment() {
        let text = install_prompt(Path::new(
            "C:/Users/me/.config/opencode/plugins/lullshift-observer.js",
        ));
        assert!(text.contains("lullshift-observer.js"));
        assert!(text.contains("EXPERIMENTAL") && text.contains("OpenCode 2.x"));
        assert!(text.contains("never replaced"));
    }
}
