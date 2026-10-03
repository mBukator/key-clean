#![forbid(unsafe_code)]
// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use keyclean_win::host;

fn main() {
    // Engine-host mode (ADR 0009) is decided before anything touches Tauri, so the engine process
    // never starts a webview or the single-instance plugin.
    let mut args = std::env::args_os().skip(1);
    if args.next().is_some_and(|first| first == host::ENGINE_FLAG) {
        let rest: Vec<String> = args.map(|a| a.to_string_lossy().into_owned()).collect();
        std::process::exit(host::run_from_args(&rest));
    }
    keyclean_lib::run();
}
