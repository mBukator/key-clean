//! Monotonic time from `QueryPerformanceCounter`.
//!
//! QPC keeps counting through sleep and hibernate and is unaffected by wall-clock changes
//! (<https://learn.microsoft.com/en-us/windows/win32/sysinfo/acquiring-high-resolution-time-stamps>).
//! The hook compares raw tick counts against a precomputed deadline, which is O(1) and
//! allocation-free.

use std::sync::OnceLock;
use std::time::Duration;

use keyclean_core::time::{Clock, MonoTime};
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

const NANOS_PER_SEC: u128 = 1_000_000_000;

/// The current QPC tick count. Returns 0 if the call fails (it can't on Windows XP and later).
pub(crate) fn ticks() -> u64 {
    let mut count = 0i64;
    // SAFETY: `count` is a valid, writable i64 for the duration of the call.
    let _ = unsafe { QueryPerformanceCounter(&mut count) };
    u64::try_from(count).unwrap_or(0)
}

/// Ticks per second. Fixed at boot, so it is read once.
pub(crate) fn frequency() -> u64 {
    static FREQUENCY: OnceLock<u64> = OnceLock::new();
    *FREQUENCY.get_or_init(|| {
        let mut freq = 0i64;
        // SAFETY: `freq` is a valid, writable i64 for the duration of the call.
        let _ = unsafe { QueryPerformanceFrequency(&mut freq) };
        u64::try_from(freq).unwrap_or(0).max(1)
    })
}

/// Converts a duration to QPC ticks, saturating.
pub(crate) fn duration_to_ticks(d: Duration) -> u64 {
    let ticks = d.as_nanos().saturating_mul(u128::from(frequency())) / NANOS_PER_SEC;
    u64::try_from(ticks).unwrap_or(u64::MAX)
}

/// Converts QPC ticks to a duration, saturating.
pub(crate) fn ticks_to_duration(ticks: u64) -> Duration {
    let nanos = u128::from(ticks).saturating_mul(NANOS_PER_SEC) / u128::from(frequency());
    Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
}

/// The engine's real clock.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct QpcClock;

impl QpcClock {
    /// The time for an already-read tick count, so callers can share one reading between the
    /// session (in [`MonoTime`]) and the hook (in ticks).
    pub(crate) fn at(ticks: u64) -> MonoTime {
        MonoTime::from_since_origin(ticks_to_duration(ticks))
    }
}

impl Clock for QpcClock {
    fn now(&self) -> MonoTime {
        Self::at(ticks())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_are_monotonic() {
        let a = ticks();
        let b = ticks();
        assert!(b >= a);
    }

    #[test]
    fn conversions_round_trip_within_a_tick() {
        for d in [
            Duration::from_millis(1),
            Duration::from_secs(10),
            Duration::from_secs(1800),
        ] {
            let back = ticks_to_duration(duration_to_ticks(d));
            let tick = ticks_to_duration(1).max(Duration::from_nanos(1));
            assert!(back <= d && d - back <= tick, "{d:?} -> {back:?}");
        }
    }

    #[test]
    fn huge_durations_saturate() {
        assert_eq!(duration_to_ticks(Duration::MAX), u64::MAX);
    }
}
