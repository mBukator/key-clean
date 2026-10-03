//! Data sent to the webview. Mirrors `src/shared/types/engine.ts`. Contains no key data.

use keyclean_win::keyclean_core::{countdown, presets};
use keyclean_win::{
    EndReason, EngineError, EngineEvent, KeyboardDevice, SessionState, SystemTransition,
};
use serde::Serialize;

/// Name of the event emitted to the main window on every status change.
pub const STATUS_EVENT: &str = "engine-status";

/// A plain-language error (by string key) plus technical details (§46).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorDto {
    message_key: String,
    details: String,
}

impl ErrorDto {
    /// The app only locks for one of the presets (§11).
    pub fn not_a_preset(seconds: u64) -> Self {
        ErrorDto {
            message_key: "error.invalid_duration".to_owned(),
            details: format!("{seconds} s is not one of the lock duration presets"),
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
        }
    }

    /// Records whether the engine started, and why not.
    pub fn set_engine(&mut self, available: bool, error: Option<ErrorDto>) {
        self.engine_available = available;
        self.error = error;
    }

    /// Folds one engine event into the status. Notices don't change what is displayed.
    pub fn apply(&mut self, event: &EngineEvent) {
        match event {
            EngineEvent::Status(status) => {
                if status.state == SessionState::Starting {
                    self.error = None;
                    self.last_end_reason = None;
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
                self.last_end_reason = Some(end_reason_name(*reason));
            }
            EngineEvent::Error(e) => self.error = Some(ErrorDto::from(e)),
            EngineEvent::Notice(_) => {}
        }
    }
}

/// The lock durations the window offers (§11), from `keyclean-core`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LockOptionsDto {
    preset_seconds: Vec<u64>,
    default_seconds: u64,
}

impl LockOptionsDto {
    /// The presets and the default.
    pub fn current() -> Self {
        LockOptionsDto {
            preset_seconds: presets::DURATION_PRESETS
                .iter()
                .map(|d| d.as_secs())
                .collect(),
            default_seconds: presets::DEFAULT_DURATION.as_secs(),
        }
    }
}

/// A keyboard for the device list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyboardDto {
    id: String,
    name: Option<String>,
}

impl From<KeyboardDevice> for KeyboardDto {
    fn from(k: KeyboardDevice) -> Self {
        KeyboardDto {
            id: k.id,
            name: k.name,
        }
    }
}

fn state_name(state: SessionState) -> &'static str {
    match state {
        SessionState::Idle => "idle",
        SessionState::Starting => "starting",
        SessionState::Locked => "locked",
        SessionState::Unlocking => "unlocking",
    }
}

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
        })
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
    fn lock_options_come_from_the_presets() {
        let json = serde_json::to_value(LockOptionsDto::current()).unwrap();
        assert_eq!(json["presetSeconds"], serde_json::json!([30, 60, 120, 300]));
        assert_eq!(json["defaultSeconds"], 120);
    }

    #[test]
    fn non_preset_error_uses_the_invalid_duration_string() {
        let json = serde_json::to_value(ErrorDto::not_a_preset(7)).unwrap();
        assert_eq!(json["messageKey"], "error.invalid_duration");
        assert!(json["details"].as_str().unwrap().contains("7 s"));
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
        for r in reasons {
            let key = format!("endReason.{}", end_reason_name(r));
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
        ] {
            assert!(strings.contains_key(key), "missing {key}");
        }
    }
}
