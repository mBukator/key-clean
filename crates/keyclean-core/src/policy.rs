//! Safety policy: how long a lock may last.
//!
//! Every lock gets two limits. The *session* duration is when the lock ends normally
//! ([`EndReason::Timeout`](crate::session::EndReason::Timeout)). The *hard deadline* is an
//! independent backstop that is enforced by the hook and by a watchdog thread:
//! `min(max_lock, session + 10 s)`. Debug builds additionally cap the session at 15 s and the
//! hard deadline at 20 s, so a development build can never lock for long.

use std::time::Duration;

/// Default for the user's "maximum lock duration" setting (§32).
pub const DEFAULT_MAX_LOCK: Duration = Duration::from_secs(30 * 60);

/// Absolute ceiling for the "maximum lock duration" setting. A corrupt or hostile setting can't
/// raise the hard deadline beyond this.
pub const ABSOLUTE_MAX_LOCK: Duration = Duration::from_secs(60 * 60);

/// How far the hard deadline may trail the session duration.
pub const HARD_DEADLINE_GRACE: Duration = Duration::from_secs(10);

/// Session cap in development builds.
pub const DEV_MAX_SESSION: Duration = Duration::from_secs(15);

/// Hard-deadline cap in development builds.
pub const DEV_MAX_HARD_DEADLINE: Duration = Duration::from_secs(20);

/// How long the hook may stay installed after a session ends, waiting for the keys whose presses
/// were blocked to be released (so their key-ups don't reach Windows unpaired).
pub const DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

/// Which set of caps applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SafetyProfile {
    /// Debug builds: session ≤ 15 s, hard deadline ≤ 20 s, "DEV CAP" shown in the UI.
    Dev,
    /// Release builds: real durations.
    Release,
}

/// The durations a lock will actually use, measured from the moment it starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LockPlan {
    /// When the lock ends normally.
    pub session: Duration,
    /// When the lock ends no matter what. Always `>= session`.
    pub hard_deadline: Duration,
    /// True when the development caps were applied (the UI shows "DEV CAP").
    pub dev_cap: bool,
}

/// Why a lock request was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyError {
    /// The requested duration was zero.
    ZeroDuration,
    /// The maximum lock duration setting was zero.
    ZeroMaxLock,
}

impl std::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PolicyError::ZeroDuration => f.write_str("requested lock duration is zero"),
            PolicyError::ZeroMaxLock => f.write_str("maximum lock duration setting is zero"),
        }
    }
}

impl std::error::Error for PolicyError {}

