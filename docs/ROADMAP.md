# KeyClean roadmap

**Future sessions start from this file.** Read it first, then the spec sections for the current
milestone (`docs/SPEC.md`, cited as §N), then CLAUDE.md's invariants.

Rules (CLAUDE.md "How we work"): one milestone at a time; plan first; the next milestone starts only after
Max confirms the current one passed its manual test (`docs/testing/manual/Mn.md`). Milestones follow the
build order in §61, grouped by the spec phases §51-§59. Acceptance criteria come from the MVP definition
of done (§60) where it applies.

Status values: **not started**, **in progress**, **code-complete - awaiting manual verification**,
**done**.

## Current status

| Milestone                                      | Phase       | Status                                       |
| ---------------------------------------------- | ----------- | -------------------------------------------- |
| M0 Scaffold and tooling                        | §51 Phase 0 | **done**                                     |
| M1 Keyboard lock + emergency unlock + deadline | §51-§52     | **done**                                     |
| M2 Timer and automatic unlock                  | §52 Phase 1 | **done**                                     |
| M3 Safety hardening                            | §53 Phase 2 | **done**                                     |
| M4 Device detection                            | §52 Phase 1 | **done**                                     |
| M5 Mouse and touchpad lock                     | §52 Phase 1 | **done**                                     |
| M6 Full-screen overlay                         | §54 Phase 3 | code-complete - awaiting manual verification |
| M7 MVP dashboard                               | §54 Phase 3 | not started                                  |
| M8 System tray                                 | §55 Phase 4 | not started                                  |
| M9 Global shortcut                             | §55 Phase 4 | not started                                  |
| M10 Start with Windows + notifications         | §55 Phase 4 | not started                                  |
| M11 Packaging                                  | §55 Phase 4 | not started                                  |
| M12 Open-source release                        | §56 Phase 5 | not started                                  |
| M13 Keyboard diagnostics                       | §57 Phase 6 | not started                                  |
| M14 Advanced device management                 | §58 Phase 7 | not started                                  |
| (later) Optional advanced features             | §59 Phase 8 | not planned - only after the core is stable  |

---

## Phase 0 - Windows input research (§51)

Research is done: `docs/research/phase-0-windows-input.md`, `docs/research/fail-safe-matrix.md`, ADRs
0001-0007 in `docs/decisions/`. The §51 prototype (detect → lock → emergency unlock → unlock) is M1.

### M0 - Scaffold and tooling

Status: **done** (CI passed on the pushed branch, 2026-10-03).
Manual test: none of its own; M1.md step 1
confirms both targets build and start.

Scope: Cargo workspace (`keyclean-core`, `keyclean-win`, `src-tauri`), `panic = "abort"` profiles, bun +
Vite + React + TS strict + Tailwind v4, ESLint and Prettier, commitlint + husky + lint-staged + merlin,
i18n (`locales/en/strings.json`, typed `t()`, Rust loader), CI on `windows-latest`, repo docs (README,
CHANGELOG, CONTRIBUTING, SECURITY, CODE_OF_CONDUCT, Apache-2.0 LICENSE), git workflow doc.

Acceptance:

- [x] `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace` pass
- [x] `bun run typecheck`, `bun run lint`, `bun run format:check` pass
- [x] CI passes on the pushed branch
- [x] Commit hooks run (commitlint, lint-staged, pre-push gate)
- [x] App launches (`bun tauri dev`) - §60 "Launches successfully"

### M1 - Keyboard lock, emergency unlock, hard deadline (§62)

Status: **done** (verified by Max, 2026-10-03). Verification: `cargo run -p keyclean-e2e`
(automated harness, ADR 0008) plus the manual part of `docs/testing/manual/M1.md`.

Automated harness: **12/12 passed** on Max's machine (Windows 11 25H2), 2026-10-02. Part B results
(2026-10-02, recorded in M1.md): Win+L, Ctrl+Alt+Del, elevated window, sleep and On-Screen Keyboard
behave as documented; Win+G and Fn/brightness get through and are documented as unblockable; Sticky
Keys not tested.

**Focus bug found and fixed in code:** with KeyClean's own window focused, Windows sometimes skipped
the hook (Win opened Start, Ctrl+Alt+K needed several tries). The engine now runs in its own process
(ADR 0009, `docs/research/webview-focus-hook.md`). Harness on the engine-process build, 2026-10-03:
**13/13 passed** (S13 is opt-in and wasn't run), including S14 (Ctrl+Alt+K with the app window
focused, 19 ms) and the new no-leftover-process checks in S10/S11. Focus checks F1/F3 with real keys
passed on 2026-10-03. Win+G re-checked with Notepad focused still gets through, so it stays documented
as unblockable.

