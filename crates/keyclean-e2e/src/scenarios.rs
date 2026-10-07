//! The automated M1-M6 checks. Each maps to steps in docs/testing/manual/M1.md, or to
//! docs/testing/manual/M2.md / M3.md when its steps start with "M2" / "M3". The M5 checks (mouse
//! and touchpad lock, ADR 0013) are marked "M5", the M6 checks (full-screen overlay, ADR 0014)
//! "M6".

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError, mpsc};
use std::time::{Duration, Instant};

use keyclean_win::keyclean_core::countdown;
use keyclean_win::testkit::inject::{Stroke, vk};
use keyclean_win::testkit::mouse::{self, Button, MouseInput};
use keyclean_win::testkit::observer::{MouseKind, Observer};
use keyclean_win::testkit::system::Rect;
use keyclean_win::testkit::{self, held, system};
use keyclean_win::{
    EndReason, EngineClient, EngineEvent, EngineNotice, LockTargets, SessionState, SystemTransition,
};

use crate::harness::{
    CursorGuard, EngineRig, MOUSE_ONLY, PROBE_WINDOW, dev_request, expect_mouse_probes_blocked,
    expect_mouse_probes_pass, expect_no_held_buttons, expect_no_raw_input_registration,
    expect_no_stuck_keys, expect_probes_blocked, expect_probes_pass, expect_reason, probe_passes,
    release_mouse_buttons, send, send_mouse, sleep, unusable_probes_note, wait_until,
};

/// Result of one scenario.
pub enum Outcome {
    Pass(String),
    Fail(String),
    Skip(String),
}

impl From<Result<String, String>> for Outcome {
    fn from(r: Result<String, String>) -> Self {
        match r {
            Ok(detail) => Outcome::Pass(detail),
            Err(detail) => Outcome::Fail(detail),
        }
    }
}

pub struct Ctx<'a> {
    pub observer: &'a Observer,
    pub rig: &'a mut EngineRig,
}

pub struct Scenario {
    pub id: &'static str,
    pub steps: &'static str,
    pub name: &'static str,
    pub run: fn(&mut Ctx<'_>) -> Outcome,
}

const CHORD: [u8; 3] = [vk::LCONTROL, vk::LMENU, vk::K];

pub fn engine_scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            id: "S1",
            steps: "1",
            name: "Keyboard list",
            run: keyboards,
        },
        Scenario {
            id: "S2",
            steps: "2, 10",
            name: "Lock blocks input, Ctrl+Alt+K unlocks",
            run: chord_unlock,
        },
        Scenario {
            id: "S3",
            steps: "3",
            name: "Chord held with auto-repeat types nothing",
            run: chord_held,
        },
        Scenario {
            id: "S4",
            steps: "4",
            name: "Timer releases on time",
            run: timer_release,
        },
        Scenario {
            id: "S5",
            steps: "9",
            name: "Keys held at lock start don't stick",
            run: held_at_start,
        },
        Scenario {
            id: "S6",
            steps: "5",
            name: "System shortcuts don't get past KeyClean",
            run: shortcuts,
        },
        Scenario {
            id: "S7",
            steps: "15",
            name: "AltGr+K counts as the chord",
            run: altgr,
        },
        Scenario {
            id: "S8",
            steps: "7",
            name: "Missing key-ups end the drain (DrainTimedOut)",
            run: missing_ups,
        },
        Scenario {
            id: "S16",
            steps: "M2 2",
            name: "Hard deadline releases a lock with no session timer",
            run: safety_timeout,
        },
        Scenario {
            id: "S17",
            steps: "M3 A",
            name: "A silently removed hook is detected and reported",
            run: hook_lost,
        },
        Scenario {
            id: "S18",
            steps: "M3 A",
            name: "A failed hook install locks nothing and says so",
            run: install_fails,
        },
        Scenario {
            id: "S26",
            steps: "M5",
            name: "Keyboard+mouse lock blocks every mouse event, Ctrl+Alt+K unlocks",
            run: mouse_chord_unlock,
        },
        Scenario {
            id: "S27",
            steps: "M5",
            name: "Mouse-only lock: keys pass, Ctrl+Alt+K unlocks without K reaching Windows",
            run: mouse_only_chord,
        },
        Scenario {
            id: "S28",
            steps: "M5",
            name: "Mouse-only lock: the timer releases on time",
            run: mouse_only_timer,
        },
        Scenario {
            id: "S29",
            steps: "M5",
            name: "Mouse-only lock: the hard deadline releases it while only the mouse moves",
            run: mouse_only_deadline,
        },
        Scenario {
            id: "S31",
            steps: "M5",
            name: "A silently removed mouse hook is detected and reported",
            run: mouse_hook_lost,
        },
        Scenario {
            id: "S32",
            steps: "M5",
            name: "A failed mouse hook install locks nothing and says so",
            run: mouse_install_fails,
        },
        // Last of the in-process checks: every session above must have removed its Raw Input
        // registration.
        Scenario {
            id: "S22",
            steps: "M3 A",
            name: "No Raw Input registration while idle",
            run: idle_registration,
        },
    ]
}

/// S20, opt-in (`--stall`): a real `LowLevelHooksTimeout`.
pub fn hook_timeout_scenario() -> Scenario {
    Scenario {
        id: "S20",
        steps: "M3 A",
        name: "A hook Windows times out is detected",
        run: hook_timeout,
    }
}

pub fn process_scenarios(skip_app: bool) -> Vec<Scenario> {
    let mut list = vec![
        Scenario {
            id: "S9",
            steps: "8",
            name: "Killing lock_smoke mid-lock releases input",
            run: kill_lock_smoke,
        },
        Scenario {
            id: "S30",
            steps: "M5",
            name: "Killing lock_smoke --mouse mid-lock releases the mouse",
            run: kill_lock_smoke_mouse,
        },
        Scenario {
            id: "S19",
            steps: "M3 A",
            name: "A hung engine thread is ended by the watchdog",
            run: engine_hang,
        },
    ];
    if !skip_app {
        list.push(Scenario {
            id: "S10",
            steps: "8",
            name: "Killing the app mid-lock releases input",
            run: kill_app,
        });
        list.push(Scenario {
            id: "S11",
            steps: "13",
            name: "Closing the app window mid-lock releases input",
            run: close_app,
        });
        list.push(Scenario {
            id: "S12",
            steps: "14",
            name: "A second app instance exits",
            run: second_instance,
        });
        list.push(Scenario {
            id: "S15",
            steps: "M2 1",
            name: "Countdown ticks through the engine process",
            run: countdown_ticks,
        });
        list.push(Scenario {
            id: "S21",
            steps: "M3 8",
            name: "Killing the engine process mid-lock: input back, engine restarted",
            run: kill_engine,
        });
        list.push(Scenario {
            id: "S23",
            steps: "M3 9",
            name: "taskkill without /F ends the app cleanly",
            run: taskkill_app,
        });
        list.extend([
            Scenario {
                id: "S34",
                steps: "M6",
                name: "The overlay covers the monitor before anything is blocked",
                run: overlay_before_lock,
            },
            Scenario {
                id: "S35",
                steps: "M6",
                name: "The overlay closes when the timer ends the lock",
                run: overlay_timer_end,
            },
            Scenario {
                id: "S36",
                steps: "M6",
                name: "An overlay that never confirms locks nothing",
                run: overlay_no_ack,
            },
            Scenario {
                id: "S37",
                steps: "M6",
                name: "Closing the overlay mid-lock releases input",
                run: overlay_closed,
            },
            Scenario {
                id: "S38",
                steps: "M6",
                name: "Hiding the overlay mid-lock releases input",
                run: overlay_hidden,
            },
            Scenario {
                id: "S39",
                steps: "M6",
                name: "Minimizing the overlay mid-lock releases input",
                run: overlay_minimized,
            },
            Scenario {
                id: "S40",
                steps: "M6",
                name: "A silent overlay (no heartbeat) ends the lock",
                run: overlay_silent,
            },
        ]);
    }
    list
}

/// S41, opt-in (`--overlay-latency`): how long the overlay takes to confirm, cold and warm.
pub fn overlay_latency_scenario() -> Scenario {
    Scenario {
        id: "S41",
        steps: "M6",
        name: "Overlay confirmation latency over 10 locks (cold and warm)",
        run: overlay_latency,
    }
}

/// S33, opt-in (`--mouse-diag`): what touchpad gestures do during a mouse-only lock.
pub fn mouse_diag_scenario() -> Scenario {
    Scenario {
        id: "S33",
        steps: "diagnostic",
        name: "Touchpad gestures during a mouse-only lock (diagnostic)",
        run: mouse_diagnostic,
    }
}

pub fn session_lock_scenario() -> Scenario {
    Scenario {
        id: "S13",
        steps: "6",
        name: "Workstation lock ends the session",
        run: session_lock,
    }
}

// ---------------------------------------------------------------------------------------------
// In-process engine scenarios
// ---------------------------------------------------------------------------------------------

fn keyboards(_ctx: &mut Ctx<'_>) -> Outcome {
    use keyclean_win::keyclean_core::devices::DeviceKind;
    match keyclean_win::input_devices() {
        Ok(list) => {
            let keyboards = list
                .iter()
                .filter(|d| d.kind == DeviceKind::Keyboard)
                .count();
            if keyboards == 0 {
                Outcome::Fail("no keyboards listed".into())
            } else {
                Outcome::Pass(format!(
                    "{keyboards} keyboard(s) among {} device(s) listed",
                    list.len()
                ))
            }
        }
        Err(e) => Outcome::Fail(e.details()),
    }
}

fn chord_unlock(ctx: &mut Ctx<'_>) -> Outcome {
    let mut run = || -> Result<String, String> {
        let since = ctx.rig.lock_and_wait(Duration::from_secs(10))?;
        expect_probes_blocked(ctx.observer)?;
        ctx.observer.reset();
        let pressed = Instant::now();
        send(&chord_strokes(&CHORD))?;
        let ended = ctx.rig.wait_ended(since, Duration::from_secs(3))?;
        expect_reason(&ended, EndReason::Emergency)?;
        let latency = pressed.elapsed();
        if latency > Duration::from_millis(500) {
            return Err(format!("unlock took {latency:?} (limit 500 ms)"));
        }
        if CHORD
            .iter()
            .any(|&k| ctx.observer.downs(k) > 0 || ctx.observer.ups(k) > 0)
        {
            return Err("chord keys reached Windows".into());
        }
        expect_probes_pass(ctx.observer)?;
        expect_no_stuck_keys()?;
        Ok(format!(
            "unlocked {} ms after the chord",
            latency.as_millis()
        ))
    };
    let result = run();
    ctx.rig.ensure_idle();
    result.into()
}

fn chord_held(ctx: &mut Ctx<'_>) -> Outcome {
    let mut run = || -> Result<String, String> {
        let since = ctx.rig.lock_and_wait(Duration::from_secs(10))?;
        ctx.observer.reset();
        send(&CHORD.map(Stroke::down))?;
        // Auto-repeat of K (~30 per second) for 3 s, as a held keyboard would produce.
        let until = Instant::now() + Duration::from_secs(3);
        while Instant::now() < until {
            send(&[Stroke::down(vk::K)])?;
            sleep(Duration::from_millis(33));
        }
        send(&[
            Stroke::up(vk::K),
            Stroke::up(vk::LMENU),
            Stroke::up(vk::LCONTROL),
        ])?;
        let ended = ctx.rig.wait_ended(since, Duration::from_secs(5))?;
        expect_reason(&ended, EndReason::Emergency)?;
        if ended.notices.contains(&EngineNotice::DrainTimedOut) {
            return Err("drain timed out while the keys were still repeating".into());
        }
        let leaked: u32 = CHORD
            .iter()
            .map(|&k| ctx.observer.downs(k) + ctx.observer.ups(k))
            .sum();
        if leaked > 0 {
            return Err(format!("{leaked} chord key event(s) reached Windows"));
        }
        expect_probes_pass(ctx.observer)?;
        expect_no_stuck_keys()?;
        Ok("~90 repeats over 3 s, none reached Windows".into())
    };
    let result = run();
    ctx.rig.ensure_idle();
    result.into()
}

fn timer_release(ctx: &mut Ctx<'_>) -> Outcome {
    let mut run = || -> Result<String, String> {
        let since = ctx.rig.lock_and_wait(Duration::from_secs(3))?;
        let ended = ctx.rig.wait_ended(since, Duration::from_secs(8))?;
        expect_reason(&ended, EndReason::Timeout)?;
        let secs = ended.after.as_secs_f64();
        if !(2.9..=3.8).contains(&secs) {
            return Err(format!("released after {secs:.2} s (expected about 3 s)"));
        }
        expect_probes_pass(ctx.observer)?;
        expect_no_stuck_keys()?;
        Ok(format!("3 s lock released after {secs:.2} s"))
    };
    let result = run();
    ctx.rig.ensure_idle();
    result.into()
}

/// S16: with the session timer suppressed, the hard deadline alone must end the lock. No input is
/// sent, so the watchdog (not the hook's per-event check) is the exit under test.
fn safety_timeout(ctx: &mut Ctx<'_>) -> Outcome {
    let mut run = || -> Result<String, String> {
        // Dev hard deadline for a 3 s lock: 3 s + 10 s grace = 13 s.
        let since = ctx.rig.lock_without_session_timer(Duration::from_secs(3))?;
        let ended = ctx.rig.wait_ended(since, Duration::from_secs(18))?;
        expect_reason(&ended, EndReason::HardDeadline)?;
        let secs = ended.after.as_secs_f64();
        if !(12.9..=14.0).contains(&secs) {
            return Err(format!("released after {secs:.2} s (expected about 13 s)"));
        }
        expect_probes_pass(ctx.observer)?;
        expect_no_stuck_keys()?;
        Ok(format!(
            "3 s lock without a session timer released by the hard deadline after {secs:.2} s"
        ))
    };
    let result = run();
    ctx.rig.ensure_idle();
    result.into()
}

