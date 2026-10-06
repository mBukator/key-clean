//! Shared plumbing: the in-process engine, probes, waits and cleanup.

use std::sync::OnceLock;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use keyclean_win::keyclean_core::policy::DEV_MAX_HARD_DEADLINE;
use keyclean_win::testkit::inject::{self, Stroke, vk};
use keyclean_win::testkit::mouse::{self, Button, MouseInput};
use keyclean_win::testkit::observer::{MouseKind, Observer};
use keyclean_win::testkit::{held, system};
use keyclean_win::{
    EndReason, Engine, EngineEvent, EngineNotice, LockRequest, LockTargets, SessionState,
};

/// How long to wait for an injected event to reach (or not reach) the observer.
pub const PROBE_WINDOW: Duration = Duration::from_millis(350);

/// Keys the harness injects; the stuck-key check looks at exactly these.
pub const WATCHED_KEYS: &[(u8, &str)] = &[
    (vk::LSHIFT, "LShift"),
    (vk::LCONTROL, "LCtrl"),
    (vk::RCONTROL, "RCtrl"),
    (vk::LMENU, "LAlt"),
    (vk::RMENU, "RAlt/AltGr"),
    (vk::LWIN, "LWin"),
    (vk::K, "K"),
    (vk::X, "X"),
    (vk::G, "G"),
    (vk::TAB, "Tab"),
    (vk::ESCAPE, "Esc"),
    (vk::F13, "F13"),
    (vk::F13 + 1, "F14"),
    (vk::F13 + 2, "F15"),
];

pub fn sleep(d: Duration) {
    std::thread::sleep(d);
}

/// Polls `cond` every 5 ms until it holds or `timeout` passes.
pub fn wait_until(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let until = Instant::now() + timeout;
    loop {
        if cond() {
            return true;
        }
        if Instant::now() >= until {
            return false;
        }
        sleep(Duration::from_millis(5));
    }
}

/// Sends strokes, turning an injection failure into a check failure.
pub fn send(strokes: &[Stroke]) -> Result<(), String> {
    inject::send(strokes).map_err(|e| format!("injection failed: {e}"))
}

/// Taps probe key `key` and reports whether it got past KeyClean.
pub fn probe_passes(observer: &Observer, key: u8) -> Result<bool, String> {
    observer.reset();
    send(&[Stroke::down(key), Stroke::up(key)])?;
    Ok(wait_until(PROBE_WINDOW, || {
        observer.downs(key) > 0 && observer.ups(key) > 0
    }))
}

/// Fails unless probe keys get through.
pub fn expect_probes_pass(observer: &Observer) -> Result<(), String> {
    for key in [vk::F13, vk::F13 + 1] {
        if !probe_passes(observer, key)? {
            return Err("input did not get through after the lock ended".into());
        }
    }
    Ok(())
}

/// Fails unless probe keys are blocked.
pub fn expect_probes_blocked(observer: &Observer) -> Result<(), String> {
    for key in [vk::F13, vk::F13 + 1, vk::F13 + 2] {
        if probe_passes(observer, key)? {
            return Err("a probe key got through during the lock".into());
        }
    }
    Ok(())
}

/// Names of watched keys Windows still considers held.
pub fn stuck_keys() -> Vec<&'static str> {
    WATCHED_KEYS
        .iter()
        .filter(|(key, _)| held::is_held(*key))
        .map(|(_, name)| *name)
        .collect()
}

/// Fails if any watched key is stuck down.
pub fn expect_no_stuck_keys() -> Result<(), String> {
    let stuck = stuck_keys();
    if stuck.is_empty() {
        Ok(())
    } else {
        Err(format!("stuck keys after unlock: {}", stuck.join(", ")))
    }
}

/// Releases any watched key the harness may have left down (only after a failure).
pub fn release_watched_keys() {
    let held: Vec<Stroke> = WATCHED_KEYS
        .iter()
        .filter(|(key, _)| held::is_held(*key))
        .map(|(key, _)| Stroke::up(*key))
        .collect();
    if !held.is_empty() {
        let _ = inject::send(&held);
    }
}

