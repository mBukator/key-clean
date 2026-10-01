//! Data sent to the webview. Mirrors `src/shared/types/engine.ts`. Contains no key data.

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
    lock_seconds: u64,
}

impl StatusDto {
    /// The status before any engine event arrives.
    pub fn initial(dev_cap: bool, lock_seconds: u64) -> Self {
        StatusDto {
            state: state_name(SessionState::Idle),
            dev_cap,
            last_end_reason: None,
            error: None,
            engine_available: false,
            lock_seconds,
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
            }
            EngineEvent::SessionEnded { reason } => {
                self.last_end_reason = Some(end_reason_name(*reason));
            }
            EngineEvent::Error(e) => self.error = Some(ErrorDto::from(e)),
            EngineEvent::Notice(_) => {}
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

    fn status(state: SessionState) -> EngineEvent {
        EngineEvent::Status(EngineStatus {
            state,
            dev_cap: true,
            session_remaining: None,
            hard_deadline_remaining: None,
        })
    }

    #[test]
    fn serializes_camel_case() {
        let mut dto = StatusDto::initial(true, 10);
        dto.set_engine(true, None);
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["lockSeconds"], 10);
        assert_eq!(json["state"], "idle");
        assert_eq!(json["devCap"], true);
        assert_eq!(json["engineAvailable"], true);
        assert!(json["lastEndReason"].is_null());
    }

    #[test]
    fn a_session_round_trip() {
        let mut dto = StatusDto::initial(true, 10);
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
