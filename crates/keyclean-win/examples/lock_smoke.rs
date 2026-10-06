//! Manual smoke test for the M1 engine. **Engages a real keyboard lock — run it by hand only.**
//!
//! ```text
//! cargo run -p keyclean-win --example lock_smoke            # keyboard only
//! cargo run -p keyclean-win --example lock_smoke -- --mouse  # keyboard, mouse and touchpad
//! ```
//!
//! Prints the detected devices, counts down 3 s, locks the keyboard (and with `--mouse` the mouse
//! and touchpad too) for at most 15 s (the
//! development cap, applied here even in `--release`), then prints how the lock ended. Ways out:
//! Ctrl+Alt+K, the 15 s timer, the 20 s hard deadline, or killing the process
//! (`Stop-Process -Name lock_smoke -Force`). See docs/testing/manual/M1.md.

use std::time::Duration;

use keyclean_win::keyclean_core::countdown;
use keyclean_win::keyclean_core::policy::{DEV_MAX_HARD_DEADLINE, DEV_MAX_SESSION};
use keyclean_win::{Engine, EngineEvent, LockRequest, LockTargets};

fn main() {
    let targets = if std::env::args().any(|arg| arg == "--mouse") {
        LockTargets::ALL
    } else {
        LockTargets::KEYBOARD
    };
    let (engine, events) = match Engine::start() {
        Ok(started) => started,
        Err(e) => {
            eprintln!("engine failed to start: {}", e.details());
            std::process::exit(1);
        }
    };

    match keyclean_win::input_devices() {
        Ok(devices) if devices.is_empty() => println!("No input devices detected."),
        Ok(devices) => {
            println!("Detected input devices:");
            for d in devices {
                println!(
                    "  - {:?}, {:?}: {}",
                    d.kind,
                    d.capability,
                    d.name.as_deref().unwrap_or("(unnamed)")
                );
                println!("    {}", d.id);
            }
        }
        Err(e) => println!("Could not list devices: {}", e.details()),
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
        targets,
    };
    if let Err(e) = engine.lock(request) {
        eprintln!("lock request failed: {}", e.details());
        std::process::exit(1);
    }
    let what = if targets.mouse {
        "Keyboard, mouse and touchpad"
    } else {
        "Keyboard"
    };
    println!("{what} locked. Press Ctrl+Alt+K to unlock, or wait up to 15 s.");

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
            Ok(EngineEvent::DeviceChanged(c)) => println!("device: {c:?}"),
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