Scope: `keyclean-core` (session state machine, policy, chord, key tracking, fake clock - all
unit-tested); `keyclean-win` engine thread with `WH_KEYBOARD_LL` hook, hidden notification window,
watchdog, system-transition unlocks, keyboard enumeration, run by the app as a separate engine process
(ADR 0009); `lock_smoke` example; bare Tauri shell (Lock
button, status line, DEV CAP badge, keyboard list); end-to-end harness `keyclean-e2e` with the
`testkit` feature. Hard deadline, watchdog and debug dev cap are part of
M1 so no build can lock indefinitely.

Acceptance (§62 and §60 "Input"):

- [x] Press a button → keyboard input stops → Ctrl+Alt+K → keyboard works again immediately
- [x] Keyboard can be locked
- [x] Keyboard can be unlocked
- [x] Emergency shortcut still works, also with KeyClean's own window focused (S14, F1/F3)
- [x] Input returns to normal after unlock (no stuck keys)
- [x] Safety timeout works (dev cap releases without the chord)
- [x] Forced app termination tested (M1.md step 8)
- [x] Sleep during a lock leaves KeyClean idle after wake (M1.md step 12)
- [x] No keystrokes stored or logged (§60 "Privacy")

## Phase 1 - Core lock engine (§52)

### M2 - Timer and automatic unlock

Status: **done** (verified by Max, 2026-10-03). Verification: `cargo run -p keyclean-e2e` plus Parts
B and C of `docs/testing/manual/M2.md`.

Harness, 2026-10-03: S1-S14 and S16 passed (S16: the hard deadline alone released a 3 s lock after
13.01 s). S15 first failed with a single countdown status because the harness ran a `keyclean.exe`
built before M2 (`cargo run -p keyclean-e2e` doesn't rebuild the app); the harness now skips the app
checks when the exe is older than its sources. Re-run on a fresh build: S15 passed (5..1, worst tick
7 ms off, released after 5.01 s). Manual Parts B and C all passed, including the 30 s, 1 min, 2 min and
5 min presets in a release build against a stopwatch, the 5 min lock with KeyClean in the background,
Unlock now, Ctrl+Alt+K, a kill mid-lock, and idle CPU after a lock.

The first manual run also found that the window couldn't call the new commands (missing from
`build.rs` and the capability); fixed, with a test that keeps the three lists in sync.

Done in code: presets 30 s / 1 min / 2 min / 5 min (default 2 min) in `keyclean-core::presets`, the
app rejects any other duration; the engine emits a status each time the displayed second changes
(`TIMER_COUNTDOWN`, rounding in `keyclean-core::countdown`), never at idle or while draining; **Unlock
now** button (`UserRequest`); "Cleaning complete" after a `Timeout`. The max-lock setting stays at its
30 min default until settings land (M7).

Scope: duration presets (§11), countdown events from the engine, accurate monotonic session timer
(QPC), automatic unlock (§16), "unlock now" request from the UI. Release builds allow real durations up
to the max-lock setting (60 min absolute ceiling, ADR 0006).

Acceptance (§60 "Timer"):

- [x] Preset durations work (M2.md steps 6-9, release build)
- [x] Countdown is accurate (S15; M2.md steps 6-7 against a stopwatch)
- [x] Automatic unlock works (M2.md steps 4, 6-8)
- [x] Safety timeout works (S16; dev cap in step 4)

## Phase 2 - Safety (§53)

### M3 - Safety hardening

Status: **done** (verified by Max, 2026-10-04). Verification: `cargo run -p keyclean-e2e`
(S17-S24, plus `--stall` for S20 and `--raw-diag` for S25) and Parts B and C of
`docs/testing/manual/M3.md`.

Harness, 2026-10-04: **22/22 passed**, plus S20 and S25 run once each. The first run found two bugs:

- **S19:** the watchdog's `abort()` went through Windows Error Reporting, which kept a hung engine,
  and its hook, about 5 s longer. It now uses `TerminateProcess`.
- **S17:** S6's synthesized Win+G opened the Xbox Game Bar. While its overlay is open, Windows
  delivers no Raw Input, so the lost-hook check is blind. Win+G left S6, and the limitation is
  documented in ADR 0010.

