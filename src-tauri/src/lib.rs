//! KeyClean app shell: Tauri setup and the command/event bridge to the native engine.
//!
//! The engine runs in its own process (ADR 0009) and owns the lock. This shell only forwards
//! requests and displays events, so a hung, crashed or closed webview can't affect unlocking
//! (invariant 3).

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod bridge;
mod devices;
// Partly used until the tray and notifications land.
#[allow(dead_code)]
mod i18n;
mod overlay;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use keyclean_win::SessionState;
use keyclean_win::keyclean_core::policy::SafetyProfile;
use keyclean_win::keyclean_core::presets;
use keyclean_win::keyclean_core::restart::RestartBudget;
use keyclean_win::keyclean_core::time::MonoTime;
use keyclean_win::{
    EngineClient, EngineError, EngineEvent, LockRequest, LockTargets, safety_profile,
};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WindowEvent};

use bridge::{DevicesDto, ErrorDto, LockOptionsDto, STATUS_EVENT, StatusDto};
use devices::DeviceStore;
use overlay::OverlayState;
const MAIN_WINDOW: &str = "main";
/// How long exit waits for the relay to pass on the engine's last events.
const RELAY_DRAIN: Duration = Duration::from_secs(1);

/// The engine, if it started. Taken out and dropped on exit, which releases any lock.
struct EngineSlot(Mutex<Option<EngineClient>>);

/// Signals when the event relay has passed on the engine's last event.
struct RelayDone(Mutex<Option<Receiver<()>>>);

/// The latest status, for windows that ask instead of listening.
struct StatusStore(Mutex<StatusDto>);

/// Set once the app starts exiting, so an engine that dies then isn't restarted.
struct Exiting(AtomicBool);

/// Recent engine restarts, on a monotonic clock that starts with the app.
struct Restarts {
    budget: Mutex<RestartBudget>,
    origin: Instant,
}

impl Restarts {
    /// Whether the engine may be restarted now; records the restart if so.
    fn allow(&self) -> bool {
        let now = MonoTime::from_since_origin(self.origin.elapsed());
        guard(&self.budget).allow(now)
    }
}

fn guard<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Locks the keyboard, the mouse, or both for `seconds`, which must be one of the presets. Shows
/// the overlay first and locks only once it is confirmed visible (invariant 11). The engine still
/// applies the safety policy (max lock, hard deadline, debug dev cap) and rejects a lock that
/// blocks nothing. Async because building a window in a synchronous command deadlocks on Windows.
#[tauri::command]
async fn lock_input(
    app: AppHandle,
    seconds: u64,
    keyboard: bool,
    mouse: bool,
) -> Result<(), ErrorDto> {
    let duration = Duration::from_secs(seconds);
    if !presets::is_preset(duration) {
        return Err(ErrorDto::not_a_preset(seconds));
    }
    let request = LockRequest::new(duration).with_targets(LockTargets { keyboard, mouse });
    overlay::lock(app, request).await
}

/// Asks the engine to end the current lock now (`UserRequest`).
#[tauri::command]
fn unlock_input(engine: State<'_, EngineSlot>) -> Result<(), ErrorDto> {
    match guard(&engine.0).as_ref() {
        Some(engine) => engine.unlock().map_err(|e| ErrorDto::from(&e)),
        None => Err(ErrorDto::from(&EngineError::NotRunning)),
    }
}

#[tauri::command]
fn get_lock_options() -> LockOptionsDto {
    LockOptionsDto::current()
}

#[tauri::command]
fn get_status(store: State<'_, StatusStore>) -> StatusDto {
    guard(&store.0).clone()
}

/// The current device list. Updates arrive as `devices-changed` events; a window subscribes first,
/// then calls this, so it can't miss one.
#[tauri::command]
fn list_devices(devices: State<'_, DeviceStore>) -> DevicesDto {
    devices.snapshot()
}

