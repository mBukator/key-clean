//! The line-delimited JSON protocol between the app and its engine process (ADR 0009).
//!
//! The app sends [`WireCommand`]s on the engine's stdin; the engine sends [`WireEvent`]s on its
//! stdout, one JSON object per line. Only commands and session events cross — never key data.

use std::time::Duration;

use keyclean_core::session::{EndReason, SessionState, SystemTransition};
use serde::{Deserialize, Serialize};

use crate::engine::{
    DeviceChange, DeviceClass, EngineEvent, EngineNotice, EngineStatus, LockRequest, LockTargets,
};
use crate::error::EngineError;

/// App → engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub(crate) enum WireCommand {
    Lock {
        duration_ms: u64,
        max_lock_ms: u64,
        /// Lock the keyboard. Defaults to true, so a request without targets is a keyboard lock.
        #[serde(default = "yes")]
        keyboard: bool,
        /// Lock the mouse and touchpad.
        #[serde(default)]
        mouse: bool,
    },
    Unlock,
    Shutdown,
}

const fn yes() -> bool {
    true
}

impl WireCommand {
    pub(crate) fn lock(request: LockRequest) -> Self {
        WireCommand::Lock {
            duration_ms: millis(request.duration),
            max_lock_ms: millis(request.max_lock),
            keyboard: request.targets.keyboard,
            mouse: request.targets.mouse,
        }
    }
}

impl From<WireCommand> for Option<LockRequest> {
    fn from(command: WireCommand) -> Self {
        match command {
            WireCommand::Lock {
                duration_ms,
                max_lock_ms,
                keyboard,
                mouse,
            } => Some(LockRequest {
                duration: Duration::from_millis(duration_ms),
                max_lock: Duration::from_millis(max_lock_ms),
                targets: LockTargets { keyboard, mouse },
            }),
            _ => None,
        }
    }
}

