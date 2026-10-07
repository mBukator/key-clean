//! The full-screen overlay and the rule that nothing is locked without it (invariant 11,
//! ADR 0014).
//!
//! A lock request shows the overlay on the monitor under the cursor, waits for the page to confirm
//! it is visible, and only then asks the engine to lock. During the lock the app checks on every
//! countdown status that the overlay is still up; if it was closed, hidden, minimized, left on
//! another virtual desktop or stopped answering, the app ends the lock. The overlay can end a lock
//! early, never keep one going: the engine owns the lock and its exits (invariant 3).

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use keyclean_win::keyclean_core::overlay::{
    AttemptId, ConfirmGate, Confirmation, OverlayLoss, OverlayWatch, WindowFacts,
};
use keyclean_win::keyclean_core::policy::{SafetyProfile, plan_lock};
use keyclean_win::keyclean_core::time::MonoTime;
use keyclean_win::{EngineError, LockRequest, safety_profile};
use serde::Serialize;
use tauri::window::Color;
use tauri::{AppHandle, Manager, Monitor, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::bridge::ErrorDto;
use crate::{EngineSlot, StatusStore, guard, i18n};

/// Prefix of the overlay window labels; each attempt gets its own window, `overlay-<n>`, so a
/// window still closing never collides with the next one. The overlay capability matches it.
const LABEL_PREFIX: &str = "overlay-";

/// The overlay's page, a separate small entry point (Vite multi-page build).
const OVERLAY_PAGE: &str = "overlay.html";

/// The overlay's background (matches the page), painted before the page loads so the screen
/// never flashes white.
const BACKGROUND: Color = Color(10, 9, 8, 255);

/// What the lock request waiting for the overlay is told.
enum Wake {
    /// The page confirmed it is visible, this long after the request.
    Confirmed(Duration),
    /// The window closed before it confirmed.
    Gone,
}

/// What the overlay page shows before the engine's first status arrives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlaySessionDto {
    attempt: u64,
    /// The lock length after the safety policy (the debug dev cap shortens it).
    seconds: u64,
    keyboard: bool,
    mouse: bool,
    dev_cap: bool,
}

struct Inner {
    gate: ConfirmGate,
    /// The attempt waiting for its overlay, and how to wake its request.
    waiter: Option<(AttemptId, Sender<Wake>)>,
    /// What the current attempt's page shows.
    session: Option<OverlaySessionDto>,
    /// The overlay of the running lock, once confirmed.
    watch: Option<(AttemptId, OverlayWatch)>,
    /// The attempt whose overlay window is open (pending or watched).
    window: Option<AttemptId>,
}

/// The overlay's state. At idle nothing is open and nothing runs.
pub struct OverlayState {
    inner: Mutex<Inner>,
    origin: Instant,
}

impl OverlayState {
    pub fn new() -> Self {
        OverlayState {
            inner: Mutex::new(Inner {
                gate: ConfirmGate::new(),
                waiter: None,
                session: None,
                watch: None,
                window: None,
            }),
            origin: Instant::now(),
        }
    }

