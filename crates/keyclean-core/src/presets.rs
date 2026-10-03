//! Lock duration presets (§11). The single source for the durations the UI offers.

use std::time::Duration;

/// The durations offered for a lock, shortest first.
pub const DURATION_PRESETS: [Duration; 4] = [
    Duration::from_secs(30),
    Duration::from_secs(60),
    Duration::from_secs(2 * 60),
    Duration::from_secs(5 * 60),
];

/// The preset selected by default (§11, §32).
pub const DEFAULT_DURATION: Duration = Duration::from_secs(2 * 60);

/// Whether `duration` is one of [`DURATION_PRESETS`].
pub fn is_preset(duration: Duration) -> bool {
    DURATION_PRESETS.contains(&duration)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::DEFAULT_MAX_LOCK;

    #[test]
    fn default_is_a_preset() {
        assert!(is_preset(DEFAULT_DURATION));
    }

    #[test]
    fn presets_are_sorted_and_within_the_default_max_lock() {
        assert!(DURATION_PRESETS.windows(2).all(|w| w[0] < w[1]));
        assert!(DURATION_PRESETS.iter().all(|d| *d <= DEFAULT_MAX_LOCK));
    }

    #[test]
    fn other_durations_are_not_presets() {
        for secs in [0, 1, 10, 29, 31, 90, 600, 3600] {
            assert!(!is_preset(Duration::from_secs(secs)), "{secs}");
        }
        assert!(!is_preset(Duration::from_millis(30_001)));
    }
}
