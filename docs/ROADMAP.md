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

| Milestone                                      | Phase       | Status                                           |
| ---------------------------------------------- | ----------- | ------------------------------------------------ |
| M0 Scaffold and tooling                        | §51 Phase 0 | **code-complete - awaiting manual verification** |
| M1 Keyboard lock + emergency unlock + deadline | §51-§52     | **code-complete - awaiting manual verification** |
| M2 Timer and automatic unlock                  | §52 Phase 1 | not started                                      |
| M3 Safety hardening                            | §53 Phase 2 | not started                                      |
| M4 Device detection                            | §52 Phase 1 | not started                                      |
| M5 Mouse and touchpad lock                     | §52 Phase 1 | not started                                      |
| M6 Full-screen overlay                         | §54 Phase 3 | not started                                      |
| M7 MVP dashboard                               | §54 Phase 3 | not started                                      |
| M8 System tray                                 | §55 Phase 4 | not started                                      |
| M9 Global shortcut                             | §55 Phase 4 | not started                                      |
| M10 Start with Windows + notifications         | §55 Phase 4 | not started                                      |
| M11 Packaging                                  | §55 Phase 4 | not started                                      |
| M12 Open-source release                        | §56 Phase 5 | not started                                      |
| M13 Keyboard diagnostics                       | §57 Phase 6 | not started                                      |
| M14 Advanced device management                 | §58 Phase 7 | not started                                      |
| (later) Optional advanced features             | §59 Phase 8 | not planned - only after the core is stable      |

---

## Phase 0 - Windows input research (§51)

Research is done: `docs/research/phase-0-windows-input.md`, `docs/research/fail-safe-matrix.md`, ADRs
0001-0007 in `docs/decisions/`. The §51 prototype (detect → lock → emergency unlock → unlock) is M1.

### M0 - Scaffold and tooling

Status: **code-complete - awaiting manual verification**. Manual test: none of its own; M1.md step 1
confirms both targets build and start.

Scope: Cargo workspace (`keyclean-core`, `keyclean-win`, `src-tauri`), `panic = "abort"` profiles, bun +
Vite + React + TS strict + Tailwind v4, ESLint and Prettier, commitlint + husky + lint-staged + merlin,
i18n (`locales/en/strings.json`, typed `t()`, Rust loader), CI on `windows-latest`, repo docs (README,
CHANGELOG, CONTRIBUTING, SECURITY, CODE_OF_CONDUCT, Apache-2.0 LICENSE), git workflow doc.

Acceptance:

- [ ] `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace` pass
- [ ] `bun run typecheck`, `bun run lint`, `bun run format:check` pass
- [ ] CI passes on the pushed branch
- [ ] Commit hooks run (commitlint, lint-staged, pre-push gate)
- [ ] App launches (`bun tauri dev`) - §60 "Launches successfully"

### M1 - Keyboard lock, emergency unlock, hard deadline (§62)

Status: **code-complete - awaiting manual verification**. Verification: `cargo run -p keyclean-e2e`
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

- [ ] Press a button → keyboard input stops → Ctrl+Alt+K → keyboard works again immediately
- [ ] Keyboard can be locked
- [ ] Keyboard can be unlocked
- [ ] Emergency shortcut still works, also with KeyClean's own window focused (S14, F1/F3)
- [ ] Input returns to normal after unlock (no stuck keys)
- [ ] Safety timeout works (dev cap releases without the chord)
- [ ] Forced app termination tested (M1.md step 8)
- [ ] Sleep during a lock leaves KeyClean idle after wake (M1.md step 12)
- [ ] No keystrokes stored or logged (§60 "Privacy")

## Phase 1 - Core lock engine (§52)

### M2 - Timer and automatic unlock

Status: not started. Manual test: `docs/testing/manual/M2.md`.

Scope: duration presets (§11), countdown events from the engine, accurate monotonic session timer
(QPC), automatic unlock (§16), "unlock now" request from the UI. Release builds allow real durations up
to the max-lock setting (60 min absolute ceiling, ADR 0006).

Acceptance (§60 "Timer"):

- [ ] Preset durations work
- [ ] Countdown is accurate
- [ ] Automatic unlock works
- [ ] Safety timeout works

## Phase 2 - Safety (§53)

### M3 - Safety hardening

Status: not started. Manual test: `docs/testing/manual/M3.md`.

Scope: silent hook removal detection (Raw Input sink compared with the hook's last callback, Max's
decision to defer from M1); sleep/wake, shutdown, restart and session switch tested end to end; device
disconnect/reconnect during a lock (§26, §47); timer drift; system clock changes; crash recovery; every
"Phase 2 (M3)" row in the fail-safe matrix; a `WM_CLOSE` broadcast to the app's windows (e.g.
`taskkill` without `/F`) currently hangs the UI (the lock still ends): make the app exit cleanly
instead.

Acceptance (§60 "Safety"):

- [ ] Forced app termination tested
- [ ] Sleep/wake tested
- [ ] Shutdown tested
- [ ] Restart tested
- [ ] Device disconnect tested
- [ ] Device reconnect tested
- [ ] Silent hook removal is detected and reported (input released, user told)
- [ ] Clock change does not change the lock length

## Phase 1 (continued) - devices and pointing input (§52, §22-§24)

### M4 - Device detection

Status: not started. Manual test: `docs/testing/manual/M4.md`.

Scope: live keyboard / mouse / touchpad list (§24), arrival and removal via `WM_INPUT_DEVICE_CHANGE`
(`RIDEV_DEVNOTIFY` on the hidden window), capability model per device (ADR 0004). Still informational:
every keyboard is locked.

Acceptance:

- [ ] Connected devices are listed with friendly names
- [ ] Plugging and unplugging updates the list without restarting
- [ ] Each device shows Supported / Limited / Unsupported
- [ ] Idle KeyClean still registers for no input (invariant 4)

### M5 - Mouse and touchpad lock (§22, §23)

Status: not started. Manual test: `docs/testing/manual/M5.md`.

Scope: `WH_MOUSE_LL` on the same engine thread, full mouse lock (§22) with the same four exits, touchpad
handled through the capability model (Supported / Limited / Unsupported, ADR 0004). Emergency chord
stays keyboard-based.

Acceptance:

- [ ] Selected input is blocked (§60 "Input")
- [ ] Mouse lock releases on every exit (timer, chord, hard deadline, process death)
- [ ] Touchpad shows the right capability level and behaves as documented

## Phase 3 - MVP UI (§54)

### M6 - Full-screen overlay

Status: not started. Manual test: `docs/testing/manual/M6.md`.

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

Status: not started. Manual test: `docs/testing/manual/M7.md`.

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
- **Repo setup (Max):** push `main` and `develop` once, then enable branch protection for both in GitHub
  settings. Claude doesn't change repo settings.