fn held_at_start(ctx: &mut Ctx<'_>) -> Outcome {
    let mut run = || -> Result<String, String> {
        send(&[Stroke::down(vk::LSHIFT), Stroke::down(vk::LCONTROL)])?;
        if !wait_until(PROBE_WINDOW, || {
            held::is_held(vk::LSHIFT) && held::is_held(vk::LCONTROL)
        }) {
            return Err("Windows didn't register Shift+Ctrl as held before the lock".into());
        }
        let since = ctx.rig.lock_and_wait(Duration::from_secs(4))?;
        ctx.observer.reset();
        for _ in 0..5 {
            send(&[Stroke::down(vk::LSHIFT)])?; // auto-repeat while locked
            sleep(Duration::from_millis(33));
        }
        sleep(PROBE_WINDOW);
        if ctx.observer.downs(vk::LSHIFT) > 0 {
            return Err("repeats of a held key got through the lock".into());
        }
        send(&[Stroke::up(vk::LSHIFT), Stroke::up(vk::LCONTROL)])?;
        if !wait_until(PROBE_WINDOW, || {
            ctx.observer.ups(vk::LSHIFT) > 0 && ctx.observer.ups(vk::LCONTROL) > 0
        }) {
            return Err("releases of keys held at lock start were blocked".into());
        }
        let ended = ctx.rig.wait_ended(since, Duration::from_secs(8))?;
        expect_reason(&ended, EndReason::Timeout)?;
        expect_no_stuck_keys()?;
        Ok("repeats blocked, releases passed, nothing stuck".into())
    };
    let result = run();
    ctx.rig.ensure_idle();
    result.into()
}

/// System shortcuts S6 sends during a lock. Shortcuts that open something come last, in case one
/// gets through. Win+G isn't here: it is documented as unblockable (the Game Bar acts on it outside
/// the hook, even when synthesized), and the open overlay would spoil later checks (see S25).
const SHORTCUTS: [(&str, &[u8]); 6] = [
    ("Win", &[vk::LWIN]),
    ("Alt+Tab", &[vk::LMENU, vk::TAB]),
    ("Ctrl+Esc", &[vk::LCONTROL, vk::ESCAPE]),
    ("Win+X", &[vk::LWIN, vk::X]),
    ("Volume up", &[vk::VOLUME_UP]),
    ("Ctrl+Shift+Esc", &[vk::LCONTROL, vk::LSHIFT, vk::ESCAPE]),
];

/// Opens the Xbox Game Bar even during a lock. While its overlay is open, Windows delivers no Raw
/// Input to KeyClean, so the lost-hook check can't run (ADR 0010) [tested 2026-10-04, S25].
const GAME_BAR: (&str, &[u8]) = ("Win+G", &[vk::LWIN, vk::G]);

fn shortcuts(ctx: &mut Ctx<'_>) -> Outcome {
    let combos = SHORTCUTS;
    let mut run = || -> Result<String, String> {
        let since = ctx.rig.lock_and_wait(Duration::from_secs(12))?;
        let mut leaked = Vec::new();
        for (name, keys) in combos {
            ctx.observer.reset();
            send(&chord_strokes(keys))?;
            sleep(Duration::from_millis(250));
            if keys.iter().any(|&k| ctx.observer.downs(k) > 0) {
                leaked.push(name);
            }
        }
        send(&chord_strokes(&CHORD))?;
        let ended = ctx.rig.wait_ended(since, Duration::from_secs(3))?;
        expect_reason(&ended, EndReason::Emergency)?;
        // Probes must pass again, or the observer itself stopped working and saw nothing.
        expect_probes_pass(ctx.observer)?;
        expect_no_stuck_keys()?;
        if leaked.is_empty() {
            Ok(format!("{} shortcuts injected, all blocked", combos.len()))
        } else {
            Err(format!("got through: {}", leaked.join(", ")))
        }
    };
    let result = run();
    ctx.rig.ensure_idle();
    result.into()
}

fn altgr(ctx: &mut Ctx<'_>) -> Outcome {
    let mut run = || -> Result<String, String> {
        let since = ctx.rig.lock_and_wait(Duration::from_secs(10))?;
        // AltGr arrives as LCtrl + RAlt.
        send(&chord_strokes(&[vk::LCONTROL, vk::RMENU, vk::K]))?;
        let ended = ctx.rig.wait_ended(since, Duration::from_secs(3))?;
        expect_reason(&ended, EndReason::Emergency)?;
        expect_probes_pass(ctx.observer)?;
        expect_no_stuck_keys()?;
        Ok("LCtrl+RAlt+K ended the lock (layout-independent approximation)".into())
    };
    let result = run();
    ctx.rig.ensure_idle();
    result.into()
}

fn missing_ups(ctx: &mut Ctx<'_>) -> Outcome {
    let mut run = || -> Result<String, String> {
        let since = ctx.rig.lock_and_wait(Duration::from_secs(3))?;
        // Like Ctrl+Alt+Del: the presses reach the hook, the releases never do.
        send(&[Stroke::down(vk::LCONTROL), Stroke::down(vk::LMENU)])?;
        let ended = ctx.rig.wait_ended(since, Duration::from_secs(9))?;
        expect_reason(&ended, EndReason::Timeout)?;
        if !ended.notices.contains(&EngineNotice::DrainTimedOut) {
            return Err("no DrainTimedOut notice".into());
        }
        // Clean up: these releases now go to Windows, which never saw the presses (harmless).
        send(&[Stroke::up(vk::LMENU), Stroke::up(vk::LCONTROL)])?;
        expect_probes_pass(ctx.observer)?;
        expect_no_stuck_keys()?;
        Ok(format!(
            "released {:.1} s after locking (3 s lock + 2 s drain)",
            ended.after.as_secs_f64()
        ))
    };
    let result = run();
    ctx.rig.ensure_idle();
    result.into()
}

fn session_lock(ctx: &mut Ctx<'_>) -> Outcome {
    let mut run = || -> Result<String, String> {
        let since = ctx.rig.lock_and_wait(Duration::from_secs(15))?;
        system::lock_workstation().map_err(|e| format!("LockWorkStation failed: {e}"))?;
        let ended = ctx.rig.wait_ended(since, Duration::from_secs(5))?;
        expect_reason(
            &ended,
            EndReason::SystemTransition(SystemTransition::SessionLock),
        )?;
        Ok("session lock ended the KeyClean lock".into())
    };
    let result = run();
    ctx.rig.ensure_idle();
    result.into()
}

fn chord_strokes(keys: &[u8]) -> Vec<Stroke> {
    keys.iter()
        .map(|&k| Stroke::down(k))
        .chain(keys.iter().rev().map(|&k| Stroke::up(k)))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Out-of-process scenarios (lock_smoke and the app)
// ---------------------------------------------------------------------------------------------

/// `target/<profile>/` — the directory the harness binary runs from.
fn target_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(PathBuf::from)
}

/// Kills the child when dropped, so a failed scenario never leaves a process behind.
struct Guard(Child);

impl Drop for Guard {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn wait_exit(child: &mut Child, timeout: Duration) -> bool {
    wait_until(timeout, || matches!(child.try_wait(), Ok(Some(_))))
}

/// Polls with probes until KeyClean (in another process) blocks three in a row, so one probe
/// delayed by a busy system can't pass for a lock.
fn wait_blocked(observer: &Observer, timeout: Duration) -> Result<(), String> {
    let until = Instant::now() + timeout;
    let mut blocked_in_a_row = 0;
    while Instant::now() < until {
        if crate::harness::probe_passes(observer, vk::F13)? {
            blocked_in_a_row = 0;
            sleep(Duration::from_millis(150));
        } else {
            blocked_in_a_row += 1;
            if blocked_in_a_row == 3 {
                return Ok(());
            }
        }
    }
    Err(format!("the lock didn't engage within {timeout:?}"))
}

fn kill_lock_smoke(ctx: &mut Ctx<'_>) -> Outcome {
    let Some(exe) = target_dir().map(|d| d.join("examples").join("lock_smoke.exe")) else {
        return Outcome::Skip("can't locate the target directory".into());
    };
    if !exe.exists() {
        return Outcome::Skip(
            "build it first: cargo build -p keyclean-win --example lock_smoke".into(),
        );
    }
    let run = || -> Result<String, String> {
        let child = Command::new(&exe)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("couldn't start lock_smoke: {e}"))?;
        let mut guard = Guard(child);
        let stdout = guard.0.stdout.take().ok_or("no stdout")?;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if line.starts_with("Keyboard locked") {
                    let _ = tx.send(());
                }
            }
        });
        rx.recv_timeout(Duration::from_secs(10))
            .map_err(|_| "lock_smoke didn't report a lock within 10 s".to_string())?;
        wait_blocked(ctx.observer, Duration::from_secs(2))?;
        guard.0.kill().map_err(|e| format!("kill failed: {e}"))?;
        let killed = Instant::now();
        if !wait_exit(&mut guard.0, Duration::from_secs(3)) {
            return Err("lock_smoke didn't exit after kill".into());
        }
        expect_probes_pass(ctx.observer)?;
        expect_no_stuck_keys()?;
        Ok(format!(
            "input back {} ms after the kill",
            killed.elapsed().as_millis()
        ))
    };
    let result = run();
    crate::harness::release_watched_keys();
    result.into()
}

/// How to build the app exe the app checks need: a debug build with the UI embedded. A plain
/// `cargo build -p keyclean` loads the UI from a dev server that isn't running, so the overlay
/// never confirms and nothing locks.
const APP_BUILD: &str = "bun run build && cargo build -p keyclean --features tauri/custom-protocol";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The app exe, or why the app checks are skipped. Checked once per run: about ten scenarios ask.
fn app_exe() -> Result<PathBuf, Outcome> {
    static CHECKED: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    CHECKED
        .get_or_init(check_app_exe)
        .clone()
        .map_err(Outcome::Skip)
}

fn check_app_exe() -> Result<PathBuf, String> {
    let exe = target_dir()
        .map(|d| d.join("keyclean.exe"))
        .ok_or("can't locate the target directory")?;
    if !exe.exists() {
        return Err(format!("build it first: {APP_BUILD}"));
    }
    // `cargo run -p keyclean-e2e` doesn't rebuild the app, so an old exe would test old code.
    if let Some(newer) = newer_app_source(&exe) {
        return Err(format!(
            "keyclean.exe is older than {}; rebuild it: {APP_BUILD}",
            newer.display()
        ));
    }
    // The built overlay script's name carries a content hash. An exe that embeds the current UI
    // contains that name (as the key of its embedded asset); one that loads the UI from the dev
    // server, or embeds an older build, doesn't.
    let script = overlay_script_name().ok_or(format!(
        "no dist/assets/overlay-*.js found; build the UI and the app: {APP_BUILD}"
    ))?;
    // Vite rewrites dist/ on every build, so a UI source newer than the script means dist/ (and
    // with it the exe) is older than the UI.
    if let Some(newer) = newer_than(&repo_root().join("dist/assets").join(&script), UI_SOURCES) {
        return Err(format!(
            "dist/ is older than {}; rebuild the UI and the app: {APP_BUILD}",
            newer.display()
        ));
    }
    let bytes = std::fs::read(&exe).map_err(|e| format!("couldn't read keyclean.exe: {e}"))?;
    // `str::contains` runs std's optimized search, even in this debug build.
    if !String::from_utf8_lossy(&bytes).contains(&script) {
        return Err(format!(
            "keyclean.exe doesn't embed the current UI ({script}); rebuild it: {APP_BUILD}"
        ));
    }
    Ok(exe)
}

/// The file name of the built overlay script, `overlay-<hash>.js`, if the UI is built.
fn overlay_script_name() -> Option<String> {
    std::fs::read_dir(repo_root().join("dist/assets"))
        .ok()?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .find(|name| name.starts_with("overlay-") && name.ends_with(".js"))
}

/// The sources of the UI the exe embeds (built into dist/), relative to the repository root.
const UI_SOURCES: &[&str] = &[
    "src",
    "locales",
    "index.html",
    "overlay.html",
    "vite.config.ts",
];

/// A source file the app binary is built from that changed after `exe` was built, if any.
fn newer_app_source(exe: &Path) -> Option<PathBuf> {
    let rust = [
        "crates/keyclean-core/src",
        "crates/keyclean-win/src",
        "src-tauri/src",
        "src-tauri/capabilities",
        "src-tauri/build.rs",
        "src-tauri/tauri.conf.json",
    ];
    newer_than(exe, &rust).or_else(|| newer_than(exe, UI_SOURCES))
}

/// A file under `rels` (files or directories, relative to the repository root) that changed
/// after `built` was written, if any.
fn newer_than(built: &Path, rels: &[&str]) -> Option<PathBuf> {
    let built = std::fs::metadata(built).and_then(|m| m.modified()).ok()?;
    let root = repo_root();
    let mut pending: Vec<PathBuf> = rels.iter().map(|rel| root.join(rel)).collect();
    while let Some(path) = pending.pop() {
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&path) {
                pending.extend(entries.filter_map(|e| e.ok().map(|e| e.path())));
            }
        } else if meta.modified().is_ok_and(|changed| changed > built) {
            return Some(path);
        }
    }
    None
}

