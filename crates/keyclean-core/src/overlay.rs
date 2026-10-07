//! The overlay rule (invariant 11, ADR 0014): never locked without a visible overlay.
//!
//! A lock attempt first shows the overlay and waits for it to confirm that it is visible
//! ([`ConfirmGate`]); only a confirmation within [`CONFIRM_BUDGET`] lets the lock start. During the
//! lock, [`OverlayWatch`] decides from what the app can observe whether the overlay is still up;
//! if not, the app ends the lock. Ending early is the safe direction: the webview can end a lock,
//! never extend one (invariant 3).

use std::time::Duration;

use crate::time::MonoTime;

/// How long the overlay has to confirm it is visible, counted from the lock request.
pub const CONFIRM_BUDGET: Duration = Duration::from_secs(2);

/// How long the overlay may stay silent during a lock before it counts as crashed or hung. It
/// answers every countdown status, which arrives once a second.
pub const HEARTBEAT_LIMIT: Duration = Duration::from_secs(3);

/// Identifies one lock attempt, so an answer meant for an earlier attempt is never mistaken for
/// the current one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AttemptId(pub u64);

/// What a confirmation from the overlay means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Confirmation {
    /// The current attempt's overlay is visible in time; the lock may start. Carries how long it
    /// took from the lock request.
    Accepted(Duration),
    /// The current attempt's overlay confirmed after the budget ran out; nothing may be locked.
    Late(Duration),
    /// No attempt is waiting, or this answer belongs to another one. Ignore it.
    Stale,
}

/// One pending lock attempt at a time.
#[derive(Clone, Debug, Default)]
pub struct ConfirmGate {
    next: u64,
    pending: Option<(AttemptId, MonoTime)>,
}

impl ConfirmGate {
    /// A gate with no attempt pending.
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts an attempt requested at `now`, or returns `None` while another is pending.
    pub fn begin(&mut self, now: MonoTime) -> Option<AttemptId> {
        if self.pending.is_some() {
            return None;
        }
        self.next = self.next.wrapping_add(1);
        let id = AttemptId(self.next);
        self.pending = Some((id, now));
        Some(id)
    }

    /// The attempt waiting for its overlay, if any.
    pub fn pending(&self) -> Option<AttemptId> {
        self.pending.map(|(id, _)| id)
    }

    /// Time left for the pending attempt to be confirmed, or `None` if no attempt is pending.
    pub fn remaining(&self, now: MonoTime) -> Option<Duration> {
        self.pending.map(|(_, started)| {
            CONFIRM_BUDGET.saturating_sub(now.saturating_duration_since(started))
        })
    }

    /// Records the overlay's confirmation for `id` at `now`. An accepted or late confirmation
    /// ends the attempt.
    pub fn confirm(&mut self, id: AttemptId, now: MonoTime) -> Confirmation {
        match self.pending {
            Some((pending, started)) if pending == id => {
                self.pending = None;
                let took = now.saturating_duration_since(started);
                if took <= CONFIRM_BUDGET {
                    Confirmation::Accepted(took)
                } else {
                    Confirmation::Late(took)
                }
            }
            _ => Confirmation::Stale,
        }
    }

    /// Gives up the pending attempt `id` (timed out, overlay gone, or the lock failed). Does
    /// nothing if `id` isn't the pending attempt.
    pub fn abandon(&mut self, id: AttemptId) {
        if self.pending.is_some_and(|(pending, _)| pending == id) {
            self.pending = None;
        }
    }
}

/// Why the overlay no longer counts as visible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayLoss {
    /// The window was closed or destroyed.
    Closed,
    /// The window is hidden.
    Hidden,
    /// The window is minimized (for example by Show desktop).
    Minimized,
    /// The user switched to another virtual desktop; the overlay stayed behind.
    OtherDesktop,
    /// The page stopped answering (crashed or hung).
    Silent,
}

