//! Automated end-to-end checks for KeyClean's milestones M1-M6 (ADR 0008).
//!
//! **This engages real keyboard and mouse locks.** Run it by hand only, never from `cargo test`
//! or by an agent. Each lock is clamped to the development caps (15 s, 20 s hard deadline), the
//! real Ctrl+Alt+K works throughout, and an outside watchdog kills this process after 10 minutes.
//!
//! The app checks need a debug app exe with the UI embedded (`tauri/custom-protocol`); a plain
//! `cargo build -p keyclean` loads the UI from a dev server, so its overlay never confirms.
//!
//! ```text
//! cargo build -p keyclean-win --example lock_smoke
//! bun run build && cargo build -p keyclean --features tauri/custom-protocol
//! cargo run -p keyclean-e2e -- [--skip-app] [--session-lock] [--stall] [--raw-diag] [--mouse-diag] [--overlay-latency]
//! ```
//!
//! `--overlay-latency` adds S41: ten 1 s locks in one app launch, reporting how long the overlay
//! takes to confirm, cold and warm (about 45 s more).
//!
//! Exit codes: 0 all passed, 1 a check failed, 2 the environment check failed (injected keys or
//! mouse moves don't reach the observer: another hook, or no interactive desktop), 3 setup error.
//!
//! ```text
//! ```

#![forbid(unsafe_code)]

mod harness;
mod report;
mod scenarios;

use std::os::windows::process::CommandExt;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use keyclean_win::testkit::inject::vk;
use keyclean_win::testkit::observer::Observer;

use harness::EngineRig;
use report::Report;
use scenarios::{Ctx, Outcome};

/// The outside watchdog's limit on the whole run. The default run takes about 7 minutes (the M5
/// mouse checks add about 40 s, the M6 overlay checks S34-S40 about 100 s: seven app launches of
/// 10-17 s each, settle time included); `--overlay-latency` adds about 45 s.
const RUN_LIMIT: Duration = Duration::from_secs(600);

const EXIT_FAILED: i32 = 1;
const EXIT_ENVIRONMENT: i32 = 2;
const EXIT_SETUP: i32 = 3;

/// `CREATE_NO_WINDOW`: the watchdog gets no console, so it can't hide or share the user's.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

struct Options {
    skip_app: bool,
    session_lock: bool,
    stall: bool,
    raw_diag: bool,
    mouse_diag: bool,
    overlay_latency: bool,
    only: Option<Vec<String>>,
}

impl Options {
    fn parse() -> Result<Options, String> {
        let mut options = Options {
            skip_app: false,
            session_lock: false,
            stall: false,
            raw_diag: false,
            mouse_diag: false,
            overlay_latency: false,
            only: None,
        };
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--only" => {
                    let ids = args
                        .next()
                        .filter(|ids| !ids.starts_with("--"))
                        .ok_or(format!("--only needs a list such as S2,S11\n\n{USAGE}"))?;
                    options.only =
                        Some(ids.split(',').map(|id| id.trim().to_uppercase()).collect());
                }
                "--skip-app" => options.skip_app = true,
                "--session-lock" => options.session_lock = true,
                "--stall" => options.stall = true,
                "--raw-diag" => options.raw_diag = true,
                "--mouse-diag" => options.mouse_diag = true,
                "--overlay-latency" => options.overlay_latency = true,
                "--help" | "-h" => return Err(USAGE.into()),
                other => return Err(format!("unknown option {other}\n\n{USAGE}")),
            }
        }
        Ok(options)
    }
}

