//! Data sent to the webview. Mirrors `src/shared/types/engine.ts`. Contains no key data.

use keyclean_win::keyclean_core::devices::{Capability, DeviceKind, InputDevice};
use keyclean_win::keyclean_core::settings::{
    DurationError, MAX_MAX_LOCK_MINUTES, MIN_DURATION, MIN_MAX_LOCK_MINUTES, Settings,
};
use keyclean_win::keyclean_core::{countdown, presets};
use keyclean_win::{
    DeviceChange, DeviceClass, EndReason, EngineError, EngineEvent, EngineNotice, LockTargets,
    SessionState, SystemTransition,
};
use serde::Serialize;

use crate::tray::TrayMode;

/// Name of the event emitted to the main window on every status change.
pub const STATUS_EVENT: &str = "engine-status";

/// Name of the event emitted to the main window when the device list changes.
pub const DEVICES_EVENT: &str = "devices-changed";

/// A plain-language error (by string key) plus technical details (§46).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorDto {
    message_key: String,
    pub details: String,
}

impl ErrorDto {
    /// Whether this is the error `e` would give.
    pub fn is(&self, e: &EngineError) -> bool {
        self.message_key == e.message_key()
    }

    /// The requested lock duration is outside 10 s to the max-lock setting (ADR 0015).
    pub fn invalid_duration(seconds: u64, max_lock_secs: u64, why: DurationError) -> Self {
        ErrorDto {
            message_key: "error.invalid_duration".to_owned(),
            details: format!(
                "{seconds} s is {why} (allowed: {}-{max_lock_secs} s)",
                MIN_DURATION.as_secs()
            ),
        }
    }

    /// The overlay didn't confirm it was visible within the budget, so nothing was locked
    /// (invariant 11).
    pub fn overlay_timeout(details: String) -> Self {
        ErrorDto {
            message_key: "error.overlay_timeout".to_owned(),
            details,
        }
    }

    /// The overlay couldn't be opened or closed before it confirmed, so nothing was locked.
    pub fn overlay_failed(details: String) -> Self {
        ErrorDto {
            message_key: "error.overlay_failed".to_owned(),
            details,
        }
    }
}

impl From<&EngineError> for ErrorDto {
    fn from(e: &EngineError) -> Self {
        ErrorDto {
            message_key: e.message_key().to_owned(),
            details: e.details(),
        }
    }
}

/// What a lock blocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetsDto {
    keyboard: bool,
    mouse: bool,
}

impl From<LockTargets> for TargetsDto {
    fn from(t: LockTargets) -> Self {
        TargetsDto {
            keyboard: t.keyboard,
            mouse: t.mouse,
        }
    }
}

/// What the main window displays.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusDto {
    state: &'static str,
    dev_cap: bool,
    last_end_reason: Option<&'static str>,
    error: Option<ErrorDto>,
    engine_available: bool,
    /// Whole seconds left in the lock, rounded up (§15), while starting or locked.
    countdown_secs: Option<u64>,
    /// The latest notice to show (a string key), until the next lock starts.
    notice: Option<&'static str>,
    /// What the current session blocks, or the last one once it ends; `None` before any lock.
    targets: Option<TargetsDto>,
    /// Set when the app ended the lock because the overlay went away, so the engine's
    /// `UserRequest` end reads as `overlayLost`.
    #[serde(skip)]
    overlay_lost: bool,
    /// Set when the requested lock ends at its own hard deadline (its length equals the max-lock
    /// setting), so the engine's `HardDeadline` end reads as `timeout` (ADR 0015).
    #[serde(skip)]
    session_meets_hard_deadline: bool,
}

impl StatusDto {
    /// The status before any engine event arrives.
    pub fn initial(dev_cap: bool) -> Self {
        StatusDto {
            state: state_name(SessionState::Idle),
            dev_cap,
            last_end_reason: None,
            error: None,
            engine_available: false,
            countdown_secs: None,
            notice: None,
            targets: None,
            overlay_lost: false,
            session_meets_hard_deadline: false,
        }
    }

    /// Whether no lock is running or starting.
    pub fn is_idle(&self) -> bool {
        self.state == state_name(SessionState::Idle)
    }

    /// Records why a lock request failed before the engine was asked, for a request with no
    /// window to show it (the tray). The dashboard and the tray read it from the status (ADR
    /// 0016). Cleared when the next lock starts.
    pub fn set_request_error(&mut self, error: ErrorDto) {
        self.error = Some(error);
    }