/// How a session ended, as seen by the harness.
pub struct Ended {
    pub reason: EndReason,
    pub after: Duration,
    pub notices: Vec<EngineNotice>,
    /// Message keys of the engine errors reported before the end (only collected by
    /// [`EngineRig::wait_ended_with_errors`]).
    pub errors: Vec<String>,
    /// Whether a Locked status arrived while waiting for the end.
    pub saw_locked: bool,
}

/// A lock request for `duration` whose hard deadline stays within the development caps.
pub fn dev_request(duration: Duration) -> LockRequest {
    dev_request_for(duration, LockTargets::KEYBOARD)
}

/// [`dev_request`] for the given targets.
pub fn dev_request_for(duration: Duration, targets: LockTargets) -> LockRequest {
    LockRequest {
        duration,
        max_lock: DEV_MAX_HARD_DEADLINE,
        targets,
    }
}

/// Mice and touchpads only: keys pass, except the key that completes Ctrl+Alt+K (ADR 0013).
pub const MOUSE_ONLY: LockTargets = LockTargets {
    keyboard: false,
    mouse: true,
};

/// The in-process engine plus its event stream.
pub struct EngineRig {
    engine: Engine,
    events: Receiver<EngineEvent>,
    state: SessionState,
}

impl EngineRig {
    pub fn start() -> Result<Self, String> {
        let (engine, events) = Engine::start().map_err(|e| e.details())?;
        Ok(EngineRig {
            engine,
            events,
            state: SessionState::Idle,
        })
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Requests a keyboard lock of `duration`, clamped to the development caps.
    pub fn lock(&mut self, duration: Duration) -> Result<Instant, String> {
        self.lock_with(duration, LockTargets::KEYBOARD)
    }

    /// Requests a lock of `targets` for `duration`, clamped to the development caps.
    pub fn lock_with(
        &mut self,
        duration: Duration,
        targets: LockTargets,
    ) -> Result<Instant, String> {
        self.drain_pending();
        let at = Instant::now();
        self.engine
            .lock(dev_request_for(duration, targets))
            .map_err(|e| e.details())?;
        Ok(at)
    }

    /// Requests a keyboard lock with no session timer, so only the hard deadline (or another
    /// exit) can end it, and waits until it is engaged.
    pub fn lock_without_session_timer(&mut self, duration: Duration) -> Result<Instant, String> {
        self.lock_without_session_timer_with(duration, LockTargets::KEYBOARD)
    }

    /// [`lock_without_session_timer`](Self::lock_without_session_timer) for `targets`.
    pub fn lock_without_session_timer_with(
        &mut self,
        duration: Duration,
        targets: LockTargets,
    ) -> Result<Instant, String> {
        self.drain_pending();
        let at = Instant::now();
        self.engine
            .lock_without_session_timer(dev_request_for(duration, targets))
            .map_err(|e| e.details())?;
        self.wait_state(SessionState::Locked, Duration::from_secs(3))?;
        Ok(at)
    }

    /// Requests a keyboard lock and waits until it is engaged.
    pub fn lock_and_wait(&mut self, duration: Duration) -> Result<Instant, String> {
        self.lock_with_and_wait(duration, LockTargets::KEYBOARD)
    }

    /// Requests a lock of `targets` and waits until it is engaged.
    pub fn lock_with_and_wait(
        &mut self,
        duration: Duration,
        targets: LockTargets,
    ) -> Result<Instant, String> {
        let at = self.lock_with(duration, targets)?;
        self.wait_state(SessionState::Locked, Duration::from_secs(3))?;
        Ok(at)
    }

    fn drain_pending(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            self.note(&event);
        }
    }

    fn note(&mut self, event: &EngineEvent) {
        if let EngineEvent::Status(status) = event {
            self.state = status.state;
        }
    }

    /// Waits until the engine reports `state`.
    pub fn wait_state(&mut self, state: SessionState, timeout: Duration) -> Result<(), String> {
        let until = Instant::now() + timeout;
        while self.state != state {
            let left = until.saturating_duration_since(Instant::now());
            match self.events.recv_timeout(left) {
                Ok(EngineEvent::Error(e)) => return Err(format!("engine error: {}", e.details())),
                Ok(event) => self.note(&event),
                Err(RecvTimeoutError::Timeout) => {
                    return Err(format!("engine did not reach {state:?} within {timeout:?}"));
                }
                Err(RecvTimeoutError::Disconnected) => return Err("engine stopped".into()),
            }
        }
        Ok(())
    }