/// Computes the durations for a lock request.
///
/// `requested` is clamped to `max_lock`, which is itself clamped to [`ABSOLUTE_MAX_LOCK`]. The hard
/// deadline is `min(max_lock, session + HARD_DEADLINE_GRACE)`. Under [`SafetyProfile::Dev`] the
/// session is further capped at [`DEV_MAX_SESSION`] and the hard deadline at
/// [`DEV_MAX_HARD_DEADLINE`].
pub fn plan_lock(
    requested: Duration,
    max_lock: Duration,
    profile: SafetyProfile,
) -> Result<LockPlan, PolicyError> {
    if requested.is_zero() {
        return Err(PolicyError::ZeroDuration);
    }
    if max_lock.is_zero() {
        return Err(PolicyError::ZeroMaxLock);
    }

    let max_lock = max_lock.min(ABSOLUTE_MAX_LOCK);
    let mut session = requested.min(max_lock);
    let mut hard_deadline = max_lock.min(session.saturating_add(HARD_DEADLINE_GRACE));

    let dev_cap = profile == SafetyProfile::Dev;
    if dev_cap {
        session = session.min(DEV_MAX_SESSION);
        hard_deadline = hard_deadline
            .min(session.saturating_add(HARD_DEADLINE_GRACE))
            .min(DEV_MAX_HARD_DEADLINE);
    }

    Ok(LockPlan {
        session,
        hard_deadline,
        dev_cap,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    fn release(requested: Duration, max_lock: Duration) -> LockPlan {
        plan_lock(requested, max_lock, SafetyProfile::Release).unwrap()
    }

    fn dev(requested: Duration, max_lock: Duration) -> LockPlan {
        plan_lock(requested, max_lock, SafetyProfile::Dev).unwrap()
    }

    #[test]
    fn release_hard_deadline_is_session_plus_grace() {
        let plan = release(secs(120), DEFAULT_MAX_LOCK);
        assert_eq!(plan.session, secs(120));
        assert_eq!(plan.hard_deadline, secs(130));
        assert!(!plan.dev_cap);
    }

    #[test]
    fn release_hard_deadline_never_exceeds_max_lock() {
        // Session within 10 s of the max: the max wins.
        let plan = release(secs(1795), DEFAULT_MAX_LOCK);
        assert_eq!(plan.session, secs(1795));
        assert_eq!(plan.hard_deadline, DEFAULT_MAX_LOCK);
    }

    #[test]
    fn release_session_is_clamped_to_max_lock() {
        let plan = release(secs(4000), DEFAULT_MAX_LOCK);
        assert_eq!(plan.session, DEFAULT_MAX_LOCK);
        assert_eq!(plan.hard_deadline, DEFAULT_MAX_LOCK);
    }

    #[test]
    fn max_lock_setting_is_clamped_to_absolute_ceiling() {
        let plan = release(Duration::MAX, Duration::MAX);
        assert_eq!(plan.session, ABSOLUTE_MAX_LOCK);
        assert_eq!(plan.hard_deadline, ABSOLUTE_MAX_LOCK);
    }

    #[test]
    fn dev_caps_session_and_hard_deadline() {
        let plan = dev(secs(120), DEFAULT_MAX_LOCK);
        assert_eq!(plan.session, DEV_MAX_SESSION);
        assert_eq!(plan.hard_deadline, DEV_MAX_HARD_DEADLINE);
        assert!(plan.dev_cap);
    }

    #[test]
    fn dev_short_session_keeps_grace_under_cap() {
        let plan = dev(secs(5), DEFAULT_MAX_LOCK);
        assert_eq!(plan.session, secs(5));
        assert_eq!(plan.hard_deadline, secs(15));
    }

    #[test]
    fn dev_ten_second_session() {
        // The M1 shell's Lock button.
        let plan = dev(secs(10), DEFAULT_MAX_LOCK);
        assert_eq!(plan.session, secs(10));
        assert_eq!(plan.hard_deadline, DEV_MAX_HARD_DEADLINE);
    }

    #[test]
    fn dev_respects_small_max_lock() {
        let plan = dev(secs(60), secs(8));
        assert_eq!(plan.session, secs(8));
        assert_eq!(plan.hard_deadline, secs(8));
    }

    #[test]
    fn hard_deadline_is_never_before_session() {
        for profile in [SafetyProfile::Dev, SafetyProfile::Release] {
            for requested in [1, 5, 9, 10, 14, 15, 16, 25, 600, 1790, 1800, 5000] {
                for max_lock in [1, 10, 15, 20, 600, 1800, 3600, 7200] {
                    let plan = plan_lock(secs(requested), secs(max_lock), profile).unwrap();
                    assert!(plan.hard_deadline >= plan.session, "{plan:?}");
                    assert!(plan.session <= secs(requested));
                    assert!(plan.hard_deadline <= ABSOLUTE_MAX_LOCK);
                    if profile == SafetyProfile::Dev {
                        assert!(plan.session <= DEV_MAX_SESSION);
                        assert!(plan.hard_deadline <= DEV_MAX_HARD_DEADLINE);
                    }
                }
            }
        }
    }

    #[test]
    fn sub_second_durations_are_allowed() {
        let plan = release(Duration::from_millis(1), DEFAULT_MAX_LOCK);
        assert_eq!(plan.session, Duration::from_millis(1));
        assert_eq!(plan.hard_deadline, Duration::from_millis(10_001));
    }

    #[test]
    fn zero_inputs_are_rejected() {
        assert_eq!(
            plan_lock(Duration::ZERO, DEFAULT_MAX_LOCK, SafetyProfile::Release),
            Err(PolicyError::ZeroDuration)
        );
        assert_eq!(
            plan_lock(secs(10), Duration::ZERO, SafetyProfile::Dev),
            Err(PolicyError::ZeroMaxLock)
        );
    }
}