    /// What the tray shows for this status.
    pub fn tray_mode(&self) -> TrayMode {
        match self.state {
            s if s == state_name(SessionState::Starting) => TrayMode::Starting,
            s if s == state_name(SessionState::Locked) => {
                TrayMode::Cleaning(self.countdown_secs.unwrap_or(0))
            }
            s if s == state_name(SessionState::Unlocking) => TrayMode::Unlocking,
            _ if !self.engine_available => TrayMode::Unavailable,
            _ if self.error.is_some() => TrayMode::Problem,
            _ => TrayMode::Ready,
        }
    }

    /// Whether the last lock ended for a reason the user should read in the dashboard: an engine
    /// failure or a lost overlay (ADR 0016). The other reasons are the user's own doing, the
    /// timer, or a system transition.
    pub fn ended_with_problem(&self) -> bool {
        matches!(
            self.last_end_reason,
            Some(reason) if reason == OVERLAY_LOST || reason == end_reason_name(EndReason::EngineError)
        )
    }

    /// A new lock is being requested: an earlier lost-overlay mark no longer applies.
    /// `session_meets_hard_deadline` comes from the lock's plan (`LockPlan`).
    pub fn lock_requested(&mut self, session_meets_hard_deadline: bool) {
        self.overlay_lost = false;
        self.session_meets_hard_deadline = session_meets_hard_deadline;
    }

    /// Records that the app is ending the lock because the overlay went away (ADR 0014).
    pub fn mark_overlay_lost(&mut self) {
        self.overlay_lost = true;
    }

    /// Whether the current or last session left the keyboard free (the engine then only watches
    /// it for the emergency chord). Unknown targets count as a keyboard lock.
    fn keyboard_free(&self) -> bool {
        self.targets.is_some_and(|t| !t.keyboard)
    }

    /// Records whether the engine started, and why not.
    pub fn set_engine(&mut self, available: bool, error: Option<ErrorDto>) {
        self.engine_available = available;
        self.error = error;
    }

    /// Records that the engine process died and a new, idle one took its place.
    pub fn set_engine_restarted(&mut self) {
        self.set_engine(true, None);
        self.state = state_name(SessionState::Idle);
        self.countdown_secs = None;
        self.targets = None;
        self.notice = Some("notice.engine_restarted");
    }

    /// Folds one engine event into the status. Only notices the user should see are kept.
    pub fn apply(&mut self, event: &EngineEvent) {
        match event {
            EngineEvent::Status(status) => {
                if status.state == SessionState::Starting {
                    self.error = None;
                    self.last_end_reason = None;
                }
                let was_idle = self.state == state_name(SessionState::Idle);
                if was_idle && matches!(status.state, SessionState::Starting | SessionState::Locked)
                {
                    // A new lock starts: earlier notices no longer apply.
                    self.notice = None;
                }
                if let Some(targets) = status.targets {
                    // Kept after the session ends, so its late errors and notices still read right.
                    self.targets = Some(TargetsDto::from(targets));
                }
                self.state = state_name(status.state);
                self.dev_cap = status.dev_cap;
                self.countdown_secs = match status.state {
                    SessionState::Starting | SessionState::Locked => {
                        status.session_remaining.map(countdown::display_secs)
                    }
                    SessionState::Idle | SessionState::Unlocking => None,
                };
            }
            EngineEvent::SessionEnded { reason } => {
                let overlay_lost = std::mem::take(&mut self.overlay_lost);
                let full_length = std::mem::take(&mut self.session_meets_hard_deadline);
                self.last_end_reason = Some(match reason {
                    EndReason::UserRequest if overlay_lost => OVERLAY_LOST,
                    // The lock ran its full length, which is also its hard deadline.
                    EndReason::HardDeadline if full_length => end_reason_name(EndReason::Timeout),
                    _ => end_reason_name(*reason),
                });
            }
            EngineEvent::Error(e) => {
                let mut dto = ErrorDto::from(e);
                if matches!(e, EngineError::HookLost) && self.keyboard_free() {
                    // The keyboard hook only watched for the emergency chord.
                    dto.message_key = "error.chord_hook_lost".to_owned();
                }
                self.error = Some(dto);
            }
            EngineEvent::Notice(notice) => {
                let key = match notice {
                    EngineNotice::ElevatedWindowBypass if self.keyboard_free() => {
                        Some("notice.elevated_window_chord")
                    }
                    _ => notice_key(*notice),
                };
                if let Some(key) = key {
                    self.notice = Some(key);
                }
            }
            EngineEvent::DeviceChanged(change) => {
                let keyboard_free = self.keyboard_free();
                self.notice = Some(match change {
                    DeviceChange::Arrived(DeviceClass::Keyboard) if keyboard_free => {
                        "notice.keyboard_connected_unlocked"
                    }
                    DeviceChange::Arrived(DeviceClass::Keyboard) => "notice.keyboard_connected",
                    DeviceChange::Removed(DeviceClass::Keyboard) => "notice.keyboard_disconnected",
                    DeviceChange::Arrived(DeviceClass::Mouse) => "notice.mouse_connected",
                    DeviceChange::Removed(DeviceClass::Mouse) => "notice.mouse_disconnected",
                });
            }
        }
    }
}

