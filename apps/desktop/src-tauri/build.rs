fn main() {
    // Register app commands with Tauri's ACL so each one must be granted in a capability file.
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "dashboard",
            "claude_code_status",
            "claude_code_install",
            "claude_code_uninstall",
            "notification_prefs",
            "set_notification_prefs",
            "activities_list",
            "activity_info",
            "activity_storage_get",
            "activity_storage_set",
            "activity_storage_remove",
            "activity_snapshot",
            "activity_save_snapshot",
            "activity_clear_snapshot",
        ]),
    ))
    .expect("failed to run tauri-build");
}