/// Starts the app with its stderr (session events only, never keys) saved next to the harness as
/// `e2e-<id>-app.log`, for diagnosing failures.
fn start_app(exe: &PathBuf, autolock_secs: Option<u64>, log_id: &str) -> Result<Guard, String> {
    start_app_with(exe, autolock_secs, &[], log_id)
}

/// [`start_app`] with extra environment variables, such as the overlay faults.
fn start_app_with(
    exe: &PathBuf,
    autolock_secs: Option<u64>,
    env: &[(&str, &str)],
    log_id: &str,
) -> Result<Guard, String> {
    let mut cmd = Command::new(exe);
    // Only what this scenario asks for, never a leftover from the user's shell.
    for name in [
        "KEYCLEAN_E2E_AUTOLOCK",
        "KEYCLEAN_E2E_AUTOLOCK_REPEAT",
        "KEYCLEAN_E2E_OVERLAY",
    ] {
        cmd.env_remove(name);
    }
    cmd.envs(env.iter().copied());
    cmd.stdout(Stdio::null());
    let log = target_dir()
        .map(|d| d.join(format!("e2e-{log_id}-app.log")))
        .and_then(|path| std::fs::File::create(path).ok());
    match log {
        Some(file) => cmd.stderr(Stdio::from(file)),
        None => cmd.stderr(Stdio::null()),
    };
    if let Some(secs) = autolock_secs {
        cmd.env("KEYCLEAN_E2E_AUTOLOCK", secs.to_string());
    }
    cmd.spawn()
        .map(Guard)
        .map_err(|e| format!("couldn't start the app: {e}"))
}

/// Window class of the app's own windows (set by Tauri). Only these get the close request, like
/// a user clicking X; the hidden helper windows of tao and the single-instance plugin don't.
const APP_WINDOW_CLASS: &str = "Tauri Window";

/// A killed app's WebView2 helper processes take a moment to exit and release its data folder;
/// starting the next instance too early can stall that instance's UI thread.
const WEBVIEW_SETTLE: Duration = Duration::from_secs(4);

/// Waits until every app window answers messages; returns how long that took.
fn wait_responsive(pid: u32, timeout: Duration) -> Option<Duration> {
    let started = Instant::now();
    wait_until(timeout, || {
        let windows = system::windows_of(pid);
        !windows.is_empty() && windows.iter().all(|w| w.responding)
    })
    .then(|| started.elapsed())
}

/// Polls with probes until input gets through; returns how long that took.
fn wait_input_back(observer: &Observer, timeout: Duration) -> Result<Option<Duration>, String> {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if crate::harness::probe_passes(observer, vk::F13)? {
            return Ok(Some(started.elapsed()));
        }
    }
    Ok(None)
}

fn describe(d: Option<Duration>, missing: &str) -> String {
    d.map_or(missing.to_string(), |d| {
        format!("after {} ms", d.as_millis())
    })
}

/// How long the engine process (ADR 0009) may outlive the app.
const ENGINE_EXIT_WAIT: Duration = Duration::from_secs(5);

/// Counts running `keyclean.exe` processes (the app and its engine process). Uses `tasklist` in
/// CSV form and matches the image name, not its localized "no tasks" message.
fn keyclean_processes() -> Result<usize, String> {
    let out = Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq keyclean.exe", "/FO", "CSV", "/NH"])
        .output()
        .map_err(|e| format!("couldn't run tasklist: {e}"))?;
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.to_ascii_lowercase().starts_with("\"keyclean.exe\""))
        .count())
}

/// Waits until no `keyclean.exe` is left; returns how long that took.
fn expect_no_keyclean_left() -> Result<Duration, String> {
    let started = Instant::now();
    let mut last = Ok(0);
    if wait_until(ENGINE_EXIT_WAIT, || {
        last = keyclean_processes();
        matches!(last, Ok(0))
    }) {
        return Ok(started.elapsed());
    }
    let count = last?;
    Err(format!(
        "{count} keyclean.exe process(es) still running {} s after the app exited; the engine \
         process must exit with the app (a KeyClean started outside the harness also counts)",
        ENGINE_EXIT_WAIT.as_secs()
    ))
}

fn kill_app(ctx: &mut Ctx<'_>) -> Outcome {
    let exe = match app_exe() {
        Ok(exe) => exe,
        Err(skip) => return skip,
    };
    let run = || -> Result<String, String> {
        let mut app = start_app(&exe, Some(15), "S10")?;
        wait_app_locked(ctx.observer, "S10", Duration::from_secs(20))?;
        app.0.kill().map_err(|e| format!("kill failed: {e}"))?;
        if !wait_exit(&mut app.0, Duration::from_secs(5)) {
            return Err("the app didn't exit after kill".into());
        }
        // The hook lives in the engine process, which exits once it sees the app is gone.
        let engine_gone = expect_no_keyclean_left()?;
        expect_probes_pass(ctx.observer)?;
        expect_no_stuck_keys()?;
        Ok(format!(
            "engine process gone {} ms after the app; input back right after",
            engine_gone.as_millis()
        ))
    };
    let result = run();
    crate::harness::release_watched_keys();
    sleep(WEBVIEW_SETTLE);
    result.into()
}

/// How far a countdown tick may be from the second boundary it marks.
const TICK_TOLERANCE: Duration = Duration::from_millis(150);

/// How long S15 listens after the lock for stray countdown statuses.
const IDLE_QUIET: Duration = Duration::from_secs(2);

/// S15: a 5 s lock run by the real engine process (app exe in `--engine` mode, over the pipe)
/// reports 5, 4, 3, 2, 1 once each, on whole-second boundaries, then ends on its timer and sends
/// nothing more once idle.
fn countdown_ticks(ctx: &mut Ctx<'_>) -> Outcome {
    let exe = match app_exe() {
        Ok(exe) => exe,
        Err(skip) => return skip,
    };
    let run = || -> Result<String, String> {
        let (client, events) = EngineClient::start_from(&exe).map_err(|e| e.details())?;
        let lock = Duration::from_secs(5);
        let requested = Instant::now();
        client.lock(dev_request(lock)).map_err(|e| e.details())?;

        let until = requested + Duration::from_secs(12);
        // (displayed second, when it arrived)
        let mut ticks: Vec<(u64, Instant)> = Vec::new();
        // When the first locked status says the session ends.
        let mut deadline: Option<Instant> = None;
        let ended = loop {
            let left = until.saturating_duration_since(Instant::now());
            match events.recv_timeout(left) {
                Ok(EngineEvent::Status(status)) if status.state == SessionState::Locked => {
                    let now = Instant::now();
                    if let Some(remaining) = status.session_remaining {
                        deadline.get_or_insert(now + remaining);
                        ticks.push((countdown::display_secs(remaining), now));
                    }
                }
                Ok(EngineEvent::SessionEnded { reason }) => break (reason, requested.elapsed()),
                Ok(EngineEvent::Error(e)) => return Err(format!("engine error: {}", e.details())),
                Ok(_) => {}
                Err(_) => return Err("the lock didn't end within 12 s".into()),
            }
        };
        // After the end: one Idle status, then silence (no countdown ticks while idle).
        let quiet_until = Instant::now() + IDLE_QUIET;
        let mut idle_statuses = 0;
        loop {
            let left = quiet_until.saturating_duration_since(Instant::now());
            match events.recv_timeout(left) {
                Ok(EngineEvent::Status(status)) => {
                    idle_statuses += 1;
                    if status.state != SessionState::Idle || idle_statuses > 1 {
                        return Err(format!(
                            "status {:?} arrived after the lock ended; nothing is expected after \
                             the Idle status",
                            status.state
                        ));
                    }
                }
                Ok(EngineEvent::Error(e)) => return Err(format!("engine error: {}", e.details())),
                Ok(_) => {}
                Err(_) => break,
            }
        }
        if idle_statuses == 0 {
            return Err("no Idle status after the lock ended".into());
        }
        drop(client); // Stops the engine process.

        let (reason, after) = ended;
        if reason != EndReason::Timeout {
            return Err(format!("ended with {reason:?}, expected Timeout"));
        }
        let secs = after.as_secs_f64();
        if !(5.0..=5.4).contains(&secs) {
            return Err(format!("released after {secs:.2} s (expected about 5 s)"));
        }
        // The countdown timer may report 0 just before the session timer ends the lock.
        if ticks.last().is_some_and(|(shown, _)| *shown == 0) {
            ticks.pop();
        }
        let shown: Vec<u64> = ticks.iter().map(|(shown, _)| *shown).collect();
        if shown != [5, 4, 3, 2, 1] {
            return Err(format!(
                "countdown showed {shown:?}, expected [5, 4, 3, 2, 1]"
            ));
        }
        let deadline = deadline.ok_or("no locked status arrived")?;
        let mut worst = Duration::ZERO;
        for (shown, at) in ticks.iter().skip(1) {
            let expected = deadline - Duration::from_secs(*shown);
            let off = if *at > expected {
                *at - expected
            } else {
                expected - *at
            };
            if off > TICK_TOLERANCE {
                return Err(format!(
                    "{shown} arrived {} ms off its second boundary (tolerance {} ms)",
                    off.as_millis(),
                    TICK_TOLERANCE.as_millis()
                ));
            }
            worst = worst.max(off);
        }
        expect_probes_pass(ctx.observer)?;
        expect_no_stuck_keys()?;
        Ok(format!(
            "showed 5..1 on time (worst {} ms off), released after {secs:.2} s",
            worst.as_millis()
        ))
    };
    let result = run();
    crate::harness::release_watched_keys();
    result.into()
}

fn close_app(ctx: &mut Ctx<'_>) -> Outcome {
    let exe = match app_exe() {
        Ok(exe) => exe,
        Err(skip) => return skip,
    };
    let run = || -> Result<String, String> {
        let mut app = start_app(&exe, Some(15), "S11")?;
        let pid = app.0.id();
        wait_app_locked(ctx.observer, "S11", Duration::from_secs(20))?;
        let responsive = wait_responsive(pid, Duration::from_secs(8));
        let classes: Vec<String> = system::windows_of(pid)
            .into_iter()
            .map(|w| {
                let state = if w.responding {
                    ""
                } else {
                    " (not responding)"
                };
                format!("{}{state}", w.class)
            })
            .collect();
        // The lock must still be engaged right before the close, so only the close can release it.
        expect_probes_blocked(ctx.observer).map_err(|_| {
            "the autolock ended before the close was sent (app too slow)".to_string()
        })?;
        // Only the main window: closing the overlay would end the lock by itself (S37), and this
        // check is about the main window.
        let main = system::main_window(pid, APP_WINDOW_CLASS).ok_or("found no main window")?;
        if !system::request_close_window(main) {
            return Err("couldn't send the close to the main window".into());
        }
        let closed = Instant::now();
        let input_back = wait_input_back(ctx.observer, Duration::from_secs(5))?;
        let exited = wait_exit(&mut app.0, Duration::from_secs(20)).then(|| closed.elapsed());

        let detail = format!(
            "windows responding: {}; app windows [{}], close sent to the main window (not the \
             overlay); input back: {}; process exit: {}; log: e2e-S11-app.log",
            describe(responsive, "no (waited 8 s)"),
            classes.join(", "),
            describe(input_back, "NO (waited 5 s)"),
            describe(exited, "still running after 20 s"),
        );
        if input_back.is_none() || exited.is_none() {
            return Err(detail);
        }
        let engine_gone = expect_no_keyclean_left().map_err(|e| format!("{detail}; {e}"))?;
        let detail = format!(
            "{detail}; no keyclean.exe left {} ms after exit",
            engine_gone.as_millis()
        );
        // The app logs why the session ended; the close must be what ended it, not the timer.
        let log = target_dir()
            .map(|d| d.join("e2e-S11-app.log"))
            .and_then(|path| std::fs::read_to_string(path).ok())
            .unwrap_or_default();
        if !log.contains("session ended: UserRequest") {
            return Err(format!(
                "{detail}; the app log doesn't show the close ending the session"
            ));
        }
        expect_no_stuck_keys()?;
        Ok(format!("{detail}; {}", unlock_path_note(&log)))
    };
    let result = run();
    crate::harness::release_watched_keys();
    sleep(WEBVIEW_SETTLE);
    result.into()
}

fn second_instance(_ctx: &mut Ctx<'_>) -> Outcome {
    let exe = match app_exe() {
        Ok(exe) => exe,
        Err(skip) => return skip,
    };
    let run = || -> Result<String, String> {
        let mut first = start_app(&exe, None, "S12-first")?;
        let pid = first.0.id();
        if wait_responsive(pid, Duration::from_secs(20)).is_none() {
            return Err("the first instance didn't show a responding window".into());
        }
        let mut second = start_app(&exe, None, "S12-second")?;
        if !wait_exit(&mut second.0, Duration::from_secs(10)) {
            return Err("the second instance kept running".into());
        }
        if !matches!(first.0.try_wait(), Ok(None)) {
            return Err("the first instance exited".into());
        }
        system::request_close(pid, APP_WINDOW_CLASS);
        if !wait_exit(&mut first.0, Duration::from_secs(10)) {
            return Err(
                "second instance exited, but the first didn't exit after its window closed".into(),
            );
        }
        Ok("second instance exited; first kept running, then closed cleanly".into())
    };
    let result = run();
    sleep(WEBVIEW_SETTLE);
    result.into()
}

