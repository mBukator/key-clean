//! Which Windows notifications KeyClean shows (§31, ADR 0018). Pure: the app asks here, then
//! shows the toast.
//!
//! A toast never appears while a lock is starting or running (the overlay already says what is
//! happening), never while the dashboard is open (it shows the same thing), and never when the
//! user turned notifications off. Toasts carry the end reason and what was locked, never key data
//! (privacy invariants).

use crate::policy::LockTargets;
use crate::session::{EndReason, SystemTransition};

/// A notification KeyClean may show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toast {
    /// The lock ran its full length.
    CleaningComplete(LockTargets),
    /// The hard deadline ended the lock before its timer.
    SafetyLimit,
    /// The computer went to sleep during the lock.
    Sleep,
    /// The computer was locked during the lock.
    ComputerLocked,
    /// The Windows session was switched or disconnected during the lock.
    SessionSwitched,
    /// The start shortcut couldn't be registered at startup (another app may hold it).
    ShortcutUnavailable,
    /// The input engine didn't start.
    EngineFailedToStart,
    /// The input engine stopped and won't be restarted.
    EngineStopped,
}

impl Toast {
    /// The toast after a lock that ended for `reason`, as the user should read it (a full-length
    /// lock that hit its own hard deadline reads as [`EndReason::Timeout`]). `None` when the user
    /// ended it (emergency chord, Unlock, Quit, a lost overlay), when Windows is signing out or
    /// shutting down, or after an engine error, which opens the dashboard instead (ADR 0016).
    pub fn after_lock(reason: EndReason, targets: LockTargets) -> Option<Toast> {
        match reason {
            EndReason::Timeout => Some(Toast::CleaningComplete(targets)),
            EndReason::HardDeadline => Some(Toast::SafetyLimit),
            EndReason::SystemTransition(SystemTransition::Suspend) => Some(Toast::Sleep),
            EndReason::SystemTransition(SystemTransition::SessionLock) => {
                Some(Toast::ComputerLocked)
            }
            EndReason::SystemTransition(SystemTransition::SessionDisconnect) => {
                Some(Toast::SessionSwitched)
            }
            EndReason::SystemTransition(SystemTransition::EndSession)
            | EndReason::Emergency
            | EndReason::UserRequest
            | EndReason::EngineError => None,
        }
    }

    /// The key of the toast's text in `locales/en/strings.json`.
    pub fn body_key(self) -> &'static str {
        match self {
            Toast::CleaningComplete(LockTargets {
                keyboard: true,
                mouse: true,
            }) => "notify.complete_both",
            Toast::CleaningComplete(LockTargets { mouse: true, .. }) => "notify.complete_mouse",
            Toast::CleaningComplete(_) => "notify.complete_keyboard",
            Toast::SafetyLimit => "notify.safety_limit",
            Toast::Sleep => "notify.sleep",
            Toast::ComputerLocked => "notify.computer_locked",
            Toast::SessionSwitched => "notify.session_switched",
            Toast::ShortcutUnavailable => "error.shortcut_unavailable",
            Toast::EngineFailedToStart => "error.engine_start",
            Toast::EngineStopped => "error.engine_stopped",
        }
    }

    /// A short name for the log (`notification: requested (<name>)`).
    pub fn name(self) -> &'static str {
        match self {
            Toast::CleaningComplete(_) => "cleaningComplete",
            Toast::SafetyLimit => "safetyLimit",
            Toast::Sleep => "sleep",
            Toast::ComputerLocked => "computerLocked",
            Toast::SessionSwitched => "sessionSwitched",
            Toast::ShortcutUnavailable => "shortcutUnavailable",
            Toast::EngineFailedToStart => "engineFailedToStart",
            Toast::EngineStopped => "engineStopped",
        }
    }
}

/// Whether a toast may be shown now: notifications are on, no dashboard is open to show the same
/// thing, and no lock is starting or running.
pub fn may_show(enabled: bool, dashboard_open: bool, lock_active: bool) -> bool {
    enabled && !dashboard_open && !lock_active
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOUSE: LockTargets = LockTargets {
        keyboard: false,
        mouse: true,
    };

    #[test]
    fn the_user_ending_a_lock_gets_no_toast() {
        for reason in [
            EndReason::Emergency,
            EndReason::UserRequest,
            EndReason::EngineError,
            EndReason::SystemTransition(SystemTransition::EndSession),
        ] {
            assert_eq!(
                Toast::after_lock(reason, LockTargets::KEYBOARD),
                None,
                "{reason:?}"
            );
        }
    }

    #[test]
    fn the_timer_and_system_transitions_get_a_toast() {
        assert_eq!(
            Toast::after_lock(EndReason::Timeout, MOUSE),
            Some(Toast::CleaningComplete(MOUSE))
        );
        assert_eq!(
            Toast::after_lock(EndReason::HardDeadline, MOUSE),
            Some(Toast::SafetyLimit)
        );
        assert_eq!(
            Toast::after_lock(
                EndReason::SystemTransition(SystemTransition::Suspend),
                MOUSE
            ),
            Some(Toast::Sleep)
        );
        assert_eq!(
            Toast::after_lock(
                EndReason::SystemTransition(SystemTransition::SessionLock),
                MOUSE
            ),
            Some(Toast::ComputerLocked)
        );
        assert_eq!(
            Toast::after_lock(
                EndReason::SystemTransition(SystemTransition::SessionDisconnect),
                MOUSE
            ),
            Some(Toast::SessionSwitched)
        );
    }

    #[test]
    fn complete_text_names_what_was_locked() {
        assert_eq!(
            Toast::CleaningComplete(LockTargets::KEYBOARD).body_key(),
            "notify.complete_keyboard"
        );
        assert_eq!(
            Toast::CleaningComplete(MOUSE).body_key(),
            "notify.complete_mouse"
        );
        assert_eq!(
            Toast::CleaningComplete(LockTargets::ALL).body_key(),
            "notify.complete_both"
        );
    }

    #[test]
    fn only_with_notifications_on_no_dashboard_and_no_lock() {
        assert!(may_show(true, false, false));
        for (enabled, dashboard, lock) in [
            (false, false, false),
            (true, true, false),
            (true, false, true),
            (false, true, true),
        ] {
            assert!(
                !may_show(enabled, dashboard, lock),
                "{enabled} {dashboard} {lock}"
            );
        }
    }

    #[test]
    fn every_toast_has_its_text() {
        let strings = include_str!("../../../locales/en/strings.json");
        for toast in [
            Toast::CleaningComplete(LockTargets::KEYBOARD),
            Toast::CleaningComplete(MOUSE),
            Toast::CleaningComplete(LockTargets::ALL),
            Toast::SafetyLimit,
            Toast::Sleep,
            Toast::ComputerLocked,
            Toast::SessionSwitched,
            Toast::ShortcutUnavailable,
            Toast::EngineFailedToStart,
            Toast::EngineStopped,
        ] {
            let key = format!("\"{}\":", toast.body_key());
            assert!(strings.contains(&key), "{key} missing");
            assert!(!toast.name().is_empty());
        }
    }
}
