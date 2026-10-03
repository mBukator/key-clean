//! KeyClean app shell: Tauri setup and the command/event bridge to the native engine.
//!
//! The engine runs in its own process (ADR 0009) and owns the lock. This shell only forwards
//! requests and displays events, so a hung, crashed or closed webview can't affect unlocking
//! (invariant 3).

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod bridge;
// Used by the tray and notifications once they land.
#[allow(dead_code)]
mod i18n;

use std::sync::mpsc::{self, Receiver};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use keyclean_win::keyclean_core::policy::SafetyProfile;
use keyclean_win::{EngineClient, EngineError, EngineEvent, LockRequest, safety_profile};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State};

use bridge::{ErrorDto, KeyboardDto, STATUS_EVENT, StatusDto};

/// Milestone 1 lock length. Debug builds are capped by the engine anyway (15 s / 20 s).
const M1_LOCK: Duration = Duration::from_secs(10);
const MAIN_WINDOW: &str = "main";
/// How long exit waits for the relay to pass on the engine's last events.
const RELAY_DRAIN: Duration = Duration::from_secs(1);

/// The engine, if it started. Taken out and dropped on exit, which releases any lock.
struct EngineSlot(Mutex<Option<EngineClient>>);

/// Signals when the event relay has passed on the engine's last event.
struct RelayDone(Mutex<Option<Receiver<()>>>);

/// The latest status, for windows that ask instead of listening.
struct StatusStore(Mutex<StatusDto>);

fn guard<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[tauri::command]
fn lock_keyboard(engine: State<'_, EngineSlot>) -> Result<(), ErrorDto> {
    match guard(&engine.0).as_ref() {
        Some(engine) => engine
            .lock(LockRequest::new(M1_LOCK))
            .map_err(|e| ErrorDto::from(&e)),
        None => Err(ErrorDto::from(&EngineError::NotRunning)),
    }
}

#[tauri::command]
fn get_status(store: State<'_, StatusStore>) -> StatusDto {
    guard(&store.0).clone()
}

#[tauri::command]
fn list_keyboards(engine: State<'_, EngineSlot>) -> Result<Vec<KeyboardDto>, ErrorDto> {
    match guard(&engine.0).as_ref() {
        Some(engine) => engine
            .devices()
            .map(|list| list.into_iter().map(KeyboardDto::from).collect())
            .map_err(|e| ErrorDto::from(&e)),
        None => Err(ErrorDto::from(&EngineError::NotRunning)),
    }
}

fn start_engine(app: &AppHandle) {
    let dev_cap = safety_profile() == SafetyProfile::Dev;
    // Managed before the engine starts, so the relay never drops an event.
    app.manage(StatusStore(Mutex::new(StatusDto::initial(
        dev_cap,
        M1_LOCK.as_secs(),
    ))));
    let set_engine = |available: bool, error: Option<ErrorDto>| {
        if let Some(store) = app.try_state::<StatusStore>() {
            guard(&store.0).set_engine(available, error);
        }
    };
    let engine = match EngineClient::start() {
        Ok((engine, events)) => {
            // Recorded before the relay starts, so an early engine failure it reports isn't
            // overwritten.
            set_engine(true, None);
            app.manage(RelayDone(Mutex::new(forward_events(app.clone(), events))));
            #[cfg(debug_assertions)]
            e2e_autolock(&engine);
            Some(engine)
        }
        Err(e) => {
            eprintln!("[keyclean] engine failed to start: {}", e.details());
            set_engine(false, Some(ErrorDto::from(&e)));
            None
        }
    };
    app.manage(EngineSlot(Mutex::new(engine)));
}

/// Debug builds only: `KEYCLEAN_E2E_AUTOLOCK=<seconds>` locks right after startup, so the
/// end-to-end harness can test the app without clicking (ADR 0008). Clamped to the dev cap;
/// release builds don't contain this.
#[cfg(debug_assertions)]
fn e2e_autolock(engine: &EngineClient) {
    let seconds = std::env::var("KEYCLEAN_E2E_AUTOLOCK")
        .ok()
        .and_then(|v| v.parse::<u64>().ok());
    if let Some(seconds) = seconds {
        let duration = Duration::from_secs(seconds.clamp(1, 15));
        if let Err(e) = engine.lock(LockRequest::new(duration)) {
            eprintln!("[keyclean] e2e autolock failed: {}", e.details());
        }
    }
}

/// Relays engine events to the main window. Ends when the engine stops; the returned receiver
/// then gets a signal (or disconnects).
fn forward_events(app: AppHandle, events: Receiver<EngineEvent>) -> Option<Receiver<()>> {
    let (done_tx, done_rx) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("keyclean-events".into())
        .spawn(move || {
            for event in events {
                // Session events only; never key data (privacy invariants).
                match &event {
                    EngineEvent::Notice(notice) => eprintln!("[keyclean] notice: {notice:?}"),
                    EngineEvent::Error(e) => eprintln!("[keyclean] error: {}", e.details()),
                    EngineEvent::SessionEnded { reason } => {
                        eprintln!("[keyclean] session ended: {reason:?}");
                    }
                    EngineEvent::Status(_) => {}
                }
                let Some(store) = app.try_state::<StatusStore>() else {
                    continue;
                };
                let snapshot = {
                    let mut status = guard(&store.0);
                    status.apply(&event);
                    if let EngineEvent::Error(e @ EngineError::EngineProcess(_)) = &event {
                        // The engine process is gone; nothing more can be locked.
                        status.set_engine(false, Some(ErrorDto::from(e)));
                    }
                    status.clone()
                };
                let _ = app.emit_to(MAIN_WINDOW, STATUS_EVENT, snapshot);
            }
            let _ = done_tx.send(());
        });
    match spawned {
        Ok(_) => Some(done_rx),
        Err(e) => {
            eprintln!("[keyclean] could not start the event relay: {e}");
            None
        }
    }
}

fn stop_engine(app: &AppHandle) {
    if let Some(slot) = app.try_state::<EngineSlot>() {
        let engine = guard(&slot.0).take();
        drop(engine); // Releases any lock and stops the engine process.
    }
    // Let the relay log how the session ended before the process exits.
    if let Some(relay) = app.try_state::<RelayDone>()
        && let Some(done) = guard(&relay.0).take()
    {
        let _ = done.recv_timeout(RELAY_DRAIN);
    }
}

fn focus_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Builds and runs the Tauri application. Exits the process with code 1 if Tauri fails.
pub fn run() {
    let app = tauri::Builder::default()
        // Must be registered first (invariant 12: single instance).
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            focus_main_window(app);
        }))
        .setup(|app| {
            start_engine(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            lock_keyboard,
            get_status,
            list_keyboards
        ])
        .build(tauri::generate_context!());

    match app {
        Ok(app) => app.run(|handle, event| {
            if let RunEvent::Exit = event {
                stop_engine(handle);
            }
        }),
        Err(err) => {
            eprintln!("KeyClean failed to start: {err}");
            std::process::exit(1);
        }
    }
}