/// S14: Ctrl+Alt+K must end a lock while the app's overlay (a WebView2 window of the app) has
/// focus.
///
/// Windows stops calling a low-level hook while a WebView2 window of the hook's own process has
/// focus (Max's manual tests F1/F3, X1). Runs without the observer hook, because a second hook was
/// reported to mask the problem; the app log tells how the session ended.
pub fn focused_app_chord() -> Outcome {
    let exe = match app_exe() {
        Ok(exe) => exe,
        Err(skip) => return skip,
    };
    let run = || -> Result<String, String> {
        let mut app = start_app(&exe, Some(15), "S14")?;
        let pid = app.0.id();
        let log_path = target_dir().map(|d| d.join("e2e-S14-app.log"));
        let read_log = || {
            log_path
                .as_ref()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .unwrap_or_default()
        };
        let ended_line = |log: &str| {
            log.lines()
                .find(|l| l.contains("session ended:"))
                .map_or("no session end logged".to_string(), str::to_string)
        };
        let outcome = (|| {
            // The lock starts once the overlay confirms it is visible.
            if !wait_until(Duration::from_secs(20), || {
                let log = read_log();
                log.contains(CONFIRMED) || log.contains(NOTHING_LOCKED)
            }) || !read_log().contains(CONFIRMED)
            {
                return Err(format!(
                    "the overlay didn't confirm it was visible within 20 s ({}); log: \
                     e2e-S14-app.log",
                    overlay_log_note(&read_log())
                ));
            }
            let (overlay, rect) = wait_overlay(pid, Duration::from_secs(2))
                .ok_or("the overlay confirmed, but no overlay window was found")?;
            sleep(Duration::from_millis(500));
            // An empty spot in the middle of the overlay (its only button is at the top right).
            let (x, y) = ((rect.left + rect.right) / 2, (rect.top + rect.bottom) / 2);
            keyclean_win::testkit::inject::click(x, y).map_err(|e| format!("click failed: {e}"))?;
            sleep(Duration::from_millis(500));
            // Without these two, a pass wouldn't prove anything about the focused-window case.
            if system::foreground_hwnd() != Some(overlay) {
                return Err(format!(
                    "the overlay didn't have focus after the click, so the focus case wasn't \
                     tested ({})",
                    foreground_note()
                ));
            }
            let before = read_log();
            if before.contains("session ended:") {
                return Err(format!(
                    "the autolock ended before the chord was sent ({}); app too slow",
                    ended_line(&before)
                ));
            }
            let pressed = Instant::now();
            send(&chord_strokes(&CHORD))?;
            wait_until(Duration::from_secs(3), || {
                read_log().contains("session ended:")
            });
            let log = read_log();
            if log.contains("session ended: Emergency") {
                Ok(format!(
                    "Ctrl+Alt+K ended the lock {} ms after it was pressed, with the overlay \
                     focused",
                    pressed.elapsed().as_millis()
                ))
            } else {
                Err(format!(
                    "Ctrl+Alt+K didn't end the lock while the overlay had focus (the hook went \
                     deaf; within 3 s: {}); log: e2e-S14-app.log",
                    ended_line(&log)
                ))
            }
        })();
        system::request_close(pid, APP_WINDOW_CLASS);
        let _ = wait_exit(&mut app.0, Duration::from_secs(20));
        outcome
    };
    let result = run();
    crate::harness::release_watched_keys();
    sleep(WEBVIEW_SETTLE);
    result.into()
}

// ---------------------------------------------------------------------------------------------
// M3: safety hardening
// ---------------------------------------------------------------------------------------------

/// How soon the hook-liveness check must end a lock after input gets past a lost hook: the check
/// runs 250 ms after the first `WM_INPUT`, plus margin.
const HOOK_LOST_LIMIT: Duration = Duration::from_secs(1);

/// The focused window's class and process image name, for failure messages.
fn foreground_note() -> String {
    let Some((class, pid)) = system::foreground_window() else {
        return "no foreground window".into();
    };
    let image = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .output()
        .ok()
        .and_then(|out| {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()
                .and_then(|l| l.split(',').next())
                .map(|name| name.trim_matches('"').to_string())
        })
        .unwrap_or_else(|| "?".into());
    format!("foreground: {class} in {image} (pid {pid})")
}

/// What the engine's Raw Input sink saw, for failure messages: "no WM_INPUT" means the detector
/// had nothing to compare (e.g. injected keys don't produce raw input), not that it ignored it.
fn raw_input_note(before: u64) -> String {
    let seen = testkit::raw_input_seen().saturating_sub(before);
    if seen == 0 {
        "the engine saw no WM_INPUT at all".into()
    } else {
        format!("the engine saw {seen} WM_INPUT message(s)")
    }
}

/// S17: the engine's hook disappears without the engine knowing (as when Windows removes a hook
/// that timed out). The next keystroke gets through; the liveness check must notice, end the lock
/// with `error.hook_lost` and leave nothing registered.
fn hook_lost(ctx: &mut Ctx<'_>) -> Outcome {
    let mut run = || -> Result<String, String> {
        let raw_before = testkit::raw_input_seen();
        ctx.rig.lock_and_wait(Duration::from_secs(10))?;
        let registered = system::registered_raw_input_count();
        expect_probes_blocked(ctx.observer)?;
        let raw_while_blocking = testkit::raw_input_seen().saturating_sub(raw_before);
        ctx.rig
            .engine()
            .testkit_drop_hook()
            .map_err(|e| e.details())?;
        sleep(Duration::from_millis(100));
        let typed = Instant::now();
        if !probe_passes(ctx.observer, vk::F13)? {
            return Err("a probe was still blocked after the hook was removed".into());
        }
        // Diagnostics for a failure: was Raw Input registered, did blocked keys produce any, and
        // what had focus when the key got through?
        let focus = foreground_note();
        let diag = || {
            format!(
                "registrations during the lock: {registered:?}; WM_INPUT while the hook blocked:                  {raw_while_blocking}; {}; {focus}",
                raw_input_note(raw_before)
            )
        };
        let ended = ctx
            .rig
            .wait_ended_with_errors(typed, Duration::from_secs(3))
            .map_err(|e| format!("{e}; {}", diag()))?;
        expect_reason(&ended, EndReason::EngineError)?;
        if !ended.errors.iter().any(|k| k == "error.hook_lost") {
            return Err(format!(
                "ended without error.hook_lost (errors: {:?})",
                ended.errors
            ));
        }
        if ended.after > HOOK_LOST_LIMIT {
            return Err(format!(
                "detected only {} ms after the leaked key (limit {} ms)",
                ended.after.as_millis(),
                HOOK_LOST_LIMIT.as_millis()
            ));
        }
        expect_probes_pass(ctx.observer)?;
        expect_no_stuck_keys()?;
        expect_no_raw_input_registration()?;
        Ok(format!(
            "lock ended {} ms after the first leaked key; {}; notices {:?}",
            ended.after.as_millis(),
            raw_input_note(raw_before),
            ended.notices
        ))
    };
    let result = run();
    ctx.rig.ensure_idle();
    result.into()
}

/// S18: Windows refuses the hook (forced by the test kit). Nothing may be locked, the error must
/// say so, and the engine must be ready again.
fn install_fails(ctx: &mut Ctx<'_>) -> Outcome {
    let mut run = || -> Result<String, String> {
        ctx.rig.engine().testkit_fail_next_install();
        let since = ctx.rig.lock(Duration::from_secs(3))?;
        let ended = ctx
            .rig
            .wait_ended_with_errors(since, Duration::from_secs(3))?;
        expect_reason(&ended, EndReason::EngineError)?;
        if !ended.errors.iter().any(|k| k == "error.hook_install") {
            return Err(format!(
                "ended without error.hook_install (errors: {:?})",
                ended.errors
            ));
        }
        expect_probes_pass(ctx.observer)?;
        expect_no_raw_input_registration()?;
        // The engine must still lock normally afterwards.
        ctx.rig.lock_and_wait(Duration::from_secs(2))?;
        expect_probes_blocked(ctx.observer)?;
        Ok(format!(
            "ended after {} ms with error.hook_install; nothing blocked; next lock works",
            ended.after.as_millis()
        ))
    };
    let result = run();
    ctx.rig.ensure_idle();
    result.into()
}

/// S22: the engine registers Raw Input only during a session (invariant 4). The device watch the
/// app runs at idle (ADR 0012) is running too: it must not count as an input registration. A short
/// mouse-only lock runs first, so the mouse registration (ADR 0013) must be gone as well.
fn idle_registration(ctx: &mut Ctx<'_>) -> Outcome {
    ctx.rig.ensure_idle();
    let mut mouse_lock = || -> Result<usize, String> {
        let since = ctx
            .rig
            .lock_with_and_wait(Duration::from_secs(2), MOUSE_ONLY)?;
        let during = system::registered_raw_input_count();
        let ended = ctx.rig.wait_ended(since, Duration::from_secs(6))?;
        expect_reason(&ended, EndReason::Timeout)?;
        match during {
            // Keyboards (for the chord's liveness check) and mice.
            Some(2) => Ok(2),
            other => Err(format!(
                "a mouse-only lock registered {other:?} Raw Input device class(es), expected \
                 exactly 2"
            )),
        }
    };
    let during = mouse_lock();
    ctx.rig.ensure_idle();
    let during = match during {
        Ok(n) => n,
        Err(e) => return Outcome::Fail(e),
    };
    if let Err(e) = expect_no_raw_input_registration() {
        return Outcome::Fail(format!("after a mouse-only lock: {e}"));
    }
    let (changed, _unused) = std::sync::mpsc::channel();
    let watch = match keyclean_win::DeviceWatch::start(changed) {
        Ok(watch) => watch,
        Err(e) => return Outcome::Fail(format!("device watch didn't start: {}", e.details())),
    };
    let result = expect_no_raw_input_registration();
    drop(watch);
    result
        .map(|()| {
            format!(
                "GetRegisteredRawInputDevices reports none after every session (a mouse-only lock \
                 had {during}) and with the device watch running"
            )
        })
        .into()
}

/// S20 (opt-in): one hook callback sleeps for 1.5 s, past `LowLevelHooksTimeout` (1 s at most).
/// Windows then passes that key on without the hook's verdict, so the liveness check must end the
/// lock with `error.hook_lost`. Whether Windows also removed the hook shows in the engine's
/// `HookAlreadyRemoved` notice (its own unhook then fails).
fn hook_timeout(ctx: &mut Ctx<'_>) -> Outcome {
    let mut run = || -> Result<String, String> {
        let raw_before = testkit::raw_input_seen();
        ctx.rig.lock_and_wait(Duration::from_secs(12))?;
        expect_probes_blocked(ctx.observer)?;
        testkit::stall_next_callback(1500);
        ctx.observer.reset();
        let typed = Instant::now();
        send(&[Stroke::down(vk::F13), Stroke::up(vk::F13)])?;
        let leaked = wait_until(Duration::from_secs(3), || ctx.observer.downs(vk::F13) > 0);
        let ended = ctx
            .rig
            .wait_ended_with_errors(typed, Duration::from_secs(5))
            .map_err(|e| {
                format!(
                    "{e}; stalled key leaked: {leaked}; {}",
                    raw_input_note(raw_before)
                )
            })?;
        expect_reason(&ended, EndReason::EngineError)?;
        if !ended.errors.iter().any(|k| k == "error.hook_lost") {
            return Err(format!(
                "ended without error.hook_lost (errors: {:?})",
                ended.errors
            ));
        }
        expect_probes_pass(ctx.observer)?;
        expect_no_stuck_keys()?;
        let removed = ended.notices.contains(&EngineNotice::HookAlreadyRemoved);
        Ok(format!(
            "stalled key leaked: {leaked}; lock ended {} ms after it with error.hook_lost;              Windows removed the hook: {removed}; {}",
            ended.after.as_millis(),
            raw_input_note(raw_before)
        ))
    };
    let result = run();
    ctx.rig.ensure_idle();
    result.into()
}

/// Hidden argument: run [`child_hang`] instead of the harness.
pub const CHILD_HANG_FLAG: &str = "--child-hang";

/// Child process for S19: locks 3 s with an in-process engine, then hangs the engine thread. The
/// watchdog must end this process at the hard deadline (13 s) plus its 1 s grace. Exit code 4
/// means it didn't.
pub fn child_hang() -> i32 {
    let Ok(mut rig) = EngineRig::start() else {
        println!("engine failed to start");
        return 3;
    };
    if let Err(e) = rig.lock_and_wait(Duration::from_secs(3)) {
        println!("lock failed: {e}");
        return 3;
    }
    if rig.engine().testkit_hang(Duration::from_secs(60)).is_err() {
        println!("hang request failed");
        return 3;
    }
    println!("locked and hung");
    sleep(Duration::from_secs(30));
    println!("still alive");
    4
}

