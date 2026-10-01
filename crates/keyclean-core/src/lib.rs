//! Pure session logic for KeyClean: state machine, timers, safety policy, chord detection.
//! No Windows dependencies.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