    fn now(&self) -> MonoTime {
        MonoTime::from_since_origin(self.origin.elapsed())
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn label(id: AttemptId) -> String {
    format!("{LABEL_PREFIX}{}", id.0)
}

/// Debug builds only: `KEYCLEAN_E2E_OVERLAY=no-ack` makes the app ignore the page's confirmation
/// and `no-tick` its heartbeat, so the end-to-end harness can test both failure paths (ADR 0008).
#[cfg(debug_assertions)]
fn fault(name: &str) -> bool {
    std::env::var("KEYCLEAN_E2E_OVERLAY").is_ok_and(|v| v == name)
}

#[cfg(not(debug_assertions))]
fn fault(_name: &str) -> bool {
    false
}

/// Shows the overlay, waits for it to confirm it is visible, then asks the engine to lock. Nothing
/// is locked if the overlay doesn't confirm within the budget (invariant 11).
pub async fn lock(app: AppHandle, request: LockRequest) -> Result<(), ErrorDto> {
    let requested = Instant::now();
    // Checked before anything is shown, so a request the engine would refuse opens no window.
    let targets = request
        .targets
        .checked()
        .map_err(|e| ErrorDto::from(&EngineError::InvalidRequest(e)))?;
    let plan = plan_lock(request.duration, request.max_lock, safety_profile())
        .map_err(|e| ErrorDto::from(&EngineError::InvalidRequest(e)))?;
    {
        let engine = app.state::<EngineSlot>();
        if guard(&engine.0).is_none() {
            return Err(ErrorDto::from(&EngineError::NotRunning));
        }
        if !guard(&app.state::<StatusStore>().0).is_idle() {
            return Err(ErrorDto::from(&EngineError::AlreadyActive));
        }
    }

    let state = app.state::<OverlayState>();
    let (id, wake) = {
        let mut inner = state.lock();
        // One overlay at a time: a confirmed attempt keeps its window record until it locks or
        // gives up, so a second request in between is refused rather than taking it over.
        if inner.watch.is_some() || inner.window.is_some() {
            return Err(ErrorDto::from(&EngineError::AlreadyActive));
        }
        let Some(id) = inner.gate.begin(state.now()) else {
            return Err(ErrorDto::from(&EngineError::AlreadyActive));
        };
        let (tx, rx) = mpsc::channel();
        inner.waiter = Some((id, tx));
        inner.window = Some(id);
        inner.session = Some(OverlaySessionDto {
            attempt: id.0,
            seconds: plan.session.as_secs(),
            keyboard: targets.keyboard,
            mouse: targets.mouse,
            dev_cap: safety_profile() == SafetyProfile::Dev,
        });
        (id, rx)
    };

    let confirmed = show_and_wait(&app, &state, id, wake, requested).await;
    let took = match confirmed {
        Ok(took) => took,
        Err(e) => {
            abandon(&app, &state, id);
            eprintln!("[keyclean] overlay: nothing locked: {}", e.details);
            return Err(e);
        }
    };

    eprintln!(
        "[keyclean] overlay: confirmed visible {} ms after the request",
        took.as_millis()
    );
    let locked = {
        // Held while the lock is requested, so a window closing at this moment either ends the
        // lock after it starts (it sees the watch) or prevents it (the window record is gone).
        // Lock order: overlay state, then the engine slot; nothing takes them the other way.
        let mut inner = state.lock();
        if inner.waiter.as_ref().is_some_and(|(w, _)| *w == id) {
            inner.waiter = None;
        }
        if inner.window != Some(id) {
            drop(inner);
            close_window(&app, &label(id));
            return Err(ErrorDto::overlay_failed(
                "the overlay closed right after it confirmed".to_owned(),
            ));
        }
        inner.watch = Some((id, OverlayWatch::new(state.now())));
        // Cleared here, not when the lock starts: the overlay may be lost before the engine's
        // first status arrives.
        guard(&app.state::<StatusStore>().0).lock_requested();
        match guard(&app.state::<EngineSlot>().0).as_ref() {
            Some(engine) => engine.lock(request).map_err(|e| ErrorDto::from(&e)),
            None => Err(ErrorDto::from(&EngineError::NotRunning)),
        }
    };
    if locked.is_err() {
        finish(&app);
    }
    locked
}

/// Builds the overlay hidden, moves it over the monitor under the cursor in physical pixels, shows
/// it, and waits for the page's confirmation. Returns how long that took from the request.
async fn show_and_wait(
    app: &AppHandle,
    state: &State<'_, OverlayState>,
    id: AttemptId,
    wake: Receiver<Wake>,
    requested: Instant,
) -> Result<Duration, ErrorDto> {
    let failed = |what: &str, e: tauri::Error| ErrorDto::overlay_failed(format!("{what}: {e}"));
    let monitor = monitor_under_cursor(app);

    let window = WebviewWindowBuilder::new(app, label(id), WebviewUrl::App(OVERLAY_PAGE.into()))
        .title(i18n::t("app.title"))
        .visible(false)
        .decorations(false)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .shadow(false)
        .focused(true)
        .background_color(BACKGROUND)
        .build()
        .map_err(|e| failed("building the overlay window failed", e))?;
    let built = requested.elapsed();
    let handle = app.clone();
    window.on_window_event(move |event| {
        if matches!(
            event,
            tauri::WindowEvent::CloseRequested { .. } | tauri::WindowEvent::Destroyed
        ) {
            gone(&handle, id);
        }
    });

    // Position first, then size: moving onto a monitor with another scale factor may resize
    // the window, so the size is set once it is there.
    match &monitor {
        Some(monitor) => {
            window
                .set_position(*monitor.position())
                .map_err(|e| failed("positioning the overlay failed", e))?;
            window
                .set_size(*monitor.size())
                .map_err(|e| failed("sizing the overlay failed", e))?;
        }
        None => window
            .maximize()
            .map_err(|e| failed("no monitor found, and maximizing the overlay failed", e))?,
    }
    window
        .show()
        .map_err(|e| failed("showing the overlay failed", e))?;
    // Focus keeps typed keys (in a mouse-only lock) off the windows underneath. Not fatal: the
    // overlay still covers them.
    if let Err(e) = window.set_focus() {
        eprintln!("[keyclean] overlay: couldn't take focus: {e}");
    }
    let shown = requested.elapsed();

    let remaining = state.lock().gate.remaining(state.now()).unwrap_or_default();
    let woke = tauri::async_runtime::spawn_blocking(move || wake.recv_timeout(remaining))
        .await
        .map_err(|e| ErrorDto::overlay_failed(format!("waiting for the overlay failed: {e}")))?;
    let timings = format!(
        "built {} ms, shown {} ms after the request",
        built.as_millis(),
        shown.as_millis()
    );
    match woke {
        Ok(Wake::Confirmed(took)) => {
            confirm_still_up(&window).map_err(|why| {
                ErrorDto::overlay_failed(format!("{why} after it confirmed ({timings})"))
            })?;
            eprintln!("[keyclean] overlay: {timings}");
            Ok(took)
        }
        Ok(Wake::Gone) | Err(RecvTimeoutError::Disconnected) => Err(ErrorDto::overlay_failed(
            format!("the overlay closed before it confirmed ({timings})"),
        )),
        Err(RecvTimeoutError::Timeout) => Err(ErrorDto::overlay_timeout(format!(
            "the overlay didn't confirm it was visible within {} ms ({timings})",
            keyclean_win::keyclean_core::overlay::CONFIRM_BUDGET.as_millis()
        ))),
    }
}

/// Re-checks the window itself once the page has confirmed.
fn confirm_still_up(window: &WebviewWindow) -> Result<(), &'static str> {
    if window.is_minimized().unwrap_or(false) {
        return Err("the overlay was minimized");
    }
    if !window.is_visible().unwrap_or(true) {
        return Err("the overlay was hidden");
    }
    Ok(())
}

/// The monitor under the cursor (Max's decision for M6), or the primary monitor.
fn monitor_under_cursor(app: &AppHandle) -> Option<Monitor> {
    let under_cursor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten());
    under_cursor.or_else(|| app.primary_monitor().ok().flatten())
}

