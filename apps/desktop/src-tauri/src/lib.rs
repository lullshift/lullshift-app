use agent_core::{SessionSnapshot, mock};
use time::OffsetDateTime;

/// Sprint 00 mock agent feed. Replaced by real session state in Sprints 01-02.
#[tauri::command]
fn mock_sessions() -> Vec<SessionSnapshot> {
    mock::sample_sessions(OffsetDateTime::now_utc())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![mock_sessions])
        .run(tauri::generate_context!())
        .expect("error while running Lullshift");
}
