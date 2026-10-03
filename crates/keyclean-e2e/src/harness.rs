//! Shared plumbing: the in-process engine, probes, waits and cleanup.

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use keyclean_win::keyclean_core::policy::DEV_MAX_HARD_DEADLINE;
use keyclean_win::testkit::held;
use keyclean_win::testkit::inject::{self, Stroke, vk};
use keyclean_win::testkit::observer::Observer;
use keyclean_win::{EndReason, Engine, EngineEvent, EngineNotice, LockRequest, SessionState};

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
}

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

    /// Requests a lock of `duration`, clamped to the development caps.
    pub fn lock(&mut self, duration: Duration) -> Result<Instant, String> {
        self.drain_pending();
        let request = LockRequest {
            duration,
            max_lock: DEV_MAX_HARD_DEADLINE,
        };
        let at = Instant::now();
        self.engine.lock(request).map_err(|e| e.details())?;
        Ok(at)
    }

    /// Requests a lock and waits until it is engaged.
    pub fn lock_and_wait(&mut self, duration: Duration) -> Result<Instant, String> {
        let at = self.lock(duration)?;
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

    /// Waits for the session to end, collecting notices on the way.
    pub fn wait_ended(&mut self, since: Instant, timeout: Duration) -> Result<Ended, String> {
        let until = Instant::now() + timeout;
        let mut notices = Vec::new();
        loop {
            let left = until.saturating_duration_since(Instant::now());
            match self.events.recv_timeout(left) {
                Ok(EngineEvent::SessionEnded { reason }) => {
                    let after = since.elapsed();
                    // Let the trailing Idle status arrive too.
                    let _ = self.wait_state(SessionState::Idle, Duration::from_secs(1));
                    return Ok(Ended {
                        reason,
                        after,
                        notices,
                    });
                }
                Ok(EngineEvent::Notice(n)) => notices.push(n),
                Ok(EngineEvent::Error(e)) => return Err(format!("engine error: {}", e.details())),
                Ok(event) => self.note(&event),
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
