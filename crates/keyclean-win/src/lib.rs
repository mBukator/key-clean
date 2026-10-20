//! All Win32 and all `unsafe` for KeyClean: engine thread, low-level hooks, device
//! enumeration, system events, watchdog, and the engine process the app talks to (ADR 0009).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod autostart;
pub mod client;
mod close_guard;
mod desktop;
mod device_watch;
mod devices;
mod display;
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
mod taskbar;
#[cfg(feature = "testkit")]
pub mod testkit;
mod watchdog;

pub use autostart::{AutostartError, autostart_state, disable_autostart, enable_autostart};
pub use client::EngineClient;
pub use close_guard::{guard_dashboard_close, guard_helper_windows};
pub use desktop::is_on_current_desktop;
pub use device_watch::DeviceWatch;
pub use devices::input_devices;
pub use display::DisplayRequest;
pub use engine::{
    DeviceChange, DeviceClass, Engine, EngineEvent, EngineNotice, EngineStatus, LockRequest,
    LockTargets, safety_profile,
};
pub use error::EngineError;
pub use foreground::is_elevated_above_us;
pub use keyclean_core;
pub use keyclean_core::session::{EndReason, SessionState, SystemTransition};
pub use raw_input::registered_raw_input_count;
pub use taskbar::{taskbar_uses_light_theme, watch_setting_changes};