/// What a session blocks, in a status event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WireTargets {
    keyboard: bool,
    mouse: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WireState {
    Idle,
    Starting,
    Locked,
    Unlocking,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WireReason {
    Timeout,
    Emergency,
    HardDeadline,
    Suspend,
    EndSession,
    SessionLock,
    SessionDisconnect,
    EngineError,
    UserRequest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WireNotice {
    HookAlreadyRemoved,
    DrainTimedOut,
    PowerNotificationUnavailable,
    SessionNotificationUnavailable,
    LivenessCheckUnavailable,
    RawInputNotRemoved,
    ElevatedWindowBypass,
    ElevatedWindowMouseBypass,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WireDeviceChange {
    Arrived,
    Removed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WireDeviceClass {
    #[default]
    Keyboard,
    Mouse,
}

/// Engine → app.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub(crate) enum WireEvent {
    /// The engine started and accepts commands.
    Ready,
    Status {
        state: WireState,
        dev_cap: bool,
        session_remaining_ms: Option<u64>,
        hard_deadline_remaining_ms: Option<u64>,
        /// What the session blocks; absent while idle.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        targets: Option<WireTargets>,
    },
    Ended {
        reason: WireReason,
    },
    Error {
        message_key: String,
        details: String,
    },
    Notice {
        notice: WireNotice,
    },
    /// A keyboard or mouse was connected or disconnected during a lock. No name or id crosses.
    Device {
        change: WireDeviceChange,
        #[serde(default)]
        class: WireDeviceClass,
    },
}

fn millis(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

impl From<SessionState> for WireState {
    fn from(s: SessionState) -> Self {
        match s {
            SessionState::Idle => WireState::Idle,
            SessionState::Starting => WireState::Starting,
            SessionState::Locked => WireState::Locked,
            SessionState::Unlocking => WireState::Unlocking,
        }
    }
}

impl From<WireState> for SessionState {
    fn from(s: WireState) -> Self {
        match s {
            WireState::Idle => SessionState::Idle,
            WireState::Starting => SessionState::Starting,
            WireState::Locked => SessionState::Locked,
            WireState::Unlocking => SessionState::Unlocking,
        }
    }
}

impl From<EndReason> for WireReason {
    fn from(r: EndReason) -> Self {
        match r {
            EndReason::Timeout => WireReason::Timeout,
            EndReason::Emergency => WireReason::Emergency,
            EndReason::HardDeadline => WireReason::HardDeadline,
            EndReason::SystemTransition(SystemTransition::Suspend) => WireReason::Suspend,
            EndReason::SystemTransition(SystemTransition::EndSession) => WireReason::EndSession,
            EndReason::SystemTransition(SystemTransition::SessionLock) => WireReason::SessionLock,
            EndReason::SystemTransition(SystemTransition::SessionDisconnect) => {
                WireReason::SessionDisconnect
            }
            EndReason::EngineError => WireReason::EngineError,
            EndReason::UserRequest => WireReason::UserRequest,
        }
    }
}

impl From<WireReason> for EndReason {
    fn from(r: WireReason) -> Self {
        match r {
            WireReason::Timeout => EndReason::Timeout,
            WireReason::Emergency => EndReason::Emergency,
            WireReason::HardDeadline => EndReason::HardDeadline,
            WireReason::Suspend => EndReason::SystemTransition(SystemTransition::Suspend),
            WireReason::EndSession => EndReason::SystemTransition(SystemTransition::EndSession),
            WireReason::SessionLock => EndReason::SystemTransition(SystemTransition::SessionLock),
            WireReason::SessionDisconnect => {
                EndReason::SystemTransition(SystemTransition::SessionDisconnect)
            }
            WireReason::EngineError => EndReason::EngineError,
            WireReason::UserRequest => EndReason::UserRequest,
        }
    }
}

impl From<EngineNotice> for WireNotice {
    fn from(n: EngineNotice) -> Self {
        match n {
            EngineNotice::HookAlreadyRemoved => WireNotice::HookAlreadyRemoved,
            EngineNotice::DrainTimedOut => WireNotice::DrainTimedOut,
            EngineNotice::PowerNotificationUnavailable => WireNotice::PowerNotificationUnavailable,
            EngineNotice::SessionNotificationUnavailable => {
                WireNotice::SessionNotificationUnavailable
            }
            EngineNotice::LivenessCheckUnavailable => WireNotice::LivenessCheckUnavailable,
            EngineNotice::RawInputNotRemoved => WireNotice::RawInputNotRemoved,
            EngineNotice::ElevatedWindowBypass => WireNotice::ElevatedWindowBypass,
            EngineNotice::ElevatedWindowMouseBypass => WireNotice::ElevatedWindowMouseBypass,
        }
    }
}

impl From<WireNotice> for EngineNotice {
    fn from(n: WireNotice) -> Self {
        match n {
            WireNotice::HookAlreadyRemoved => EngineNotice::HookAlreadyRemoved,
            WireNotice::DrainTimedOut => EngineNotice::DrainTimedOut,
            WireNotice::PowerNotificationUnavailable => EngineNotice::PowerNotificationUnavailable,
            WireNotice::SessionNotificationUnavailable => {
                EngineNotice::SessionNotificationUnavailable
            }
            WireNotice::LivenessCheckUnavailable => EngineNotice::LivenessCheckUnavailable,
            WireNotice::RawInputNotRemoved => EngineNotice::RawInputNotRemoved,
            WireNotice::ElevatedWindowBypass => EngineNotice::ElevatedWindowBypass,
            WireNotice::ElevatedWindowMouseBypass => EngineNotice::ElevatedWindowMouseBypass,
        }
    }
}

impl From<DeviceChange> for (WireDeviceChange, WireDeviceClass) {
    fn from(c: DeviceChange) -> Self {
        let class = |class| match class {
            DeviceClass::Keyboard => WireDeviceClass::Keyboard,
            DeviceClass::Mouse => WireDeviceClass::Mouse,
        };
        match c {
            DeviceChange::Arrived(c) => (WireDeviceChange::Arrived, class(c)),
            DeviceChange::Removed(c) => (WireDeviceChange::Removed, class(c)),
        }
    }
}

impl From<(WireDeviceChange, WireDeviceClass)> for DeviceChange {
    fn from((change, class): (WireDeviceChange, WireDeviceClass)) -> Self {
        let class = match class {
            WireDeviceClass::Keyboard => DeviceClass::Keyboard,
            WireDeviceClass::Mouse => DeviceClass::Mouse,
        };
        match change {
            WireDeviceChange::Arrived => DeviceChange::Arrived(class),
            WireDeviceChange::Removed => DeviceChange::Removed(class),
        }
    }
}

impl From<LockTargets> for WireTargets {
    fn from(t: LockTargets) -> Self {
        WireTargets {
            keyboard: t.keyboard,
            mouse: t.mouse,
        }
    }
}

impl From<WireTargets> for LockTargets {
    fn from(t: WireTargets) -> Self {
        LockTargets {
            keyboard: t.keyboard,
            mouse: t.mouse,
        }
    }
}

impl From<&EngineEvent> for WireEvent {
    fn from(event: &EngineEvent) -> Self {
        match event {
            EngineEvent::Status(s) => WireEvent::Status {
                state: s.state.into(),
                dev_cap: s.dev_cap,
                session_remaining_ms: s.session_remaining.map(millis),
                hard_deadline_remaining_ms: s.hard_deadline_remaining.map(millis),
                targets: s.targets.map(Into::into),
            },
            EngineEvent::SessionEnded { reason } => WireEvent::Ended {
                reason: (*reason).into(),
            },
            EngineEvent::Error(e) => WireEvent::Error {
                message_key: e.message_key().to_owned(),
                details: e.details(),
            },
            EngineEvent::Notice(n) => WireEvent::Notice {
                notice: (*n).into(),
            },
            EngineEvent::DeviceChanged(c) => {
                let (change, class) = (*c).into();
                WireEvent::Device { change, class }
            }
        }
    }
}

impl WireEvent {
    /// The app-side event, or `None` for the `Ready` handshake.
    pub(crate) fn into_event(self) -> Option<EngineEvent> {
        Some(match self {
            WireEvent::Ready => return None,
            WireEvent::Status {
                state,
                dev_cap,
                session_remaining_ms,
                hard_deadline_remaining_ms,
                targets,
            } => EngineEvent::Status(EngineStatus {
                state: state.into(),
                dev_cap,
                session_remaining: session_remaining_ms.map(Duration::from_millis),
                hard_deadline_remaining: hard_deadline_remaining_ms.map(Duration::from_millis),
                targets: targets.map(Into::into),
            }),
            WireEvent::Ended { reason } => EngineEvent::SessionEnded {
                reason: reason.into(),
            },
            WireEvent::Error {
                message_key,
                details,
            } => EngineEvent::Error(EngineError::Remote {
                message_key,
                details,
            }),
            WireEvent::Notice { notice } => EngineEvent::Notice(notice.into()),
            WireEvent::Device { change, class } => {
                EngineEvent::DeviceChanged((change, class).into())
            }
        })
    }
}

/// Serializes one protocol message as a single line (no trailing newline).
pub(crate) fn to_line<T: Serialize>(message: &T) -> String {
    // Serializing these plain enums can't fail; fall back to an empty object just in case.
    serde_json::to_string(message).unwrap_or_else(|_| "{}".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip_event(event: EngineEvent) -> EngineEvent {
        let line = to_line(&WireEvent::from(&event));
        let wire: WireEvent = serde_json::from_str(&line).unwrap();
        wire.into_event().unwrap()
    }

    #[test]
    fn commands_round_trip() {
        let lock = WireCommand::lock(LockRequest {
            duration: Duration::from_secs(10),
            max_lock: Duration::from_secs(1800),
            targets: LockTargets {
                keyboard: false,
                mouse: true,
            },
        });
        for command in [lock, WireCommand::Unlock, WireCommand::Shutdown] {
            let parsed: WireCommand = serde_json::from_str(&to_line(&command)).unwrap();
            assert_eq!(parsed, command);
        }
        assert_eq!(
            to_line(&lock),
            r#"{"cmd":"lock","duration_ms":10000,"max_lock_ms":1800000,"keyboard":false,"mouse":true}"#
        );
        let request: Option<LockRequest> = lock.into();
        let request = request.unwrap();
        assert_eq!(request.duration, Duration::from_secs(10));
        assert_eq!(
            request.targets,
            LockTargets {
                keyboard: false,
                mouse: true
            }
        );
    }

    #[test]
    fn lock_without_targets_is_a_keyboard_lock() {
        let parsed: WireCommand =
            serde_json::from_str(r#"{"cmd":"lock","duration_ms":10000,"max_lock_ms":1800000}"#)
                .unwrap();
        let request: Option<LockRequest> = parsed.into();
        assert_eq!(request.unwrap().targets, LockTargets::KEYBOARD);
    }

    #[test]
    fn every_end_reason_round_trips() {
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
        for reason in reasons {
            let event = EngineEvent::SessionEnded { reason };
            assert_eq!(round_trip_event(event.clone()), event);
        }
    }

    #[test]
    fn status_notices_and_errors_round_trip() {
        let status = EngineEvent::Status(EngineStatus {
            state: SessionState::Locked,
            dev_cap: true,
            session_remaining: Some(Duration::from_millis(9500)),
            hard_deadline_remaining: None,
            targets: Some(LockTargets::ALL),
        });
        assert_eq!(round_trip_event(status.clone()), status);
        let idle = EngineEvent::Status(EngineStatus {
            state: SessionState::Idle,
            dev_cap: false,
            session_remaining: None,
            hard_deadline_remaining: None,
            targets: None,
        });
        assert_eq!(round_trip_event(idle.clone()), idle);

        for notice in [
            EngineNotice::HookAlreadyRemoved,
            EngineNotice::DrainTimedOut,
            EngineNotice::PowerNotificationUnavailable,
            EngineNotice::SessionNotificationUnavailable,
            EngineNotice::LivenessCheckUnavailable,
            EngineNotice::RawInputNotRemoved,
            EngineNotice::ElevatedWindowBypass,
            EngineNotice::ElevatedWindowMouseBypass,
        ] {
            let event = EngineEvent::Notice(notice);
            assert_eq!(round_trip_event(event.clone()), event);
        }

        for class in [DeviceClass::Keyboard, DeviceClass::Mouse] {
            for change in [DeviceChange::Arrived(class), DeviceChange::Removed(class)] {
                let event = EngineEvent::DeviceChanged(change);
                assert_eq!(round_trip_event(event.clone()), event);
            }
        }
        assert_eq!(
            to_line(&WireEvent::from(&EngineEvent::DeviceChanged(
                DeviceChange::Removed(DeviceClass::Mouse)
            ))),
            r#"{"event":"device","change":"removed","class":"mouse"}"#
        );
        let old: WireEvent =
            serde_json::from_str(r#"{"event":"device","change":"arrived"}"#).unwrap();
        assert_eq!(
            old.into_event(),
            Some(EngineEvent::DeviceChanged(DeviceChange::Arrived(
                DeviceClass::Keyboard
            )))
        );

        let error = round_trip_event(EngineEvent::Error(EngineError::AlreadyActive));
        match error {
            EngineEvent::Error(e) => {
                assert_eq!(e.message_key(), "error.already_locked");
                assert_eq!(e.details(), EngineError::AlreadyActive.details());
            }
            other => panic!("unexpected {other:?}"),
        }

        for (error, key) in [
            (EngineError::HookLost, "error.hook_lost"),
            (EngineError::MouseHookLost, "error.mouse_hook_lost"),
            (
                EngineError::InvalidRequest(keyclean_core::policy::PolicyError::NoTargets),
                "error.no_targets",
            ),
        ] {
            match round_trip_event(EngineEvent::Error(error.clone())) {
                EngineEvent::Error(e) => {
                    assert_eq!(e.message_key(), key);
                    assert_eq!(e.details(), error.details());
                }
                other => panic!("unexpected {other:?}"),
            }
        }
    }

    #[test]
    fn ready_is_not_an_app_event() {
        assert_eq!(WireEvent::Ready.into_event(), None);
        let parsed: WireEvent = serde_json::from_str(r#"{"event":"ready"}"#).unwrap();
        assert_eq!(parsed, WireEvent::Ready);
    }

    #[test]
    fn garbage_is_rejected() {
        assert!(serde_json::from_str::<WireCommand>("lock please").is_err());
        assert!(serde_json::from_str::<WireCommand>(r#"{"cmd":"format_c"}"#).is_err());
    }
}