/// Gives up attempt `id`: no lock, window closed.
fn abandon(app: &AppHandle, state: &OverlayState, id: AttemptId) {
    let window = {
        let mut inner = state.lock();
        inner.gate.abandon(id);
        if inner.waiter.as_ref().is_some_and(|(w, _)| *w == id) {
            inner.waiter = None;
        }
        take_window(&mut inner, id)
    };
    if let Some(label) = window {
        close_window(app, &label);
    }
}

/// Clears the open window record if it belongs to `id`, returning its label.
fn take_window(inner: &mut Inner, id: AttemptId) -> Option<String> {
    if inner.window == Some(id) {
        inner.window = None;
        inner.session = None;
        Some(label(id))
    } else {
        None
    }
}

fn close_window(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label)
        && let Err(e) = window.destroy()
    {
        eprintln!("[keyclean] overlay: couldn't close the window: {e}");
    }
}

/// The overlay window of attempt `id` is closing. Before it confirmed, the waiting request gives
/// up; during the lock, the lock ends.
fn gone(app: &AppHandle, id: AttemptId) {
    let state = app.state::<OverlayState>();
    let (waiter, watched) = {
        let mut inner = state.lock();
        let waiter = match inner.waiter.take() {
            Some((w, tx)) if w == id => Some(tx),
            other => {
                inner.waiter = other;
                None
            }
        };
        let watched = inner.watch.is_some_and(|(w, _)| w == id);
        if !watched {
            // Closing by itself; nothing left to close. A confirmed attempt sees this before it
            // locks.
            take_window(&mut inner, id);
        }
        (waiter, watched)
    };
    if let Some(tx) = waiter {
        let _ = tx.send(Wake::Gone);
    }
    if watched {
        end_lock(app, id, OverlayLoss::Closed);
    }
}

