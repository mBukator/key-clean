//! KeyClean app shell: Tauri setup, command/event bridge, windows and tray.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

// Used by the tray and notifications once they land.
#[allow(dead_code)]
mod i18n;

/// Builds and runs the Tauri application. Exits the process with code 1 if Tauri fails.
pub fn run() {
    let result = tauri::Builder::default().run(tauri::generate_context!());
    if let Err(err) = result {
        eprintln!("KeyClean failed to start: {err}");
        std::process::exit(1);
    }
}
