//! The engine: one dedicated thread that owns the hook, a hidden top-level window for system
//! notifications, the session state machine and its own message loop.
//!
//! Commands arrive through a channel plus a posted wake-up message; events leave through a
//! channel. The UI only sends requests and displays events, so a hung or closed webview can't
//! affect unlocking (invariant 3).
//!
//! Exits from a lock, each independent of the others (invariant 1):
//! - the session timer (`SetTimer` on the engine window) → `Timeout`;
//! - the hard deadline, checked inside the hook on every event → `HardDeadline`;
//! - the hard deadline, enforced by the watchdog thread → `HardDeadline` (or process abort);
//! - the emergency chord, detected inside the hook → `Emergency`;
//! - system transitions (suspend, end of session, workstation lock, disconnect);
//! - process exit, after which Windows removes the hook.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::{Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use keyclean_core::policy::{DEFAULT_MAX_LOCK, DRAIN_TIMEOUT, LockPlan, SafetyProfile, plan_lock};
use keyclean_core::session::{EndReason, Session, SessionState, SystemTransition};
use keyclean_core::time::Clock;
use windows::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, HANDLE, HWND, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Power::{
    HPOWERNOTIFY, RegisterSuspendResumeNotification, UnregisterSuspendResumeNotification,
};
use windows::Win32::System::RemoteDesktop::{
    NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification, WTSUnRegisterSessionNotification,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DEVICE_NOTIFY_WINDOW_HANDLE, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetMessageW, HHOOK, KillTimer, MSG, PBT_APMSUSPEND, PostMessageW, PostQuitMessage,
    RegisterClassExW, SetTimer, WM_CLOSE, WM_DESTROY, WM_ENDSESSION, WM_POWERBROADCAST,
    WM_QUERYENDSESSION, WM_TIMER, WM_WTSSESSION_CHANGE, WNDCLASSEXW, WS_EX_TOOLWINDOW,
    WS_OVERLAPPED, WTS_CONSOLE_DISCONNECT, WTS_REMOTE_DISCONNECT, WTS_SESSION_LOCK,
    WTS_SESSION_LOGOFF,
};
use windows::core::w;

use crate::devices::{self, KeyboardDevice};
use crate::error::EngineError;
use crate::hook;
use crate::msg::{
    TIMER_DRAIN, TIMER_SESSION, WM_COMMANDS_READY, WM_HOOK_CHORD, WM_HOOK_DEADLINE,
    WM_HOOK_DRAINED, WM_WATCHDOG_EXPIRED,
};
use crate::qpc::{self, QpcClock};
use crate::watchdog::Watchdog;

/// How long `Engine::drop` waits for the engine thread to release input and exit.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(3);

static ENGINE_ACTIVE: AtomicBool = AtomicBool::new(false);

/// The safety profile this build enforces. Debug builds always use the development caps,
/// whatever the caller wants (invariant 13).
pub fn safety_profile() -> SafetyProfile {
    if cfg!(debug_assertions) {
        SafetyProfile::Dev
    } else {
        SafetyProfile::Release
    }
}

/// A request to lock the keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LockRequest {
    /// How long the lock should last.
    pub duration: Duration,
    /// The user's "maximum lock duration" setting (bounds the hard deadline).
    pub max_lock: Duration,
}

impl LockRequest {
    /// A lock of `duration` with the default maximum lock duration.
    pub const fn new(duration: Duration) -> Self {
        LockRequest {
            duration,
            max_lock: DEFAULT_MAX_LOCK,
        }
    }
}

/// A snapshot of the engine's session, for display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineStatus {
    /// Where the session is.
    pub state: SessionState,
    /// Whether this build applies the development caps ("DEV CAP").
    pub dev_cap: bool,
    /// Time left until the session ends normally, while starting or locked.
    pub session_remaining: Option<Duration>,
    /// Time left until the hard deadline, while starting or locked.
    pub hard_deadline_remaining: Option<Duration>,
}

