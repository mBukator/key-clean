//! User settings (§32) and the lock duration range (ADR 0015). Pure: the app reads and writes the
//! settings file and hands the stored values to [`Settings::from_stored`], which never fails.
//!
//! Nothing about a running lock is stored here (invariant 9): only the maximum lock duration and the
//! choices of the last lock that started.

use std::time::Duration;

use crate::policy::{ABSOLUTE_MAX_LOCK, DEFAULT_MAX_LOCK, LockTargets};
use crate::presets::DEFAULT_DURATION;

/// The shortest lock a user can ask for (the Custom field's lower bound).
pub const MIN_DURATION: Duration = Duration::from_secs(10);

/// The smallest maximum lock duration setting, in minutes.
pub const MIN_MAX_LOCK_MINUTES: u64 = 1;

/// The largest maximum lock duration setting, in minutes ([`ABSOLUTE_MAX_LOCK`], ADR 0006).
pub const MAX_MAX_LOCK_MINUTES: u64 = ABSOLUTE_MAX_LOCK.as_secs() / 60;

/// The default maximum lock duration setting, in minutes ([`DEFAULT_MAX_LOCK`]).
pub const DEFAULT_MAX_LOCK_MINUTES: u64 = DEFAULT_MAX_LOCK.as_secs() / 60;

/// Why a requested lock duration was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DurationError {
    /// Shorter than [`MIN_DURATION`].
    TooShort,
    /// Longer than the maximum lock duration setting.
    TooLong,
    /// Not a whole number of seconds.
    NotWholeSeconds,
}

impl std::fmt::Display for DurationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DurationError::TooShort => f.write_str("shorter than the minimum lock duration"),
            DurationError::TooLong => f.write_str("longer than the maximum lock duration setting"),
            DurationError::NotWholeSeconds => f.write_str("not a whole number of seconds"),
        }
    }
}

impl std::error::Error for DurationError {}

/// Checks a requested lock duration against the range [`MIN_DURATION`]..=`max_lock`, in whole
/// seconds. `max_lock` is itself capped at [`ABSOLUTE_MAX_LOCK`]. The engine's safety policy
/// (`plan_lock`) still applies on top of this.
pub fn check_duration(duration: Duration, max_lock: Duration) -> Result<Duration, DurationError> {
    let max_lock = max_lock.min(ABSOLUTE_MAX_LOCK);
    if duration.subsec_nanos() != 0 {
        Err(DurationError::NotWholeSeconds)
    } else if duration < MIN_DURATION {
        Err(DurationError::TooShort)
    } else if duration > max_lock {
        Err(DurationError::TooLong)
    } else {
        Ok(duration)
    }
}

/// Clamps a maximum lock duration setting to [`MIN_MAX_LOCK_MINUTES`]..=[`MAX_MAX_LOCK_MINUTES`].
pub fn clamp_max_lock_minutes(minutes: u64) -> u64 {
    minutes.clamp(MIN_MAX_LOCK_MINUTES, MAX_MAX_LOCK_MINUTES)
}

/// The values as read from the settings file. Each is `None` when missing or of the wrong type.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StoredSettings {
    /// Maximum lock duration, in minutes.
    pub max_lock_minutes: Option<u64>,
    /// Duration of the last lock that started, in seconds.
    pub last_duration_secs: Option<u64>,
    /// Whether the last lock that started blocked the keyboard.
    pub last_keyboard: Option<bool>,
    /// Whether the last lock that started blocked the mouse and touchpad.
    pub last_mouse: Option<bool>,
}

/// Settings in effect, always within their ranges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    max_lock_minutes: u64,
    last_duration: Duration,
    last_targets: LockTargets,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            max_lock_minutes: DEFAULT_MAX_LOCK_MINUTES,
            last_duration: DEFAULT_DURATION,
            last_targets: LockTargets::KEYBOARD,
        }
    }
}

impl Settings {
    /// Settings from stored values. Missing values take their defaults; out-of-range ones are
    /// clamped; targets that block nothing fall back to the keyboard.
    pub fn from_stored(stored: StoredSettings) -> Self {
        let max_lock_minutes = stored
            .max_lock_minutes
            .map_or(DEFAULT_MAX_LOCK_MINUTES, clamp_max_lock_minutes);
        let targets = LockTargets {
            keyboard: stored.last_keyboard.unwrap_or(true),
            mouse: stored.last_mouse.unwrap_or(false),
        };
        let mut settings = Settings {
            max_lock_minutes,
            last_duration: stored
                .last_duration_secs
                .map_or(DEFAULT_DURATION, Duration::from_secs),
            last_targets: targets.checked().unwrap_or(LockTargets::KEYBOARD),
        };
        settings.fit_last_duration();
        settings
    }

    /// The values to write to the settings file.
    pub fn to_stored(self) -> StoredSettings {
        StoredSettings {
            max_lock_minutes: Some(self.max_lock_minutes),
            last_duration_secs: Some(self.last_duration.as_secs()),
            last_keyboard: Some(self.last_targets.keyboard),
            last_mouse: Some(self.last_targets.mouse),
        }
    }

