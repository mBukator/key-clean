//! The session state machine (§48).
//!
//! ```text
//! Idle ──start──▶ Starting ──locked──▶ Locked
//!                    │                   │
//!                    └──────end(reason)──┴──▶ Unlocking ──finished──▶ Idle
//! ```
//!
//! The native engine owns one [`Session`]; the UI never decides whether input is locked.
//! Illegal transitions return [`TransitionError`] and leave the state unchanged — they never panic.

use std::time::Duration;

use crate::policy::LockPlan;
use crate::time::MonoTime;

/// Where a session is in its lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    /// No lock and no hook.
    Idle,
    /// The engine is installing the hook.
    Starting,
    /// Input is blocked.
    Locked,
    /// The lock has ended; the engine is releasing input.
    Unlocking,
}

/// A system event that ends a session immediately (invariant 9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemTransition {
    /// The machine is going to sleep or hibernate.
    Suspend,
    /// Windows is shutting down, restarting, or logging the user off.
    EndSession,
    /// The workstation was locked (Win+L or otherwise).
    SessionLock,
    /// The console session was disconnected or switched away (fast user switching, RDP).
    SessionDisconnect,
}

/// Why a session ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndReason {
    /// The session duration elapsed.
    Timeout,
    /// The user pressed the emergency chord (Ctrl+Alt+K).
    Emergency,
    /// The hard deadline elapsed (hook check or watchdog).
    HardDeadline,
    /// A system transition (suspend, shutdown, session lock or switch).
    SystemTransition(SystemTransition),
    /// The engine failed; input was released.
    EngineError,
    /// The app asked to unlock (UI button, tray, app exit).
    UserRequest,
}

/// The input that caused a transition attempt, for error reporting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionEvent {
    /// [`Session::start`].
    Start,
    /// [`Session::locked`].
    Locked,
    /// [`Session::end`].
    End(EndReason),
    /// [`Session::finished`].
    Finished,
}

/// A transition that isn't allowed from the current state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransitionError {
    /// The state the session was in.
    pub from: SessionState,
    /// The event that was rejected.
    pub event: SessionEvent,
}

impl std::fmt::Display for TransitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} is not allowed while {:?}", self.event, self.from)
    }
}

impl std::error::Error for TransitionError {}

/// The deadlines of the current session, as absolute monotonic times.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Deadlines {
    /// When the session started.
    pub started_at: MonoTime,
    /// When the session ends normally.
    pub session: MonoTime,
    /// When the session ends no matter what.
    pub hard: MonoTime,
    /// Whether the development caps were applied.
    pub dev_cap: bool,
}

impl Deadlines {
    fn from_plan(plan: LockPlan, now: MonoTime) -> Self {
        Deadlines {
            started_at: now,
            session: now.saturating_add(plan.session),
            hard: now.saturating_add(plan.hard_deadline),
            dev_cap: plan.dev_cap,
        }
    }
}

/// One lock session at a time, owned by the engine.
#[derive(Clone, Debug)]
pub struct Session {
    state: SessionState,
    deadlines: Option<Deadlines>,
    end_reason: Option<EndReason>,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    /// An idle session.
    pub const fn new() -> Self {
        Session {
            state: SessionState::Idle,
            deadlines: None,
            end_reason: None,
        }
    }

    /// The current state.
    pub const fn state(&self) -> SessionState {
        self.state
    }

    /// The current session's deadlines, if one is in progress.
    pub const fn deadlines(&self) -> Option<Deadlines> {
        self.deadlines
    }

    /// Why the current session is ending, once [`Session::end`] has been accepted.
    pub const fn end_reason(&self) -> Option<EndReason> {
        self.end_reason
    }

    /// `Idle → Starting`. Fixes both deadlines relative to `now`, so the hard deadline also covers
    /// the time spent installing the hook.
    pub fn start(&mut self, plan: LockPlan, now: MonoTime) -> Result<Deadlines, TransitionError> {
        self.require(SessionState::Idle, SessionEvent::Start)?;
        let deadlines = Deadlines::from_plan(plan, now);
        self.state = SessionState::Starting;
        self.deadlines = Some(deadlines);
        self.end_reason = None;
        Ok(deadlines)
    }

    /// `Starting → Locked`, once the hook is installed and blocking.
    pub fn locked(&mut self) -> Result<(), TransitionError> {
        self.require(SessionState::Starting, SessionEvent::Locked)?;
        self.state = SessionState::Locked;
        Ok(())
    }