    /// Waits for the session to end, collecting notices on the way. An engine error fails it.
    pub fn wait_ended(&mut self, since: Instant, timeout: Duration) -> Result<Ended, String> {
        self.wait_ended_inner(since, timeout, false, &mut || {})
    }

    /// Like [`wait_ended`](Self::wait_ended), but engine errors are expected and collected, for
    /// scenarios where a failure is what ends the session.
    pub fn wait_ended_with_errors(
        &mut self,
        since: Instant,
        timeout: Duration,
    ) -> Result<Ended, String> {
        self.wait_ended_inner(since, timeout, true, &mut || {})
    }

    /// Like [`wait_ended_with_errors`](Self::wait_ended_with_errors), but calls `on_end` the
    /// moment the session-ended event arrives (before waiting for the Idle status).
    pub fn wait_ended_with_errors_then(
        &mut self,
        since: Instant,
        timeout: Duration,
        on_end: &mut dyn FnMut(),
    ) -> Result<Ended, String> {
        self.wait_ended_inner(since, timeout, true, on_end)
    }

    fn wait_ended_inner(
        &mut self,
        since: Instant,
        timeout: Duration,
        collect_errors: bool,
        on_end: &mut dyn FnMut(),
    ) -> Result<Ended, String> {
        let until = Instant::now() + timeout;
        let mut notices = Vec::new();
        let mut errors = Vec::new();
        let mut saw_locked = false;
        loop {
            let left = until.saturating_duration_since(Instant::now());
            match self.events.recv_timeout(left) {
                Ok(EngineEvent::SessionEnded { reason }) => {
                    on_end();
                    let after = since.elapsed();
                    // Let the trailing Idle status arrive too.
                    let _ = self.wait_state(SessionState::Idle, Duration::from_secs(1));
                    return Ok(Ended {
                        reason,
                        after,
                        notices,
                        errors,
                        saw_locked,
                    });
                }
                Ok(EngineEvent::Notice(n)) => notices.push(n),
                Ok(EngineEvent::Error(e)) if collect_errors => {
                    errors.push(e.message_key().to_owned());
                }
                Ok(EngineEvent::Error(e)) => return Err(format!("engine error: {}", e.details())),
                Ok(event) => {
                    if matches!(&event, EngineEvent::Status(s) if s.state == SessionState::Locked) {
                        saw_locked = true;
                    }
                    self.note(&event);
                }
                Err(RecvTimeoutError::Timeout) => {
                    return Err(format!("session did not end within {timeout:?}"));
                }
                Err(RecvTimeoutError::Disconnected) => return Err("engine stopped".into()),
            }
        }
    }

    /// Returns the engine to idle after a scenario, whatever happened.
    pub fn ensure_idle(&mut self) {
        self.drain_pending();
        if self.state != SessionState::Idle {
            let _ = self.engine.unlock();
            let _ = self.wait_state(
                SessionState::Idle,
                DEV_MAX_HARD_DEADLINE + Duration::from_secs(5),
            );
        }
        release_watched_keys();
    }
}

/// Fails unless the process has no Raw Input registration: the engine registers only during a
/// session (invariant 4).
pub fn expect_no_raw_input_registration() -> Result<(), String> {
    match system::registered_raw_input_count() {
        Some(0) => Ok(()),
        Some(n) => Err(format!(
            "{n} Raw Input registration(s) left while idle (invariant 4)"
        )),
        None => Err("GetRegisteredRawInputDevices failed".into()),
    }
}

/// Fails unless `ended` has `expected` as its reason.
pub fn expect_reason(ended: &Ended, expected: EndReason) -> Result<(), String> {
    if ended.reason == expected {
        Ok(())
    } else {
        Err(format!(
            "ended with {:?}, expected {expected:?}",
            ended.reason
        ))
    }
}

// ---------------------------------------------------------------------------------------------
// Mouse probes
// ---------------------------------------------------------------------------------------------

/// One mouse probe: a tagged event (or press and release) of one kind. The observer swallows
/// tagged buttons and wheel turns after counting them, so a probe never clicks or scrolls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Probe {
    Move,
    Left,
    Right,
    Middle,
    X1,
    X2,
    Wheel,
    HWheel,
}

