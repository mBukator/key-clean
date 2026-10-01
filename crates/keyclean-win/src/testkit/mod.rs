//! Test kit for the automated end-to-end harness (`crates/keyclean-e2e`, ADR 0008).
//!
//! Only compiled with the non-default `testkit` feature; the app never calls it. It lets a test
//! program drive a real lock without a human:
//! - [`inject`] synthesizes key events with `SendInput`, each tagged with [`inject::TAG`];
//! - [`observer::Observer`] is a low-level hook installed *before* the engine's, so it only sees
//!   events that got past KeyClean. It counts tagged events and ignores everything else, so it
//!   never sees or records what a person types;
//! - [`held`] reports which keys Windows believes are down (the stuck-key check);
//! - [`system`] has process-window and workstation-lock helpers.
//!
//! Anything that uses this engages real input locks: run it by hand, never from `cargo test`.

pub mod held;
pub mod inject;
pub mod observer;
pub mod system;