/// Something noteworthy that isn't an error, for session logs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineNotice {
    /// Windows had already removed the hook when the engine tried to (e.g. a hook timeout).
    HookAlreadyRemoved,
    /// Some blocked keys were never released (e.g. Ctrl+Alt+Del); the hook was removed after the
    /// drain timeout.
    DrainTimedOut,
    /// Suspend notifications couldn't be registered; the broadcast is still received.
    PowerNotificationUnavailable,
    /// Workstation lock/switch notifications couldn't be registered.
    SessionNotificationUnavailable,
}

/// Everything the engine reports. Contains no key data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineEvent {
    /// The session state changed.
    Status(EngineStatus),
    /// A session ended and input is released.
    SessionEnded {
        /// Why it ended.
        reason: EndReason,
    },
    /// A request failed, or a failure ended the session (input is released).
    Error(EngineError),
    /// A notice for session logs.
    Notice(EngineNotice),
}

enum Command {
    Lock(LockRequest),
    Unlock,
    Shutdown,
}

/// Handle to the running engine. Dropping it releases any lock and stops the engine thread.
pub struct Engine {
    hwnd: isize,
    commands: Sender<Command>,
    // Behind a Mutex only so `Engine` is `Sync` (Tauri managed state); never contended.
    exited: Mutex<Receiver<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Engine {
    /// Starts the engine thread. Only one engine may run per process.
    pub fn start() -> Result<(Engine, Receiver<EngineEvent>), EngineError> {
        if ENGINE_ACTIVE.swap(true, Ordering::AcqRel) {
            return Err(EngineError::AlreadyRunning);
        }
        let (command_tx, command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (exit_tx, exit_rx) = mpsc::channel();

        let spawned = std::thread::Builder::new()
            .name("keyclean-engine".into())
            .spawn(move || {
                engine_thread(command_rx, event_tx, ready_tx);
                ENGINE_ACTIVE.store(false, Ordering::Release);
                let _ = exit_tx.send(());
            });
        let thread = match spawned {
            Ok(thread) => thread,
            Err(e) => {
                ENGINE_ACTIVE.store(false, Ordering::Release);
                return Err(EngineError::ThreadSpawn(e.to_string()));
            }
        };

        match ready_rx.recv() {
            Ok(Ok(hwnd)) => Ok((
                Engine {
                    hwnd,
                    commands: command_tx,
                    exited: Mutex::new(exit_rx),
                    thread: Some(thread),
                },
                event_rx,
            )),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => {
                let _ = thread.join();
                Err(EngineError::NotRunning)
            }
        }
    }

    /// Asks the engine to lock the keyboard. The outcome arrives as events.
    pub fn lock(&self, request: LockRequest) -> Result<(), EngineError> {
        self.send(Command::Lock(request))
    }

    /// Asks the engine to end the current session (`EndReason::UserRequest`).
    pub fn unlock(&self) -> Result<(), EngineError> {
        self.send(Command::Unlock)
    }

    /// Lists connected keyboards (name and id only).
    pub fn devices(&self) -> Result<Vec<KeyboardDevice>, EngineError> {
        devices::keyboards()
    }

    /// Releases any lock and stops the engine. Same as dropping it.
    pub fn shutdown(self) {}

    fn send(&self, command: Command) -> Result<(), EngineError> {
        self.commands
            .send(command)
            .map_err(|_| EngineError::NotRunning)?;
        // SAFETY: PostMessageW only enqueues; if the window is gone it fails without side effects.
        unsafe {
            PostMessageW(
                Some(HWND(self.hwnd as *mut _)),
                WM_COMMANDS_READY,
                WPARAM(0),
                LPARAM(0),
            )
        }
        .map_err(|_| EngineError::NotRunning)
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let exited = self.send(Command::Shutdown).is_ok()
            && self
                .exited
                .get_mut()
                .unwrap_or_else(PoisonError::into_inner)
                .recv_timeout(SHUTDOWN_WAIT)
                .is_ok();
        if exited && let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        // If the engine thread didn't answer, it is left detached. Windows removes its hook when
        // the thread or the process exits.
    }
}

/// State owned by the engine thread.
struct EngineState {
    hwnd: HWND,
    session: Session,
    hook: Option<HHOOK>,
    commands: Receiver<Command>,
    events: Sender<EngineEvent>,
    watchdog: Watchdog,
    /// Increments per lock; hook messages carry it so stale ones are ignored.
    generation: u64,
    /// The watchdog arming of the current session, while armed.
    watchdog_generation: Option<u64>,
    power_notify: Option<HPOWERNOTIFY>,
    session_notify: bool,
}

thread_local! {
    static STATE: RefCell<Option<EngineState>> = const { RefCell::new(None) };
}

fn engine_thread(
    commands: Receiver<Command>,
    events: Sender<EngineEvent>,
    ready: SyncSender<Result<isize, EngineError>>,
) {
    let hwnd = match create_window() {
        Ok(hwnd) => hwnd,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    let watchdog = match Watchdog::spawn() {
        Ok(w) => w,
        Err(e) => {
            // SAFETY: `hwnd` was created on this thread and is destroyed once.
            let _ = unsafe { DestroyWindow(hwnd) };
            let _ = ready.send(Err(EngineError::ThreadSpawn(e.to_string())));
            return;
        }
    };
    hook::set_engine_window(hwnd);

    // SAFETY: `hwnd` is a valid top-level window owned by this thread.
    let power_notify =
        unsafe { RegisterSuspendResumeNotification(HANDLE(hwnd.0), DEVICE_NOTIFY_WINDOW_HANDLE) }
            .ok();
    if power_notify.is_none() {
        let _ = events.send(EngineEvent::Notice(
            EngineNotice::PowerNotificationUnavailable,
        ));
    }
    // SAFETY: as above; unregistered before the window is destroyed.
    let session_notify =
        unsafe { WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) }.is_ok();
    if !session_notify {
        let _ = events.send(EngineEvent::Notice(
            EngineNotice::SessionNotificationUnavailable,
        ));
    }

    let state = EngineState {
        hwnd,
        session: Session::new(),
        hook: None,
        commands,
        events,
        watchdog,
        generation: 0,
        watchdog_generation: None,
        power_notify,
        session_notify,
    };
    let _ = STATE.try_with(|cell| *cell.borrow_mut() = Some(state));
    let _ = ready.send(Ok(hwnd.0 as isize));

    let mut msg = MSG::default();
    loop {
        // SAFETY: `msg` is a valid MSG buffer; None retrieves messages for this whole thread.
        let result = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        match result.0 {
            0 => break,
            -1 => {
                with_state(|s| s.fail(EngineError::MessageLoop));
                break;
            }
            _ => {
                // SAFETY: `msg` was filled in by GetMessageW.
                unsafe { DispatchMessageW(&msg) };
            }
        }
    }

    // Release input before anything else is torn down.
    with_state(|s| s.end_now(EndReason::UserRequest));
    let state = STATE
        .try_with(|cell| cell.borrow_mut().take())
        .ok()
        .flatten();
    if let Some(state) = state {
        if let Some(handle) = state.power_notify {
            // SAFETY: `handle` came from RegisterSuspendResumeNotification and is released once.
            let _ = unsafe { UnregisterSuspendResumeNotification(handle) };
        }
        if state.session_notify {
            // SAFETY: registered above for this window, which still exists.
            let _ = unsafe { WTSUnRegisterSessionNotification(state.hwnd) };
        }
        // SAFETY: the window belongs to this thread and is destroyed once. Done outside the
        // RefCell borrow because DestroyWindow re-enters the window procedure.
        let _ = unsafe { DestroyWindow(state.hwnd) };
        drop(state); // joins the watchdog thread
    }
}

fn create_window() -> Result<HWND, EngineError> {
    // SAFETY: returns this module's handle without transferring ownership.
    let module = unsafe { GetModuleHandleW(None) }.map_err(|e| EngineError::window_setup(&e))?;
    let class_name = w!("KeyCleanEngineWindow");
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(window_proc),
        hInstance: module.into(),
        lpszClassName: class_name,
        ..Default::default()
    };
    // SAFETY: `class` is fully initialized and `class_name` is a static wide string.
    if unsafe { RegisterClassExW(&class) } == 0 {
        // SAFETY: GetLastError has no preconditions.
        if unsafe { GetLastError() } != ERROR_CLASS_ALREADY_EXISTS {
            return Err(EngineError::window_setup(
                &windows::core::Error::from_thread(),
            ));
        }
    }
    // A hidden top-level window (never shown): message-only windows don't receive broadcasts such
    // as WM_POWERBROADCAST and WM_QUERYENDSESSION. WS_EX_TOOLWINDOW keeps it off the taskbar.
    // SAFETY: the class was registered above; all pointers are static or None.
    unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class_name,
            w!("KeyClean engine"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(module.into()),
            None,
        )
    }
    .map_err(|e| EngineError::window_setup(&e))
}

