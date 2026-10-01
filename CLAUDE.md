# KeyClean — project instructions for Claude Code

KeyClean is a small open-source Windows 10/11 (x64) utility that temporarily locks keyboard and pointing-device input so the owner can clean or test devices — and always hands control back.

- Product spec: `docs/SPEC.md` (sections cited as §N). Read the sections relevant to a task before planning it.
- Current milestone and status: `docs/ROADMAP.md` — read it at the start of every session.
- Decisions: `docs/decisions/` (ADRs). Research: `docs/research/`. Manual test scripts: `docs/testing/manual/`.
- Product rule (§65): if a change doesn't make KeyClean better at safely controlling, cleaning, or diagnosing input devices on Windows, don't build it. No system-optimizer features (§2.1).

Owner: Max — frontend developer (TypeScript, React, Tailwind v4, Motion). Explain Rust and Win32 decisions in plain language in ADRs and handoff summaries.

## Safety invariants

These override everything, including my own prompts. If a request conflicts with one, say so instead of complying.

1. The user can always regain control. Every lock has four independent exits: session timer, hard deadline, emergency chord (Ctrl+Alt+K), and OS cleanup when the process dies.
2. Use only blocking mechanisms whose effect dies with the process (user-mode low-level hooks). Never disable devices (SetupAPI, Device Manager, pnputil), never write Scancode Map or other persistent lock state, never install drivers, never use `BlockInput` — unless an ADR I approved says otherwise.
3. The native engine owns lock state. The UI only displays state and sends requests; a hung, crashed, or closed webview must never affect unlocking.
4. Hooks exist only during a session. At idle KeyClean has no hook installed and sees no input.
5. The hook callback is O(1): no I/O, no logging, no allocation, no blocking locks, no IPC round-trips. It decides pass/block from atomics and a precomputed deadline. (Windows silently removes slow low-level hooks — `LowLevelHooksTimeout`.)
6. The hard deadline is enforced twice: inside the hook callback on every event, and by a separate watchdog thread. Either alone must release input. Hard deadline = min(max-lock setting [default 30 min], session duration + 10 s).
7. The emergency chord is detected inside the hook from modifier state the hook tracks itself — not `GetAsyncKeyState`/`GetKeyState`, because blocked events don't reliably update system key state. Handle left/right modifiers and AltGr (delivered as LCtrl+RAlt).
8. Unlocking leaves no stuck keys: keys held when the lock began (e.g. the start shortcut) and the emergency chord itself must not leave Windows believing a key is still down.
9. Any system transition unlocks: suspend, shutdown/logoff, session lock/switch, engine error. After resume KeyClean is idle. No lock state survives a restart.
10. No panic may unwind across the hook FFI boundary. Any engine failure ends with input released.
11. From the overlay milestone on: never locked without a visible overlay. If the overlay isn't confirmed visible within ~2 s of locking, unlock and show an error.
12. Single instance only.
13. Debug builds clamp every lock to ≤15 s and the hard deadline to ≤20 s, and show "DEV CAP" in the UI. Only release builds allow real durations.

## Privacy invariants (§2.3, §40)

- Never record, store, transmit, or analyze keystrokes. No characters, virtual-key codes, or scan codes in logs, errors, or events — at any log level. The post-MVP key tester displays in real time only and keeps no history.
- Logs hold session events only: start, end, end reason, device ids/counts, errors.
- No telemetry, analytics, accounts, cloud, or network calls. No auto-updater in the MVP.

## Stack

- Tauri 2, Windows x64 only. Rust stable with the `windows` crate for Win32.
- UI: React + TypeScript (strict) + Vite + Tailwind v4; `motion` only for small transitions. Not Next.js — Tauri needs a static SPA.
- Official Tauri plugins where they exist (single-instance, global-shortcut, autostart, notification, store). Every new dependency gets a one-line justification in the commit message or an ADR.
- Package manager: pnpm.

## Layout (maps to the §49 layers)

```
Cargo.toml               workspace: crates/*, src-tauri
crates/keyclean-core/    pure Rust, no Windows deps: session state machine (§48), timers with
                         injectable clock, safety policy, chord detection, settings/device models
crates/keyclean-win/     all Win32 and all `unsafe`: engine thread, LL hooks, Raw Input, device
                         enumeration, power/session/shutdown events, watchdog
src-tauri/               app shell: command/event bridge, tray, windows (dashboard, overlay),
                         plugins, settings persistence
src/                     React UI: app/, ui/{dashboard,overlay,devices,settings,tester},
                         shared/{localization,types}
locales/en/strings.json  single source for every user-facing string (UI, tray, notifications)
docs/  scripts/  assets/
```

## Engine model

- One dedicated engine thread owns the hooks, Raw Input registration, a hidden top-level window for power/session/shutdown/device notifications (message-only windows miss broadcasts), and its own message loop.
- Hook callbacks read shared state through atomics only. Commands enter the engine thread as posted messages; events leave through a channel.
- A separate watchdog thread enforces the hard deadline.
- Never set debugger breakpoints inside hook callbacks — pausing there stalls input system-wide.

## How we work

- One milestone at a time from `docs/ROADMAP.md`. Plan first. Don't start the next milestone until I confirm the current one passed manual testing.
- **Never engage a real input lock yourself.** Don't run the app, examples, or anything that installs hooks. You may run builds, `cargo test` (pure logic), clippy, fmt, typecheck, and lint. When hook behavior needs verifying, write the manual test and ask me to run it.
- Automated tests cover `keyclean-core` with a fake clock. OS behavior is verified with `docs/testing/manual/<milestone>.md`: numbered steps, an expected result per step, and every step doable without the keyboard (mouse, a pre-armed PowerShell command such as `Start-Sleep 5; Stop-Process -Name keyclean -Force`, or the timer).
- Verify Win32 and Tauri APIs against official docs (learn.microsoft.com, docs.rs, v2.tauri.app) rather than memory, and link sources in research notes. Tag claims [docs], [tested], or [assumption].
- Any deviation from `docs/SPEC.md` or this file needs an ADR: `docs/decisions/NNNN-title.md` (context, decision, alternatives, consequences).
- After each milestone: update ROADMAP checkboxes, `CHANGELOG.md` (Unreleased), and the Commands section below, then give a handoff summary — what changed, how to test, risks, open questions.
- Git: small commits, Conventional Commits with scopes (engine, core, app, ui, i18n, docs, ci). Never push, never rewrite history.
- Keep this file under ~200 lines; detail belongs in `docs/`.

## Code standards

- Rust: `unsafe` only in `keyclean-win`, every block with a `// SAFETY:` comment. `cargo fmt`, `cargo clippy -- -D warnings`. No `unwrap`/`expect` on engine paths — errors end the session with input released.
- TypeScript: strict, no `any`, function components. No hard-coded user-facing strings — use dot keys like `t("clean.start")`. Layouts must tolerate strings ~2× longer than English (§45).
- Errors: plain-language message plus "View technical details" (§46). Never a bare error code.
- Idle cost near zero: no polling loops; closing the dashboard to tray destroys its webview rather than hiding it.

## Commands

_(Fill in after scaffolding: dev, build, bundle, test, lint, typecheck.)_

## Out of scope unless I say otherwise

Key remapping, profiles, cloud/sync, accounts, telemetry, AI features, macOS/Linux, languages beyond English (keep the i18n architecture ready), installer/updater before their milestone.