/// The lock durations the window offers (§11), from `keyclean-core`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LockOptionsDto {
    preset_seconds: Vec<u64>,
    default_seconds: u64,
    /// The shortest lock (the lower bound of the Custom field).
    min_seconds: u64,
    /// The longest lock: the max-lock setting.
    max_seconds: u64,
    max_lock_minutes: u64,
    min_max_lock_minutes: u64,
    max_max_lock_minutes: u64,
    /// The duration to preselect: the last started lock's, within range.
    last_seconds: u64,
    /// The targets to preselect: the last started lock's.
    last_targets: TargetsDto,
}

impl LockOptionsDto {
    /// The presets, the range and the remembered choices under `settings`.
    pub fn current(settings: &Settings) -> Self {
        LockOptionsDto {
            preset_seconds: presets::DURATION_PRESETS
                .iter()
                .map(|d| d.as_secs())
                .collect(),
            default_seconds: presets::DEFAULT_DURATION.as_secs(),
            min_seconds: MIN_DURATION.as_secs(),
            max_seconds: settings.max_lock().as_secs(),
            max_lock_minutes: settings.max_lock_minutes(),
            min_max_lock_minutes: MIN_MAX_LOCK_MINUTES,
            max_max_lock_minutes: MAX_MAX_LOCK_MINUTES,
            last_seconds: settings.last_duration().as_secs(),
            last_targets: TargetsDto::from(settings.last_targets()),
        }
    }
}

/// An input device for the device list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDto {
    id: String,
    pub name: Option<String>,
    kind: &'static str,
    capability: &'static str,
}

impl From<InputDevice> for DeviceDto {
    fn from(d: InputDevice) -> Self {
        DeviceDto {
            id: d.id,
            name: d.name,
            kind: match d.kind {
                DeviceKind::Keyboard => "keyboard",
                DeviceKind::Mouse => "mouse",
                DeviceKind::Touchpad => "touchpad",
                DeviceKind::Touchscreen => "touchscreen",
                DeviceKind::Pen => "pen",
            },
            capability: match d.capability {
                Capability::Supported => "supported",
                Capability::Limited => "limited",
                Capability::Unsupported => "unsupported",
            },
        }
    }
}

/// The device list, with the reason it may be incomplete or stale.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevicesDto {
    pub devices: Vec<DeviceDto>,
    /// Set when listing failed (the old list is kept) or when the list can't update by itself.
    pub error: Option<ErrorDto>,
}

fn state_name(state: SessionState) -> &'static str {
    match state {
        SessionState::Idle => "idle",
        SessionState::Starting => "starting",
        SessionState::Locked => "locked",
        SessionState::Unlocking => "unlocking",
    }
}

/// The string key for notices the user should see; the rest are only logged.
fn notice_key(notice: EngineNotice) -> Option<&'static str> {
    match notice {
        EngineNotice::ElevatedWindowBypass => Some("notice.elevated_window"),
        EngineNotice::ElevatedWindowMouseBypass => Some("notice.elevated_window_mouse"),
        EngineNotice::LivenessCheckUnavailable => Some("notice.liveness_unavailable"),
        EngineNotice::HookAlreadyRemoved
        | EngineNotice::DrainTimedOut
        | EngineNotice::PowerNotificationUnavailable
        | EngineNotice::SessionNotificationUnavailable
        | EngineNotice::RawInputNotRemoved => None,
    }
}

/// The end reason the app reports when it ended the lock because the overlay went away. The
/// engine only sees an unlock request.
const OVERLAY_LOST: &str = "overlayLost";

