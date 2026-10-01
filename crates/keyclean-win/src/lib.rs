//! All Win32 and all `unsafe` for KeyClean: engine thread, low-level hooks, device
//! enumeration, system events, watchdog.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod devices;
mod engine;
mod error;
mod hook;
mod keys;
mod msg;
mod qpc;
mod watchdog;

pub use devices::KeyboardDevice;
pub use engine::{Engine, EngineEvent, EngineNotice, EngineStatus, LockRequest, safety_profile};
pub use error::EngineError;
pub use keyclean_core;
pub use keyclean_core::session::{EndReason, SessionState, SystemTransition};