impl Probe {
    pub const ALL: [Probe; 8] = [
        Probe::Move,
        Probe::Left,
        Probe::Right,
        Probe::Middle,
        Probe::X1,
        Probe::X2,
        Probe::Wheel,
        Probe::HWheel,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Probe::Move => "move",
            Probe::Left => "left button",
            Probe::Right => "right button",
            Probe::Middle => "middle button",
            Probe::X1 => "X1 button",
            Probe::X2 => "X2 button",
            Probe::Wheel => "wheel",
            Probe::HWheel => "horizontal wheel",
        }
    }

    /// The events of this probe; a move goes `dx` to the right.
    fn inputs(self, dx: i32) -> Vec<MouseInput> {
        let click = |b| vec![MouseInput::Down(b), MouseInput::Up(b)];
        match self {
            Probe::Move => vec![MouseInput::Move { dx, dy: 0 }],
            Probe::Left => click(Button::Left),
            Probe::Right => click(Button::Right),
            Probe::Middle => click(Button::Middle),
            Probe::X1 => click(Button::X1),
            Probe::X2 => click(Button::X2),
            Probe::Wheel => vec![MouseInput::Wheel(120)],
            Probe::HWheel => vec![MouseInput::HWheel(120)],
        }
    }

    /// The kinds the observer counts for this probe.
    fn kinds(self) -> &'static [MouseKind] {
        match self {
            Probe::Move => &[MouseKind::Move],
            Probe::Left => &[MouseKind::LeftDown, MouseKind::LeftUp],
            Probe::Right => &[MouseKind::RightDown, MouseKind::RightUp],
            Probe::Middle => &[MouseKind::MiddleDown, MouseKind::MiddleUp],
            Probe::X1 => &[MouseKind::X1Down, MouseKind::X1Up],
            Probe::X2 => &[MouseKind::X2Down, MouseKind::X2Up],
            Probe::Wheel => &[MouseKind::Wheel],
            Probe::HWheel => &[MouseKind::HWheel],
        }
    }

    /// Every event of this probe got past KeyClean.
    fn passed(self, observer: &Observer) -> bool {
        self.kinds().iter().all(|&k| observer.mouse(k) > 0)
    }

    /// Any event of this probe got past KeyClean.
    fn leaked(self, observer: &Observer) -> bool {
        self.kinds().iter().any(|&k| observer.mouse(k) > 0)
    }
}

/// Probe kinds that didn't reach the observer even while idle (set once by
/// [`check_mouse_environment`]); the checks leave them out and say so.
static UNUSABLE_PROBES: OnceLock<Vec<Probe>> = OnceLock::new();

fn usable_probes() -> Vec<Probe> {
    let unusable = UNUSABLE_PROBES.get().map_or(&[][..], Vec::as_slice);
    Probe::ALL
        .into_iter()
        .filter(|p| !unusable.contains(p))
        .collect()
}

/// A note for scenario details when some probe kinds can't be used on this machine.
pub fn unusable_probes_note() -> String {
    match UNUSABLE_PROBES.get() {
        Some(list) if !list.is_empty() => {
            format!(
                " (not probed, they don't reach hooks here: {})",
                names(list)
            )
        }
        _ => String::new(),
    }
}

pub fn names(probes: &[Probe]) -> String {
    probes
        .iter()
        .map(|p| p.name())
        .collect::<Vec<_>>()
        .join(", ")
}

fn send_probes(probes: &[Probe], dx: i32) -> Result<(), String> {
    let events: Vec<MouseInput> = probes.iter().flat_map(|p| p.inputs(dx)).collect();
    mouse::send(&events).map_err(|e| format!("mouse injection failed: {e}"))
}

/// Sends tagged mouse moves, turning an injection failure into a check failure.
pub fn send_mouse(events: &[MouseInput]) -> Result<(), String> {
    mouse::send(events).map_err(|e| format!("mouse injection failed: {e}"))
}

