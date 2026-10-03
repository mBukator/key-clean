//! Pure session logic for KeyClean: state machine, timers, safety policy, chord detection.
//! No Windows dependencies.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod chord;
pub mod keystate;
pub mod policy;
pub mod session;
pub mod time;
