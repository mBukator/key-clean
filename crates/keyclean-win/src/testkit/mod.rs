//! Test kit for the automated end-to-end harness (`crates/keyclean-e2e`, ADR 0008).
//!
//! Only compiled with the non-default `testkit` feature; the app never calls it. It lets a test
//! program drive a real lock without a human:
//! - [`inject`] synthesizes key events with `SendInput`, each tagged with [`inject::TAG`];
//! - [`observer::Observer`] is a low-level hook installed *before* the engine's, so it only sees
//!   events that got past KeyClean. It counts tagged events and ignores everything else, so it
//!   never sees or records what a person types;
//! - [`held`] reports which keys Windows believes are down (the stuck-key check);
//! - [`system`] has process-window, workstation-lock and Raw Input registration helpers;
//! - [`stall_next_callback`], [`raw_input_seen`], [`raw_mouse_seen`] and [`liveness_misses`]
//!   drive and observe the hook-liveness check.
//!
//! Anything that uses this engages real input locks: run it by hand, never from `cargo test`.

pub mod held;
pub mod inject;
pub mod observer;
pub mod system;

pub use crate::watchdog::WATCHDOG_EXIT_CODE;

use std::sync::atomic::Ordering;

/// Makes the next keyboard hook callback sleep for `ms` milliseconds, to provoke a real
/// `LowLevelHooksTimeout`. Only this test kit can set it; the app's hook never sleeps.
pub fn stall_next_callback(ms: u32) {
    crate::hook::STALL_NEXT_CALLBACK_MS.store(ms, Ordering::Release);
}

/// How many `WM_INPUT` messages the engine window has received in this process. The engine reads
/// only their header (keyboard or mouse), never their contents.
pub fn raw_input_seen() -> u64 {
    crate::engine::RAW_INPUT_SEEN.load(Ordering::Acquire)
}

/// How many of those `WM_INPUT` messages came from a mouse (including touchpads).
pub fn raw_mouse_seen() -> u64 {
    crate::engine::RAW_MOUSE_SEEN.load(Ordering::Acquire)
}

/// Failed liveness checks counted while `Engine::testkit_liveness_report_only` is on, as
/// `(keyboard, mouse)`. Each check covers the first raw message of a 250 ms window, so this
/// counts windows in which raw input arrived but the hook wasn't called, not single messages.
pub fn liveness_misses() -> (u64, u64) {
    let misses = &crate::engine::LIVENESS_MISSES;
    (
        misses[0].load(Ordering::Acquire),
        misses[1].load(Ordering::Acquire),
    )
}