    /// The maximum lock duration setting, in minutes.
    pub fn max_lock_minutes(&self) -> u64 {
        self.max_lock_minutes
    }

    /// The maximum lock duration setting.
    pub fn max_lock(&self) -> Duration {
        Duration::from_secs(self.max_lock_minutes * 60)
    }

    /// The duration to preselect: the last lock's, always within the current range.
    pub fn last_duration(&self) -> Duration {
        self.last_duration
    }

    /// The targets to preselect: the last lock's, never empty.
    pub fn last_targets(&self) -> LockTargets {
        self.last_targets
    }

    /// Changes the maximum lock duration (clamped to its range). A remembered duration longer than
    /// the new maximum is shortened to it.
    pub fn set_max_lock_minutes(&mut self, minutes: u64) {
        self.max_lock_minutes = clamp_max_lock_minutes(minutes);
        self.fit_last_duration();
    }

    /// Remembers the choices of a lock that started. Values outside the range are ignored, so a
    /// stored choice is always one the user could make again.
    pub fn remember_lock(&mut self, duration: Duration, targets: LockTargets) {
        if check_duration(duration, self.max_lock()).is_ok() {
            self.last_duration = duration;
        }
        if let Ok(targets) = targets.checked() {
            self.last_targets = targets;
        }
    }

