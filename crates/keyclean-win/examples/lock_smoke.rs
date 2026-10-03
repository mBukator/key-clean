//! Manual smoke test for the M1 engine. **Engages a real keyboard lock — run it by hand only.**
//!
//! ```text
//! cargo run -p keyclean-win --example lock_smoke
//! ```
//!
//! Prints the detected keyboards, counts down 3 s, locks the keyboard for at most 15 s (the
//! development cap, applied here even in `--release`), then prints how the lock ended. Ways out:
//! Ctrl+Alt+K, the 15 s timer, the 20 s hard deadline, or killing the process
//! (`Stop-Process -Name lock_smoke -Force`). See docs/testing/manual/M1.md.

use std::time::Duration;

use keyclean_win::keyclean_core::countdown;
use keyclean_win::keyclean_core::policy::{DEV_MAX_HARD_DEADLINE, DEV_MAX_SESSION};
use keyclean_win::{Engine, EngineEvent, LockRequest};

fn main() {
    let (engine, events) = match Engine::start() {
        Ok(started) => started,
        Err(e) => {
            eprintln!("engine failed to start: {}", e.details());
            std::process::exit(1);
        }
    };

    match engine.devices() {
        Ok(keyboards) if keyboards.is_empty() => println!("No keyboards detected."),
        Ok(keyboards) => {
            println!("Detected keyboards:");
            for k in keyboards {
                println!("  - {}", k.name.as_deref().unwrap_or("(unnamed keyboard)"));
                println!("    {}", k.id);
            }
        }
        Err(e) => println!("Could not list keyboards: {}", e.details()),
    }

    for n in (1..=3).rev() {
        println!("Locking in {n}...");
        std::thread::sleep(Duration::from_secs(1));
    }

    // Clamped to the development caps unconditionally, so even a release build of this example
    // can never lock for long.
    let request = LockRequest {
        duration: DEV_MAX_SESSION,
        max_lock: DEV_MAX_HARD_DEADLINE,
    };
    if let Err(e) = engine.lock(request) {
        eprintln!("lock request failed: {}", e.details());
        std::process::exit(1);
    }
    println!("Keyboard locked. Press Ctrl+Alt+K to unlock, or wait up to 15 s.");

    // Generous upper bound: hard deadline + drain + slack.
    let give_up = Duration::from_secs(40);
    let started = std::time::Instant::now();
    loop {
        let remaining = give_up.saturating_sub(started.elapsed());
        match events.recv_timeout(remaining) {
            Ok(EngineEvent::Status(status)) => {
                let left = status
                    .session_remaining
                    .map(|r| format!(", {} s left", countdown::display_secs(r)))
                    .unwrap_or_default();
                println!(
                    "state: {:?}{left}{}",
                    status.state,
                    if status.dev_cap { " (DEV CAP)" } else { "" }
                );
            }
            Ok(EngineEvent::SessionEnded { reason }) => {
                println!("Unlocked. End reason: {reason:?}");
                break;
            }
            Ok(EngineEvent::Error(e)) => println!("error: {}", e.details()),
            Ok(EngineEvent::Notice(n)) => println!("notice: {n:?}"),
            Err(_) => {
                println!(
                    "No session end reported within {give_up:?}; exiting (input is released on exit)."
                );
                break;
            }
        }
    }
    drop(engine);
}
