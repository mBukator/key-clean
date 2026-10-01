# KeyClean — session 1 kickoff (Phase 0 + Milestone 1)

ultrathink.

Read @docs/SPEC.md in full. `CLAUDE.md` is already loaded; its safety and privacy invariants override anything in this prompt.

## Goal of this session

Finish spec Phase 0 (§51) and the first milestone (§62):

> Press Lock → keyboard input stops → press Ctrl+Alt+K → keyboard works again immediately.

Milestone 1 already includes the engine's hard deadline, watchdog, and debug dev cap, so no build can ever lock indefinitely. The user-facing timer and countdown come in Milestone 2.

Not in this session: overlay, tray, dashboard design, mouse/touchpad locking, per-device blocking, packaging, license choice, any network code (§61: don't start with the UI).

## Hard stops

1. **After the environment check**, if anything required is missing.
2. **After presenting the plan** (research findings + implementation plan). Wait for my approval. If you're in plan mode, do the environment check and research read-only and put everything into the plan.
3. **After Milestone 1 is code-complete.** Give me the manual test script and wait for my results. Do not start Milestone 2.

## Step 1 — Environment check

Report as a table (tool · version found · OK/missing):

- Native Windows, not WSL. If WSL: stop — Windows builds and hook testing need native Windows. Include Windows edition/build and CPU architecture.
- rustup, rustc, cargo (stable, `x86_64-pc-windows-msvc`), Visual Studio Build Tools with the C++ workload and Windows SDK, Node.js LTS, pnpm, git, WebView2 Runtime.
- For anything missing, give me the exact install command (winget where possible). Don't install anything yourself.

## Step 2 — Phase 0 research

Use subagents in parallel to keep this context clean — suggested split: (A) input interception, (B) system events and failure safety, (C) Tauri 2. Verify against official docs and tag each finding [docs], [needs prototype], or [assumption].

**A. Input interception**

1. `WH_KEYBOARD_LL` / `WH_MOUSE_LL`: thread and message-loop requirements; `LowLevelHooksTimeout` and silent hook removal — how to detect or mitigate it, and what the user would experience.
2. What user mode cannot block (Ctrl+Alt+Del for certain; check Win+L and other OS-reserved combos). These become documented escape hatches, not bugs.
3. Key-state consistency: modifier tracking inside the hook, AltGr, key repeat, keys already held when the lock starts, and how to guarantee no stuck keys after unlock.
4. UIPI / integrity levels: what happens when an elevated window (e.g. Task Manager) has focus during a lock; trade-offs of running elevated or with `uiAccess`. Recommend a default.
5. Injected input (`LLKHF_INJECTED` / `LLMHF_INJECTED`): block or pass? Consider on-screen keyboard, accessibility tools, remote desktop.
6. Device identity: enumerating keyboards, mice, and touchpads via Raw Input with friendly names; correlating Raw Input device handles with hook events for per-device blocking (V1), including ordering/timing pitfalls; compare the Interception driver (install friction, licensing, failure mode). Recommend the V1 approach — the MVP locks all keyboards.
7. Precision touchpads: how their input reaches the mouse hook and whether it can be told apart from a mouse. Define the capability model (Supported / Limited / Unsupported, §23).

**B. System events and failure safety**

8. Which window/thread receives `WM_POWERBROADCAST`, `WM_QUERYENDSESSION`/`WM_ENDSESSION`, `WM_WTSSESSION_CHANGE`, `WM_INPUT_DEVICE_CHANGE`; confirm the hidden-window design in CLAUDE.md.
9. Confirm hooks are released on normal exit, crash, panic/abort, and forced termination (Task Manager, `Stop-Process -Force`). What happens if only the engine thread dies?
10. Monotonic time on Windows (`std::time::Instant` / QPC) across sleep and wall-clock changes — even though our policy is "suspend = unlock".

**C. Tauri 2**

11. Tray-only runtime with zero windows (not exiting when the last window closes); single-instance; global shortcut; autostart; notifications; settings store; per-window capability scoping; NSIS vs MSI bundling; WebView2 on Windows 10.
12. Full-screen always-on-top overlay: creation latency, one window per monitor vs active monitor only, DPI scaling, focus behavior. Recommend an approach for the overlay milestone.

## Step 3 — Present the plan (hard stop 2)

- Key findings per question, short, with links.
- The files you'll write after approval: `docs/research/phase-0-windows-input.md`; `docs/research/fail-safe-matrix.md` (every failure case in §47, plus panic and silent hook removal → mechanism → expected result → manual test); ADRs 0001 stack (Tauri 2, and which finding would make us switch), 0002 blocking mechanism, 0003 system-transition policy, 0004 per-device strategy, plus any others the research calls for.
- The implementation plan for Steps 4–8, adjusted by what you found.
- Open questions for me — at least: license (§43, don't pick one), injected-input policy, overlay on all monitors vs active monitor.

## Step 4 — Scaffold (after approval)

- `git init`, `.gitignore`, Cargo workspace and pnpm project laid out exactly as in CLAUDE.md.
- Tauri 2 + React + TypeScript + Vite + Tailwind v4; crates `keyclean-core` and `keyclean-win`.
- i18n: `locales/en/strings.json`, a small typed `t()` for React, and a Rust loader for tray/notification strings. Only the strings M1 needs for now.
- Tooling: rustfmt, clippy, strict TS, lint and format config, package scripts. If my user-level CLAUDE.md defines TypeScript conventions, follow them.
- GitHub Actions on `windows-latest`: fmt check, clippy `-D warnings`, `cargo test`, typecheck, lint. No release workflow yet.
- Repo docs: README stub with the §42 tagline; `CHANGELOG.md` (Keep a Changelog); `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md` stubs. No `LICENSE` until I choose.
- Fill in the Commands section of CLAUDE.md. Commit.

## Step 5 — `keyclean-core` (pure, fully unit-tested)

- §48 state machine with typed events and end reasons: `Timeout`, `Emergency`, `HardDeadline`, `SystemTransition`, `EngineError`, `UserRequest`. Illegal transitions return errors, never panic.
- `Clock` trait plus fake clock; deadline math on monotonic durations; hard deadline = min(max lock, duration + 10 s); debug-build dev cap (15 s lock / 20 s hard deadline).
- Emergency-chord detector as a pure function over (key event, tracked modifier state): left/right Ctrl and Alt, AltGr, key repeat, K pressed before the modifiers, modifiers released mid-chord.
- Tests for every transition, deadline edge, and chord case. Commit.

## Step 6 — `keyclean-win` engine (Milestone 1, keyboard only)

- Engine thread with message loop, hidden top-level window, and watchdog thread, as described in CLAUDE.md.
- Hook installed on lock and removed on unlock. Callback: past hard deadline → pass and request teardown; emergency chord → flip the lock flag atomically, swallow the chord, request teardown; anything else → block (letters, numbers, F-keys, modifiers, navigation, Win, Alt+Tab, Ctrl/Alt combos — §21).
- No stuck keys after unlock (invariant 8).
- Suspend, shutdown/logoff, session lock/switch → unlock. Basic wiring now; hardening is Phase 2.
- Keyboard detection: list connected keyboards (name and id only) via Raw Input.
- Public API along the lines of `Engine::start()`, `lock(LockRequest)`, `unlock(reason)`, `devices()`, and an event receiver. No key data ever leaves the crate.
- `crates/keyclean-win/examples/lock_smoke.rs`: prints detected keyboards, counts down 3 s, locks for the dev-capped duration, prints the end reason. **Write it; don't run it.** Commit.

## Step 7 — Bare Tauri shell

- One unstyled window: a Lock button (10 s in debug), a status line driven by engine events, and the detected keyboard list. No overlay, tray, or styling.
- The engine lives in Rust app state, fully independent of the webview. Give the window only the capabilities it needs. Commit.

## Step 8 — Handoff (hard stop 3)

- `docs/ROADMAP.md`: milestones in §61 order, grouped by spec phases (§51–§59), each with scope, acceptance criteria taken from §60, and its manual test file. Mark M0 and M1 "code-complete — awaiting manual verification". Future sessions start from this file.
- `docs/testing/manual/M1.md`, runnable against both `lock_smoke` and the Tauri shell, covering at least: lock → Ctrl+Alt+K unlock; dev-cap auto-release without the chord; recording which of Win, Alt+Tab, Ctrl+Esc, Ctrl+Shift+Esc, and Win+L get blocked; Ctrl+Alt+Del still reachable (documented escape hatch); forced kill mid-lock via a pre-armed PowerShell command; starting a lock while holding modifiers; no stuck modifiers afterwards; an elevated window focused during a lock; sleep during a lock.
- Final message: what's done, the exact commands for me to run, risks you found, and your questions. Then stop.