    /// `Starting | Locked → Unlocking`. The first accepted reason wins; ending a session that is
    /// already unlocking or idle is rejected so callers can ignore duplicate signals (for example a
    /// chord and a timeout arriving together).
    pub fn end(&mut self, reason: EndReason) -> Result<(), TransitionError> {
        match self.state {
            SessionState::Starting | SessionState::Locked => {
                self.state = SessionState::Unlocking;
                self.end_reason = Some(reason);
                Ok(())
            }
            from => Err(TransitionError {
                from,
                event: SessionEvent::End(reason),
            }),
        }
    }

    /// `Unlocking → Idle`, once input is released. Returns why the session ended.
    pub fn finished(&mut self) -> Result<EndReason, TransitionError> {
        self.require(SessionState::Unlocking, SessionEvent::Finished)?;
        let reason = self.end_reason.take().unwrap_or(EndReason::EngineError);
        self.state = SessionState::Idle;
        self.deadlines = None;
        Ok(reason)
    }

    /// Which deadline, if any, has passed at `now` while the session is starting or locked. The hard
    /// deadline takes precedence when both have passed.
    pub fn expired(&self, now: MonoTime) -> Option<EndReason> {
        if !matches!(self.state, SessionState::Starting | SessionState::Locked) {
            return None;
        }
        let deadlines = self.deadlines?;
        if now >= deadlines.hard {
            Some(EndReason::HardDeadline)
        } else if now >= deadlines.session {
            Some(EndReason::Timeout)
        } else {
            None
        }
    }

    /// Time left until the session deadline, or `None` when no session is in progress.
    pub fn remaining(&self, now: MonoTime) -> Option<Duration> {
        if !matches!(self.state, SessionState::Starting | SessionState::Locked) {
            return None;
        }
        self.deadlines
            .map(|d| d.session.saturating_duration_since(now))
    }