/// Runs `f` on the engine state. If the state is unavailable (re-entrancy or teardown), releases
/// input through the passthrough phase instead of losing the event.
fn with_state(f: impl FnOnce(&mut EngineState)) {
    let ran = STATE
        .try_with(|cell| match cell.try_borrow_mut() {
            Ok(mut guard) => match guard.as_mut() {
                Some(state) => {
                    f(state);
                    true
                }
                None => false,
            },
            Err(_) => false,
        })
        .unwrap_or(false);
    if !ran {
        hook::force_passthrough();
    }
}

/// The hidden window's procedure.
///
/// # Safety
/// Called by Windows only, with valid window-procedure arguments.
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_COMMANDS_READY => with_state(EngineState::handle_commands),
        WM_HOOK_CHORD => with_state(|s| s.on_hook_end(wparam, EndReason::Emergency)),
        WM_HOOK_DEADLINE => with_state(|s| s.on_hook_end(wparam, EndReason::HardDeadline)),
        WM_HOOK_DRAINED => with_state(|s| s.on_drained(wparam)),
        WM_WATCHDOG_EXPIRED => with_state(|s| s.on_watchdog(wparam)),
        // Never let the hidden window be closed (e.g. `taskkill` without /F sends WM_CLOSE to
        // every top-level window); it carries the timers and the system notifications.
        WM_CLOSE => {}
        WM_DESTROY => {
            // Only reached unexpectedly while running (normal teardown has already taken the
            // state, so this is a no-op then). Release input and stop the engine thread.
            with_state(|s| s.fail(EngineError::WindowDestroyed));
            // SAFETY: ends this thread's message loop.
            unsafe { PostQuitMessage(0) };
        }
        WM_TIMER => match wparam.0 {
            TIMER_SESSION => with_state(EngineState::on_session_timer),
            TIMER_DRAIN => with_state(EngineState::on_drain_timeout),
            _ => {}
        },
        WM_POWERBROADCAST => {
            if wparam.0 as u32 == PBT_APMSUSPEND {
                with_state(|s| s.end_now(EndReason::SystemTransition(SystemTransition::Suspend)));
            }
            return LRESULT(1);
        }
        WM_QUERYENDSESSION => {
            // Never block shutdown (§47); release input right away.
            with_state(|s| s.end_now(EndReason::SystemTransition(SystemTransition::EndSession)));
            return LRESULT(1);
        }
        WM_ENDSESSION => {
            if wparam.0 != 0 {
                with_state(|s| {
                    s.end_now(EndReason::SystemTransition(SystemTransition::EndSession))
                });
            }
            return LRESULT(0);
        }
        WM_WTSSESSION_CHANGE => {
            let transition = match wparam.0 as u32 {
                WTS_SESSION_LOCK => Some(SystemTransition::SessionLock),
                WTS_CONSOLE_DISCONNECT | WTS_REMOTE_DISCONNECT => {
                    Some(SystemTransition::SessionDisconnect)
                }
                WTS_SESSION_LOGOFF => Some(SystemTransition::EndSession),
                _ => None,
            };
            if let Some(t) = transition {
                with_state(|s| s.end_now(EndReason::SystemTransition(t)));
            }
            return LRESULT(0);
        }
        _ => {
            // SAFETY: default handling for every other message, with the original arguments.
            return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
        }
    }
    LRESULT(0)
}