/// What the app could observe about the overlay window at one check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowFacts {
    /// The window still exists.
    pub exists: bool,
    /// The window is shown.
    pub visible: bool,
    /// The window is minimized.
    pub minimized: bool,
    /// Whether the window is on the current virtual desktop; `None` when Windows couldn't say,
    /// which doesn't count as a loss.
    pub on_current_desktop: Option<bool>,
}

impl WindowFacts {
    /// A window that is up and on the current desktop.
    pub const UP: WindowFacts = WindowFacts {
        exists: true,
        visible: true,
        minimized: false,
        on_current_desktop: Some(true),
    };
}

/// Watches a confirmed overlay for the length of one lock.
#[derive(Clone, Copy, Debug)]
pub struct OverlayWatch {
    last_heartbeat: MonoTime,
}

impl OverlayWatch {
    /// Starts watching an overlay confirmed at `now`. The confirmation counts as its first
    /// heartbeat.
    pub fn new(now: MonoTime) -> Self {
        OverlayWatch {
            last_heartbeat: now,
        }
    }

    /// Records that the overlay answered at `now`.
    pub fn heartbeat(&mut self, now: MonoTime) {
        self.last_heartbeat = self.last_heartbeat.max(now);
    }

    /// Whether the overlay still counts as visible at `now`, given `facts`. A closed window wins
    /// over every other finding.
    pub fn check(&self, now: MonoTime, facts: WindowFacts) -> Option<OverlayLoss> {
        if !facts.exists {
            Some(OverlayLoss::Closed)
        } else if facts.minimized {
            Some(OverlayLoss::Minimized)
        } else if !facts.visible {
            Some(OverlayLoss::Hidden)
        } else if facts.on_current_desktop == Some(false) {
            Some(OverlayLoss::OtherDesktop)
        } else if now.saturating_duration_since(self.last_heartbeat) > HEARTBEAT_LIMIT {
            Some(OverlayLoss::Silent)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::{Clock, FakeClock};

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_confirmation_in_time_is_accepted() {
        let clock = FakeClock::new();
        let mut gate = ConfirmGate::new();
        let id = gate.begin(clock.now()).unwrap();
        assert_eq!(gate.pending(), Some(id));
        clock.advance(ms(350));
        assert_eq!(gate.remaining(clock.now()), Some(ms(1650)));
        assert_eq!(
            gate.confirm(id, clock.now()),
            Confirmation::Accepted(ms(350))
        );
        assert_eq!(gate.pending(), None);
        assert_eq!(gate.remaining(clock.now()), None);
    }

    #[test]
    fn exactly_at_the_budget_is_still_in_time() {
        let clock = FakeClock::new();
        let mut gate = ConfirmGate::new();
        let id = gate.begin(clock.now()).unwrap();
        clock.advance(CONFIRM_BUDGET);
        assert_eq!(gate.remaining(clock.now()), Some(Duration::ZERO));
        assert_eq!(
            gate.confirm(id, clock.now()),
            Confirmation::Accepted(CONFIRM_BUDGET)
        );
    }

    #[test]
    fn a_late_confirmation_never_locks() {
        let clock = FakeClock::new();
        let mut gate = ConfirmGate::new();
        let id = gate.begin(clock.now()).unwrap();
        clock.advance(CONFIRM_BUDGET + ms(1));
        assert_eq!(gate.remaining(clock.now()), Some(Duration::ZERO));
        assert_eq!(
            gate.confirm(id, clock.now()),
            Confirmation::Late(CONFIRM_BUDGET + ms(1))
        );
        assert_eq!(gate.pending(), None, "a late answer ends the attempt");
    }

    #[test]
    fn one_attempt_at_a_time() {
        let clock = FakeClock::new();
        let mut gate = ConfirmGate::new();
        let first = gate.begin(clock.now()).unwrap();
        assert_eq!(gate.begin(clock.now()), None);
        gate.abandon(first);
        let second = gate.begin(clock.now()).unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn an_answer_for_an_earlier_attempt_is_stale() {
        let clock = FakeClock::new();
        let mut gate = ConfirmGate::new();
        let first = gate.begin(clock.now()).unwrap();
        gate.abandon(first);
        assert_eq!(gate.confirm(first, clock.now()), Confirmation::Stale);

        let second = gate.begin(clock.now()).unwrap();
        assert_eq!(gate.confirm(first, clock.now()), Confirmation::Stale);
        assert_eq!(
            gate.pending(),
            Some(second),
            "a stale answer changes nothing"
        );
        // Abandoning an attempt that isn't pending leaves the current one alone.
        gate.abandon(first);
        assert_eq!(gate.pending(), Some(second));
        assert!(matches!(
            gate.confirm(second, clock.now()),
            Confirmation::Accepted(_)
        ));
        assert_eq!(gate.confirm(second, clock.now()), Confirmation::Stale);
    }

    #[test]
    fn no_attempt_means_stale() {
        let mut gate = ConfirmGate::new();
        assert_eq!(
            gate.confirm(AttemptId(1), MonoTime::ZERO),
            Confirmation::Stale
        );
    }

    #[test]
    fn an_overlay_that_is_up_and_answering_is_fine() {
        let clock = FakeClock::new();
        let mut watch = OverlayWatch::new(clock.now());
        for _ in 0..10 {
            clock.advance(Duration::from_secs(1));
            watch.heartbeat(clock.now());
            assert_eq!(watch.check(clock.now(), WindowFacts::UP), None);
        }
    }

    #[test]
    fn each_way_of_losing_the_overlay_is_named() {
        let watch = OverlayWatch::new(MonoTime::ZERO);
        let now = MonoTime::ZERO;
        let cases = [
            (
                WindowFacts {
                    exists: false,
                    ..WindowFacts::UP
                },
                OverlayLoss::Closed,
            ),
            (
                WindowFacts {
                    visible: false,
                    ..WindowFacts::UP
                },
                OverlayLoss::Hidden,
            ),
            (
                WindowFacts {
                    minimized: true,
                    ..WindowFacts::UP
                },
                OverlayLoss::Minimized,
            ),
            (
                WindowFacts {
                    on_current_desktop: Some(false),
                    ..WindowFacts::UP
                },
                OverlayLoss::OtherDesktop,
            ),
        ];
        for (facts, loss) in cases {
            assert_eq!(watch.check(now, facts), Some(loss), "{facts:?}");
        }
    }

    #[test]
    fn a_closed_window_wins_over_other_findings() {
        let watch = OverlayWatch::new(MonoTime::ZERO);
        let gone = WindowFacts {
            exists: false,
            visible: false,
            minimized: true,
            on_current_desktop: Some(false),
        };
        assert_eq!(
            watch.check(MonoTime::from_since_origin(Duration::from_secs(60)), gone),
            Some(OverlayLoss::Closed)
        );
    }

    #[test]
    fn an_unknown_desktop_is_not_a_loss() {
        let watch = OverlayWatch::new(MonoTime::ZERO);
        let facts = WindowFacts {
            on_current_desktop: None,
            ..WindowFacts::UP
        };
        assert_eq!(watch.check(MonoTime::ZERO, facts), None);
    }

    #[test]
    fn silence_past_the_limit_counts_as_a_crash() {
        let clock = FakeClock::new();
        let mut watch = OverlayWatch::new(clock.now());
        clock.advance(HEARTBEAT_LIMIT);
        assert_eq!(
            watch.check(clock.now(), WindowFacts::UP),
            None,
            "at the limit"
        );
        clock.advance(ms(1));
        assert_eq!(
            watch.check(clock.now(), WindowFacts::UP),
            Some(OverlayLoss::Silent)
        );
        watch.heartbeat(clock.now());
        assert_eq!(watch.check(clock.now(), WindowFacts::UP), None);
    }

    #[test]
    fn an_older_heartbeat_never_moves_the_last_one_back() {
        let clock = FakeClock::new();
        let mut watch = OverlayWatch::new(clock.now());
        clock.advance(Duration::from_secs(2));
        watch.heartbeat(clock.now());
        watch.heartbeat(MonoTime::ZERO);
        clock.advance(Duration::from_secs(3));
        assert_eq!(watch.check(clock.now(), WindowFacts::UP), None);
    }
}
