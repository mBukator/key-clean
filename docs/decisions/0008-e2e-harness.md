# ADR 0008 - Automated end-to-end harness for OS behavior

- Status: Accepted, 2026-10-01
- Deciders: Max (decision 2026-10-01)
- Related: `crates/keyclean-e2e`, `crates/keyclean-win/src/testkit/`, `docs/testing/manual/M1.md`

## Context

The project guide said OS behavior is verified only by manual scripts. M1.md has 18 steps against two
targets, which takes 30-40 minutes per run and will grow with every milestone. Most of those steps
don't need a human: they check whether keys pass or get blocked, how a lock ends, and whether any key
is left stuck.

Windows lets a program synthesize key events with `SendInput`. They travel through every low-level
keyboard hook like physical keys, only marked `LLKHF_INJECTED`
[docs](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput). KeyClean
blocks injected input during a lock (ADR 0007), so synthesized keys exercise the real pass/block path.

## Decision

Add an opt-in harness that Max runs by hand: `cargo run -p keyclean-e2e`.

- **Observer hook.** Windows calls low-level hooks newest-first, and a hook that blocks an event stops
  it from reaching older hooks
  [docs](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc). The harness
  installs its own counting hook _before_ the engine locks, so anything it sees got past KeyClean. No
  test window or focus handling is needed.
- **Tagged, harmless probes.** Every injected event carries a marker in `dwExtraInfo`; the observer
  counts only marked events and never looks at real typing. Pass/block probes use F13-F24, which type
  nothing if they leak.
- **Environment check.** Before any lock, probes must reach the observer. If another hook swallows them
  (AutoHotkey, PowerToys) or there is no interactive desktop, the harness stops with exit code 2
  instead of reporting false results.
- **Safety.** Every lock is clamped to the development caps; the real Ctrl+Alt+K always works; an
  outside PowerShell process kills the harness after 6 minutes (10 minutes from M6); child processes
  are killed when a scenario ends.
- **Code layout.** All `unsafe` code (injection, observer hook, key state, window helpers) lives in
  `keyclean-win` behind a non-default `testkit` feature. The harness crate is `#![forbid(unsafe_code)]`.
  `cargo test` compiles it but never runs it.
- **App hook.** Debug builds of the app read `KEYCLEAN_E2E_AUTOLOCK=<seconds>` and lock right after
  startup (clamped to 15 s), so the harness can test the app without clicking. Release builds don't
  contain it.
  From M6 (ADR 0014) the autolock starts once the event loop is ready and goes through the overlay
  like every lock; `KEYCLEAN_E2E_AUTOLOCK_REPEAT` repeats it and `KEYCLEAN_E2E_OVERLAY=no-ack|no-tick`
  injects overlay faults. The app exe the harness uses must embed the UI
  (`cargo build -p keyclean --features tauri/custom-protocol`).
- **CI trial.** `.github/workflows/e2e.yml` runs the harness on `windows-latest`, triggered manually only.
  The environment check reports whether hosted runners can run it.

What stays manual: Ctrl+Alt+Del (can't be synthesized), an elevated window (Windows blocks synthesized
input into it, so a test would prove nothing), Sticky Keys and the On-Screen Keyboard (they treat
synthesized input differently), sleep, firmware keys, and a short check with real keys.

## Alternatives

- **Manual only (status quo)** - simplest, but slow and error-prone, and it gets worse every milestone.
- **A focused test window instead of an observer hook** - needs `SetForegroundWindow`, which Windows
  restricts for background processes, so it would be flaky.
- **UI automation of the app (tauri-driver / WebDriver)** - needed eventually for the dashboard, but
  heavy for M1's bare window. Deferred to M7.
- **Hook tests inside `cargo test`** - rejected: `cargo test` must never install a hook (project guide),
  and agents run it.

## Consequences

- An M1 verification run becomes about 2 minutes hands-off plus about 5 minutes of manual checks.
- Synthesized input isn't physical input: hardware auto-repeat timing, firmware keys and real
  keyboard-layout AltGr behavior still need the short manual pass.
- With Cargo feature unification, `cargo build --workspace` also compiles the test kit into the app
  binary as unused code. Release builds of the app (`bun tauri build`) don't enable it.
- The project guide's "How we work" rule changes: OS behavior is verified by the harness plus a manual
  script.