/// Ends the lock because the overlay of attempt `id` went away. Runs once per attempt.
fn end_lock(app: &AppHandle, id: AttemptId, loss: OverlayLoss) {
    let state = app.state::<OverlayState>();
    let window = {
        let mut inner = state.lock();
        if !inner.watch.is_some_and(|(w, _)| w == id) {
            return;
        }
        inner.watch = None;
        take_window(&mut inner, id)
    };
    eprintln!("[keyclean] overlay lost ({loss:?}); ending the lock");
    guard(&app.state::<StatusStore>().0).mark_overlay_lost();
    if let Some(engine) = guard(&app.state::<EngineSlot>().0).as_ref()
        && let Err(e) = engine.unlock()
    {
        eprintln!("[keyclean] overlay: unlock request failed: {}", e.details());
    }
    if let Some(label) = window {
        close_window(app, &label);
    }
    crate::focus_main_window(app);
}

/// Called on every countdown status during a lock: checks on the main thread, where the window
/// lives, that the overlay is still up. Only runs during a lock, so nothing polls at idle.
pub fn check_soon(app: &AppHandle) {
    let watched = app.state::<OverlayState>().lock().watch.map(|(id, _)| id);
    if watched.is_none() {
        return;
    }
    let handle = app.clone();
    if let Err(e) = app.run_on_main_thread(move || check(&handle)) {
        eprintln!("[keyclean] overlay: couldn't schedule the check: {e}");
    }
}

fn check(app: &AppHandle) {
    let state = app.state::<OverlayState>();
    let Some((id, _)) = state.lock().watch else {
        return;
    };
    let facts = match app.get_webview_window(&label(id)) {
        None => WindowFacts {
            exists: false,
            ..WindowFacts::UP
        },
        Some(window) => WindowFacts {
            exists: true,
            // A getter that fails says nothing; it doesn't count as a loss.
            visible: window.is_visible().unwrap_or(true),
            minimized: window.is_minimized().unwrap_or(false),
            on_current_desktop: window
                .hwnd()
                .ok()
                .and_then(|hwnd| keyclean_win::is_on_current_desktop(hwnd.0)),
        },
    };
    let loss = {
        let inner = state.lock();
        match inner.watch {
            Some((w, watch)) if w == id => watch.check(state.now(), facts),
            _ => None,
        }
    };
    if let Some(loss) = loss {
        end_lock(app, id, loss);
    }
}

/// Closes the overlay once the lock is over (or never started), and brings the main window back
/// (§16). Does nothing if no lock is being watched.
pub fn finish(app: &AppHandle) {
    let state = app.state::<OverlayState>();
    let window = {
        let mut inner = state.lock();
        let Some((id, _)) = inner.watch.take() else {
            return;
        };
        take_window(&mut inner, id)
    };
    if let Some(label) = window {
        close_window(app, &label);
    }
    crate::focus_main_window(app);
}

/// The page asks what to show before the engine's first status arrives.
#[tauri::command]
pub fn get_overlay_session(state: State<'_, OverlayState>) -> Option<OverlaySessionDto> {
    state.lock().session
}

/// The page confirms it is visible: its locked screen has rendered (after a double
/// `requestAnimationFrame`) and the document is visible.
#[tauri::command]
pub fn overlay_ready(state: State<'_, OverlayState>, attempt: u64) {
    if fault("no-ack") {
        return;
    }
    let mut inner = state.lock();
    let id = AttemptId(attempt);
    match inner.gate.confirm(id, state.now()) {
        Confirmation::Accepted(took) => {
            if let Some((w, tx)) = inner.waiter.take() {
                if w == id {
                    let _ = tx.send(Wake::Confirmed(took));
                } else {
                    inner.waiter = Some((w, tx));
                }
            }
        }
        Confirmation::Late(took) => {
            eprintln!(
                "[keyclean] overlay: confirmed too late ({} ms); nothing locked",
                took.as_millis()
            );
        }
        Confirmation::Stale => {}
    }
}

/// The page answers a countdown status: it is alive. Silence past the limit ends the lock.
#[tauri::command]
pub fn overlay_tick(state: State<'_, OverlayState>, attempt: u64) {
    if fault("no-tick") {
        return;
    }
    let now = state.now();
    let mut inner = state.lock();
    if let Some((id, watch)) = inner.watch.as_mut()
        && id.0 == attempt
    {
        watch.heartbeat(now);
    }
}
