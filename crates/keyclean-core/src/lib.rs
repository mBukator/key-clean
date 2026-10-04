//! Pure session logic for KeyClean: state machine, timers, safety policy, chord detection.
//! No Windows dependencies.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod chord;
pub mod countdown;
pub mod devices;
pub mod keystate;
pub mod liveness;
pub mod policy;
pub mod presets;
pub mod restart;
pub mod session;
pub mod time;
