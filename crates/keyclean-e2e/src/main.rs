//! Automated end-to-end checks for KeyClean's milestones M1-M3 (ADR 0008).
//!
//! **This engages real keyboard locks.** Run it by hand only, never from `cargo test` or by an
//! agent. Each lock is clamped to the development caps (15 s, 20 s hard deadline), the real
//! Ctrl+Alt+K works throughout, and an outside watchdog kills this process after 6 minutes.
//!
//! ```text
//! cargo build -p keyclean-win --example lock_smoke
//! bun run build && cargo build -p keyclean
//! cargo run -p keyclean-e2e -- [--skip-app] [--session-lock] [--stall]
//! ```
//!
//! Exit codes: 0 all passed, 1 a check failed, 2 the environment check failed (injected keys don't
//! reach the observer: another keyboard hook, or no interactive desktop), 3 setup error.
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

/// The outside watchdog's limit on the whole run.
const RUN_LIMIT: Duration = Duration::from_secs(360);

const EXIT_FAILED: i32 = 1;
const EXIT_ENVIRONMENT: i32 = 2;
const EXIT_SETUP: i32 = 3;

/// `CREATE_NO_WINDOW`: the watchdog gets no console, so it can't hide or share the user's.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

struct Options {
    skip_app: bool,
    session_lock: bool,
    stall: bool,
    only: Option<Vec<String>>,
}

impl Options {
    fn parse() -> Result<Options, String> {
        let mut options = Options {
            skip_app: false,
            session_lock: false,
            stall: false,
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
                "--help" | "-h" => return Err(USAGE.into()),
                other => return Err(format!("unknown option {other}\n\n{USAGE}")),
            }
        }
        Ok(options)
    }
}

const USAGE: &str =
    "usage: cargo run -p keyclean-e2e -- [--skip-app] [--session-lock] [--stall] [--only S2,S11]
  --only IDS      run only these checks (comma-separated IDs from the report)
  --skip-app      skip the scenarios that start the app (target/debug/keyclean.exe)
  --session-lock  also lock the workstation (you'll have to sign back in); runs last
  --stall         also stall the hook past Windows' timeout (S20; input lags up to 1 s once)";

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
    println!("KeyClean end-to-end checks (M1-M3)");
    println!("This locks your keyboard several times over about 4 minutes (6 minutes at most).");
    println!("  - Hands off the keyboard until it finishes. The mouse is never locked.");
    println!("  - Click into an empty Notepad window now (anything that leaks lands there).");
    println!("  - The real Ctrl+Alt+K always unlocks. Ctrl+C now to cancel.");
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
    if options.session_lock {
        scenarios.push(scenarios::session_lock_scenario());
    }

    let run_s14 = !options.skip_app
        && options
            .only
            .as_ref()
            .is_none_or(|only| only.iter().any(|id| id == "S14"));
    if let Some(only) = &options.only {
        for id in only {
            if id != "S14" && !scenarios.iter().any(|s| s.id == id.as_str()) {
                let hint = if id == "S13" {
                    " (S13 also needs --session-lock)"
                } else if id == "S20" {
                    " (S20 also needs --stall)"
                } else if options.skip_app
                    && ["S10", "S11", "S12", "S15", "S21", "S23"].contains(&id.as_str())
                {
                    " (app checks are off with --skip-app)"
                } else {
                    ""
                };
                eprintln!("warning: --only {id} matches no check{hint}");
            }
        }
        scenarios.retain(|s| only.iter().any(|id| id == s.id));
        if scenarios.is_empty() && !run_s14 {
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

    // S14 runs last and without the observer hook, which could mask the bug it looks for.
    if run_s14 {
        print!("S14 Ctrl+Alt+K works while the app window has focus ... ");
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
            "Ctrl+Alt+K works while the app window has focus",
            outcome,
        );
    }

    report.print(started.elapsed());
    if let Some(path) = report.write_next_to_exe(started.elapsed()) {
        println!("Report written to {}", path.display());
    }
    if report.failed() == 0 { 0 } else { EXIT_FAILED }
}
