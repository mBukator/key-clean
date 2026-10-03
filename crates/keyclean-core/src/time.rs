//! Monotonic time for session deadlines.
//!
//! All deadline math runs on [`MonoTime`], a point on a monotonic timeline with an arbitrary
//! origin. The real clock (in `keyclean-win`) is backed by `QueryPerformanceCounter`, which keeps
//! counting through sleep and ignores wall-clock changes. Tests use [`FakeClock`].

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// A point on a monotonic timeline, stored as the time elapsed since the clock's origin.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MonoTime(Duration);

impl MonoTime {
    /// The clock's origin.
    pub const ZERO: MonoTime = MonoTime(Duration::ZERO);

    /// A time `since_origin` after the clock's origin.
    pub const fn from_since_origin(since_origin: Duration) -> Self {
        MonoTime(since_origin)
    }

    /// Time elapsed since the clock's origin.
    pub const fn since_origin(self) -> Duration {
        self.0
    }

    /// `self + d`, saturating at the maximum representable time instead of overflowing.
    pub fn saturating_add(self, d: Duration) -> Self {
        MonoTime(self.0.saturating_add(d))
    }

    /// `self - earlier`, or zero if `earlier` is later than `self`.
    pub fn saturating_duration_since(self, earlier: MonoTime) -> Duration {
        self.0.saturating_sub(earlier.0)
    }
}

/// A source of monotonic time.
pub trait Clock: Send + Sync {
    /// The current time. Must never go backwards.
    fn now(&self) -> MonoTime;
}

/// A manually driven clock for tests. Starts at [`MonoTime::ZERO`].
#[derive(Debug, Default)]
pub struct FakeClock {
    nanos: AtomicU64,
}

impl FakeClock {
    /// A fake clock at the origin.
    pub const fn new() -> Self {
        FakeClock {
            nanos: AtomicU64::new(0),
        }
    }

    /// Moves the clock forward by `d`.
    pub fn advance(&self, d: Duration) {
        let step = u64::try_from(d.as_nanos()).unwrap_or(u64::MAX);
        // A compare-exchange loop rather than `fetch_update`, which newer Rust deprecates in favour
        // of `try_update`, which older toolchains lack.
        let mut current = self.nanos.load(Ordering::SeqCst);
        while let Err(actual) = self.nanos.compare_exchange_weak(
            current,
            current.saturating_add(step),
            Ordering::SeqCst,
            Ordering::SeqCst,
        ) {
            current = actual;
        }
    }
}

impl Clock for FakeClock {
    fn now(&self) -> MonoTime {
        MonoTime(Duration::from_nanos(self.nanos.load(Ordering::SeqCst)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_clock_starts_at_zero_and_advances() {
        let clock = FakeClock::new();
        assert_eq!(clock.now(), MonoTime::ZERO);
        clock.advance(Duration::from_millis(1500));
        assert_eq!(clock.now().since_origin(), Duration::from_millis(1500));
        clock.advance(Duration::from_secs(2));
        assert_eq!(clock.now().since_origin(), Duration::from_millis(3500));
    }

    #[test]
    fn saturating_math_never_panics() {
        let max = MonoTime::from_since_origin(Duration::MAX);
        assert_eq!(max.saturating_add(Duration::from_secs(1)), max);
        let early = MonoTime::from_since_origin(Duration::from_secs(1));
        let late = MonoTime::from_since_origin(Duration::from_secs(5));
        assert_eq!(early.saturating_duration_since(late), Duration::ZERO);
        assert_eq!(
            late.saturating_duration_since(early),
            Duration::from_secs(4)
        );
    }
}