const USAGE: &str = "usage: cargo run -p keyclean-e2e -- [--skip-app] [--session-lock] [--stall] [--raw-diag] [--mouse-diag] [--overlay-latency] [--only S2,S11]
  --only IDS         run only these checks (comma-separated IDs from the report)
  --skip-app         skip the scenarios that start the app (target/debug/keyclean.exe)
  --session-lock     also lock the workstation (you'll have to sign back in); runs last
  --stall            also stall the hook past Windows' timeout (S20; input lags up to 1 s once)
  --raw-diag         also run S25: the lost-hook check after each system shortcut (opens Game Bar)
  --mouse-diag       also run S33: you use the touchpad during a 15 s mouse-only lock (prompts)
  --overlay-latency  also run S41: overlay confirmation times over ten 1 s locks (about 45 s)";

/// Kills this process after `RUN_LIMIT`, from outside, in case the harness hangs. Disarmed on
/// drop.
struct OutsideWatchdog(Option<Child>);

impl OutsideWatchdog {
    fn arm() -> OutsideWatchdog {
        // Holds a handle to this process from the start, so a recycled PID is never killed, and
        // exits by itself as soon as this process ends.
        let script = format!(
            "$p = Get-Process -Id {} -ErrorAction SilentlyContinue; if ($p -and -not $p.WaitForExit({})) {{ $p.Kill() }}",
            std::process::id(),
            RUN_LIMIT.as_millis()
        );
        let child = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .ok();
        if child.is_none() {
            eprintln!("warning: couldn't start the outside watchdog (powershell)");
        }
        OutsideWatchdog(child)
    }
}

impl Drop for OutsideWatchdog {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn main() {
    // Child process of S19 (a hung engine thread); the watchdog is expected to end it.
    if std::env::args().nth(1).as_deref() == Some(scenarios::CHILD_HANG_FLAG) {
        std::process::exit(scenarios::child_hang());
    }
    let options = match Options::parse() {
        Ok(o) => o,
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(EXIT_SETUP);
        }
    };
    std::process::exit(run(&options));
}

fn run(options: &Options) -> i32 {
    println!("KeyClean end-to-end checks (M1-M6)");
    println!(
        "This locks your keyboard and mouse several times over about 7 minutes (10 minutes at \
         most)."
    );
    println!("  - Hands off the keyboard, mouse and touchpad until it finishes.");
    println!("  - Click into an empty Notepad window now (anything that leaks lands there) and");
    println!("    leave the pointer over it; the harness puts the pointer back after each check.");
    println!("  - The real Ctrl+Alt+K always unlocks. Ctrl+C now to cancel.");
    if options.mouse_diag {
        println!("  - Near the end (S33) you'll be asked to use the touchpad; watch for GO.");
    }
    for n in (1..=5).rev() {
        println!("Starting in {n}...");
        std::thread::sleep(Duration::from_secs(1));
    }

    let _watchdog = OutsideWatchdog::arm();
    keyclean_win::testkit::system::make_dpi_aware();
    let started = Instant::now();
    let mut report = Report::new();

    // The observer must be installed before any lock, so KeyClean's hook runs first.
    let observer = match Observer::start() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("couldn't install the observer hook: {e}");
            return EXIT_SETUP;
        }
    };

    // Sanity: injected probes must reach the observer while nothing is locked. Otherwise another
    // hook is swallowing input, or there is no interactive desktop (e.g. a CI runner).
    match harness::probe_passes(&observer, vk::F13) {
        Ok(true) => {}
        Ok(false) => {
            eprintln!(
                "environment check failed: injected keys don't reach the observer. Another \
                 keyboard hook (AutoHotkey, PowerToys, a remapper) may be swallowing them, or \
                 this session has no interactive desktop."
            );
            return EXIT_ENVIRONMENT;
        }
        Err(e) => {
            eprintln!("environment check failed: {e}");
            return EXIT_ENVIRONMENT;
        }
    }

    // The same for the mouse. Kinds that don't reach hooks at all here are left out of the mouse
    // checks (and named in their results); without moves the mouse can't be tested.
    match harness::check_mouse_environment(&observer) {
        Ok(missing) if missing.contains(&harness::Probe::Move) => {
            eprintln!(
                "environment check failed: injected mouse moves don't reach the observer. \
                 Another mouse hook may be swallowing them, or this session has no interactive \
                 desktop."
            );
            return EXIT_ENVIRONMENT;
        }
        Ok(missing) if !missing.is_empty() => {
            eprintln!(
                "warning: these injected mouse events don't reach low-level hooks here and are \
                 left out of the mouse checks: {}",
                harness::names(&missing)
            );
        }
        Ok(_) => {}
        Err(e) => {
            eprintln!("environment check failed: {e}");
            return EXIT_ENVIRONMENT;
        }
    }

    let mut rig = match EngineRig::start() {
        Ok(rig) => rig,
        Err(e) => {
            eprintln!("engine failed to start: {e}");
            return EXIT_SETUP;
        }
    };

    let mut scenarios = scenarios::engine_scenarios();
    if options.stall {
        // Before S22, so its Raw Input cleanup is checked too.
        let at = scenarios
            .iter()
            .position(|s| s.id == "S22")
            .unwrap_or(scenarios.len());
        scenarios.insert(at, scenarios::hook_timeout_scenario());
    }
    scenarios.extend(scenarios::process_scenarios(options.skip_app));
    if options.overlay_latency && !options.skip_app {
        scenarios.push(scenarios::overlay_latency_scenario());
    }
    // Last of the observer checks: touchpad gestures pass the keyboard hook of a mouse-only lock
    // as shortcuts (Task View, desktop switch) and could spoil the checks after them.
    if options.mouse_diag {
        scenarios.push(scenarios::mouse_diag_scenario());
    }
    if options.session_lock {
        scenarios.push(scenarios::session_lock_scenario());
    }

    let run_s24 = options
        .only
        .as_ref()
        .is_none_or(|only| only.iter().any(|id| id == "S24"));
    let run_s14 = !options.skip_app
        && options
            .only
            .as_ref()
            .is_none_or(|only| only.iter().any(|id| id == "S14"));
    if let Some(only) = &options.only {
        for id in only {
            if !["S14", "S24", "S25"].contains(&id.as_str())
                && !scenarios.iter().any(|s| s.id == id.as_str())
            {
                let hint = if id == "S13" {
                    " (S13 also needs --session-lock)"
                } else if id == "S20" {
                    " (S20 also needs --stall)"
                } else if id == "S33" {
                    " (S33 also needs --mouse-diag)"
                } else if id == "S41" && !options.overlay_latency {
                    " (S41 also needs --overlay-latency)"
                } else if options.skip_app
                    && [
                        "S10", "S11", "S12", "S15", "S21", "S23", "S34", "S35", "S36", "S37",
                        "S38", "S39", "S40", "S41",
                    ]
                    .contains(&id.as_str())
                {
                    " (app checks are off with --skip-app)"
                } else {
                    ""
                };
                eprintln!("warning: --only {id} matches no check{hint}");
            }
        }
        scenarios.retain(|s| only.iter().any(|id| id == s.id));
        if scenarios.is_empty() && !run_s14 && !run_s24 && !options.raw_diag {
            eprintln!("--only matched no checks");
            return EXIT_SETUP;
        }
        report.mark_partial(&only.join(","));
    }

    for scenario in &scenarios {
        print!("{} {} ... ", scenario.id, scenario.name);
        let outcome = {
            let mut ctx = Ctx {
                observer: &observer,
                rig: &mut rig,
            };
            (scenario.run)(&mut ctx)
        };
        let label = match &outcome {
            Outcome::Pass(_) => "PASS",
            Outcome::Fail(_) => "FAIL",
            Outcome::Skip(_) => "SKIP",
        };
        println!("{label}");
        report.add(scenario.id, scenario.steps, scenario.name, outcome);
        if options.session_lock && scenario.id == "S13" {
            println!("Sign back in to Windows; the results follow.");
        }
    }

    drop(rig);
    drop(observer);

    // S24 repeats S17 without the observer hook, as in the real engine process.
    if run_s24 {
        print!("S24 A removed hook is detected with no other hook in the process ... ");
        let outcome = scenarios::hook_lost_without_observer();
        println!(
            "{}",
            match &outcome {
                Outcome::Pass(_) => "PASS",
                Outcome::Fail(_) => "FAIL",
                Outcome::Skip(_) => "SKIP",
            }
        );
        report.add(
            "S24",
            "M3 A",
            "A removed hook is detected with no other hook in the process",
            outcome,
        );
    }

    if options.raw_diag {
        print!("S25 Raw Input after each system shortcut (diagnostic) ... ");
        let outcome = scenarios::raw_input_diagnostic();
        println!(
            "{}",
            match &outcome {
                Outcome::Pass(_) => "PASS",
                Outcome::Fail(_) => "FAIL",
                Outcome::Skip(_) => "SKIP",
            }
        );
        report.add(
            "S25",
            "diagnostic",
            "Raw Input after each system shortcut",
            outcome,
        );
    }

    // S14 runs last and without the observer hook, which could mask the bug it looks for.
    if run_s14 {
        print!("S14 Ctrl+Alt+K works while the overlay has focus ... ");
        let outcome = scenarios::focused_app_chord();
        println!(
            "{}",
            match &outcome {
                Outcome::Pass(_) => "PASS",
                Outcome::Fail(_) => "FAIL",
                Outcome::Skip(_) => "SKIP",
            }
        );
        report.add(
            "S14",
            "F3 (focus)",
            "Ctrl+Alt+K works while the overlay has focus",
            outcome,
        );
    }

    report.print(started.elapsed());
    if let Some(path) = report.write_next_to_exe(started.elapsed()) {
        println!("Report written to {}", path.display());
    }
    if report.failed() == 0 { 0 } else { EXIT_FAILED }
}