    fn require(&self, expected: SessionState, event: SessionEvent) -> Result<(), TransitionError> {
        if self.state == expected {
            Ok(())
        } else {
            Err(TransitionError {
                from: self.state,
                event,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{DEFAULT_MAX_LOCK, SafetyProfile, plan_lock};
    use crate::time::{Clock, FakeClock};

    fn plan(secs: u64) -> LockPlan {
        plan_lock(
            Duration::from_secs(secs),
            DEFAULT_MAX_LOCK,
            SafetyProfile::Release,
        )
        .unwrap()
    }

    fn locked_session(clock: &FakeClock, secs: u64) -> Session {
        let mut s = Session::new();
        s.start(plan(secs), clock.now()).unwrap();
        s.locked().unwrap();
        s
    }

    #[test]
    fn full_happy_path() {
        let clock = FakeClock::new();
        let mut s = Session::new();
        assert_eq!(s.state(), SessionState::Idle);

        let d = s.start(plan(60), clock.now()).unwrap();
        assert_eq!(s.state(), SessionState::Starting);
        assert_eq!(d.session.since_origin(), Duration::from_secs(60));
        assert_eq!(d.hard.since_origin(), Duration::from_secs(70));

        s.locked().unwrap();
        assert_eq!(s.state(), SessionState::Locked);

        s.end(EndReason::Emergency).unwrap();
        assert_eq!(s.state(), SessionState::Unlocking);
        assert_eq!(s.end_reason(), Some(EndReason::Emergency));

        assert_eq!(s.finished(), Ok(EndReason::Emergency));
        assert_eq!(s.state(), SessionState::Idle);
        assert_eq!(s.deadlines(), None);
        assert_eq!(s.end_reason(), None);
    }

    #[test]
    fn every_end_reason_is_accepted_from_locked() {
        let reasons = [
            EndReason::Timeout,
            EndReason::Emergency,
            EndReason::HardDeadline,
            EndReason::SystemTransition(SystemTransition::Suspend),
            EndReason::SystemTransition(SystemTransition::EndSession),
            EndReason::SystemTransition(SystemTransition::SessionLock),
            EndReason::SystemTransition(SystemTransition::SessionDisconnect),
            EndReason::EngineError,
            EndReason::UserRequest,
        ];
        for reason in reasons {
            let clock = FakeClock::new();
            let mut s = locked_session(&clock, 30);
            s.end(reason).unwrap();
            assert_eq!(s.finished(), Ok(reason));
        }
    }

    #[test]
    fn starting_can_end_before_locking() {
        let clock = FakeClock::new();
        let mut s = Session::new();
        s.start(plan(30), clock.now()).unwrap();
        s.end(EndReason::EngineError).unwrap();
        assert_eq!(s.finished(), Ok(EndReason::EngineError));
    }

    #[test]
    fn first_end_reason_wins() {
        let clock = FakeClock::new();
        let mut s = locked_session(&clock, 30);
        s.end(EndReason::Emergency).unwrap();
        let err = s.end(EndReason::Timeout).unwrap_err();
        assert_eq!(err.from, SessionState::Unlocking);
        assert_eq!(err.event, SessionEvent::End(EndReason::Timeout));
        assert_eq!(s.finished(), Ok(EndReason::Emergency));
    }

    #[test]
    fn illegal_transitions_are_errors_and_change_nothing() {
        let clock = FakeClock::new();

        // From Idle.
        let mut s = Session::new();
        assert_eq!(
            s.locked(),
            Err(TransitionError {
                from: SessionState::Idle,
                event: SessionEvent::Locked
            })
        );
        assert!(s.end(EndReason::UserRequest).is_err());
        assert!(s.finished().is_err());
        assert_eq!(s.state(), SessionState::Idle);

        // From Starting.
        s.start(plan(30), clock.now()).unwrap();
        assert!(s.start(plan(30), clock.now()).is_err());
        assert!(s.finished().is_err());
        assert_eq!(s.state(), SessionState::Starting);

        // From Locked.
        s.locked().unwrap();
        assert!(s.start(plan(30), clock.now()).is_err());
        assert!(s.locked().is_err());
        assert!(s.finished().is_err());
        assert_eq!(s.state(), SessionState::Locked);

        // From Unlocking.
        s.end(EndReason::Timeout).unwrap();
        assert!(s.start(plan(30), clock.now()).is_err());
        assert!(s.locked().is_err());
        assert_eq!(s.state(), SessionState::Unlocking);
    }

    #[test]
    fn a_new_session_can_start_after_finishing() {
        let clock = FakeClock::new();
        let mut s = locked_session(&clock, 30);
        s.end(EndReason::UserRequest).unwrap();
        s.finished().unwrap();
        clock.advance(Duration::from_secs(5));
        let d = s.start(plan(10), clock.now()).unwrap();
        assert_eq!(d.started_at.since_origin(), Duration::from_secs(5));
        assert_eq!(d.session.since_origin(), Duration::from_secs(15));
    }

    #[test]
    fn expiry_edges() {
        let clock = FakeClock::new();
        let s = locked_session(&clock, 60);

        clock.advance(Duration::from_millis(59_999));
        assert_eq!(s.expired(clock.now()), None);

        clock.advance(Duration::from_millis(1));
        assert_eq!(s.expired(clock.now()), Some(EndReason::Timeout));

        clock.advance(Duration::from_millis(9_999));
        assert_eq!(s.expired(clock.now()), Some(EndReason::Timeout));

        clock.advance(Duration::from_millis(1));
        assert_eq!(s.expired(clock.now()), Some(EndReason::HardDeadline));
    }

    #[test]
    fn hard_deadline_wins_after_a_long_gap() {
        // E.g. the machine slept: QPC kept counting, so both deadlines are past on resume.
        let clock = FakeClock::new();
        let s = locked_session(&clock, 60);
        clock.advance(Duration::from_secs(3600));
        assert_eq!(s.expired(clock.now()), Some(EndReason::HardDeadline));
    }

    #[test]
    fn expiry_applies_while_starting() {
        let clock = FakeClock::new();
        let mut s = Session::new();
        s.start(plan(1), clock.now()).unwrap();
        clock.advance(Duration::from_secs(11));
        assert_eq!(s.expired(clock.now()), Some(EndReason::HardDeadline));
    }

    #[test]
    fn no_expiry_or_remaining_outside_a_lock() {
        let clock = FakeClock::new();
        let mut s = locked_session(&clock, 5);
        s.end(EndReason::UserRequest).unwrap();
        clock.advance(Duration::from_secs(100));
        assert_eq!(s.expired(clock.now()), None);
        assert_eq!(s.remaining(clock.now()), None);
        s.finished().unwrap();
        assert_eq!(s.expired(clock.now()), None);
        assert_eq!(s.remaining(clock.now()), None);
    }

    #[test]
    fn remaining_counts_down_and_saturates() {
        let clock = FakeClock::new();
        let s = locked_session(&clock, 120);
        assert_eq!(s.remaining(clock.now()), Some(Duration::from_secs(120)));
        clock.advance(Duration::from_millis(18_500));
        assert_eq!(
            s.remaining(clock.now()),
            Some(Duration::from_millis(101_500))
        );
        clock.advance(Duration::from_secs(500));
        assert_eq!(s.remaining(clock.now()), Some(Duration::ZERO));
    }
}