Manual Parts B and C, 2026-10-04: all passed except the skipped optional steps (3, sign-out and
session disconnect; 10, drift under load).

- **Shutdown and restart:** neither was delayed, and KeyClean was idle afterwards.
- **Sleep** (S3 standby): ended as "the computer was locked".
- **Others that passed:** the clock jump of ±1 h, keyboard unplug and replug, killing the engine,
  `taskkill` without `/F`, and the physical Win+G.
- **Admin window (step 7):** typing into an elevated window was **blocked**. In M1 it got through,
  before the engine moved to its own process.

Done in code:

- **Lost-hook check (ADR 0010).** During a session only, a Raw Input sink on the engine window
  (`RIDEV_INPUTSINK | RIDEV_DEVNOTIFY`) is compared with the hook's last-callback time. A lost hook
  ends the lock with `error.hook_lost`.
  Known limitation: while the Xbox Game Bar overlay is open (Win+G can't be blocked), Windows
  delivers no raw input, so the check is blind; the lock itself keeps working (harness S25).
- **Admin windows.** When an elevated window has focus, the lock stays on with a warning (Max's
  decision, 2026-10-04).
- **Keyboard notices.** Connecting or disconnecting a keyboard during a lock shows a notice and
  re-lists keyboards. The lock continues (ADR 0004).
- **Engine restart (ADR 0011).** A dead engine process is restarted idle, at most 3 times in 5
  minutes.
- **Clean `taskkill`.** `taskkill` without `/F` now exits the app cleanly: tao's and the
  single-instance plugin's helper windows are subclassed so `WM_CLOSE` means "exit".
- **Fault injection.** Testkit faults for a lost hook, a failed install, a hung engine thread and a
  real hook timeout.
- **Verification only.** Shutdown, restart, sign-out, switch user, sleep, clock changes and drift
  needed no new code. Each has a manual step.
- **Research.** `docs/research/m3-safety.md`.

Scope: silent hook removal detection (Raw Input sink compared with the hook's last callback, Max's
decision to defer from M1); sleep/wake, shutdown, restart and session switch tested end to end; device
disconnect/reconnect during a lock (§26, §47); timer drift; system clock changes; crash recovery; every
"Phase 2 (M3)" row in the fail-safe matrix; a `WM_CLOSE` broadcast to the app's windows (e.g.
`taskkill` without `/F`) currently hangs the UI (the lock still ends): make the app exit cleanly
instead.

Acceptance (§60 "Safety"):

- [x] Forced app termination tested
- [x] Sleep/wake tested
- [x] Shutdown tested
- [x] Restart tested
- [x] Device disconnect tested
- [x] Device reconnect tested
- [x] Silent hook removal is detected and reported (input released, user told)
- [x] Clock change does not change the lock length

## Phase 1 (continued) - devices and pointing input (§52, §22-§24)

### M4 - Device detection

Status: **done** (verified by Max, 2026-10-04). Verification: `cargo run -p keyclean-e2e` (Part A) and
the device list in `docs/testing/manual/M4.md` (Part B). Step 13 (lock and unplug during a lock) was
not run; Max accepted M4 without it.

Done in code:

- **Device model (`keyclean-core::devices`).** Pure, unit-tested: Keyboard and Mouse are Supported,
  a precision touchpad (HID 0x0D/0x05) is Limited, touchscreens and pens are Unsupported, other HID
  collections are ignored. A touchpad's companion mouse collection (same parent device node) folds
  into it, and the same for touchscreens and pens. Same-kind entries in one external container fold
  into one row; built-in devices never fold by container. Nothing else is merged.
- **Enumeration (`keyclean-win::devices::input_devices`).** Raw Input list plus `RIDI_DEVICEINFO`
  usage, friendly names from the configuration manager. Replaces `list_keyboards`.
- **Live updates (ADR 0012).** `CM_Register_Notification` for the keyboard, mouse and HID interface
  classes, in the app process (`keyclean-win::DeviceWatch`), no input registration. The app waits
  for 400 ms of quiet, lists again, and emits `devices-changed` only when the list changed.
- **Window.** Devices section grouped by kind, a capability label per device, plain-language notes
  for Limited and Unsupported, and the line that mice and touchpads aren't locked yet. The
  `list_keyboards` command became `list_devices`.
- **Harness.** S1 counts keyboards among all devices; S22 now runs with the device watch active.
- **Research.** `docs/research/m4-devices.md`.