/// S19: the engine thread hangs mid-lock (in a child process, because the watchdog aborts the
/// process). Input must come back by the hard deadline plus the watchdog's 1 s grace.
fn engine_hang(ctx: &mut Ctx<'_>) -> Outcome {
    let Ok(exe) = std::env::current_exe() else {
        return Outcome::Skip("can't locate the harness executable".into());
    };
    let run = || -> Result<String, String> {
        let child = Command::new(&exe)
            .arg(CHILD_HANG_FLAG)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("couldn't start the child: {e}"))?;
        let mut guard = Guard(child);
        let stdout = guard.0.stdout.take().ok_or("no stdout")?;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        });
        let first = rx
            .recv_timeout(Duration::from_secs(10))
            .map_err(|_| "the child didn't report a lock within 10 s".to_string())?;
        if first != "locked and hung" {
            return Err(format!("child: {first}"));
        }
        let hung = Instant::now();
        // No input is sent while hung: the stalled hook would hold each key up to 1 s.
        if !wait_exit(&mut guard.0, Duration::from_secs(20)) {
            return Err("the child was still running 20 s after the hang".into());
        }
        let after = hung.elapsed().as_secs_f64();
        let code = guard.0.try_wait().ok().flatten().and_then(|s| s.code());
        if code == Some(4) {
            return Err("the watchdog didn't end the hung child".into());
        }
        let expected = i32::try_from(testkit::WATCHDOG_EXIT_CODE).ok();
        if code != expected {
            return Err(format!(
                "the child ended {after:.2} s after the hang with exit code {code:?}, not the                  watchdog's {expected:?}"
            ));
        }
        if !(11.5..=15.5).contains(&after) {
            return Err(format!(
                "the child ended {after:.2} s after the hang (expected about 14 s: 13 s hard \
                 deadline + 1 s grace); exit code {code:?}"
            ));
        }
        expect_probes_pass(ctx.observer)?;
        expect_no_stuck_keys()?;
        Ok(format!(
            "watchdog ended the hung engine's process {after:.2} s after the hang (exit code \
             {code:?}); input back"
        ))
    };
    let result = run();
    crate::harness::release_watched_keys();
    result.into()
}

/// PIDs of running `keyclean.exe` processes (the app and its engine process).
fn keyclean_pids() -> Result<Vec<u32>, String> {
    let out = Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq keyclean.exe", "/FO", "CSV", "/NH"])
        .output()
        .map_err(|e| format!("couldn't run tasklist: {e}"))?;
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.to_ascii_lowercase().starts_with("\"keyclean.exe\""))
        .filter_map(|l| l.split(',').nth(1)?.trim_matches('"').parse().ok())
        .collect())
}

fn app_log(id: &str) -> String {
    target_dir()
        .map(|d| d.join(format!("e2e-{id}-app.log")))
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_default()
}

/// S21: the engine process is killed mid-lock. Input must come back at once, and the app must
/// stay up and start a new, idle engine.
fn kill_engine(ctx: &mut Ctx<'_>) -> Outcome {
    let exe = match app_exe() {
        Ok(exe) => exe,
        Err(skip) => return skip,
    };
    let run = || -> Result<String, String> {
        let mut app = start_app(&exe, Some(15), "S21")?;
        let app_pid = app.0.id();
        wait_app_locked(ctx.observer, "S21", Duration::from_secs(20))?;
        let (overlay, _) = system::overlay_window(app_pid, APP_WINDOW_CLASS)
            .ok_or("the lock engaged, but no overlay window was found")?;
        let engine_pid = keyclean_pids()?
            .into_iter()
            .find(|&pid| pid != app_pid)
            .ok_or("found no engine process")?;
        Command::new("taskkill")
            .args(["/F", "/PID", &engine_pid.to_string()])
            .output()
            .map_err(|e| format!("couldn't run taskkill: {e}"))?;
        let input_back = wait_input_back(ctx.observer, Duration::from_secs(3))?
            .ok_or("input wasn't back within 3 s of killing the engine")?;
        // The app closes the overlay when its engine dies.
        if !wait_until(Duration::from_secs(2), || overlay_gone(app_pid, overlay)) {
            return Err(format!(
                "input back {} ms after the kill, but the overlay was still open 2 s later",
                input_back.as_millis()
            ));
        }
        if !wait_until(Duration::from_secs(8), || {
            app_log("S21").contains("engine restarted")
        }) {
            return Err(format!(
                "input back {} ms after the kill, but the app log shows no engine restart \
                 (log: e2e-S21-app.log)",
                input_back.as_millis()
            ));
        }
        if !matches!(app.0.try_wait(), Ok(None)) {
            return Err("the app exited after its engine was killed".into());
        }
        // The new engine must come up idle: nothing may be blocked after the restart.
        expect_probes_pass(ctx.observer).map_err(|e| format!("after the engine restart: {e}"))?;
        let mut pids = Vec::new();
        if !wait_until(Duration::from_secs(3), || {
            pids = keyclean_pids().unwrap_or_default();
            pids.len() == 2 && !pids.contains(&engine_pid)
        }) {
            return Err(format!(
                "expected the app and one new engine process, found PIDs {pids:?}"
            ));
        }
        app.0.kill().map_err(|e| format!("kill failed: {e}"))?;
        let _ = wait_exit(&mut app.0, Duration::from_secs(5));
        expect_no_keyclean_left()?;
        expect_no_stuck_keys()?;
        Ok(format!(
            "input back {} ms after the kill; overlay closed; new engine process started; app \
             kept running",
            input_back.as_millis()
        ))
    };
    let result = run();
    crate::harness::release_watched_keys();
    sleep(WEBVIEW_SETTLE);
    result.into()
}

/// S23: `taskkill` without `/F` sends WM_CLOSE to the app's windows, including tao's and the
/// single-instance plugin's hidden helpers. The app must exit cleanly instead of hanging.
fn taskkill_app(ctx: &mut Ctx<'_>) -> Outcome {
    let exe = match app_exe() {
        Ok(exe) => exe,
        Err(skip) => return skip,
    };
    let run = || -> Result<String, String> {
        let mut app = start_app(&exe, Some(15), "S23")?;
        let pid = app.0.id();
        wait_app_locked(ctx.observer, "S23", Duration::from_secs(20))?;
        let _ = wait_responsive(pid, Duration::from_secs(8));
        let classes: Vec<String> = system::windows_of(pid)
            .into_iter()
            .map(|w| w.class)
            .collect();
        expect_probes_blocked(ctx.observer).map_err(|_| {
            "the autolock ended before taskkill was sent (app too slow)".to_string()
        })?;
        // Without /F: taskkill asks the processes to close (WM_CLOSE to their windows).
        Command::new("taskkill")
            .args(["/IM", "keyclean.exe"])
            .output()
            .map_err(|e| format!("couldn't run taskkill: {e}"))?;
        let sent = Instant::now();
        let input_back = wait_input_back(ctx.observer, Duration::from_secs(5))?;
        let exited = wait_exit(&mut app.0, Duration::from_secs(10)).then(|| sent.elapsed());
        let log = app_log("S23");
        let guard_line = log
            .lines()
            .find(|l| l.contains("close guard:"))
            .unwrap_or("no close guard line")
            .to_string();
        let detail = format!(
            "app windows [{}]; {guard_line}; input back: {}; process exit: {}; log: \
             e2e-S23-app.log",
            classes.join(", "),
            describe(input_back, "NO (waited 5 s)"),
            describe(exited, "still running after 10 s"),
        );
        if input_back.is_none() || exited.is_none() {
            return Err(detail);
        }
        expect_no_keyclean_left().map_err(|e| format!("{detail}; {e}"))?;
        if !log.contains("session ended: UserRequest") {
            return Err(format!(
                "{detail}; the app log doesn't show the exit ending the session"
            ));
        }
        expect_no_stuck_keys()?;
        Ok(format!("{detail}; {}", unlock_path_note(&log)))
    };
    let result = run();
    crate::harness::release_watched_keys();
    sleep(WEBVIEW_SETTLE);
    result.into()
}

/// S24: S17 again, after the observer hook is gone, so the engine's hook is the only one in this
/// process, as in the real engine process.
pub fn hook_lost_without_observer() -> Outcome {
    let run = || -> Result<String, String> {
        let mut rig = EngineRig::start()?;
        let raw_before = testkit::raw_input_seen();
        rig.lock_and_wait(Duration::from_secs(10))?;
        let registered = system::registered_raw_input_count();
        rig.engine().testkit_drop_hook().map_err(|e| e.details())?;
        sleep(Duration::from_millis(100));
        let typed = Instant::now();
        // F13 is harmless if it reaches the focused window.
        send(&[Stroke::down(vk::F13), Stroke::up(vk::F13)])?;
        let result = rig
            .wait_ended_with_errors(typed, Duration::from_secs(3))
            .map_err(|e| {
                format!(
                    "{e}; registrations during the lock: {registered:?}; {}",
                    raw_input_note(raw_before)
                )
            })
            .and_then(|ended| {
                expect_reason(&ended, EndReason::EngineError)?;
                if !ended.errors.iter().any(|k| k == "error.hook_lost") {
                    return Err(format!(
                        "ended without error.hook_lost (errors: {:?})",
                        ended.errors
                    ));
                }
                Ok(ended)
            });
        rig.ensure_idle();
        let ended = result?;
        expect_no_stuck_keys()?;
        expect_no_raw_input_registration()?;
        Ok(format!(
            "lock ended {} ms after the leaked key; {}",
            ended.after.as_millis(),
            raw_input_note(raw_before)
        ))
    };
    run().into()
}

/// S25 (opt-in diagnostic, `--raw-diag`): does the lost-hook check still work after each system
/// shortcut? For a baseline and then each shortcut alone, a fresh engine locks, receives the
/// shortcut, is unlocked with Ctrl+Alt+K, and then runs the S24 lost-hook check. Win+G runs last
/// and is expected to break it (the Game Bar overlay stays open; close it afterwards). Runs
/// without the observer hook, after the main checks.
pub fn raw_input_diagnostic() -> Outcome {
    let mut lines = Vec::new();
    let mut broken = Vec::new();
    let cases = std::iter::once(("baseline (no shortcut)", &[][..]))
        .chain(SHORTCUTS)
        .chain([GAME_BAR]);
    for (name, keys) in cases {
        let result = (|| -> Result<String, String> {
            let mut rig = EngineRig::start()?;
            if !keys.is_empty() {
                let since = rig.lock_and_wait(Duration::from_secs(10))?;
                send(&chord_strokes(keys))?;
                sleep(Duration::from_millis(250));
                send(&chord_strokes(&CHORD))?;
                let ended = rig.wait_ended(since, Duration::from_secs(3));
                rig.ensure_idle();
                ended?;
            }
            let raw_before = testkit::raw_input_seen();
            rig.lock_and_wait(Duration::from_secs(10))?;
            rig.engine().testkit_drop_hook().map_err(|e| e.details())?;
            sleep(Duration::from_millis(100));
            let focus = foreground_note();
            let typed = Instant::now();
            send(&[Stroke::down(vk::F13), Stroke::up(vk::F13)])?;
            let ended = rig.wait_ended_with_errors(typed, Duration::from_secs(2));
            rig.ensure_idle();
            let seen = testkit::raw_input_seen().saturating_sub(raw_before);
            Ok(match ended {
                Ok(e) if e.errors.iter().any(|k| k == "error.hook_lost") => {
                    format!("ok ({} ms, {seen} WM_INPUT; {focus})", e.after.as_millis())
                }
                _ => {
                    broken.push(name);
                    format!("BROKEN ({seen} WM_INPUT; {focus})")
                }
            })
        })();
        let line = result.unwrap_or_else(|e| {
            broken.push(name);
            format!("error: {e}")
        });
        lines.push(format!("{name}: {line}"));
        crate::harness::release_watched_keys();
    }
    let detail = format!(
        "{}; close the Game Bar overlay now (Win+G or click outside it)",
        lines.join(" | ")
    );
    // Win+G breaking the check is the documented limitation; anything else is a finding.
    broken.retain(|&name| name != GAME_BAR.0);
    if broken.is_empty() {
        Outcome::Pass(detail)
    } else {
        Outcome::Fail(format!(
            "raw input lost after: {}; {detail}",
            broken.join(", ")
        ))
    }
}

// ---------------------------------------------------------------------------------------------
// M5: mouse and touchpad lock (ADR 0013)
// ---------------------------------------------------------------------------------------------

/// Cleanup after a mouse scenario: releases any button the engine may still hold in its drain,
/// returns the engine to idle and releases any key left down.
fn finish_mouse(ctx: &mut Ctx<'_>) {
    release_mouse_buttons();
    ctx.rig.ensure_idle();
    release_mouse_buttons();
}

/// Fails unless input of both kinds gets through, no watched key is stuck and no button is held.
fn expect_all_input_back(observer: &Observer) -> Result<(), String> {
    expect_mouse_probes_pass(observer)?;
    expect_probes_pass(observer)?;
    expect_no_stuck_keys()?;
    expect_no_held_buttons()
}

/// What the engine's Raw Input sink saw from mice, for failure messages.
fn raw_mouse_note(before: u64) -> String {
    let seen = testkit::raw_mouse_seen().saturating_sub(before);
    if seen == 0 {
        "the engine saw no mouse WM_INPUT at all".into()
    } else {
        format!("the engine saw {seen} mouse WM_INPUT message(s)")
    }
}

