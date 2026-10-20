//! Windows notifications (§31, ADR 0018), through `tauri-plugin-notification`, from Rust only:
//! no window is granted the plugin's commands.
//!
//! Which toast, and when, is decided in `keyclean_core::notify`: never during a lock, never with
//! the dashboard open, never with notifications off. The text comes from the strings file, the
//! title is the product name. Toasts carry the end reason and what was locked, never key data.
//!
//! Windows shows a desktop app's toasts only under an AppUserModelID with a Start menu shortcut
//! [docs: "How to enable desktop toast notifications through an AppUserModelID"]. From
//! `target\debug` or `target\release` the plugin sends them as Windows PowerShell; an installed
//! KeyClean uses its identifier (M11's installer adds the shortcut). The plugin drops Windows'
//! answer, so a toast that doesn't appear can't be detected; the log records the request only.

use keyclean_win::keyclean_core::notify::{Toast, may_show};
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::settings::SettingsState;
use crate::{StatusStore, dashboard, guard, i18n};

/// After a lock ended (the overlay is gone by then).
pub fn after_lock(app: &AppHandle, toast: Toast) {
    show_if_allowed(app, toast, false);
}

/// A problem found with no lock running: the start shortcut or the engine.
pub fn problem(app: &AppHandle, toast: Toast) {
    let lock_active = app
        .try_state::<StatusStore>()
        .is_some_and(|store| !guard(&store.0).is_idle());
    show_if_allowed(app, toast, lock_active);
}

fn show_if_allowed(app: &AppHandle, toast: Toast, lock_active: bool) {
    // A dashboard about to appear counts as open: it will show the same thing.
    if !may_show(
        enabled(app),
        dashboard::is_open_or_opening(app),
        lock_active,
    ) {
        return;
    }
    eprintln!("[keyclean] notification: requested ({})", toast.name());
    if let Err(e) = app
        .notification()
        .builder()
        .body(i18n::t(toast.body_key()))
        .show()
    {
        eprintln!("[keyclean] notification: failed: {e}");
    }
}

/// The "Show notifications" setting. Debug builds only: `KEYCLEAN_E2E_NOTIFICATIONS=0` or `=1`
/// overrides it, so the harness neither depends on the real settings file nor fills the screen
/// with toasts (ADR 0008).
fn enabled(app: &AppHandle) -> bool {
    #[cfg(debug_assertions)]
    match std::env::var("KEYCLEAN_E2E_NOTIFICATIONS").as_deref() {
        Ok("0") => return false,
        Ok("1") => return true,
        _ => {}
    }
    app.try_state::<SettingsState>()
        .is_some_and(|settings| settings.current().notifications())
}
