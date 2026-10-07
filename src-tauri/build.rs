fn main() {
    // Listing the app's commands generates `allow-*` permissions, so each window can be granted
    // only the commands it needs (src-tauri/capabilities/).
    let attributes =
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&[
            "lock_input",
            "unlock_input",
            "get_lock_options",
            "get_status",
            "list_devices",
            "get_overlay_session",
            "overlay_ready",
            "overlay_tick",
        ]));
    if let Err(e) = tauri_build::try_build(attributes) {
        panic!("tauri build script failed: {e:#}");
    }
}
