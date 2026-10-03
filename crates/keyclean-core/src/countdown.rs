//! The visible countdown (§15): which whole second to show, and when it next changes.
//!
//! The countdown is derived from the time remaining until the session deadline, never from a
//! counter decremented once per second, so a late or early timer can't make it drift.

use std::time::Duration;

/// The whole seconds to display for `remaining`, rounded up: a 2-minute lock shows `02:00` when
/// it starts and `00:01` during its last second. Shows 0 only once the time is up.
pub fn display_secs(remaining: Duration) -> u64 {
    remaining
        .as_secs()
        .saturating_add(u64::from(remaining.subsec_nanos() > 0))
}

/// How long until [`display_secs`] changes, or `None` once it shows 0.
pub fn next_tick(remaining: Duration) -> Option<Duration> {
    let shown = display_secs(remaining);
    if shown == 0 {
        return None;
    }
    // The display drops from `shown` to `shown - 1` when `remaining` reaches `shown - 1` seconds.
    Some(remaining.saturating_sub(Duration::from_secs(shown - 1)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::{Clock, FakeClock, MonoTime};

    const SECOND: Duration = Duration::from_secs(1);

    const fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn display_rounds_up() {
        assert_eq!(display_secs(Duration::from_secs(120)), 120);
        assert_eq!(display_secs(ms(119_999)), 120);
        assert_eq!(display_secs(ms(119_000)), 119);
        assert_eq!(display_secs(ms(1)), 1);
        assert_eq!(display_secs(Duration::from_nanos(1)), 1);
        assert_eq!(display_secs(Duration::ZERO), 0);
    }

    #[test]
    fn next_tick_lands_on_the_second_boundary() {
        assert_eq!(next_tick(Duration::from_secs(120)), Some(SECOND));
        assert_eq!(next_tick(ms(119_400)), Some(ms(400)));
        assert_eq!(next_tick(ms(1_000)), Some(SECOND));
        assert_eq!(next_tick(ms(250)), Some(ms(250)));
        assert_eq!(next_tick(Duration::ZERO), None);
    }

    #[test]
    fn an_early_timer_waits_for_the_rest() {
        // Expected at 2.000 s remaining, fired 3 ms early: the display still shows 3.
        assert_eq!(display_secs(ms(2_003)), 3);
        assert_eq!(next_tick(ms(2_003)), Some(ms(3)));
    }

    #[test]
    fn a_session_counts_down_once_per_second() {
        let clock = FakeClock::new();
        let deadline = MonoTime::ZERO.saturating_add(ms(3_250));
        let remaining = || deadline.saturating_duration_since(clock.now());

        let mut shown = vec![display_secs(remaining())];
        let mut at = Vec::new();
        while let Some(after) = next_tick(remaining()) {
            clock.advance(after);
            at.push(clock.now().since_origin());
            shown.push(display_secs(remaining()));
        }
        assert_eq!(shown, [4, 3, 2, 1, 0]);
        assert_eq!(at, [ms(250), ms(1_250), ms(2_250), ms(3_250)]);
    }

    #[test]
    fn long_durations_dont_overflow() {
        assert_eq!(display_secs(Duration::MAX), u64::MAX);
        assert!(next_tick(Duration::from_secs(3_600)).is_some());
    }
}
