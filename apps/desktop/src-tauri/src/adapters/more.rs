//! Codex CLI and Cursor hook integration commands (ADR-021). Same rules as Claude Code
//! (ADR-010): install and uninstall run only from an explicit action in the `main` window, and
//! install is confirmed again in a native dialog owned by Rust.

use std::path::PathBuf;
use std::sync::Arc;

use agent_adapters::claude_code::install::{Status, UninstallOutcome};
use agent_adapters::hookfile::InstallRecord;
use agent_adapters::{codex, cursor};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use super::{
    AdapterView, CHANGE, UninstallView, blocking, disconnect_sessions, error_code, save_record_of,
    stored_record_of,
};
use crate::Core;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Agent {
    Codex,
    Cursor,
}

impl Agent {
    fn id(self) -> &'static str {
        match self {
            Self::Codex => codex::ADAPTER_ID,
            Self::Cursor => cursor::ADAPTER_ID,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Cursor => "Cursor",
        }
    }
    fn events(self) -> Vec<&'static str> {
        match self {
            Self::Codex => codex::HOOKS.iter().map(|(e, _)| *e).collect(),
            Self::Cursor => cursor::HOOKS.iter().map(|(e, _)| *e).collect(),
        }
    }
}

struct Ctx {
    agent: Agent,
    file: PathBuf,
    /// Codex only: `config.toml`, read for `[features] hooks`; never written.
    config: Option<PathBuf>,
    backups: PathBuf,
    exe: PathBuf,
    data_dir: PathBuf,
}

/// The hooks file recorded at install time if there is a record, else the current default.
fn ctx(app: &AppHandle, agent: Agent, recorded: Option<&InstallRecord>) -> Result<Ctx, String> {
    let data_dir = app.path().app_data_dir().map_err(|_| "no_data_dir")?;
    let file = match recorded {
        Some(r) => r.settings_path.clone(),
        None => match agent {
            Agent::Codex => codex::install::default_hooks_path(),
            Agent::Cursor => cursor::install::default_hooks_path(),
        }
        .ok_or("no_settings_path")?,
    };
    let exe = crate::hook_exe::hook_exe().map_err(|_| "no_executable_path")?;
    Ok(Ctx {
        agent,
        config: (agent == Agent::Codex)
            .then(codex::install::default_config_path)
            .flatten(),
        file,
        backups: data_dir.join("adapters").join(agent.id()).join("backups"),
        exe,
        data_dir,
    })
}

fn status_of(c: &Ctx) -> Status {
    match c.agent {
        Agent::Codex => {
            let cmd = codex::install::HookCommand {
                exe: c.exe.clone(),
                data_dir: c.data_dir.clone(),
            };
            codex::install::status(&c.file, c.config.as_deref(), &cmd)
        }
        Agent::Cursor => {
            let cmd = cursor::install::HookCommand {
                exe: c.exe.clone(),
                data_dir: c.data_dir.clone(),
            };
            cursor::install::status(&c.file, &cmd)
        }
    }
}

fn view(agent: Agent, c: Option<&Ctx>) -> AdapterView {
    AdapterView {
        status: c.map_or(Status::SettingsInvalid, status_of),
        settings_path: c.map(|c| c.file.to_string_lossy().into_owned()),
        events: agent.events(),
    }
}

fn do_install(
    c: &Ctx,
    previous: Option<&InstallRecord>,
) -> Result<InstallRecord, agent_adapters::claude_code::install::InstallError> {
    match c.agent {
        Agent::Codex => codex::install::install(
            &c.file,
            &c.backups,
            &codex::install::HookCommand {
                exe: c.exe.clone(),
                data_dir: c.data_dir.clone(),
            },
            previous,
        ),
        Agent::Cursor => cursor::install::install(
            &c.file,
            &c.backups,
            &cursor::install::HookCommand {
                exe: c.exe.clone(),
                data_dir: c.data_dir.clone(),
            },
            previous,
        ),
    }
}

fn do_uninstall(
    agent: Agent,
    record: &InstallRecord,
) -> Result<UninstallOutcome, agent_adapters::claude_code::install::InstallError> {
    match agent {
        Agent::Codex => codex::install::uninstall(record),
        Agent::Cursor => cursor::install::uninstall(record),
    }
}

fn install_prompt(agent: Agent, file: &std::path::Path) -> String {
    let extra = match agent {
        Agent::Codex => {
            "\n\nCodex asks you to review and trust new hooks in /hooks before they run."
        }
        Agent::Cursor => "",
    };
    format!(
        "Lullshift will add its hooks to this {} hooks file:\n\n{}\n\nA backup is saved first, and disconnecting removes the hooks again.{extra}",
        agent.name(),
        file.display()
    )
}

