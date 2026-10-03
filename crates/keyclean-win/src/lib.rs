//! All Win32 and all `unsafe` for KeyClean: engine thread, low-level hooks, device
//! enumeration, system events, watchdog, and the engine process the app talks to (ADR 0009).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod client;
mod devices;
mod engine;
mod error;
mod hook;
pub mod host;
mod keys;
mod msg;
mod protocol;
mod qpc;
#[cfg(feature = "testkit")]
pub mod testkit;
mod watchdog;

pub use client::EngineClient;
pub use devices::KeyboardDevice;
pub use engine::{Engine, EngineEvent, EngineNotice, EngineStatus, LockRequest, safety_profile};
pub use error::EngineError;
pub use keyclean_core;
pub use keyclean_core::session::{EndReason, SessionState, SystemTransition};