fn end_reason_name(reason: EndReason) -> &'static str {
    match reason {
        EndReason::Timeout => "timeout",
        EndReason::Emergency => "emergency",
        EndReason::HardDeadline => "hardDeadline",
        EndReason::SystemTransition(SystemTransition::Suspend) => "suspend",
        EndReason::SystemTransition(SystemTransition::EndSession) => "endSession",
        EndReason::SystemTransition(SystemTransition::SessionLock) => "sessionLock",
        EndReason::SystemTransition(SystemTransition::SessionDisconnect) => "sessionDisconnect",
        EndReason::EngineError => "engineError",
        EndReason::UserRequest => "userRequest",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyclean_win::EngineStatus;
    use std::time::Duration;

    fn status(state: SessionState) -> EngineEvent {
        status_with(state, None)
    }

    fn status_with(state: SessionState, session_remaining: Option<Duration>) -> EngineEvent {
        EngineEvent::Status(EngineStatus {
            state,
            dev_cap: true,
            session_remaining,
            hard_deadline_remaining: session_remaining.map(|d| d + Duration::from_secs(10)),
            targets: None,
        })
    }

    fn status_targets(state: SessionState, targets: Option<LockTargets>) -> EngineEvent {
        EngineEvent::Status(EngineStatus {
            state,
            dev_cap: true,
            session_remaining: None,
            hard_deadline_remaining: None,
            targets,
        })
    }

    const MOUSE_ONLY: LockTargets = LockTargets {
        keyboard: false,
        mouse: true,
    };

    #[test]
    fn tray_mode_follows_the_status() {
        let mut dto = StatusDto::initial(true);
        assert_eq!(dto.tray_mode(), TrayMode::Unavailable);
        dto.set_engine(true, None);
        assert_eq!(dto.tray_mode(), TrayMode::Ready);
        dto.apply(&status_with(
            SessionState::Starting,
            Some(Duration::from_secs(120)),
        ));
        assert_eq!(dto.tray_mode(), TrayMode::Starting);
        dto.apply(&status_with(
            SessionState::Locked,
            Some(Duration::from_millis(101_500)),
        ));
        assert_eq!(dto.tray_mode(), TrayMode::Cleaning(102));
        dto.apply(&status(SessionState::Unlocking));
        assert_eq!(dto.tray_mode(), TrayMode::Unlocking);
        dto.apply(&status(SessionState::Idle));
        assert_eq!(dto.tray_mode(), TrayMode::Ready);
    }

    #[test]
    fn a_failed_request_shows_as_a_problem_until_the_next_lock() {
        let mut dto = StatusDto::initial(true);
        dto.set_engine(true, None);
        dto.set_request_error(ErrorDto::overlay_timeout("test".to_owned()));
        assert_eq!(dto.tray_mode(), TrayMode::Problem);
        dto.apply(&status(SessionState::Starting));
        assert_eq!(dto.tray_mode(), TrayMode::Starting);
        dto.apply(&status(SessionState::Idle));
        assert_eq!(dto.tray_mode(), TrayMode::Ready);
    }

    #[test]
    fn a_stopped_engine_outranks_its_error() {
        let mut dto = StatusDto::initial(true);
        dto.set_engine(false, Some(ErrorDto::from(&EngineError::NotRunning)));
        assert_eq!(dto.tray_mode(), TrayMode::Unavailable);
    }

    #[test]
    fn only_an_engine_error_or_a_lost_overlay_is_a_problem_ending() {
        let ended = |reason: EndReason, overlay_lost: bool| {
            let mut dto = StatusDto::initial(true);
            dto.lock_requested(false);
            if overlay_lost {
                dto.mark_overlay_lost();
            }
            dto.apply(&EngineEvent::SessionEnded { reason });
            dto.ended_with_problem()
        };
        assert!(ended(EndReason::EngineError, false));
        assert!(ended(EndReason::UserRequest, true));
        for reason in [
            EndReason::Timeout,
            EndReason::Emergency,
            EndReason::HardDeadline,
            EndReason::UserRequest,
            EndReason::SystemTransition(SystemTransition::Suspend),
        ] {
            assert!(!ended(reason, false), "{reason:?}");
        }
        assert!(!StatusDto::initial(true).ended_with_problem());
    }

    #[test]
    fn error_identity_is_its_message() {
        let dto = ErrorDto::from(&EngineError::AlreadyActive);
        assert!(dto.is(&EngineError::AlreadyActive));
        assert!(!dto.is(&EngineError::NotRunning));
    }

    #[test]
    fn targets_serialize_camel_case_and_outlive_the_session() {
        let mut dto = StatusDto::initial(true);
        let json = serde_json::to_value(&dto).unwrap();
        assert!(json["targets"].is_null(), "no lock yet");

        dto.apply(&status_targets(
            SessionState::Starting,
            Some(LockTargets::ALL),
        ));
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(
            json["targets"],
            serde_json::json!({ "keyboard": true, "mouse": true })
        );

        dto.apply(&status_targets(SessionState::Idle, None));
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(
            json["targets"],
            serde_json::json!({ "keyboard": true, "mouse": true }),
            "kept after the lock"
        );

        dto.apply(&status_targets(SessionState::Starting, Some(MOUSE_ONLY)));
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(
            json["targets"],
            serde_json::json!({ "keyboard": false, "mouse": true })
        );

        dto.set_engine_restarted();
        assert_eq!(dto.targets, None);
    }

    #[test]
    fn keyboard_arrival_while_the_keyboard_is_free() {
        let mut dto = StatusDto::initial(true);
        dto.apply(&status_targets(SessionState::Locked, Some(MOUSE_ONLY)));
        dto.apply(&EngineEvent::DeviceChanged(DeviceChange::Arrived(
            DeviceClass::Keyboard,
        )));
        assert_eq!(dto.notice, Some("notice.keyboard_connected_unlocked"));

        dto.apply(&status_targets(SessionState::Idle, None));
        dto.apply(&status_targets(
            SessionState::Locked,
            Some(LockTargets::ALL),
        ));
        dto.apply(&EngineEvent::DeviceChanged(DeviceChange::Arrived(
            DeviceClass::Keyboard,
        )));
        assert_eq!(dto.notice, Some("notice.keyboard_connected"));
    }

    #[test]
    fn hook_lost_while_the_keyboard_is_free_names_the_chord() {
        let mut dto = StatusDto::initial(true);
        dto.apply(&status_targets(SessionState::Locked, Some(MOUSE_ONLY)));
        dto.apply(&status_targets(SessionState::Idle, None));
        dto.apply(&EngineEvent::Error(EngineError::HookLost));
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["error"]["messageKey"], "error.chord_hook_lost");
        assert!(json["error"]["details"].is_string());

        dto.apply(&status_targets(
            SessionState::Locked,
            Some(LockTargets::KEYBOARD),
        ));
        dto.apply(&EngineEvent::Error(EngineError::HookLost));
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["error"]["messageKey"], "error.hook_lost");
    }

    #[test]
    fn elevated_window_while_the_keyboard_is_free_names_the_chord() {
        let mut dto = StatusDto::initial(true);
        dto.apply(&status_targets(SessionState::Locked, Some(MOUSE_ONLY)));
        dto.apply(&EngineEvent::Notice(EngineNotice::ElevatedWindowBypass));
        assert_eq!(dto.notice, Some("notice.elevated_window_chord"));

        dto.apply(&status_targets(SessionState::Idle, None));
        dto.apply(&status_targets(
            SessionState::Locked,
            Some(LockTargets::ALL),
        ));
        dto.apply(&EngineEvent::Notice(EngineNotice::ElevatedWindowBypass));
        assert_eq!(dto.notice, Some("notice.elevated_window"));
    }

    #[test]
    fn an_unlock_for_a_lost_overlay_reads_as_overlay_lost() {
        let mut dto = StatusDto::initial(true);
        dto.apply(&status(SessionState::Starting));
        dto.apply(&status(SessionState::Locked));
        assert!(!dto.is_idle());
        dto.mark_overlay_lost();
        let json = serde_json::to_value(&dto).unwrap();
        assert!(json.get("overlayLost").is_none(), "internal only");
        dto.apply(&EngineEvent::SessionEnded {
            reason: EndReason::UserRequest,
        });
        dto.apply(&status(SessionState::Idle));
        assert!(dto.is_idle());
        assert_eq!(dto.last_end_reason, Some("overlayLost"));

        // The flag is used once; the next user request reads as one.
        dto.apply(&status(SessionState::Starting));
        dto.apply(&EngineEvent::SessionEnded {
            reason: EndReason::UserRequest,
        });
        assert_eq!(dto.last_end_reason, Some("userRequest"));
    }

    #[test]
    fn another_end_reason_wins_over_a_lost_overlay() {
        let mut dto = StatusDto::initial(true);
        dto.apply(&status(SessionState::Locked));
        dto.mark_overlay_lost();
        dto.apply(&EngineEvent::SessionEnded {
            reason: EndReason::Timeout,
        });
        assert_eq!(dto.last_end_reason, Some("timeout"));
        dto.apply(&EngineEvent::SessionEnded {
            reason: EndReason::UserRequest,
        });
        assert_eq!(dto.last_end_reason, Some("userRequest"), "flag cleared");
    }

    #[test]
    fn a_new_lock_request_clears_a_stale_overlay_flag() {
        let mut dto = StatusDto::initial(true);
        dto.mark_overlay_lost();
        dto.lock_requested(false);
        dto.apply(&status(SessionState::Starting));
        dto.apply(&EngineEvent::SessionEnded {
            reason: EndReason::UserRequest,
        });
        assert_eq!(dto.last_end_reason, Some("userRequest"));
    }

    #[test]
    fn an_overlay_lost_before_the_first_status_still_reads_as_overlay_lost() {
        let mut dto = StatusDto::initial(true);
        dto.lock_requested(false);
        dto.mark_overlay_lost();
        dto.apply(&status(SessionState::Starting));
        dto.apply(&EngineEvent::SessionEnded {
            reason: EndReason::UserRequest,
        });
        assert_eq!(dto.last_end_reason, Some("overlayLost"));
    }

    #[test]
    fn serializes_camel_case() {
        let mut dto = StatusDto::initial(true);
        dto.set_engine(true, None);
        let json = serde_json::to_value(&dto).unwrap();
        assert!(json["countdownSecs"].is_null());
        assert_eq!(json["state"], "idle");
        assert_eq!(json["devCap"], true);
        assert_eq!(json["engineAvailable"], true);
        assert!(json["lastEndReason"].is_null());
    }

    #[test]
    fn a_session_round_trip() {
        let mut dto = StatusDto::initial(true);
        dto.set_engine(true, None);
        dto.apply(&EngineEvent::Error(EngineError::AlreadyActive));
        assert!(dto.error.is_some());

        dto.apply(&status(SessionState::Starting));
        assert_eq!(dto.state, "starting");
        assert!(dto.error.is_none(), "a new lock clears the old error");

        dto.apply(&status(SessionState::Locked));
        dto.apply(&status(SessionState::Unlocking));
        dto.apply(&EngineEvent::SessionEnded {
            reason: EndReason::Emergency,
        });
        dto.apply(&status(SessionState::Idle));
        assert_eq!(dto.state, "idle");
        assert_eq!(dto.last_end_reason, Some("emergency"));
    }

    #[test]
    fn countdown_follows_the_session() {
        let mut dto = StatusDto::initial(false);
        dto.apply(&status_with(
            SessionState::Starting,
            Some(Duration::from_secs(120)),
        ));
        assert_eq!(dto.countdown_secs, Some(120));
        dto.apply(&status_with(
            SessionState::Locked,
            Some(Duration::from_millis(119_000)),
        ));
        assert_eq!(dto.countdown_secs, Some(119));
        dto.apply(&status_with(
            SessionState::Locked,
            Some(Duration::from_millis(400)),
        ));
        assert_eq!(dto.countdown_secs, Some(1), "rounds up");
        dto.apply(&status(SessionState::Unlocking));
        assert_eq!(dto.countdown_secs, None);
        dto.apply(&status(SessionState::Idle));
        assert_eq!(dto.countdown_secs, None);
        let json = serde_json::to_value(&dto).unwrap();
        assert!(json["countdownSecs"].is_null());
    }

    #[test]
    fn lock_options_come_from_the_presets_and_settings() {
        let json = serde_json::to_value(LockOptionsDto::current(&Settings::default())).unwrap();
        assert_eq!(json["presetSeconds"], serde_json::json!([30, 60, 120, 300]));
        assert_eq!(json["defaultSeconds"], 120);
        assert_eq!(json["minSeconds"], 10);
        assert_eq!(json["maxSeconds"], 1800);
        assert_eq!(json["maxLockMinutes"], 30);
        assert_eq!(json["minMaxLockMinutes"], 1);
        assert_eq!(json["maxMaxLockMinutes"], 60);
        assert_eq!(json["lastSeconds"], 120);
        assert_eq!(
            json["lastTargets"],
            serde_json::json!({"keyboard": true, "mouse": false})
        );
    }

    #[test]
    fn lock_options_follow_a_lowered_max_lock() {
        let mut settings = Settings::default();
        settings.set_max_lock_minutes(1);
        let json = serde_json::to_value(LockOptionsDto::current(&settings)).unwrap();
        assert_eq!(json["maxSeconds"], 60);
        assert_eq!(json["lastSeconds"], 60);
    }

    #[test]
    fn invalid_duration_error_uses_the_invalid_duration_string() {
        let json =
            serde_json::to_value(ErrorDto::invalid_duration(7, 1800, DurationError::TooShort))
                .unwrap();
        assert_eq!(json["messageKey"], "error.invalid_duration");
        let details = json["details"].as_str().unwrap();
        assert!(details.contains("7 s"), "{details}");
        assert!(details.contains("10-1800 s"), "{details}");
    }

    #[test]
    fn a_full_length_lock_at_its_hard_deadline_reads_as_timeout() {
        let mut dto = StatusDto::initial(false);
        dto.lock_requested(true);
        let json = serde_json::to_value(&dto).unwrap();
        assert!(
            json.get("sessionMeetsHardDeadline").is_none(),
            "internal only"
        );
        dto.apply(&status(SessionState::Locked));
        dto.apply(&EngineEvent::SessionEnded {
            reason: EndReason::HardDeadline,
        });
        assert_eq!(dto.last_end_reason, Some("timeout"));

        // Used once: a later hard-deadline end reads as one.
        dto.apply(&EngineEvent::SessionEnded {
            reason: EndReason::HardDeadline,
        });
        assert_eq!(dto.last_end_reason, Some("hardDeadline"));
    }

    #[test]
    fn a_shorter_lock_at_its_hard_deadline_keeps_the_reason() {
        let mut dto = StatusDto::initial(false);
        dto.lock_requested(false);
        dto.apply(&status(SessionState::Locked));
        dto.apply(&EngineEvent::SessionEnded {
            reason: EndReason::HardDeadline,
        });
        assert_eq!(dto.last_end_reason, Some("hardDeadline"));
    }

    #[test]
    fn a_full_length_lock_keeps_other_end_reasons() {
        let mut dto = StatusDto::initial(false);
        dto.lock_requested(true);
        dto.apply(&status(SessionState::Locked));
        dto.apply(&EngineEvent::SessionEnded {
            reason: EndReason::Emergency,
        });
        assert_eq!(dto.last_end_reason, Some("emergency"));
    }

    #[test]
    fn error_dto_uses_message_key() {
        let dto = ErrorDto::from(&EngineError::NotRunning);
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["messageKey"], "error.engine_stopped");
        assert!(json["details"].is_string());
    }

    #[test]
    fn every_end_reason_has_a_ui_string() {
        let strings: std::collections::HashMap<String, String> =
            serde_json::from_str(include_str!("../../locales/en/strings.json")).unwrap();
        let reasons = [
            EndReason::Timeout,
            EndReason::Emergency,
            EndReason::HardDeadline,
            EndReason::SystemTransition(SystemTransition::Suspend),
            EndReason::SystemTransition(SystemTransition::EndSession),
            EndReason::SystemTransition(SystemTransition::SessionLock),
            EndReason::SystemTransition(SystemTransition::SessionDisconnect),
            EndReason::EngineError,
            EndReason::UserRequest,
        ];
        let names = reasons.map(end_reason_name);
        for name in names.iter().chain([&OVERLAY_LOST]) {
            let key = format!("endReason.{name}");
            assert!(strings.contains_key(&key), "missing {key}");
        }
    }

    #[test]
    fn every_engine_error_has_a_ui_string() {
        let strings: std::collections::HashMap<String, String> =
            serde_json::from_str(include_str!("../../locales/en/strings.json")).unwrap();
        let errors = [
            EngineError::AlreadyRunning,
            EngineError::NotRunning,
            EngineError::AlreadyActive,
            EngineError::ThreadSpawn(String::new()),
            EngineError::Timer,
            EngineError::MessageLoop,
            EngineError::EngineProcess(String::new()),
            EngineError::HookLost,
            EngineError::MouseHookLost,
            EngineError::InvalidRequest(
                keyclean_win::keyclean_core::policy::PolicyError::NoTargets,
            ),
        ];
        for e in errors {
            assert!(
                strings.contains_key(e.message_key()),
                "missing {}",
                e.message_key()
            );
        }
        for key in [
            "error.invalid_duration",
            "error.hook_install",
            "error.devices",
            "error.overlay_timeout",
            "error.overlay_failed",
        ] {
            assert!(strings.contains_key(key), "missing {key}");
        }
    }

    #[test]
    fn notices_show_until_the_next_lock() {
        let mut dto = StatusDto::initial(true);
        dto.apply(&EngineEvent::Notice(
            EngineNotice::PowerNotificationUnavailable,
        ));
        assert_eq!(dto.notice, None, "log-only notices aren't shown");

        dto.apply(&status(SessionState::Starting));
        dto.apply(&status(SessionState::Locked));
        dto.apply(&EngineEvent::Notice(EngineNotice::ElevatedWindowBypass));
        assert_eq!(dto.notice, Some("notice.elevated_window"));
        dto.apply(&status(SessionState::Locked));
        assert_eq!(
            dto.notice,
            Some("notice.elevated_window"),
            "kept during the lock"
        );
        dto.apply(&EngineEvent::Notice(EngineNotice::LivenessCheckUnavailable));
        assert_eq!(dto.notice, Some("notice.liveness_unavailable"));
        dto.apply(&status(SessionState::Unlocking));
        dto.apply(&status(SessionState::Idle));
        assert_eq!(
            dto.notice,
            Some("notice.liveness_unavailable"),
            "kept after the lock"
        );

        dto.apply(&status(SessionState::Starting));
        assert_eq!(dto.notice, None, "a new lock clears it");
        let json = serde_json::to_value(&dto).unwrap();
        assert!(json["notice"].is_null());
    }

    #[test]
    fn device_changes_during_a_lock_set_a_notice() {
        let mut dto = StatusDto::initial(true);
        dto.apply(&status(SessionState::Locked));
        dto.apply(&EngineEvent::DeviceChanged(DeviceChange::Removed(
            DeviceClass::Mouse,
        )));
        assert_eq!(dto.notice, Some("notice.mouse_disconnected"));
        dto.apply(&EngineEvent::DeviceChanged(DeviceChange::Arrived(
            DeviceClass::Mouse,
        )));
        assert_eq!(dto.notice, Some("notice.mouse_connected"));
        dto.apply(&EngineEvent::DeviceChanged(DeviceChange::Removed(
            DeviceClass::Keyboard,
        )));
        assert_eq!(dto.notice, Some("notice.keyboard_disconnected"));
        dto.apply(&EngineEvent::DeviceChanged(DeviceChange::Arrived(
            DeviceClass::Keyboard,
        )));
        assert_eq!(dto.notice, Some("notice.keyboard_connected"));
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["notice"], "notice.keyboard_connected");
    }

    #[test]
    fn a_restart_shows_an_idle_engine_and_a_notice() {
        let mut dto = StatusDto::initial(true);
        dto.set_engine(true, None);
        dto.apply(&status_with(
            SessionState::Locked,
            Some(Duration::from_secs(5)),
        ));
        dto.apply(&EngineEvent::Error(EngineError::EngineProcess(
            "gone".into(),
        )));
        dto.set_engine(false, Some(ErrorDto::from(&EngineError::NotRunning)));
        dto.set_engine_restarted();
        assert!(dto.engine_available);
        assert!(dto.error.is_none());
        assert_eq!(dto.state, "idle");
        assert_eq!(dto.countdown_secs, None);
        assert_eq!(dto.notice, Some("notice.engine_restarted"));
    }

    #[test]
    fn every_notice_has_a_ui_string() {
        let strings: std::collections::HashMap<String, String> =
            serde_json::from_str(include_str!("../../locales/en/strings.json")).unwrap();
        let mut keys: Vec<&str> = [
            EngineNotice::ElevatedWindowBypass,
            EngineNotice::ElevatedWindowMouseBypass,
            EngineNotice::LivenessCheckUnavailable,
        ]
        .into_iter()
        .filter_map(notice_key)
        .collect();
        keys.extend([
            "notice.keyboard_connected",
            "notice.keyboard_connected_unlocked",
            "notice.keyboard_disconnected",
            "notice.mouse_connected",
            "notice.mouse_disconnected",
            "notice.elevated_window_chord",
            "notice.engine_restarted",
            "error.chord_hook_lost",
        ]);
        for key in keys {
            assert!(strings.contains_key(key), "missing {key}");
        }
    }
}