fn confirm_install(app: &AppHandle, c: &Ctx) -> bool {
    let mut dialog = app
        .dialog()
        .message(install_prompt(c.agent, &c.file))
        .title(format!("Connect {}?", c.agent.name()))
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

async fn status_cmd(app: AppHandle, core: Arc<Core>, agent: Agent) -> Result<AdapterView, String> {
    blocking(move || {
        let recorded: Option<InstallRecord> = stored_record_of(&core, agent.id()).ok().flatten();
        Ok(view(
            agent,
            ctx(&app, agent, recorded.as_ref()).ok().as_ref(),
        ))
    })
    .await
}

async fn install_cmd(app: AppHandle, core: Arc<Core>, agent: Agent) -> Result<AdapterView, String> {
    blocking(move || {
        let _change = CHANGE.lock().map_err(|_| "internal")?;
        // The store must be readable before anything is changed.
        let previous: Option<InstallRecord> = stored_record_of(&core, agent.id())?;
        let c = ctx(&app, agent, previous.as_ref())?;
        if !confirm_install(&app, &c) {
            return Err("cancelled".to_owned());
        }
        let record = do_install(&c, previous.as_ref()).map_err(|e| error_code(&e))?;
        if let Err(code) = save_record_of(&core, agent.id(), &record) {
            // Without a record the install can't be undone reliably: take it back out.
            return Err(match do_uninstall(agent, &record) {
                Ok(_) => code,
                Err(_) => "rollback_failed".to_owned(),
            });
        }
        Ok(view(agent, Some(&c)))
    })
    .await
}

async fn uninstall_cmd(
    app: AppHandle,
    core: Arc<Core>,
    agent: Agent,
) -> Result<UninstallView, String> {
    blocking(move || {
        let _change = CHANGE.lock().map_err(|_| "internal")?;
        let recorded: Option<InstallRecord> = stored_record_of(&core, agent.id())?;
        let c = ctx(&app, agent, recorded.as_ref())?;
        // Without a record (database lost), remove only Lullshift's own entries.
        let record = recorded.unwrap_or_else(|| InstallRecord {
            settings_path: c.file.clone(),
            backup_path: None,
            restore_exact: false,
            written_sha256: String::new(),
            created_events: agent.events().iter().map(|e| (*e).to_owned()).collect(),
            created_hooks_key: false,
            created_top_keys: Vec::new(),
        });
        let outcome = do_uninstall(agent, &record).map_err(|e| error_code(&e))?;
        let sessions_disconnected = disconnect_sessions(&core, agent.id());
        if sessions_disconnected > 0 {
            let _ = app.emit(crate::SESSIONS_CHANGED, ());
        }
        if let Some(face) = crate::current_face(&core) {
            crate::set_tray_face(&app, &core, face);
        }
        Ok(UninstallView {
            outcome,
            view: view(agent, Some(&c)),
            sessions_disconnected,
        })
    })
    .await
}

#[tauri::command]
pub async fn codex_status(
    app: AppHandle,
    core: tauri::State<'_, Arc<Core>>,
) -> Result<AdapterView, String> {
    status_cmd(app, Arc::clone(&core), Agent::Codex).await
}

#[tauri::command]
pub async fn codex_install(
    app: AppHandle,
    core: tauri::State<'_, Arc<Core>>,
) -> Result<AdapterView, String> {
    install_cmd(app, Arc::clone(&core), Agent::Codex).await
}

#[tauri::command]
pub async fn codex_uninstall(
    app: AppHandle,
    core: tauri::State<'_, Arc<Core>>,
) -> Result<UninstallView, String> {
    uninstall_cmd(app, Arc::clone(&core), Agent::Codex).await
}

#[tauri::command]
pub async fn cursor_status(
    app: AppHandle,
    core: tauri::State<'_, Arc<Core>>,
) -> Result<AdapterView, String> {
    status_cmd(app, Arc::clone(&core), Agent::Cursor).await
}

#[tauri::command]
pub async fn cursor_install(
    app: AppHandle,
    core: tauri::State<'_, Arc<Core>>,
) -> Result<AdapterView, String> {
    install_cmd(app, Arc::clone(&core), Agent::Cursor).await
}

#[tauri::command]
pub async fn cursor_uninstall(
    app: AppHandle,
    core: tauri::State<'_, Arc<Core>>,
) -> Result<UninstallView, String> {
    uninstall_cmd(app, Arc::clone(&core), Agent::Cursor).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompts_name_the_file_and_codex_mentions_trust() {
        let p = std::path::Path::new("C:/Users/me/.codex/hooks.json");
        let codex = install_prompt(Agent::Codex, p);
        assert!(codex.contains("C:/Users/me/.codex/hooks.json") && codex.contains("/hooks"));
        let cursor = install_prompt(Agent::Cursor, p);
        assert!(cursor.contains("backup") && !cursor.contains("/hooks before"));
    }
}