/// S26: a keyboard+mouse lock blocks every kind of mouse event and keeps the cursor still;
/// Ctrl+Alt+K ends it. Then a press held across the chord: its release must be swallowed (the
/// observer sees none) and the session must still end without the drain timing out.
fn mouse_chord_unlock(ctx: &mut Ctx<'_>) -> Outcome {
    let _cursor = CursorGuard::save();
    let mut run = || -> Result<String, String> {
        let since = ctx
            .rig
            .lock_with_and_wait(Duration::from_secs(10), LockTargets::ALL)?;
        expect_probes_blocked(ctx.observer)?;
        expect_mouse_probes_blocked(ctx.observer)?;
        send(&chord_strokes(&CHORD))?;
        let ended = ctx.rig.wait_ended(since, Duration::from_secs(3))?;
        expect_reason(&ended, EndReason::Emergency)?;
        expect_all_input_back(ctx.observer)?;

        // A left press during the lock, still held when the chord ends it.
        let since = ctx
            .rig
            .lock_with_and_wait(Duration::from_secs(10), LockTargets::ALL)?;
        ctx.observer.reset();
        send_mouse(&[MouseInput::Down(Button::Left)])?;
        sleep(Duration::from_millis(100));
        send(&chord_strokes(&CHORD))?;
        sleep(Duration::from_millis(300));
        send_mouse(&[MouseInput::Up(Button::Left)])?;
        let ended = ctx.rig.wait_ended(since, Duration::from_secs(5))?;
        expect_reason(&ended, EndReason::Emergency)?;
        // A leaked release could still be on its way to the observer.
        sleep(PROBE_WINDOW);
        let (downs, ups) = (
            ctx.observer.mouse(MouseKind::LeftDown),
            ctx.observer.mouse(MouseKind::LeftUp),
        );
        if downs + ups > 0 {
            return Err(format!(
                "the left button held across the unlock reached Windows ({downs} down, {ups} up)"
            ));
        }
        if ended.notices.contains(&EngineNotice::DrainTimedOut) {
            return Err("the drain timed out instead of ending on the left release".into());
        }
        expect_all_input_back(ctx.observer)?;
        Ok(format!(
            "every mouse probe blocked, cursor still, Ctrl+Alt+K unlocked; a press held across \
             the chord had its release swallowed and the session ended cleanly{}",
            unusable_probes_note()
        ))
    };
    let result = run();
    finish_mouse(ctx);
    result.into()
}

/// S27: in a mouse-only lock keys pass and the mouse is blocked. Ctrl+Alt+K ends it; Ctrl and Alt
/// reach Windows, but K, which completes the chord, doesn't (neither press nor release).
fn mouse_only_chord(ctx: &mut Ctx<'_>) -> Outcome {
    let _cursor = CursorGuard::save();
    let mut run = || -> Result<String, String> {
        let since = ctx
            .rig
            .lock_with_and_wait(Duration::from_secs(10), MOUSE_ONLY)?;
        for key in [vk::F13, vk::F13 + 1] {
            if !probe_passes(ctx.observer, key)? {
                return Err("a key probe was blocked during a mouse-only lock".into());
            }
        }
        expect_mouse_probes_blocked(ctx.observer)?;
        ctx.observer.reset();
        send(&chord_strokes(&CHORD))?;
        let ended = ctx.rig.wait_ended(since, Duration::from_secs(3))?;
        expect_reason(&ended, EndReason::Emergency)?;
        let modifiers_seen = wait_until(PROBE_WINDOW, || {
            [vk::LCONTROL, vk::LMENU]
                .iter()
                .all(|&k| ctx.observer.downs(k) > 0 && ctx.observer.ups(k) > 0)
        });
        if !modifiers_seen {
            return Err(format!(
                "Ctrl and Alt should pass in a mouse-only lock; seen: Ctrl {} down / {} up, Alt \
                 {} down / {} up",
                ctx.observer.downs(vk::LCONTROL),
                ctx.observer.ups(vk::LCONTROL),
                ctx.observer.downs(vk::LMENU),
                ctx.observer.ups(vk::LMENU)
            ));
        }
        let (k_downs, k_ups) = (ctx.observer.downs(vk::K), ctx.observer.ups(vk::K));
        if k_downs + k_ups > 0 {
            return Err(format!(
                "K, which completed the chord, reached Windows ({k_downs} down, {k_ups} up)"
            ));
        }
        expect_all_input_back(ctx.observer)?;
        Ok(format!(
            "keys passed, mouse blocked; Ctrl+Alt+K unlocked with Ctrl and Alt passing and K \
             swallowed{}",
            unusable_probes_note()
        ))
    };
    let result = run();
    finish_mouse(ctx);
    result.into()
}

/// S28: a 3 s mouse-only lock ends on its timer.
fn mouse_only_timer(ctx: &mut Ctx<'_>) -> Outcome {
    let _cursor = CursorGuard::save();
    let mut run = || -> Result<String, String> {
        let since = ctx
            .rig
            .lock_with_and_wait(Duration::from_secs(3), MOUSE_ONLY)?;
        expect_mouse_probes_blocked(ctx.observer)?;
        let ended = ctx.rig.wait_ended(since, Duration::from_secs(8))?;
        expect_reason(&ended, EndReason::Timeout)?;
        let secs = ended.after.as_secs_f64();
        if !(2.9..=3.8).contains(&secs) {
            return Err(format!("released after {secs:.2} s (expected about 3 s)"));
        }
        expect_all_input_back(ctx.observer)?;
        Ok(format!(
            "3 s mouse-only lock released after {secs:.2} s{}",
            unusable_probes_note()
        ))
    };
    let result = run();
    finish_mouse(ctx);
    result.into()
}

/// S29: a mouse-only lock with no session timer, while only the mouse moves (a tagged move every
/// 50 ms, alternating left and right; nobody types). The hard deadline must end it. As in S16, the
/// harness can't tell whether the mouse hook's per-event check or the watchdog got there first;
/// either one alone must be enough (invariant 6).
fn mouse_only_deadline(ctx: &mut Ctx<'_>) -> Outcome {
    let _cursor = CursorGuard::save();
    let mut run = || -> Result<String, String> {
        // Dev hard deadline for a 3 s lock: 3 s + 10 s grace = 13 s.
        let since = ctx
            .rig
            .lock_without_session_timer_with(Duration::from_secs(3), MOUSE_ONLY)?;
        let stop = AtomicBool::new(false);
        let (ended, moves) = std::thread::scope(|scope| {
            let injector = scope.spawn(|| {
                let mut sent = 0u32;
                let mut dx = 10;
                while !stop.load(Ordering::Acquire) {
                    if mouse::send(&[MouseInput::Move { dx, dy: 0 }]).is_ok() {
                        sent += 1;
                    }
                    dx = -dx;
                    sleep(Duration::from_millis(50));
                }
                sent
            });
            // No `?` in here: the injector must always be told to stop before the scope joins it.
            let ended = ctx.rig.wait_ended(since, Duration::from_secs(18));
            stop.store(true, Ordering::Release);
            (ended, injector.join().unwrap_or(0))
        });
        let ended = ended?;
        expect_reason(&ended, EndReason::HardDeadline)?;
        let secs = ended.after.as_secs_f64();
        if !(12.9..=14.0).contains(&secs) {
            return Err(format!("released after {secs:.2} s (expected about 13 s)"));
        }
        expect_all_input_back(ctx.observer)?;
        Ok(format!(
            "released by the hard deadline after {secs:.2} s while {moves} mouse moves were sent{}",
            unusable_probes_note()
        ))
    };
    let result = run();
    finish_mouse(ctx);
    result.into()
}

/// S30: S9 with `lock_smoke --mouse`: killing the process mid-lock must release the mouse too.
fn kill_lock_smoke_mouse(ctx: &mut Ctx<'_>) -> Outcome {
    let Some(exe) = target_dir().map(|d| d.join("examples").join("lock_smoke.exe")) else {
        return Outcome::Skip("can't locate the target directory".into());
    };
    if !exe.exists() {
        return Outcome::Skip(
            "build it first: cargo build -p keyclean-win --example lock_smoke".into(),
        );
    }
    let _cursor = CursorGuard::save();
    let run = || -> Result<Outcome, String> {
        let child = Command::new(&exe)
            .arg("--mouse")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("couldn't start lock_smoke: {e}"))?;
        let mut guard = Guard(child);
        let stdout = guard.0.stdout.take().ok_or("no stdout")?;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if line.starts_with("Keyboard, mouse and touchpad locked") {
                    let _ = tx.send(true);
                } else if line.starts_with("Keyboard locked") {
                    let _ = tx.send(false);
                }
            }
        });
        let mouse_locked = rx
            .recv_timeout(Duration::from_secs(10))
            .map_err(|_| "lock_smoke didn't report a lock within 10 s".to_string())?;
        if !mouse_locked {
            // Built before the mouse lock: it ignores --mouse. Kill it (the guard does) and skip.
            return Ok(Outcome::Skip(
                "lock_smoke.exe predates --mouse; rebuild it: cargo build -p keyclean-win \
                 --example lock_smoke"
                    .into(),
            ));
        }
        wait_blocked(ctx.observer, Duration::from_secs(2))?;
        expect_mouse_probes_blocked(ctx.observer)?;
        guard.0.kill().map_err(|e| format!("kill failed: {e}"))?;
        let killed = Instant::now();
        if !wait_exit(&mut guard.0, Duration::from_secs(3)) {
            return Err("lock_smoke didn't exit after kill".into());
        }
        expect_all_input_back(ctx.observer)?;
        Ok(Outcome::Pass(format!(
            "mouse and keys back {} ms after the kill{}",
            killed.elapsed().as_millis(),
            unusable_probes_note()
        )))
    };
    let result = run();
    crate::harness::release_watched_keys();
    release_mouse_buttons();
    result.unwrap_or_else(Outcome::Fail)
}

/// S31: the engine's mouse hook disappears without the engine knowing. The next mouse move gets
/// through; the mouse liveness check must end the lock with `error.mouse_hook_lost`. Relies on
/// injected mouse moves producing mouse `WM_INPUT`, as S17 does for keys.
fn mouse_hook_lost(ctx: &mut Ctx<'_>) -> Outcome {
    let _cursor = CursorGuard::save();
    let mut run = || -> Result<String, String> {
        let raw_before = testkit::raw_mouse_seen();
        ctx.rig
            .lock_with_and_wait(Duration::from_secs(10), LockTargets::ALL)?;
        let registered = system::registered_raw_input_count();
        expect_mouse_probes_blocked(ctx.observer)?;
        let raw_while_blocking = testkit::raw_mouse_seen().saturating_sub(raw_before);
        ctx.rig
            .engine()
            .testkit_drop_mouse_hook()
            .map_err(|e| e.details())?;
        sleep(Duration::from_millis(100));
        ctx.observer.reset();
        let moved = Instant::now();
        send_mouse(&[
            MouseInput::Move { dx: 10, dy: 0 },
            MouseInput::Move { dx: -10, dy: 0 },
        ])?;
        if !wait_until(PROBE_WINDOW, || ctx.observer.mouse(MouseKind::Move) > 0) {
            return Err("a mouse move was still blocked after the mouse hook was removed".into());
        }
        let diag = || {
            format!(
                "registrations during the lock: {registered:?}; mouse WM_INPUT while the hook \
                 blocked: {raw_while_blocking}; {}",
                raw_mouse_note(raw_before)
            )
        };
        let ended = ctx
            .rig
            .wait_ended_with_errors(moved, Duration::from_secs(3))
            .map_err(|e| format!("{e}; {}", diag()))?;
        expect_reason(&ended, EndReason::EngineError).map_err(|e| format!("{e}; {}", diag()))?;
        if !ended.errors.iter().any(|k| k == "error.mouse_hook_lost") {
            return Err(format!(
                "ended without error.mouse_hook_lost (errors: {:?}); {}",
                ended.errors,
                diag()
            ));
        }
        if ended.after > HOOK_LOST_LIMIT {
            return Err(format!(
                "detected only {} ms after the leaked move (limit {} ms)",
                ended.after.as_millis(),
                HOOK_LOST_LIMIT.as_millis()
            ));
        }
        expect_all_input_back(ctx.observer)?;
        expect_no_raw_input_registration()?;
        Ok(format!(
            "lock ended {} ms after the first leaked move; {}{}",
            ended.after.as_millis(),
            raw_mouse_note(raw_before),
            unusable_probes_note()
        ))
    };
    let result = run();
    finish_mouse(ctx);
    result.into()
}

/// S32: Windows refuses the mouse hook after the keyboard hook went in (forced by the test kit).
/// Nothing may be locked: the session never reports Locked, the keyboard hook comes out again at
/// once, no Raw Input stays registered, and the next keyboard+mouse lock works.
fn mouse_install_fails(ctx: &mut Ctx<'_>) -> Outcome {
    let _cursor = CursorGuard::save();
    let mut run = || -> Result<String, String> {
        ctx.rig.engine().testkit_fail_next_mouse_install();
        let since = ctx
            .rig
            .lock_with(Duration::from_secs(3), LockTargets::ALL)?;
        let ended = ctx
            .rig
            .wait_ended_with_errors(since, Duration::from_secs(3))?;
        expect_reason(&ended, EndReason::EngineError)?;
        if !ended.errors.iter().any(|k| k == "error.hook_install") {
            return Err(format!(
                "ended without error.hook_install (errors: {:?})",
                ended.errors
            ));
        }
        if ended.saw_locked {
            return Err("the session reported Locked although the mouse hook failed".into());
        }
        expect_probes_pass(ctx.observer)
            .map_err(|e| format!("keys blocked after the failed install: {e}"))?;
        expect_mouse_probes_pass(ctx.observer)?;
        expect_no_raw_input_registration()?;
        // The engine must still lock normally afterwards (the forced failure was used up).
        ctx.rig
            .lock_with_and_wait(Duration::from_secs(2), LockTargets::ALL)?;
        expect_mouse_probes_blocked(ctx.observer)?;
        Ok(format!(
            "ended after {} ms with error.hook_install; never locked; keys and mouse passed; \
             next lock works{}",
            ended.after.as_millis(),
            unusable_probes_note()
        ))
    };
    let result = run();
    finish_mouse(ctx);
    result.into()
}