fn millis(d: Duration) -> u32 {
    u32::try_from(d.as_millis()).unwrap_or(u32::MAX).max(1)
}

impl EngineState {
    fn emit(&self, event: EngineEvent) {
        let _ = self.events.send(event);
    }

    fn emit_status(&self) {
        let now = QpcClock.now();
        let deadlines = self.session.deadlines();
        let active = matches!(
            self.session.state(),
            SessionState::Starting | SessionState::Locked
        );
        let remaining = |at: Option<keyclean_core::time::MonoTime>| {
            at.filter(|_| active)
                .map(|t| t.saturating_duration_since(now))
        };
        self.emit(EngineEvent::Status(EngineStatus {
            state: self.session.state(),
            dev_cap: safety_profile() == SafetyProfile::Dev,
            session_remaining: remaining(deadlines.map(|d| d.session)),
            hard_deadline_remaining: remaining(deadlines.map(|d| d.hard)),
        }));
    }

    fn handle_commands(&mut self) {
        while let Ok(command) = self.commands.try_recv() {
            match command {
                Command::Lock(request) => self.lock(request),
                Command::Unlock => self.end(EndReason::UserRequest),
                Command::Shutdown => {
                    self.end_now(EndReason::UserRequest);
                    // SAFETY: ends this thread's message loop.
                    unsafe { PostQuitMessage(0) };
                }
            }
        }
    }

