fn main() {
    // Register app commands with Tauri's ACL so each one must be granted in a capability file.
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(&["dashboard"])),
    )
    .expect("failed to run tauri-build");
}