    /// Keeps the remembered duration within [`MIN_DURATION`]..=max lock. Too long is shortened to
    /// the maximum; too short falls back to the default, itself shortened if needed.
    fn fit_last_duration(&mut self) {
        let max_lock = self.max_lock();
        if self.last_duration < MIN_DURATION || self.last_duration.subsec_nanos() != 0 {
            self.last_duration = DEFAULT_DURATION;
        }
        self.last_duration = self.last_duration.min(max_lock);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::DURATION_PRESETS;

    const fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    const fn mins(m: u64) -> Duration {
        Duration::from_secs(m * 60)
    }

    fn stored(max: Option<u64>, last: Option<u64>) -> StoredSettings {
        StoredSettings {
            max_lock_minutes: max,
            last_duration_secs: last,
            ..StoredSettings::default()
        }
    }

    #[test]
    fn range_is_ten_seconds_to_max_lock() {
        let max = mins(30);
        assert_eq!(check_duration(secs(10), max), Ok(secs(10)));
        assert_eq!(check_duration(mins(30), max), Ok(mins(30)));
        assert_eq!(check_duration(secs(9), max), Err(DurationError::TooShort));
        assert_eq!(
            check_duration(Duration::ZERO, max),
            Err(DurationError::TooShort)
        );
        assert_eq!(
            check_duration(mins(30) + secs(1), max),
            Err(DurationError::TooLong)
        );
        assert_eq!(
            check_duration(Duration::from_millis(10_500), max),
            Err(DurationError::NotWholeSeconds)
        );
    }

    #[test]
    fn range_never_exceeds_the_absolute_ceiling() {
        assert_eq!(
            check_duration(ABSOLUTE_MAX_LOCK, Duration::MAX),
            Ok(ABSOLUTE_MAX_LOCK)
        );
        assert_eq!(
            check_duration(secs(7200), Duration::MAX),
            Err(DurationError::TooLong)
        );
    }

    #[test]
    fn presets_fit_the_default_range() {
        for preset in DURATION_PRESETS {
            assert_eq!(check_duration(preset, DEFAULT_MAX_LOCK), Ok(preset));
        }
    }

    #[test]
    fn longer_presets_are_out_of_range_under_a_low_max_lock() {
        assert!(check_duration(mins(1), mins(1)).is_ok());
        assert_eq!(
            check_duration(mins(2), mins(1)),
            Err(DurationError::TooLong)
        );
    }

    #[test]
    fn max_lock_constants_match_the_policy() {
        assert_eq!(mins(DEFAULT_MAX_LOCK_MINUTES), DEFAULT_MAX_LOCK);
        assert_eq!(mins(MAX_MAX_LOCK_MINUTES), ABSOLUTE_MAX_LOCK);
        assert!(MIN_DURATION <= mins(MIN_MAX_LOCK_MINUTES));
    }

    #[test]
    fn max_lock_is_clamped_to_one_to_sixty_minutes() {
        assert_eq!(clamp_max_lock_minutes(0), 1);
        assert_eq!(clamp_max_lock_minutes(1), 1);
        assert_eq!(clamp_max_lock_minutes(45), 45);
        assert_eq!(clamp_max_lock_minutes(60), 60);
        assert_eq!(clamp_max_lock_minutes(61), 60);
        assert_eq!(clamp_max_lock_minutes(u64::MAX), 60);
    }

    #[test]
    fn missing_file_means_defaults() {
        let s = Settings::from_stored(StoredSettings::default());
        assert_eq!(s, Settings::default());
        assert_eq!(s.max_lock(), DEFAULT_MAX_LOCK);
        assert_eq!(s.last_duration(), DEFAULT_DURATION);
        assert_eq!(s.last_targets(), LockTargets::KEYBOARD);
    }

    #[test]
    fn stored_values_round_trip() {
        let s = Settings::from_stored(StoredSettings {
            max_lock_minutes: Some(45),
            last_duration_secs: Some(754),
            last_keyboard: Some(false),
            last_mouse: Some(true),
        });
        assert_eq!(s.max_lock_minutes(), 45);
        assert_eq!(s.last_duration(), secs(754));
        assert_eq!(
            s.last_targets(),
            LockTargets {
                keyboard: false,
                mouse: true
            }
        );
        assert_eq!(Settings::from_stored(s.to_stored()), s);
    }

    #[test]
    fn out_of_range_max_lock_is_clamped_on_load() {
        assert_eq!(
            Settings::from_stored(stored(Some(0), None)).max_lock_minutes(),
            1
        );
        assert_eq!(
            Settings::from_stored(stored(Some(500), None)).max_lock_minutes(),
            60
        );
    }

    #[test]
    fn remembered_duration_longer_than_max_lock_is_shortened_on_load() {
        let s = Settings::from_stored(stored(Some(20), Some(45 * 60)));
        assert_eq!(s.last_duration(), mins(20));
    }

    #[test]
    fn too_short_remembered_duration_falls_back_to_the_default() {
        assert_eq!(
            Settings::from_stored(stored(None, Some(0))).last_duration(),
            DEFAULT_DURATION
        );
        assert_eq!(
            Settings::from_stored(stored(None, Some(9))).last_duration(),
            DEFAULT_DURATION
        );
    }

    #[test]
    fn default_duration_is_shortened_under_a_one_minute_max_lock() {
        let s = Settings::from_stored(stored(Some(1), None));
        assert_eq!(s.last_duration(), mins(1));
        assert!(check_duration(s.last_duration(), s.max_lock()).is_ok());
    }

    #[test]
    fn targets_that_block_nothing_fall_back_to_the_keyboard() {
        let s = Settings::from_stored(StoredSettings {
            last_keyboard: Some(false),
            last_mouse: Some(false),
            ..StoredSettings::default()
        });
        assert_eq!(s.last_targets(), LockTargets::KEYBOARD);
    }

    #[test]
    fn lowering_max_lock_shortens_the_remembered_duration() {
        let mut s = Settings::default();
        s.remember_lock(mins(25), LockTargets::KEYBOARD);
        s.set_max_lock_minutes(10);
        assert_eq!(s.max_lock_minutes(), 10);
        assert_eq!(s.last_duration(), mins(10));
    }

    #[test]
    fn raising_max_lock_keeps_the_remembered_duration() {
        let mut s = Settings::default();
        s.remember_lock(secs(90), LockTargets::ALL);
        s.set_max_lock_minutes(60);
        assert_eq!(s.last_duration(), secs(90));
        assert_eq!(s.last_targets(), LockTargets::ALL);
    }

    #[test]
    fn set_max_lock_clamps() {
        let mut s = Settings::default();
        s.set_max_lock_minutes(0);
        assert_eq!(s.max_lock_minutes(), 1);
        s.set_max_lock_minutes(1000);
        assert_eq!(s.max_lock_minutes(), 60);
    }

    #[test]
    fn invalid_choices_are_not_remembered() {
        let mut s = Settings::default();
        s.remember_lock(secs(5), LockTargets::ALL);
        assert_eq!(s.last_duration(), DEFAULT_DURATION);
        assert_eq!(s.last_targets(), LockTargets::ALL);

        s.remember_lock(
            mins(31),
            LockTargets {
                keyboard: false,
                mouse: false,
            },
        );
        assert_eq!(s.last_duration(), DEFAULT_DURATION);
        assert_eq!(s.last_targets(), LockTargets::ALL);
    }

    #[test]
    fn remember_lock_takes_exactly_max_lock_but_not_fractions() {
        let mut s = Settings::default();
        s.remember_lock(Duration::from_millis(10_500), LockTargets::KEYBOARD);
        assert_eq!(s.last_duration(), DEFAULT_DURATION);
        s.remember_lock(mins(30), LockTargets::KEYBOARD);
        assert_eq!(s.last_duration(), mins(30));
    }

    #[test]
    fn clamped_values_are_what_gets_written_back() {
        let s = Settings::from_stored(StoredSettings {
            max_lock_minutes: Some(0),
            last_duration_secs: Some(4000),
            last_keyboard: Some(false),
            last_mouse: Some(false),
        });
        assert_eq!(
            s.to_stored(),
            StoredSettings {
                max_lock_minutes: Some(1),
                last_duration_secs: Some(60),
                last_keyboard: Some(true),
                last_mouse: Some(false),
            }
        );
    }

    #[test]
    fn every_loaded_duration_is_in_range() {
        for max in [
            None,
            Some(0),
            Some(1),
            Some(2),
            Some(30),
            Some(60),
            Some(99),
        ] {
            for last in [
                None,
                Some(0),
                Some(9),
                Some(10),
                Some(120),
                Some(3600),
                Some(u64::MAX),
            ] {
                let s = Settings::from_stored(stored(max, last));
                assert!(
                    check_duration(s.last_duration(), s.max_lock()).is_ok(),
                    "{max:?} {last:?}"
                );
            }
        }
    }
}