Harness, 2026-10-04: **22/22 passed** on Max's machine. S1 listed 4 keyboards among 8 devices.
First Part B list: 8 rows for what is really a laptop plus two USB devices. A USB keyboard
(two keyboard interfaces, one mouse interface) showed 3 rows, so entries of the same kind in one
external container now fold into one row, and external devices are named by what their bus reports
("HyperX Alloy Origins") instead of "HID Keyboard Device" (research note, "Duplicates and names"). S22 passed with
the device watch running, so watching registers no input (invariant 4). Part B: all steps passed after the folding fix, except 13, which was skipped.

Not verified on hardware: Bluetooth names (no Bluetooth device was tried), the container fold on other
machines, and touchscreens and pens (none to test; the Unsupported label is covered by unit tests only).

Scope: live keyboard / mouse / touchpad list (§24), arrival and removal via
`CM_Register_Notification` in the app (ADR 0012; Raw Input stays unregistered at idle), capability
model per device (ADR 0004). Still informational: every keyboard is locked.

Acceptance:

- [x] Connected devices are listed with friendly names (M4.md steps 2-3)
- [x] Plugging and unplugging updates the list without restarting (M4.md steps 3-6)
- [x] Each device shows Supported / Limited / Unsupported (M4.md steps 2, 9, 10)
- [x] Idle KeyClean still registers for no input (invariant 4): harness S22 with the device watch
      running, M4.md Part A

### M5 - Mouse and touchpad lock (§22, §23)

Status: **done** (verified by Max, 2026-10-05). Verification: `cargo run -p keyclean-e2e` (S26-S32,
S22), `--mouse-diag` (S33), and Part B of `docs/testing/manual/M5.md`. Optional step 8 passed (Tab
and Enter reach Unlock now in a mouse-only lock); steps 9-12 weren't run.

Harness, 2026-10-05: **29/29 passed** in 126 s on Max's machine, including S26-S32. S33 with the
touchpad and the Logitech mouse: 1405 raw mouse messages, 0 mouse liveness misses, nothing past the
lock. Part B: all steps passed.

**Touchpad finding (M5 step 7):** on the ELAN1203 precision touchpad, pointer movement, taps and
clicks are blocked, but two-finger scrolling, pinch, and three- and four-finger swipes still work
during a lock. They never pass the low-level hook, and a user-mode hook can't stop them (disabling the
touchpad is forbidden by invariant 2). They also never tripped the lost-hook check. The touchpad
note in the window now names those gestures.

Done in code (ADR 0013, research note `docs/research/m5-mouse.md`):

- **Targets.** A lock blocks the keyboard, the mouse and touchpad, or both. The window has two
  checkboxes, Keyboard on and Mouse and touchpad off by default (Max's decision, 2026-10-05: locking
  the mouse removes the clickable Unlock now button). `lock_keyboard` became `lock_input`.
- **Mouse hook.** `WH_MOUSE_LL` on the engine thread, only when the mouse is locked. While locked it
  blocks movement, every button, both wheels and unknown mouse messages. Held buttons drain like keys
  (`keyclean-core::mouse`). Injected mouse input is blocked too.