    fn lock(&mut self, request: LockRequest) {
        if self.session.state() != SessionState::Idle {
            self.emit(EngineEvent::Error(EngineError::AlreadyActive));
            return;
        }
        let plan: LockPlan = match plan_lock(request.duration, request.max_lock, safety_profile()) {
            Ok(plan) => plan,
            Err(e) => {
                self.emit(EngineEvent::Error(EngineError::InvalidRequest(e)));
                return;
            }
        };

        // One QPC reading anchors the session (MonoTime) and the hook (ticks).
        let start_ticks = qpc::ticks();
        if self.session.start(plan, QpcClock::at(start_ticks)).is_err() {
            self.emit(EngineEvent::Error(EngineError::AlreadyActive));
            return;
        }
        self.emit_status();

        let hard_ticks = start_ticks.saturating_add(qpc::duration_to_ticks(plan.hard_deadline));
        self.generation = self.generation.wrapping_add(1);
        hook::begin_arming(self.generation, hard_ticks);
        self.watchdog_generation = Some(self.watchdog.arm(Instant::now() + plan.hard_deadline));

        match hook::install() {
            Ok(handle) => self.hook = Some(handle),
            Err(e) => {
                hook::clear();
                self.fail(EngineError::hook_install(&e));
                return;
            }
        }
        hook::seed_from_held_keys();
        if !hook::engage() {
            // The hard deadline passed while arming; the hook already moved to Draining.
            self.end(EndReason::HardDeadline);
            return;
        }
        let _ = self.session.locked();

        // SAFETY: `self.hwnd` is this thread's window; the timer is killed in `finish`.
        if unsafe { SetTimer(Some(self.hwnd), TIMER_SESSION, millis(plan.session), None) } == 0 {
            self.fail(EngineError::Timer);
            return;
        }
        self.emit_status();
    }