/// S33 (opt-in diagnostic, `--mouse-diag`): Max uses the touchpad during a 15 s mouse-only lock
/// while the engine only counts failed mouse liveness checks instead of ending the lock. Reports
/// the raw mouse messages, the liveness misses and any of the person's mouse events (counted by
/// kind only) that got past the lock. Informational: it fails only if input doesn't come back.
fn mouse_diagnostic(ctx: &mut Ctx<'_>) -> Outcome {
    let _cursor = CursorGuard::save();
    println!();
    println!("  Mouse diagnostic: the mouse and touchpad lock for 15 s; the keyboard stays free.");
    println!("  When it says GO, on the touchpad: move, tap, click, two-finger scroll,");
    println!("  three- and four-finger swipes left/right/up/down, and pinch.");
    println!("  A mouse, if you have one: move, click, scroll. Ctrl+Alt+K ends it early.");
    for n in (1..=5).rev() {
        println!("  Locking in {n}...");
        sleep(Duration::from_secs(1));
    }
    ctx.rig.engine().testkit_liveness_report_only(true);
    let mut run = || -> Result<String, String> {
        let raw_before = testkit::raw_mouse_seen();
        let (_, misses_before) = testkit::liveness_misses();
        let since = ctx
            .rig
            .lock_with_and_wait(Duration::from_secs(15), MOUSE_ONLY)?;
        let observer = ctx.observer;
        observer.count_untagged_mouse(true);
        println!("  GO: use the touchpad now (15 s).");
        // Stop counting the moment the session ends (timer or an early Ctrl+Alt+K), so normal
        // mouse use after the unlock isn't reported as getting past the lock.
        let mut counts = (0, 0);
        let ended =
            ctx.rig
                .wait_ended_with_errors_then(since, Duration::from_secs(20), &mut || {
                    observer.count_untagged_mouse(false);
                    counts = (
                        testkit::raw_mouse_seen().saturating_sub(raw_before),
                        testkit::liveness_misses().1.saturating_sub(misses_before),
                    );
                })?;
        println!("  STOP: the lock has ended.");
        let (raw, misses) = counts;
        let unused = if raw == 0 {
            " (no mouse input arrived; was the touchpad used?)"
        } else {
            ""
        };
        let leaked: Vec<String> = MouseKind::ALL
            .iter()
            .filter_map(|&kind| {
                let n = observer.untagged_mouse(kind);
                (n > 0).then(|| format!("{} {n}", kind.name()))
            })
            .collect();
        Ok(format!(
            "ended {:?} after {:.1} s (errors {:?}); during the lock: {raw} raw mouse \
             message(s){unused}, {misses} mouse liveness miss(es) (250 ms windows with raw mouse \
             input but no hook call), your mouse events past the lock: {}; report-only mode also \
             turned off the keyboard lost-hook check for this lock",
            ended.reason,
            ended.after.as_secs_f64(),
            ended.errors,
            if leaked.is_empty() {
                "none".to_string()
            } else {
                leaked.join(", ")
            }
        ))
    };
    let result = run();
    ctx.observer.count_untagged_mouse(false);
    ctx.rig.engine().testkit_liveness_report_only(false);
    finish_mouse(ctx);
    let back =
        expect_all_input_back(ctx.observer).and_then(|()| expect_no_raw_input_registration());
    match (result, back) {
        (Ok(detail), Ok(())) => Outcome::Pass(detail),
        (Ok(detail), Err(e)) => Outcome::Fail(format!("{e}; {detail}")),
        (Err(e), Ok(())) => Outcome::Fail(e),
        (Err(e), Err(back)) => Outcome::Fail(format!("{e}; {back}")),
    }
}

// ---------------------------------------------------------------------------------------------
// M6: the full-screen overlay (ADR 0014)
// ---------------------------------------------------------------------------------------------

/// App log line: the overlay page confirmed it is visible, so the app asks the engine to lock.
const CONFIRMED: &str = "overlay: confirmed visible ";

/// App log line: the overlay didn't confirm (or went away first), so nothing was locked.
const NOTHING_LOCKED: &str = "overlay: nothing locked";

/// The overlay lines of an app log, for failure messages (session events only, never keys).
fn overlay_log_note(log: &str) -> String {
    let lines: Vec<&str> = log
        .lines()
        .filter(|l| l.contains("overlay") || l.contains("autolock"))
        .collect();
    if lines.is_empty() {
        "the app log has no overlay lines".into()
    } else {
        format!("app log: {}", lines.join(" | "))
    }
}

/// Which path released the lock when the app exited, for S11/S23 results: the overlay closing
/// with the app also ends the lock, which can beat the exit path's own unlock. Not asserted.
fn unlock_path_note(log: &str) -> String {
    let lost = log.lines().position(|l| l.contains("overlay lost"));
    let ended = log.lines().position(|l| l.contains("session ended:"));
    match (lost, ended) {
        (Some(lost), Some(ended)) if lost < ended => {
            "unlocked via the overlay closing (\"overlay lost\" logged before the session end)"
                .into()
        }
        (Some(_), Some(_)) => {
            "unlocked via the exit path (\"overlay lost\" logged only after the session end)".into()
        }
        (None, _) => "unlocked via the exit path (no \"overlay lost\" logged)".into(),
        (Some(_), None) => "\"overlay lost\" logged, but no session end".into(),
    }
}

/// How the app log says the session ended, for failure messages.
fn ended_note(log: &str) -> String {
    log.lines()
        .find(|l| l.contains("session ended:"))
        .map_or("no session end logged".to_string(), str::to_string)
}

/// Waits until the app log of scenario `id` contains `needle`. Returns whether it did, and the
/// log as last read.
fn wait_log(id: &str, needle: &str, timeout: Duration) -> (bool, String) {
    let mut log = String::new();
    let found = wait_until(timeout, || {
        log = app_log(id);
        log.contains(needle)
    });
    (found, log)
}