- **Exits.** The keyboard hook is installed for every lock, so Ctrl+Alt+K works in a mouse-only lock.
  There it lets keys through and swallows only the key that completes the chord (Max's decision).
  Both hooks enforce the hard deadline. A failed mouse hook install removes the keyboard hook again.
- **Lost-hook check.** Mouse Raw Input during a mouse lock; each `WM_INPUT` is told apart by its
  header only (ADR 0010 amended). A miss ends the lock with "mouse lock stopped early" (Max's
  decision), unless the window under the cursor is elevated.
- **Notices and status.** Device notices say keyboard or mouse; the status line and notices follow
  what the engine says is locked.
- **Harness.** Tagged mouse probes and a mouse observer that swallows leaking probe clicks; S26-S32,
  S22 after a mouse lock, opt-in S33 `--mouse-diag`; `lock_smoke --mouse`.

Scope: `WH_MOUSE_LL` on the same engine thread, full mouse lock (§22) with the same four exits, touchpad
handled through the capability model (Supported / Limited / Unsupported, ADR 0004). Emergency chord
stays keyboard-based.

Acceptance:

- [x] Selected input is blocked (§60 "Input")
- [x] Mouse lock releases on every exit (timer, chord, hard deadline, process death)
- [x] Touchpad shows the right capability level and behaves as documented

## Phase 3 - MVP UI (§54)

### M6 - Full-screen overlay

Status: **code-complete - awaiting manual verification**. Verification: `cargo run -p keyclean-e2e`
(S14, S34-S40), `--overlay-latency` (S41) and Part B of `docs/testing/manual/M6.md`. Design:
`docs/design/overlay.png` (see `docs/design/README.md`).

Harness, 2026-10-07: **35 passed, 1 failed** in 191 s. S23 failed: `taskkill` without `/F` reached
only the overlay, so the lock ended (input back after 4 ms) but the app kept running. Fixed: a close
request on the overlay now ends the lock and exits KeyClean (Max's decision); S23 and S37 need a
re-run. S34 confirmed 769 ms after the request, S14 passed with the overlay focused. Latency (S41):
cold 393 ms, warm p50 260 ms, p95 283 ms; release build 216-253 ms, so ADR 0001's criterion is met on
Windows 11 without a pre-created window. Manual Part B: all steps passed. Touchpad gestures (Task
View, Show desktop, four-finger swipe) never ended a lock: the overlay stayed on its monitor, and
after Show desktop the taskbar showed over it. Optional steps 9 (second monitor) and 10 (release
latency) passed; step 11 (Windows 10) was skipped.

Max's decisions (plan approved 2026-10-06): the monitor under the cursor; any overlay loss ends the
lock, a virtual desktop switch too; Unlock now hidden while the mouse is locked; the system monospace
font; the main window comes back after unlock; a heartbeat catches a crashed or hung page.

Done in code (ADR 0014, research note `docs/research/m6-overlay.md`):

- **Overlay first, then the lock.** The app builds the overlay hidden on the monitor under the
  cursor, sizes it in physical pixels, shows it, and asks the engine to lock only after the page
  confirms it is visible, within 2 s of the request. Otherwise nothing is locked and the window
  says so. This reverses §12's order (ADR 0014). The engine and its protocol are unchanged.
- **Watched during the lock.** On every countdown status the app checks the overlay: closed,
  hidden, minimized, on another virtual desktop, or silent for 3 s ends the lock ("its lock screen
  was closed or hidden"). Nothing runs at idle.
- **Overlay page** (`overlay.html`, `src/ui/overlay/`): countdown with segmented progress, locked
  devices from the engine's status, Ctrl + Alt + K key caps, "Unlocking in N seconds" in the last
  5 s, DEV CAP, the current notice; Unlock now only when the mouse is free.
- **End of the lock.** The overlay is destroyed at `Unlocking`, and on a refused request or a dead
  engine; the main window comes back. Closing the main window still exits.
- **Harness.** S14 now sends Ctrl+Alt+K with the overlay focused; S34-S40 check the order, the
  timer end, a missing confirmation, and closing, hiding, minimizing or silencing the overlay;
  opt-in S41 measures latency. The harness exe embeds the UI
  (`cargo build -p keyclean --features tauri/custom-protocol`).

Not verified: Windows 10 latency (no machine), a second monitor and mixed scaling, large monitors.

Scope: overlay on **the monitor under the cursor first** (Max's decision; all monitors later). Order:
show the overlay, wait for the webview's ack (after a double `requestAnimationFrame`), then engage the
lock; if the overlay isn't confirmed visible within ~2 s, don't lock and show an error (invariant 11).
Build hidden → `set_position`/`set_size` in physical pixels → `show()`; avoid `fullscreen(true)` on
secondary monitors. **Measure WebView2 create-to-visible latency on Windows 10 and 11** - if it can't
meet the rule even with a pre-created hidden window, revisit ADR 0001.

Acceptance (§60 "Overlay"):

- [ ] Full-screen overlay appears
- [ ] Countdown is visible
- [ ] Locked devices are visible
- [ ] Emergency shortcut is visible
- [ ] Overlay disappears after unlock
- [ ] Never locked without a confirmed-visible overlay

### M7 - MVP dashboard

Status: not started. Manual test: `docs/testing/manual/M7.md`. Design: `docs/design/clean-ready.png`
(see `docs/design/README.md`).

Scope: dashboard (§9), clean screen and device selection (§10, §25 reduced to keyboard/mouse on-off per
ADR 0004), duration selector, Lock button, unlock state, plain-language errors with "View technical
details" (§46), elevated-window warning (ADR 0005). Strings from `locales/en/strings.json`, layouts
tolerate 2× text.

Acceptance (§60 "Application"):

- [ ] Has a simple dashboard
- [ ] Can start a cleaning session
- [ ] Errors are plain language with technical details

## Phase 4 - Windows integration (§55)

### M8 - System tray

Status: not started. Manual test: `docs/testing/manual/M8.md`.

Scope: tray icon and menu (§27); zero windows at idle (`app.windows: []`, `prevent_exit()` only when the
exit code is `None`); closing the dashboard destroys its webview.

Acceptance:

- [ ] Runs in system tray (§60 "Application")
- [ ] System tray works (§60 "Windows")
- [ ] Idle cost near zero with the dashboard closed

### M9 - Global shortcut

Status: not started. Manual test: `docs/testing/manual/M9.md`.

Scope: configurable start shortcut (§28) via the official global-shortcut plugin; keys held from the
shortcut leave nothing stuck (invariant 8).

Acceptance:

- [ ] Global shortcut starts a session
- [ ] No stuck keys from the start shortcut

### M10 - Start with Windows and notifications

Status: not started. Manual test: `docs/testing/manual/M10.md`.

Scope: autostart plugin (§29), notifications for session start/end and permission problems (§31)
(toasts only work in installed builds).

Acceptance:

- [ ] Startup behavior works
- [ ] Notifications work

### M11 - Packaging

Status: not started. Manual test: `docs/testing/manual/M11.md`.

Scope: NSIS installer, `installMode: currentUser`, WebView2 `downloadBootstrapper`, uninstaller.

Acceptance:

- [ ] Windows 10 works
- [ ] Windows 11 works
- [ ] Install and uninstall leave no lock state or leftovers

## Phase 5 - Open-source release (§56)

### M12 - Open-source release

Status: not started. Manual test: `docs/testing/manual/M12.md` (release checklist).

Scope: README with screenshots, install and development instructions, architecture docs, contributing
guide, security policy, license, issue and feature-request templates, changelog; first public release.

Acceptance:

- [ ] Every §60 box is ticked
- [ ] No keystrokes stored, no typed content stored, no telemetry, no account, no cloud (§60 "Privacy")
- [ ] First public release published

## Phase 6 - Keyboard diagnostics (§57)

### M13 - Keyboard diagnostics

Status: not started. Manual test: `docs/testing/manual/M13.md`.

Scope: key tester (§33, §34), visual keyboard, stuck-key detection (§35), scan-code information,
diagnostic report (§36). Real-time display only, no history (privacy invariants).

Acceptance:

- [ ] Every key press shows in real time and nothing is kept afterwards
- [ ] Stuck keys are detected
- [ ] Report contains no typed content

## Phase 7 - Advanced device management (§58)

### M14 - Advanced device management

Status: not started. Manual test: `docs/testing/manual/M14.md`.

Scope: per-device locking (only after a prototype proves hook / `WM_INPUT` ordering, ADR 0004), device
profiles, reconnect behavior, aliases, capability detection, per-device settings.

Acceptance:

- [ ] Prototype results recorded in an ADR before implementation
- [ ] Locking one keyboard leaves the others working
- [ ] Hook callback stays O(1)

## Phase 8 - Optional advanced features (§59)

Not planned. Only after the core product is stable, and only if it passes §65.

---

## Open items

- **License:** Apache-2.0 chosen by Max; `LICENSE` added, `license = "Apache-2.0"` in every Cargo.toml
  and package.json.
- **Hardware verification:** the [needs prototype] items in `docs/research/phase-0-windows-input.md`
  ("Needs hardware verification") are recorded through M1.md - shortcut swallowing, Win+L, Ctrl+Alt+Del
  after Cancel, elevated window, AltGr, Sticky Keys, On-Screen Keyboard, sleep, and Windows 10.
- **Chord held after unlock:** M1.md step 3 confirms that holding Ctrl+Alt+K for several seconds after
  unlocking types nothing (the drain stays while blocked keys repeat, up to 30 s and never past the hard deadline).
- **E2E on CI:** `.github/workflows/e2e.yml` (manual trigger) checks whether hosted Windows runners can
  run the harness. If they can, consider running it on PRs.
- **Repo setup:** done 2026-10-03. `main` and `develop` are protected like Max's other repos: PRs
  only, one code-owner review (`.github/CODEOWNERS`), checks `checks` and `pr-title` up to date,
  linear history, resolved conversations, no force-push or deletion, admins may bypass. Squash merge
  only; branches are deleted after merge. Claude changes repo settings only when Max asks.