/// Sends every probe kind while idle and records the kinds that never reach the observer, which
/// the later checks leave out. Returns those kinds; without moves the mouse can't be tested.
pub fn check_mouse_environment(observer: &Observer) -> Result<Vec<Probe>, String> {
    let saved = mouse::cursor_pos();
    observer.reset();
    send_probes(&Probe::ALL, 10)?;
    wait_until(PROBE_WINDOW, || {
        Probe::ALL.iter().all(|p| p.passed(observer))
    });
    let missing: Vec<Probe> = Probe::ALL
        .into_iter()
        .filter(|p| !p.passed(observer))
        .collect();
    // Undo the probe move.
    if let Some((x, y)) = saved {
        mouse::set_cursor_pos(x, y);
    }
    let _ = UNUSABLE_PROBES.set(missing.clone());
    Ok(missing)
}

/// Fails unless every mouse probe is blocked and the cursor stays where it was.
pub fn expect_mouse_probes_blocked(observer: &Observer) -> Result<(), String> {
    let probes = usable_probes();
    let before = cursor_now()?;
    observer.reset();
    send_probes(&probes, 10)?;
    sleep(PROBE_WINDOW);
    let leaked: Vec<Probe> = probes
        .iter()
        .copied()
        .filter(|p| p.leaked(observer))
        .collect();
    if !leaked.is_empty() {
        return Err(format!(
            "mouse probes got through during the lock: {}",
            names(&leaked)
        ));
    }
    if cursor_now()? != before {
        return Err("the cursor moved during the lock".into());
    }
    Ok(())
}

/// The cursor position, or a failure if Windows won't say: an unknown position must never pass
/// for a frozen (or moved) cursor. Never put into messages (privacy).
fn cursor_now() -> Result<(i32, i32), String> {
    mouse::cursor_pos()
        .ok_or_else(|| "GetCursorPos failed, so the cursor couldn't be checked".into())
}

/// Fails unless every mouse probe gets through and a tagged move moves the cursor.
pub fn expect_mouse_probes_pass(observer: &Observer) -> Result<(), String> {
    let probes = usable_probes();
    observer.reset();
    let before = cursor_now()?;
    send_probes(&probes, 10)?;
    wait_until(PROBE_WINDOW, || probes.iter().all(|p| p.passed(observer)));
    let missing: Vec<Probe> = probes
        .iter()
        .copied()
        .filter(|p| !p.passed(observer))
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "mouse input did not get through: {}",
            names(&missing)
        ));
    }
    // The cursor moves after the hook chain, so poll. At the right edge of the screen a move to
    // the right changes nothing, so try once to the left as well.
    let moved = || mouse::cursor_pos().is_some_and(|now| now != before);
    if wait_until(PROBE_WINDOW, moved) {
        return Ok(());
    }
    send_mouse(&[MouseInput::Move { dx: -10, dy: 0 }])?;
    if wait_until(PROBE_WINDOW, moved) {
        return Ok(());
    }
    Err("tagged moves got through, but the cursor didn't move after".into())
}

/// Fails if Windows considers any mouse button held after a check, e.g. a person holding one.
///
/// Not a stuck-button test: the observer swallows every tagged button, so no probe ever changes
/// Windows' button state. Button balance is covered by the `keyclean-core` unit tests
/// (`mouse::ButtonTracker`), by the observer counts (S26's swallowed-release check) and by the
/// manual test.
pub fn expect_no_held_buttons() -> Result<(), String> {
    let held = held::held_mouse_buttons();
    if held.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Windows reports mouse buttons held: {}",
            held.join(", ")
        ))
    }
}

/// Sends a tagged release of every button, so a press the engine swallowed doesn't keep its
/// drain waiting. Harmless: KeyClean swallows a release whose press it blocked, and the observer
/// swallows any tagged release that reaches it.
pub fn release_mouse_buttons() {
    let _ = mouse::send(&[
        MouseInput::Up(Button::Left),
        MouseInput::Up(Button::Right),
        MouseInput::Up(Button::Middle),
        MouseInput::Up(Button::X1),
        MouseInput::Up(Button::X2),
    ]);
}

/// Puts the cursor back where it was when saved, on every path out of a scenario.
pub struct CursorGuard(Option<(i32, i32)>);

impl CursorGuard {
    pub fn save() -> Self {
        CursorGuard(mouse::cursor_pos())
    }
}

impl Drop for CursorGuard {
    fn drop(&mut self) {
        if let Some((x, y)) = self.0 {
            mouse::set_cursor_pos(x, y);
        }
    }
}