    fn on_session_timer(&mut self) {
        let now = QpcClock.now();
        if let Some(reason) = self.session.expired(now) {
            self.end(reason);
        } else if let Some(remaining) = self.session.remaining(now) {
            // Timers can fire slightly early; re-arm for the rest.
            // SAFETY: same window and timer id; replaces the existing timer.
            if unsafe { SetTimer(Some(self.hwnd), TIMER_SESSION, millis(remaining), None) } == 0 {
                self.fail(EngineError::Timer);
            }
        }
    }

    /// Ends the session and drains: the hook stays until the keys whose presses were blocked are
    /// released, so Windows sees no unpaired key-ups (invariant 8).
    fn end(&mut self, reason: EndReason) {
        if self.session.end(reason).is_err() {
            return; // Already ending or idle: a duplicate signal.
        }
        self.kill_timer(TIMER_SESSION);
        hook::enter_draining();
        self.emit_status();

        let passthrough = hook::phase() != Some(keyclean_core::keystate::Phase::Draining);
        if self.hook.is_none() || passthrough || hook::drained() {
            self.finish();
            return;
        }
        // SAFETY: `self.hwnd` is this thread's window; the timer is killed in `finish`.
        if unsafe { SetTimer(Some(self.hwnd), TIMER_DRAIN, millis(DRAIN_TIMEOUT), None) } == 0 {
            self.finish();
        }
    }

    /// Ends the session and releases input immediately, without draining. Used for system
    /// transitions, where the machine may go away before a drain could finish.
    fn end_now(&mut self, reason: EndReason) {
        let _ = self.session.end(reason);
        if self.session.state() == SessionState::Unlocking {
            self.finish();
        }
    }

    /// Ends the session because of an engine failure and reports it.
    fn fail(&mut self, error: EngineError) {
        self.emit(EngineEvent::Error(error));
        self.end_now(EndReason::EngineError);
    }

    fn is_current(&self, wparam: WPARAM) -> bool {
        wparam.0 as u64 == self.generation && self.session.state() != SessionState::Idle
    }

    fn on_hook_end(&mut self, wparam: WPARAM, reason: EndReason) {
        if self.is_current(wparam) {
            self.end(reason);
        }
    }

    fn on_drained(&mut self, wparam: WPARAM) {
        if self.is_current(wparam)
            && self.session.state() == SessionState::Unlocking
            && hook::drained()
        {
            self.finish();
        }
    }

    fn on_drain_timeout(&mut self) {
        self.kill_timer(TIMER_DRAIN);
        if self.session.state() == SessionState::Unlocking {
            self.emit(EngineEvent::Notice(EngineNotice::DrainTimedOut));
            self.finish();
        }
    }

    fn on_watchdog(&mut self, wparam: WPARAM) {
        // A stale expiry (from an arming that has since been disarmed) is ignored; the watchdog
        // already stood down when it was disarmed.
        if self.watchdog_generation == Some(wparam.0 as u64) {
            self.end_now(EndReason::HardDeadline);
            self.watchdog.disarm();
            self.watchdog_generation = None;
        }
    }

    /// Removes the hook and returns to idle. Input is released when this returns.
    fn finish(&mut self) {
        self.kill_timer(TIMER_SESSION);
        self.kill_timer(TIMER_DRAIN);
        match self.hook.take() {
            Some(handle) => {
                if hook::uninstall(handle).is_err() {
                    self.emit(EngineEvent::Notice(EngineNotice::HookAlreadyRemoved));
                }
            }
            None => hook::clear(),
        }
        self.watchdog.disarm();
        self.watchdog_generation = None;
        if let Ok(reason) = self.session.finished() {
            self.emit(EngineEvent::SessionEnded { reason });
            self.emit_status();
        }
    }

    fn kill_timer(&self, id: usize) {
        // SAFETY: killing a timer that doesn't exist just fails.
        let _ = unsafe { KillTimer(Some(self.hwnd), id) };
    }
}
