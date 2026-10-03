//! The automated M1 checks. Each maps to steps in docs/testing/manual/M1.md.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use keyclean_win::testkit::inject::{Stroke, vk};
use keyclean_win::testkit::observer::Observer;
use keyclean_win::testkit::{held, system};
use keyclean_win::{EndReason, EngineNotice, SystemTransition};

use crate::harness::{
    EngineRig, PROBE_WINDOW, expect_no_stuck_keys, expect_probes_blocked, expect_probes_pass,
    expect_reason, send, sleep, wait_until,
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
    ]
}

pub fn process_scenarios(skip_app: bool) -> Vec<Scenario> {
    let mut list = vec![Scenario {
        id: "S9",
        steps: "8",
        name: "Killing lock_smoke mid-lock releases input",
        run: kill_lock_smoke,
    }];
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
    }
    list
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

fn keyboards(ctx: &mut Ctx<'_>) -> Outcome {
    match ctx.rig.engine().devices() {
        Ok(list) if !list.is_empty() => Outcome::Pass(format!("{} keyboard(s) listed", list.len())),
        Ok(_) => Outcome::Fail("no keyboards listed".into()),
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

fn shortcuts(ctx: &mut Ctx<'_>) -> Outcome {
    // Shortcuts that open something come last, in case one gets through.
    let combos: [(&str, &[u8]); 7] = [
        ("Win", &[vk::LWIN]),
        ("Alt+Tab", &[vk::LMENU, vk::TAB]),
        ("Ctrl+Esc", &[vk::LCONTROL, vk::ESCAPE]),
        ("Win+X", &[vk::LWIN, vk::X]),
        ("Volume up", &[vk::VOLUME_UP]),
        ("Ctrl+Shift+Esc", &[vk::LCONTROL, vk::LSHIFT, vk::ESCAPE]),
        ("Win+G", &[vk::LWIN, vk::G]),
    ];
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

fn app_exe() -> Result<PathBuf, Outcome> {
    let exe = target_dir()
        .map(|d| d.join("keyclean.exe"))
        .ok_or_else(|| Outcome::Skip("can't locate the target directory".into()))?;
    if exe.exists() {
        Ok(exe)
    } else {
        Err(Outcome::Skip(
            "build it first: bun run build && cargo build -p keyclean".into(),
        ))
    }
}

/// Starts the app with its stderr (session events only, never keys) saved next to the harness as
/// `e2e-<id>-app.log`, for diagnosing failures.
fn start_app(exe: &PathBuf, autolock_secs: Option<u64>, log_id: &str) -> Result<Guard, String> {
    let mut cmd = Command::new(exe);
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
        wait_blocked(ctx.observer, Duration::from_secs(20))?;
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

fn close_app(ctx: &mut Ctx<'_>) -> Outcome {
    let exe = match app_exe() {
        Ok(exe) => exe,
        Err(skip) => return skip,
    };
    let run = || -> Result<String, String> {
        let mut app = start_app(&exe, Some(15), "S11")?;
        let pid = app.0.id();
        wait_blocked(ctx.observer, Duration::from_secs(20))?;
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
        if system::request_close(pid, APP_WINDOW_CLASS) == 0 {
            return Err("found no app window to close".into());
        }
        let closed = Instant::now();
        let input_back = wait_input_back(ctx.observer, Duration::from_secs(5))?;
        let exited = wait_exit(&mut app.0, Duration::from_secs(20)).then(|| closed.elapsed());

        let detail = format!(
            "window responding: {}; app windows [{}], close sent to the Tauri Window; input back: {}; process exit: {}; \
             log: e2e-S11-app.log",
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
        Ok(detail)
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

/// S14: Ctrl+Alt+K must end a lock while the app's own window has focus.
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
        // The autolock engages as the engine starts, before the window appears.
        if wait_responsive(pid, Duration::from_secs(20)).is_none() {
            return Err("the app window didn't appear".into());
        }
        sleep(Duration::from_secs(1));
        let rect = system::window_rect(pid, APP_WINDOW_CLASS).ok_or("app window not found")?;
        // An empty spot near the bottom of the window, below the keyboard list.
        let (x, y) = ((rect.left + rect.right) / 2, rect.bottom - 30);
        keyclean_win::testkit::inject::click(x, y).map_err(|e| format!("click failed: {e}"))?;
        sleep(Duration::from_millis(500));
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
            // Without these two, a pass wouldn't prove anything about the focused-window case.
            if system::foreground_pid() != Some(pid) {
                return Err(
                    "the click didn't give the app window focus, so the focus case wasn't tested"
                        .to_string(),
                );
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
                    "Ctrl+Alt+K ended the lock {} ms after it was pressed, with the app window                      focused",
                    pressed.elapsed().as_millis()
                ))
            } else {
                Err(format!(
                    "Ctrl+Alt+K didn't end the lock while the app window had focus (the hook went                      deaf; within 3 s: {}); log: e2e-S14-app.log",
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
