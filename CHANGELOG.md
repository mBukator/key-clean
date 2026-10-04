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
- Harness check S22 now runs with the device watch active; the M4 manual test and research note.

### Changed

- The `list_keyboards` command is now `list_devices` and returns every input device with its kind and capability.
- The watchdog ends a hung engine with `TerminateProcess` instead of `abort()`, which went through Windows Error Reporting and kept the hook installed about 5 s longer.
- Harness check S6 no longer sends Win+G: it opens the Xbox Game Bar despite the lock, which blinds the lost-hook check while the overlay is open (documented limitation).

### Fixed

- `taskkill` without `/F` made the KeyClean window hang (the lock still ended). The app now exits cleanly.
- With KeyClean's own window focused, Windows could skip the keyboard hook during a lock, so Win opened the Start menu and Ctrl+Alt+K needed several tries. The hook now lives in a process without a WebView.