fn start_engine(app: &AppHandle) {
    let dev_cap = safety_profile() == SafetyProfile::Dev;
    // Managed before the engine starts, so the relay never drops an event.
    app.manage(StatusStore(Mutex::new(StatusDto::initial(dev_cap))));
    app.manage(Exiting(AtomicBool::new(false)));
    app.manage(Restarts {
        budget: Mutex::new(RestartBudget::new()),
        origin: Instant::now(),
    });
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

/// Debug builds only: `KEYCLEAN_E2E_AUTOLOCK=<seconds>` locks the keyboard right after startup,
/// so the end-to-end harness can test the app without clicking (ADR 0008). Goes through the
/// overlay like every lock (invariant 11). Clamped to the dev cap; release builds don't contain
/// this.
///
/// `KEYCLEAN_E2E_AUTOLOCK_REPEAT=<n>` (1-20, default 1) locks `n` times in a row, each once the
/// previous lock has ended and a second has passed, so the harness can measure the overlay with
/// WebView2 already warm.
#[cfg(debug_assertions)]
fn e2e_autolock(app: &AppHandle) {
    let seconds = std::env::var("KEYCLEAN_E2E_AUTOLOCK")
        .ok()
        .and_then(|v| v.parse::<u64>().ok());
    let repeat = std::env::var("KEYCLEAN_E2E_AUTOLOCK_REPEAT")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .map_or(1, |n| n.clamp(1, 20));
    let Some(seconds) = seconds else {
        return;
    };
    let duration = Duration::from_secs(seconds.clamp(1, 15));
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        for n in 0..repeat {
            if n > 0 {
                // Debug-only polling, between harness locks.
                let idle = || {
                    app.try_state::<StatusStore>()
                        .is_some_and(|store| guard(&store.0).is_idle())
                };
                pause(Duration::from_secs(1)).await;
                let waited = Instant::now();
                while !idle() {
                    if waited.elapsed() > Duration::from_secs(30) {
                        eprintln!("[keyclean] e2e autolock stopped: the lock didn't end");
                        return;
                    }
                    pause(Duration::from_millis(200)).await;
                }
                pause(Duration::from_secs(1)).await;
            }
            if let Err(e) = overlay::lock(app.clone(), LockRequest::new(duration)).await {
                eprintln!("[keyclean] e2e autolock failed: {}", e.details);
            }
        }
    });
}

#[cfg(debug_assertions)]
async fn pause(d: Duration) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(d)).await;
}

