# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Project scaffold: Rust workspace (`keyclean-core`, `keyclean-win`, Tauri app shell), React + TypeScript + Tailwind UI, shared English strings, commit tooling, CI, and repository docs.
- Keyboard lock engine (M1): a user-mode low-level keyboard hook on a dedicated engine thread that blocks every key during a lock and leaves no stuck keys afterwards.
- Emergency unlock with Ctrl+Alt+K (either Ctrl, either Alt, AltGr counts), detected inside the hook.
- End-to-end harness (`cargo run -p keyclean-e2e`): synthesizes keys and checks M1's lock, unlock, drain, kill and shutdown paths automatically; behind the non-default `testkit` feature (ADR 0008).
- Hard deadline enforced twice, by the hook on every event and by a separate watchdog thread that ends the process if the engine stops responding.
- Debug dev cap: development builds lock for at most 15 s with a 20 s hard deadline and show "DEV CAP".
- Automatic unlock on system transitions: sleep, shutdown, restart, logoff, workstation lock and session switch.
- Keyboard list (name and id only) from Raw Input.
- Bare Tauri shell with a Lock button, status line and keyboard list.
- Documentation: Phase 0 research notes, fail-safe matrix, ADRs 0001-0009, roadmap, and the M1 manual test.
- Separate engine process: the app runs the lock engine as `keyclean.exe --engine` with no WebView and talks to it over stdin/stdout (session events only). The engine process exits with the app (ADR 0009).
- Harness check S14 (Ctrl+Alt+K with the app's own window focused); S10 and S11 also check that no `keyclean.exe` is left behind.

- Lock duration presets (M2): 30 seconds, 1 minute, 2 minutes and 5 minutes, 2 minutes by default. Release builds lock for the real length, within the 30-minute maximum.
- Live countdown (M2): the engine reports each time the displayed second changes, measured on the monotonic clock, and the window shows it as mm:ss.
- **Unlock now** button, and a "Cleaning complete" message when the timer ends the lock.
- Harness checks S15 (countdown through the engine process) and S16 (the hard deadline alone ends a lock whose session timer is switched off), and the M2 manual test.

- Lost-hook check (M3): if Windows removes or skips KeyClean's keyboard hook during a lock, the lock ends within about a quarter of a second and the window says why. It uses a Raw Input sink registered only during a lock, whose messages are counted but never read (ADR 0010).
- A warning when typing reaches an administrator window during a lock; the lock stays on everywhere else.
- Notices when a keyboard is connected or disconnected during a lock; the keyboard list refreshes.
- Automatic engine restart: if the engine process dies, the app starts a new idle one, at most 3 times in 5 minutes (ADR 0011).
- Harness checks S17-S25 (lost hook, failed hook install, hung engine thread, real hook timeout with `--stall`, engine killed, no Raw Input registration at idle, `taskkill` without `/F`, lost hook with no other hook in the process, and the opt-in `--raw-diag` shortcut diagnostic), the M3 manual test, ADRs 0010 and 0011, and the M3 research notes.

- Device list (M4): keyboards, mice, touchpads, touchscreens and pens with friendly names, grouped by kind, each marked Supported, Limited or Unsupported (ADR 0004). A precision touchpad is Limited; touchscreens and pens are Unsupported.
- The list updates by itself when a device is plugged in or unplugged, also while idle. It watches device interfaces with `CM_Register_Notification` in the app and registers for no input (ADR 0012).
- The window says plainly that every keyboard is locked and mice and touchpads aren't yet.
- External devices are named by what their bus reports (for example "HyperX Alloy Origins") instead of "HID Keyboard Device", and the interfaces of one physical device show as one row per kind.
- Harness check S22 now runs with the device watch active; the M4 manual test and research note.

- Mouse and touchpad lock (M5): choose Keyboard, Mouse and touchpad, or both before locking (the keyboard alone by default). A locked mouse can't move the cursor, click or scroll; held buttons are released cleanly afterwards (ADR 0013).
- Ctrl+Alt+K, the timer, the hard deadline and closing KeyClean end every lock, also a mouse-only one. In a mouse-only lock the keyboard keeps working and the chord's last key never reaches the focused app.
- The lost-hook check covers the mouse too: if Windows removes the mouse lock, the lock ends and the window says why. Only each Raw Input message's header (keyboard or mouse) is read.
- Notices for a mouse or touchpad connected or disconnected during a lock, and status text that says what is locked.
- Precision touchpads are marked Limited: pointer movement, taps and clicks are locked, but two-finger scrolling, pinch, and three- and four-finger swipes still work, and the touchpad note says so.
- Harness checks S26-S32 (mouse lock exits, mouse-only chord, dropped and failed mouse hook), S22 after a mouse lock, the opt-in `--mouse-diag` touchpad measurement, `lock_smoke --mouse`, the M5 manual test and research note.

- Full-screen overlay (M6): every lock shows a dark overlay on the monitor under the mouse pointer with the countdown, what is locked and Ctrl + Alt + K, and "Unlocking in N seconds" in the last seconds. Unlock now is on the overlay when the mouse isn't locked (ADR 0014).
- Nothing is locked until the overlay is on screen: if it doesn't confirm within 2 s, the lock doesn't start and the window says why.
- If the overlay is closed, hidden, minimized, left on another virtual desktop or stops responding during a lock, the lock ends and the window says the lock screen was closed or hidden.
- When a lock ends, the overlay disappears at once and the main window comes back.
- Harness checks S34-S40 (overlay before the lock, gone after the timer and Ctrl+Alt+K, missing confirmation, closed, hidden, minimized and silent overlay), opt-in S41 `--overlay-latency`, S14 with the overlay focused; the M6 manual test, research note and ADR 0014.

### Changed

- Locking now waits for the overlay, so `lock_input` returns once the lock has been requested or the overlay failed.
- The harness and manual tests use a debug exe with the UI built in: `cargo build -p keyclean --features tauri/custom-protocol`.
- The window's lock commands are now `lock_input` and `unlock_input`, with keyboard and mouse choices.
- The `list_keyboards` command is now `list_devices` and returns every input device with its kind and capability.
- The watchdog ends a hung engine with `TerminateProcess` instead of `abort()`, which went through Windows Error Reporting and kept the hook installed about 5 s longer.
- Harness check S6 no longer sends Win+G: it opens the Xbox Game Bar despite the lock, which blinds the lost-hook check while the overlay is open (documented limitation).

### Fixed

- `taskkill` without `/F` made the KeyClean window hang (the lock still ended). The app now exits cleanly.
- With KeyClean's own window focused, Windows could skip the keyboard hook during a lock, so Win opened the Start menu and Ctrl+Alt+K needed several tries. The hook now lives in a process without a WebView.