/// The number right after `key` in `text`, e.g. the 412 of "confirmed visible 412 ms".
fn ms_after(text: &str, key: &str) -> Option<u64> {
    let rest = &text[text.find(key)? + key.len()..];
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// Waits until the app's autolock has gone through its overlay and KeyClean blocks probes.
/// Returns when the harness saw the overlay's confirmation in the log: the app asks the engine to
/// lock right after it. An overlay that didn't confirm shows in the error, instead of a bare "the
/// lock didn't engage".
fn wait_app_locked(observer: &Observer, id: &str, timeout: Duration) -> Result<Instant, String> {
    let mut log = String::new();
    let decided = wait_until(timeout, || {
        log = app_log(id);
        log.contains(CONFIRMED) || log.contains(NOTHING_LOCKED)
    });
    if !decided || !log.contains(CONFIRMED) {
        return Err(format!(
            "the overlay didn't confirm it was visible within {timeout:?}, so nothing locked \
             ({}); log: e2e-{id}-app.log",
            overlay_log_note(&log)
        ));
    }
    let confirmed = Instant::now();
    wait_blocked(observer, Duration::from_secs(3))?;
    Ok(confirmed)
}

/// Waits for the app's overlay: its visible, topmost window.
fn wait_overlay(pid: u32, timeout: Duration) -> Option<(isize, Rect)> {
    let mut found = None;
    wait_until(timeout, || {
        found = system::overlay_window(pid, APP_WINDOW_CLASS);
        found.is_some()
    });
    found
}

/// The overlay window `hwnd` is destroyed (not just hidden) and the app shows no other overlay.
fn overlay_gone(pid: u32, hwnd: isize) -> bool {
    !system::window_exists(hwnd) && system::overlay_window(pid, APP_WINDOW_CLASS).is_none()
}

/// Fails unless the overlay `hwnd` is gone within 1 s; returns how long that took.
fn expect_overlay_gone(pid: u32, hwnd: isize, what: &str) -> Result<Duration, String> {
    let started = Instant::now();
    if wait_until(Duration::from_secs(1), || overlay_gone(pid, hwnd)) {
        Ok(started.elapsed())
    } else if system::window_exists(hwnd) {
        Err(format!(
            "{what}, but the overlay window still existed 1 s later"
        ))
    } else {
        Err(format!(
            "{what}, but another overlay window was open 1 s later"
        ))
    }
}

/// "1920x1080 at (0, 0)", for messages.
fn rect_note(r: Rect) -> String {
    format!(
        "{}x{} at ({}, {})",
        r.right - r.left,
        r.bottom - r.top,
        r.left,
        r.top
    )
}

/// How much longer a probe that missed [`PROBE_WINDOW`] may take to arrive. A key KeyClean
/// blocked never reaches the observer; one delayed by a busy system (WebView2 starting up) still
/// does.
const LATE_PROBE: Duration = Duration::from_secs(1);

/// Taps a probe key and reports whether KeyClean blocked it: it hadn't reached the observer
/// [`LATE_PROBE`] after the probe window either.
fn probe_blocked(observer: &Observer) -> Result<bool, String> {
    if crate::harness::probe_passes(observer, vk::F13)? {
        return Ok(false);
    }
    let arrived = wait_until(LATE_PROBE, || {
        observer.downs(vk::F13) > 0 || observer.ups(vk::F13) > 0
    });
    Ok(!arrived)
}

/// Runs one overlay check against the app: starts it with a `secs` autolock and `env`, runs
/// `check`, then closes the app whatever happened (which also ends a lock a failed check left
/// running) and waits until no keyclean.exe is left.
fn overlay_check(
    ctx: &mut Ctx<'_>,
    id: &'static str,
    secs: u64,
    env: &[(&str, &str)],
    check: impl FnOnce(&Observer, &mut Child) -> Result<String, String>,
) -> Outcome {
    let exe = match app_exe() {
        Ok(exe) => exe,
        Err(skip) => return skip,
    };
    let observer = ctx.observer;
    let result = (|| -> Result<String, String> {
        let mut app = start_app_with(&exe, Some(secs), env, id)?;
        let pid = app.0.id();
        let checked = check(observer, &mut app.0);
        system::request_close(pid, APP_WINDOW_CLASS);
        let exited = wait_exit(&mut app.0, Duration::from_secs(10));
        let detail = checked?;
        if !exited {
            return Err(format!(
                "{detail}; but the app didn't exit within 10 s of its windows closing"
            ));
        }
        expect_no_keyclean_left().map_err(|e| format!("{detail}; {e}"))?;
        Ok(detail)
    })();
    crate::harness::release_watched_keys();
    sleep(WEBVIEW_SETTLE);
    result.into()
}

/// Fails unless the app process is still running.
fn expect_app_running(app: &mut Child, what: &str) -> Result<(), String> {
    if matches!(app.try_wait(), Ok(None)) {
        Ok(())
    } else {
        Err(format!("the app exited after {what}"))
    }
}

/// S34: before the first probe is blocked, the overlay is visible, topmost, and covers exactly
/// the monitor under the cursor (invariant 11). Ctrl+Alt+K ends the lock and the overlay goes
/// with it.
fn overlay_before_lock(ctx: &mut Ctx<'_>) -> Outcome {
    // Read before the app starts. The cursor stays put, so the overlay must land on this monitor.
    let Some(monitor) = system::monitor_rect_under_cursor() else {
        return Outcome::Fail("couldn't read the monitor under the cursor".into());
    };
    overlay_check(ctx, "S34", 15, &[], move |observer, app| {
        let pid = app.id();
        // A watcher thread polls every 5 ms, so an overlay that appears just before the lock is
        // caught: when any overlay was first seen, and when one first covered the monitor.
        let stop = AtomicBool::new(false);
        let first_seen: Mutex<Option<(Instant, Rect)>> = Mutex::new(None);
        let first_covering: Mutex<Option<(Instant, isize)>> = Mutex::new(None);
        let probed = std::thread::scope(|scope| {
            scope.spawn(|| {
                while !stop.load(Ordering::Relaxed) {
                    if let Some((hwnd, rect)) = system::overlay_window(pid, APP_WINDOW_CLASS) {
                        let now = Instant::now();
                        let mut seen = first_seen.lock().unwrap_or_else(PoisonError::into_inner);
                        seen.get_or_insert((now, rect));
                        drop(seen);
                        if rect == monitor {
                            first_covering
                                .lock()
                                .unwrap_or_else(PoisonError::into_inner)
                                .get_or_insert((now, hwnd));
                        }
                    }
                    sleep(Duration::from_millis(5));
                }
            });
            // Probes back to back; returns when the first blocked one was sent.
            let until = Instant::now() + Duration::from_secs(25);
            let result = loop {
                if Instant::now() >= until {
                    break Err(format!(
                        "the lock didn't engage within 25 s ({}); log: e2e-S34-app.log",
                        overlay_log_note(&app_log("S34"))
                    ));
                }
                let sent = Instant::now();
                match probe_blocked(observer) {
                    Ok(true) => break Ok(sent),
                    Ok(false) => {}
                    Err(e) => break Err(e),
                }
            };
            stop.store(true, Ordering::Relaxed);
            result
        });
        let blocked_sent = probed?;
        let seen = *first_seen.lock().unwrap_or_else(PoisonError::into_inner);
        let covering = *first_covering
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let hwnd = match (covering, seen) {
            (Some((at, hwnd)), _) if at <= blocked_sent => hwnd,
            (Some((at, _)), _) => {
                return Err(format!(
                    "a probe was blocked {} ms before the overlay covered the monitor \
                     (invariant 11)",
                    (at - blocked_sent).as_millis()
                ));
            }
            (None, Some((_, rect))) => {
                return Err(format!(
                    "the overlay covers {}, but the monitor under the cursor is {}",
                    rect_note(rect),
                    rect_note(monitor)
                ));
            }
            (None, None) => {
                return Err(
                    "a probe was blocked, but no overlay was ever visible (invariant 11)".into(),
                );
            }
        };
        let rect = monitor;
        expect_probes_blocked(observer)?;
        if system::overlay_window(pid, APP_WINDOW_CLASS).map(|(h, _)| h) != Some(hwnd) {
            return Err("the overlay went away while the lock was still on".into());
        }
        send(&chord_strokes(&CHORD))?;
        let back = wait_input_back(observer, Duration::from_secs(3))?
            .ok_or("Ctrl+Alt+K didn't give input back within 3 s")?;
        let gone = expect_overlay_gone(
            pid,
            hwnd,
            &format!("input back {} ms after Ctrl+Alt+K", back.as_millis()),
        )?;
        let (_, log) = wait_log("S34", "session ended:", Duration::from_secs(2));
        if !log.contains("session ended: Emergency") {
            return Err(format!(
                "the lock didn't end by Ctrl+Alt+K ({}); log: e2e-S34-app.log",
                ended_note(&log)
            ));
        }
        expect_probes_pass(observer)?;
        expect_no_stuck_keys()?;
        let ms = |key: &str| ms_after(&log, key).map_or("?".to_string(), |n| n.to_string());
        Ok(format!(
            "overlay visible, topmost and covering the monitor under the cursor ({}) before the \
             first blocked probe; built {} ms, shown {} ms, confirmed {} ms after the request; \
             Ctrl+Alt+K gave input back after {} ms, overlay gone {} ms after that",
            rect_note(rect),
            ms("overlay: built "),
            ms(", shown "),
            ms(CONFIRMED),
            back.as_millis(),
            gone.as_millis()
        ))
    })
}

/// S35: a 3 s lock ends by its timer, and the overlay closes with it.
fn overlay_timer_end(ctx: &mut Ctx<'_>) -> Outcome {
    overlay_check(ctx, "S35", 3, &[], |observer, app| {
        let pid = app.id();
        wait_app_locked(observer, "S35", Duration::from_secs(20))?;
        let (hwnd, _) = system::overlay_window(pid, APP_WINDOW_CLASS)
            .ok_or("the lock engaged, but no overlay window was found")?;
        wait_input_back(observer, Duration::from_secs(8))?
            .ok_or("input wasn't back within 8 s of a 3 s lock")?;
        let gone = expect_overlay_gone(pid, hwnd, "input came back")?;
        let (_, log) = wait_log("S35", "session ended:", Duration::from_secs(2));
        if !log.contains("session ended: Timeout") {
            return Err(format!(
                "the lock didn't end by its timer ({}); log: e2e-S35-app.log",
                ended_note(&log)
            ));
        }
        expect_probes_pass(observer)?;
        expect_no_stuck_keys()?;
        Ok(format!(
            "3 s lock ended by its timer; overlay gone {} ms after input came back",
            gone.as_millis()
        ))
    })
}

/// S36: the app ignores the page's confirmation (`KEYCLEAN_E2E_OVERLAY=no-ack`). Nothing may be
/// blocked, the app must say why, and the overlay must close (invariant 11).
fn overlay_no_ack(ctx: &mut Ctx<'_>) -> Outcome {
    overlay_check(
        ctx,
        "S36",
        15,
        &[("KEYCLEAN_E2E_OVERLAY", "no-ack")],
        |observer, app| {
            let pid = app.id();
            let (hwnd, _) = wait_overlay(pid, Duration::from_secs(20)).ok_or_else(|| {
                format!(
                    "no overlay appeared within 20 s ({}); log: e2e-S36-app.log",
                    overlay_log_note(&app_log("S36"))
                )
            })?;
            let appeared = Instant::now();
            let mut probes = 0;
            while appeared.elapsed() < Duration::from_secs(5) {
                if probe_blocked(observer)? {
                    return Err(format!(
                        "a probe was blocked {} ms after the overlay appeared, though the overlay \
                         never confirmed",
                        appeared.elapsed().as_millis()
                    ));
                }
                probes += 1;
                sleep(Duration::from_millis(100));
            }
            let (found, log) = wait_log("S36", NOTHING_LOCKED, Duration::from_secs(1));
            if !found {
                return Err(format!(
                    "the app log doesn't say nothing was locked ({}); log: e2e-S36-app.log",
                    overlay_log_note(&log)
                ));
            }
            if log.contains(CONFIRMED) || log.contains("session ended:") {
                return Err(format!(
                    "the app logged a confirmation or a session though the overlay never \
                     confirmed ({})",
                    overlay_log_note(&log)
                ));
            }
            expect_overlay_gone(pid, hwnd, "the app gave up on the overlay")?;
            expect_probes_pass(observer)?;
            expect_no_stuck_keys()?;
            Ok(format!(
                "{probes} probes over 5 s after the overlay appeared, none blocked; the app \
                 logged \"nothing locked\" and closed the overlay"
            ))
        },
    )
}

/// What S37-S39 do to the overlay mid-lock.
#[derive(Clone, Copy)]
enum Disturb {
    Close,
    Hide,
    Minimize,
}

/// S37-S39: the overlay is closed, hidden or minimized mid-lock. Input must come back within
/// `limit`, the app log must name the loss, the overlay window must be destroyed, and the app must
/// keep running.
fn overlay_disturbed(
    ctx: &mut Ctx<'_>,
    id: &'static str,
    how: Disturb,
    limit: Duration,
) -> Outcome {
    let (verb, done, loss) = match how {
        Disturb::Close => ("close", "closed", "Closed"),
        Disturb::Hide => ("hide", "hidden", "Hidden"),
        Disturb::Minimize => ("minimize", "minimized", "Minimized"),
    };
    let wait = limit + Duration::from_secs(3);
    overlay_check(ctx, id, 15, &[], move |observer, app| {
        let pid = app.id();
        wait_app_locked(observer, id, Duration::from_secs(20))?;
        let (hwnd, _) = system::overlay_window(pid, APP_WINDOW_CLASS)
            .ok_or("the lock engaged, but no overlay window was found")?;
        let sent = match how {
            Disturb::Close => system::request_close_window(hwnd),
            Disturb::Hide => system::hide_window(hwnd),
            Disturb::Minimize => system::minimize_window(hwnd),
        };
        if !sent {
            return Err(format!("couldn't {verb} the overlay"));
        }
        let back = wait_input_back(observer, wait)?.ok_or_else(|| {
            format!(
                "input wasn't back {} ms after the overlay was {done} ({}); log: \
                 e2e-{id}-app.log",
                wait.as_millis(),
                overlay_log_note(&app_log(id))
            )
        })?;
        if back > limit {
            return Err(format!(
                "input came back only {} ms after the overlay was {done} (limit {} ms)",
                back.as_millis(),
                limit.as_millis()
            ));
        }
        let needle = format!("overlay lost ({loss})");
        let (found, log) = wait_log(id, &needle, Duration::from_secs(2));
        if !found {
            return Err(format!(
                "input came back, but the app log has no \"{needle}\" ({}); log: e2e-{id}-app.log",
                overlay_log_note(&log)
            ));
        }
        expect_overlay_gone(pid, hwnd, "input came back")?;
        expect_app_running(app, &format!("its overlay was {done}"))?;
        expect_probes_pass(observer)?;
        expect_no_stuck_keys()?;
        Ok(format!(
            "input back {} ms after the overlay was {done} (limit {} ms); app logged \"{needle}\"; \
             overlay window destroyed; app still running",
            back.as_millis(),
            limit.as_millis()
        ))
    })
}

/// S37: SC_CLOSE to the overlay, as Alt+F4 or a close button would send.
fn overlay_closed(ctx: &mut Ctx<'_>) -> Outcome {
    overlay_disturbed(ctx, "S37", Disturb::Close, Duration::from_millis(1500))
}

/// S38: another program hides the overlay; the next countdown tick (1/s) notices.
fn overlay_hidden(ctx: &mut Ctx<'_>) -> Outcome {
    overlay_disturbed(ctx, "S38", Disturb::Hide, Duration::from_secs(2))
}

/// S39: another program minimizes the overlay; the next countdown tick notices.
fn overlay_minimized(ctx: &mut Ctx<'_>) -> Outcome {
    overlay_disturbed(ctx, "S39", Disturb::Minimize, Duration::from_secs(2))
}

/// S40: the app ignores the page's heartbeat (`KEYCLEAN_E2E_OVERLAY=no-tick`), as if the page had
/// hung. The lock must end once the heartbeat has been silent past its 3 s limit: about 3-4 s
/// after it started.
fn overlay_silent(ctx: &mut Ctx<'_>) -> Outcome {
    overlay_check(
        ctx,
        "S40",
        15,
        &[("KEYCLEAN_E2E_OVERLAY", "no-tick")],
        |observer, app| {
            let pid = app.id();
            let started = wait_app_locked(observer, "S40", Duration::from_secs(20))?;
            let (hwnd, _) = system::overlay_window(pid, APP_WINDOW_CLASS)
                .ok_or("the lock engaged, but no overlay window was found")?;
            wait_input_back(observer, Duration::from_secs(10))?.ok_or_else(|| {
                format!(
                    "input wasn't back 10 s after a silent overlay's lock started ({})",
                    overlay_log_note(&app_log("S40"))
                )
            })?;
            let secs = started.elapsed().as_secs_f64();
            if !(2.5..=6.0).contains(&secs) {
                return Err(format!(
                    "the lock ended {secs:.2} s after it started (expected 2.5-6 s; the heartbeat \
                     limit is 3 s)"
                ));
            }
            let (found, log) = wait_log("S40", "overlay lost (Silent)", Duration::from_secs(2));
            if !found {
                return Err(format!(
                    "input came back after {secs:.2} s, but the app log has no \"overlay lost \
                     (Silent)\" ({}); log: e2e-S40-app.log",
                    overlay_log_note(&log)
                ));
            }
            expect_overlay_gone(pid, hwnd, "input came back")?;
            expect_app_running(app, "its overlay went silent")?;
            expect_probes_pass(observer)?;
            expect_no_stuck_keys()?;
            Ok(format!(
                "lock ended {secs:.2} s after it started; app logged \"overlay lost (Silent)\"; \
                 overlay window destroyed; app still running"
            ))
        },
    )
}

/// How many locks S41 measures.
const LATENCY_LOCKS: usize = 10;

/// S41 (opt-in, `--overlay-latency`): one app launch locks 1 s ten times in a row. The first
/// overlay starts WebView2 cold; the rest show how fast a warm one confirms. Every one must
/// confirm within the 2 s budget.
fn overlay_latency(ctx: &mut Ctx<'_>) -> Outcome {
    let exe = match app_exe() {
        Ok(exe) => exe,
        Err(skip) => return skip,
    };
    let observer = ctx.observer;
    let result = (|| -> Result<String, String> {
        let repeat = LATENCY_LOCKS.to_string();
        let mut app = start_app_with(
            &exe,
            Some(1),
            &[("KEYCLEAN_E2E_AUTOLOCK_REPEAT", repeat.as_str())],
            "S41",
        )?;
        let pid = app.0.id();
        // Each attempt ends in a session end or a failed autolock (which includes an overlay
        // that didn't confirm).
        let attempts = |log: &str| {
            log.matches("session ended:").count() + log.matches("autolock failed").count()
        };
        let mut log = String::new();
        let finished = wait_until(Duration::from_secs(120), || {
            log = app_log("S41");
            attempts(&log) >= LATENCY_LOCKS
        });
        system::request_close(pid, APP_WINDOW_CLASS);
        let exited = wait_exit(&mut app.0, Duration::from_secs(10));
        let confirmed: Vec<u64> = log.lines().filter_map(|l| ms_after(l, CONFIRMED)).collect();
        let failed = log.matches("autolock failed").count();
        if !finished {
            return Err(format!(
                "only {} of {LATENCY_LOCKS} locks finished within 120 s; confirmed {confirmed:?} \
                 ms; log: e2e-S41-app.log",
                attempts(&log)
            ));
        }
        if confirmed.len() < LATENCY_LOCKS || failed > 0 {
            return Err(format!(
                "{} of {LATENCY_LOCKS} overlays confirmed ({failed} autolock(s) failed; {}); \
                 confirmed {confirmed:?} ms",
                confirmed.len(),
                overlay_log_note(&log)
            ));
        }
        if let Some(worst) = confirmed.iter().copied().max()
            && worst > 2000
        {
            return Err(format!(
                "an overlay took {worst} ms to confirm (budget 2000 ms); confirmed {confirmed:?} \
                 ms"
            ));
        }
        if !exited {
            return Err("the app didn't exit within 10 s of its windows closing".into());
        }
        expect_no_keyclean_left()?;
        expect_probes_pass(observer)?;
        expect_no_stuck_keys()?;
        let cold = confirmed[0];
        let mut warm = confirmed[1..].to_vec();
        warm.sort_unstable();
        // Nearest-rank percentile.
        let pct = |p: f64| {
            let rank = (p * warm.len() as f64).ceil() as usize;
            warm[rank.clamp(1, warm.len()) - 1]
        };
        Ok(format!(
            "cold {cold} ms; warm ({} locks) p50 {} ms, p95 {} ms, max {} ms; all {confirmed:?} ms",
            warm.len(),
            pct(0.5),
            pct(0.95),
            warm[warm.len() - 1]
        ))
    })();
    crate::harness::release_watched_keys();
    sleep(WEBVIEW_SETTLE);
    result.into()
}
