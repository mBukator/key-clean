//! All Win32 and all `unsafe` for KeyClean: engine thread, low-level hooks, device
//! enumeration, system events, watchdog, and the engine process the app talks to (ADR 0009).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod client;
mod close_guard;
mod device_watch;
mod devices;
mod engine;
mod error;
mod foreground;
mod hook;
pub mod host;
mod keys;
mod msg;
mod protocol;
mod qpc;
mod raw_input;
#[cfg(feature = "testkit")]
pub mod testkit;
mod watchdog;

pub use client::EngineClient;
pub use close_guard::guard_helper_windows;
pub use device_watch::DeviceWatch;
pub use devices::input_devices;
pub use engine::{
    DeviceChange, DeviceClass, Engine, EngineEvent, EngineNotice, EngineStatus, LockRequest,
    LockTargets, safety_profile,
};
pub use error::EngineError;
pub use keyclean_core;
pub use keyclean_core::session::{EndReason, SessionState, SystemTransition};