/// Relays engine events to the main window. Ends when the engine stops; if the engine process
/// died, tries to restart it first. The returned receiver then gets a signal (or disconnects).
fn forward_events(app: AppHandle, events: Receiver<EngineEvent>) -> Option<Receiver<()>> {
    let (done_tx, done_rx) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("keyclean-events".into())
        .spawn(move || {
            let mut engine_died = false;
            for event in events {
                // Only reported when the process ended without the app asking.
                engine_died |= matches!(event, EngineEvent::Error(EngineError::EngineProcess(_)));
                // Session events only; never key data (privacy invariants).
                match &event {
                    EngineEvent::Notice(notice) => eprintln!("[keyclean] notice: {notice:?}"),
                    EngineEvent::DeviceChanged(change) => {
                        eprintln!("[keyclean] device: {change:?}");
                    }
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
                let idle = snapshot.is_idle();
                // To every window: the main window and the overlay.
                let _ = app.emit(STATUS_EVENT, snapshot);
                follow_overlay(&app, &event, idle);
            }
            if engine_died {
                restart_engine(&app);
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

/// Keeps the overlay in step with the session: checked on every countdown status during a lock,
/// closed once the lock ends or a request is refused.
fn follow_overlay(app: &AppHandle, event: &EngineEvent, idle: bool) {
    match event {
        EngineEvent::Status(status) => match status.state {
            SessionState::Starting | SessionState::Locked => overlay::check_soon(app),
            SessionState::Unlocking | SessionState::Idle => overlay::finish(app),
        },
        // A dead engine released the lock.
        EngineEvent::Error(EngineError::EngineProcess(_)) => overlay::finish(app),
        // A refused request ends with one of these and no status. Other errors either come during
        // a lock (a status follows) or don't concern the lock request.
        EngineEvent::Error(EngineError::AlreadyActive | EngineError::InvalidRequest(_)) if idle => {
            overlay::finish(app);
        }
        _ => {}
    }
}

fn is_exiting(app: &AppHandle) -> bool {
    app.try_state::<Exiting>()
        .is_some_and(|e| e.0.load(Ordering::SeqCst))
}

fn update_status(app: &AppHandle, update: impl FnOnce(&mut StatusDto)) {
    if let Some(store) = app.try_state::<StatusStore>() {
        let snapshot = {
            let mut status = guard(&store.0);
            update(&mut status);
            status.clone()
        };
        let _ = app.emit(STATUS_EVENT, snapshot);
    }
}

/// Replaces an engine process that died with a new one, within the restart budget. The new
/// engine is idle: a lock is never resumed. Runs on the relay thread of the dead engine and never
/// holds the slot lock while an engine starts or stops.
fn restart_engine(app: &AppHandle) {
    if is_exiting(app) {
        return;
    }
    let allowed = app.try_state::<Restarts>().is_some_and(|r| r.allow());
    if !allowed {
        eprintln!("[keyclean] engine not restarted: too many restarts");
        return;
    }
    let Some(slot) = app.try_state::<EngineSlot>() else {
        return;
    };
    let dead = guard(&slot.0).take();
    drop(dead); // Waits for the old process outside the lock.

    let (engine, events) = match EngineClient::start() {
        Ok(started) => started,
        Err(e) => {
            eprintln!("[keyclean] engine restart failed: {}", e.details());
            update_status(app, |status| {
                status.set_engine(false, Some(ErrorDto::from(&e)))
            });
            return;
        }
    };
    let rejected = {
        let mut current = guard(&slot.0);
        // Checked under the lock `stop_engine` takes after setting the flag, so an exit either
        // sees this engine in the slot or this check sees the exit.
        if is_exiting(app) {
            Some(engine)
        } else {
            *current = Some(engine);
            None
        }
    };
    if let Some(engine) = rejected {
        drop(engine); // Outside the lock.
        return;
    }
    update_status(app, StatusDto::set_engine_restarted);
    let relay = forward_events(app.clone(), events);
    if let Some(done) = app.try_state::<RelayDone>() {
        *guard(&done.0) = relay;
    }
    eprintln!("[keyclean] engine restarted");
}

fn stop_engine(app: &AppHandle) {
    if let Some(exiting) = app.try_state::<Exiting>() {
        exiting.0.store(true, Ordering::SeqCst);
    }
    if let Some(slot) = app.try_state::<EngineSlot>() {
        let engine = guard(&slot.0).take();
        drop(engine); // Releases any lock and stops the engine process.
    }
    // Let the relay log how the session ended before the process exits. The receiver is taken out
    // first, so the lock isn't held while waiting.
    let done = app
        .try_state::<RelayDone>()
        .and_then(|relay| guard(&relay.0).take());
    if let Some(done) = done {
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
            // `setup` runs on the main thread, which owns the hidden helper windows.
            let handle = app.handle().clone();
            let guarded = keyclean_win::guard_helper_windows(move || handle.exit(0));
            eprintln!("[keyclean] close guard: {guarded} helper windows");
            app.manage(OverlayState::new());
            start_engine(app.handle());
            devices::start(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            lock_input,
            unlock_input,
            get_lock_options,
            get_status,
            list_devices,
            overlay::get_overlay_session,
            overlay::overlay_ready,
            overlay::overlay_tick
        ])
        .build(tauri::generate_context!());

    match app {
        Ok(app) => app.run(|handle, event| match event {
            // Closing the main window exits, also while the overlay is open.
            RunEvent::WindowEvent {
                label,
                event: WindowEvent::Destroyed,
                ..
            } if label == MAIN_WINDOW => handle.exit(0),
            // Once the event loop runs, so the overlay's 2 s budget doesn't include startup.
            #[cfg(debug_assertions)]
            RunEvent::Ready => e2e_autolock(handle),
            RunEvent::Exit => {
                devices::stop(handle);
                stop_engine(handle);
            }
            _ => {}
        }),
        Err(err) => {
            eprintln!("KeyClean failed to start: {err}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    /// The text between `start` and the next `end` after it.
    fn between<'a>(text: &'a str, start: &str, end: &str) -> &'a str {
        let from = text.find(start).expect("start marker") + start.len();
        let len = text[from..].find(end).expect("end marker");
        &text[from..from + len]
    }

    /// Tauri only lets the window call commands that build.rs lists and the window's capability
    /// allows; a missing one fails at runtime with "not allowed. Command not found".
    #[test]
    fn every_command_is_listed_and_allowed() {
        let handler: BTreeSet<String> = between(
            include_str!("lib.rs"),
            concat!("generate_handler", "!["),
            "]",
        )
        .split(',')
        .filter_map(|name| name.trim().rsplit("::").next())
        .map(str::to_owned)
        .filter(|name| !name.is_empty())
        .collect();
        let listed: BTreeSet<String> = between(include_str!("../build.rs"), ".commands(&[", "]")
            .split(',')
            .map(|name| name.trim().trim_matches('"').to_owned())
            .filter(|name| !name.is_empty())
            .collect();
        let allowed_by = |capability: &str| -> BTreeSet<String> {
            let capability: serde_json::Value = serde_json::from_str(capability).unwrap();
            capability["permissions"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|p| p.as_str()?.strip_prefix("allow-"))
                .map(|name| name.replace('-', "_"))
                .collect()
        };
        let main = allowed_by(include_str!("../capabilities/main.json"));
        let overlay = allowed_by(include_str!("../capabilities/overlay.json"));
        let allowed: BTreeSet<String> = main.union(&overlay).cloned().collect();

        assert!(!handler.is_empty());
        assert_eq!(handler, listed, "build.rs must list every command");
        assert_eq!(
            handler, allowed,
            "the capabilities together must allow every command"
        );
    }

    /// Each window gets only what it needs: the overlay page never starts a lock or reads the
    /// device list, and the main window never speaks for the overlay.
    #[test]
    fn windows_get_only_their_commands() {
        let permissions = |capability: &str| -> Vec<String> {
            let capability: serde_json::Value = serde_json::from_str(capability).unwrap();
            capability["permissions"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|p| p.as_str().map(str::to_owned))
                .collect()
        };
        let overlay = permissions(include_str!("../capabilities/overlay.json"));
        for denied in ["allow-lock-input", "allow-list-devices"] {
            assert!(!overlay.iter().any(|p| p == denied), "overlay has {denied}");
        }
        let main = permissions(include_str!("../capabilities/main.json"));
        for denied in [
            "allow-overlay-ready",
            "allow-overlay-tick",
            "allow-get-overlay-session",
        ] {
            assert!(
                !main.iter().any(|p| p == denied),
                "main window has {denied}"
            );
        }
    }
}
