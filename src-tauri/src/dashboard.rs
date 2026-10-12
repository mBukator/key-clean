//! The dashboard window: opened on demand, destroyed when closed (ADR 0016).
//!
//! At idle with the dashboard closed, KeyClean has no webview at all (§30). The window's size and
//! minimum size stay in `tauri.conf.json` with `"create": false`, so Tauri doesn't build it at
//! startup; [`open`] builds it from that config.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::MAIN_WINDOW;

/// Event that asks an open dashboard to show a tab.
const OPEN_TAB_EVENT: &str = "open-tab";

/// The dashboard tabs the app can ask for. Matches `TabId` in `src/app/App.tsx`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Settings,
}

impl Tab {
    fn id(self) -> &'static str {
        match self {
            Tab::Settings => "settings",
        }
    }
}

/// Set while the dashboard is being built, so a second request doesn't build another.
pub struct Building(AtomicBool);

impl Building {
    pub fn new() -> Self {
        Building(AtomicBool::new(false))
    }
}

/// Whether the dashboard window exists.
pub fn is_open(app: &AppHandle) -> bool {
    app.get_webview_window(MAIN_WINDOW).is_some()
}

/// Brings the dashboard to the front if it is open. Does nothing otherwise.
pub fn focus(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Opens the dashboard, or brings it to the front if it is open.
pub fn open(app: &AppHandle) {
    open_with(app, None);
}

/// Opens the dashboard on `tab`, or switches an open one to it and brings it to the front.
pub fn open_on(app: &AppHandle, tab: Tab) {
    open_with(app, Some(tab));
}

///
/// Always builds from an async task: building a window in an event handler (the tray menu, the
/// single-instance callback) or a synchronous command deadlocks on Windows [docs: tauri 2.12.1
/// `WebviewWindowBuilder::new`]. Safe to call from any thread.
fn open_with(app: &AppHandle, tab: Option<Tab>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if is_open(&app) {
            if let Some(tab) = tab {
                let _ = app.emit_to(MAIN_WINDOW, OPEN_TAB_EVENT, tab.id());
            }
            focus(&app);
            return;
        }
        let Some(building) = app.try_state::<Building>() else {
            return;
        };
        if building.0.swap(true, Ordering::SeqCst) {
            return;
        }
        let built = build(&app, tab);
        building.0.store(false, Ordering::SeqCst);
        if let Err(e) = built {
            eprintln!("[keyclean] couldn't open the dashboard: {e}");
        }
    });
}

fn build(app: &AppHandle, tab: Option<Tab>) -> tauri::Result<()> {
    let Some(mut config) = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == MAIN_WINDOW)
        .cloned()
    else {
        return Err(tauri::Error::WindowNotFound);
    };
    if let Some(tab) = tab {
        // The page reads its first tab from the query.
        config.url = WebviewUrl::App(format!("index.html?tab={}", tab.id()).into());
    }
    let window = WebviewWindowBuilder::from_config(app, &config)?.build()?;
    // Closing the dashboard sends KeyClean to the tray, but `taskkill` must still exit it. Set on
    // the main thread, which owns the window, once it is built; `guard_close` runs again each
    // time it gains focus, in case this one came too early.
    app.run_on_main_thread(|| {
        let guarded = keyclean_win::guard_dashboard_close();
        eprintln!("[keyclean] dashboard: close guard on {guarded} window(s)");
    })?;
    let _ = window.set_focus();
    Ok(())
}

/// Sets the dashboard's close guard again; it is idempotent. Main thread only (the app's
/// `RunEvent` handler, when the dashboard gains focus). Logs only a failure.
pub fn guard_close() {
    if keyclean_win::guard_dashboard_close() == 0 {
        eprintln!("[keyclean] dashboard: close guard found no window on focus");
    }
}
