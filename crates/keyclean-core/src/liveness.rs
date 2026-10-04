//! Hook-liveness rule: did the keyboard hook see a key that Raw Input saw?
//!
//! Windows can remove a low-level hook silently (`LowLevelHooksTimeout`). During a lock the
//! engine also registers keyboard Raw Input, which arrives independently of the hook. When a raw
//! keyboard message arrives, the engine waits [`LIVENESS_CHECK_DELAY`] and then asks
//! [`hook_alive`] whether the hook was called around the same time. Timestamps rather than
//! counts, because the two streams don't pair one-to-one (e.g. Pause and fake shifts produce extra
//! raw messages).

use std::time::Duration;

use crate::time::MonoTime;

/// How long after a raw keyboard message the engine checks the hook.
pub const LIVENESS_CHECK_DELAY: Duration = Duration::from_millis(250);

/// How much earlier than the raw message the last hook call may be and still count. Covers the
/// hook being called before the raw message is stamped.
pub const LIVENESS_SLACK: Duration = Duration::from_millis(100);

/// True if the hook was called no more than [`LIVENESS_SLACK`] before the raw message at
/// `raw_at` (or any time after it). `None` means the hook was never called this session.
pub fn hook_alive(raw_at: MonoTime, last_hook_at: Option<MonoTime>) -> bool {
    last_hook_at.is_some_and(|hook| hook.saturating_add(LIVENESS_SLACK) >= raw_at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::{Clock, FakeClock};

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn hook_just_before_raw_is_alive() {
        let clock = FakeClock::new();
        clock.advance(ms(1000));
        let hook = clock.now();
        clock.advance(ms(2));
        assert!(hook_alive(clock.now(), Some(hook)));
    }

    #[test]
    fn hook_after_raw_is_alive() {
        let clock = FakeClock::new();
        clock.advance(ms(1000));
        let raw = clock.now();
        clock.advance(ms(5));
        assert!(hook_alive(raw, Some(clock.now())));
    }

    #[test]
    fn stale_hook_is_dead() {
        let clock = FakeClock::new();
        clock.advance(ms(1000));
        let hook = clock.now();
        clock.advance(ms(5000));
        assert!(!hook_alive(clock.now(), Some(hook)));
    }

    #[test]
    fn slack_edges() {
        let clock = FakeClock::new();
        clock.advance(ms(1000));
        let hook = clock.now();
        let exactly = hook.saturating_add(LIVENESS_SLACK);
        assert!(hook_alive(exactly, Some(hook)));
        let just_past = exactly.saturating_add(Duration::from_nanos(1));
        assert!(!hook_alive(just_past, Some(hook)));
    }

    #[test]
    fn never_called_is_dead() {
        let clock = FakeClock::new();
        assert!(!hook_alive(clock.now(), None));
        clock.advance(ms(1000));
        assert!(!hook_alive(clock.now(), None));
    }
}
