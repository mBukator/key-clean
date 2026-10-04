//! Engine restart budget: how often the app may restart a crashed engine process.
//!
//! At most [`MAX_RESTARTS`] restarts in any [`RESTART_WINDOW`]. After that the engine stays
//! stopped and the error is shown, so a crash loop can't spin forever.

use std::time::Duration;

use crate::time::MonoTime;

/// Restarts allowed within one [`RESTART_WINDOW`].
pub const MAX_RESTARTS: usize = 3;

/// The sliding window the restart limit applies to.
pub const RESTART_WINDOW: Duration = Duration::from_secs(5 * 60);

/// Tracks recent restarts on the injectable clock.
#[derive(Clone, Debug)]
pub struct RestartBudget {
    max: usize,
    window: Duration,
    /// Times of the restarts still inside the window, oldest first.
    recent: Vec<MonoTime>,
}

impl Default for RestartBudget {
    fn default() -> Self {
        Self::new()
    }
}

impl RestartBudget {
    /// A budget of [`MAX_RESTARTS`] per [`RESTART_WINDOW`].
    pub fn new() -> Self {
        Self::with_limits(MAX_RESTARTS, RESTART_WINDOW)
    }

    /// A budget of `max` restarts per `window`.
    pub fn with_limits(max: usize, window: Duration) -> Self {
        RestartBudget {
            max,
            window,
            recent: Vec::with_capacity(max),
        }
    }

    /// Whether a restart at `now` is allowed. If it is, it is recorded. A restart drops out of the
    /// window once `window` has fully elapsed since it.
    pub fn allow(&mut self, now: MonoTime) -> bool {
        let window = self.window;
        self.recent
            .retain(|&at| now.saturating_duration_since(at) < window);
        if self.recent.len() >= self.max {
            return false;
        }
        self.recent.push(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::{Clock, FakeClock};

    #[test]
    fn three_allowed_then_refused() {
        let clock = FakeClock::new();
        let mut budget = RestartBudget::new();
        for _ in 0..3 {
            assert!(budget.allow(clock.now()));
            clock.advance(Duration::from_secs(10));
        }
        assert!(!budget.allow(clock.now()));
        // A refusal isn't recorded as a restart.
        assert!(!budget.allow(clock.now()));
    }

    #[test]
    fn allowed_again_after_the_window_slides() {
        let clock = FakeClock::new();
        let mut budget = RestartBudget::new();
        assert!(budget.allow(clock.now())); // t = 0
        clock.advance(Duration::from_secs(60));
        assert!(budget.allow(clock.now())); // t = 60 s
        assert!(budget.allow(clock.now())); // t = 60 s
        clock.advance(Duration::from_secs(239));
        assert!(!budget.allow(clock.now())); // t = 299 s: all three still inside
        clock.advance(Duration::from_secs(1));
        assert!(budget.allow(clock.now())); // t = 300 s: the first one dropped out
        assert!(!budget.allow(clock.now()));
    }

    #[test]
    fn exactly_at_the_window_edge_drops_out() {
        let clock = FakeClock::new();
        let mut budget = RestartBudget::with_limits(1, Duration::from_secs(5));
        assert!(budget.allow(clock.now()));
        clock.advance(Duration::from_secs(5) - Duration::from_nanos(1));
        assert!(!budget.allow(clock.now()));
        clock.advance(Duration::from_nanos(1));
        assert!(budget.allow(clock.now()));
    }
}
