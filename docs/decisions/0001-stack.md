# ADR 0001 - Stack: Tauri 2, Rust `windows` crate, React, bun

- Status: Accepted, 2026-10-01
- Deciders: Max
- Related: CLAUDE.md "Stack", `docs/research/phase-0-windows-input.md` (C11, C12)

## Context

KeyClean needs two very different halves:

- a native engine that installs a low-level keyboard hook, owns a message loop and reacts to power and
  session events - this has to be Win32 code with tight timing rules;
- a small UI (dashboard, overlay, settings) that Max, a frontend developer, can build and maintain.

The app must cost almost nothing at idle (§30) and ship as a normal Windows installer.

## Decision

- **Tauri 2** (tauri 2.12.1, tauri-build 2.7.1) as the app shell. It uses the WebView2 runtime that
  ships with Windows, so the installer stays small, and its backend is Rust, so the engine and the shell
  share one language.
- **Rust with the `windows` crate 0.62** (0.62.2 locked) for all Win32 work, confined to
  `crates/keyclean-win`. The pure logic (state machine, timers, chord, key tracking) lives in
  `crates/keyclean-core` with no Windows dependencies, so it can be unit-tested.
- **React + TypeScript (strict) + Vite + Tailwind v4** for the UI, as a static SPA (Tauri needs one).
- **bun instead of pnpm** as the package manager and script runner (Max's decision 2026-09-30). This
  deviates from the original CLAUDE.md, which said pnpm; CLAUDE.md now says bun. Tauri runs with
  `beforeDevCommand: "bun run dev"` and `bun tauri dev`; CI uses `oven-sh/setup-bun@v2`.
- **Git workflow change:** the original CLAUDE.md said "never push". It is replaced by the "Git
  workflow (MUST)" rules in `docs/development/git-workflow.md`: `main` and `develop` are protected;
  Claude may push work branches (`feat/*`, `fix/*`, ...) and open PRs into `develop`, but never pushes
  to `main` or `develop`, never merges, never force-pushes and never rewrites history.

In plain terms: the risky part (the hook) is plain Rust talking directly to Windows, and the web UI is
just a remote control for it. If the UI breaks, the engine still unlocks (invariant 3).

## Alternatives

- **Electron** - bundles its own Chromium (100 MB or more), much higher idle memory, and the native
  engine would still need a Rust or C++ addon. Rejected.
- **Native Win32 / WinUI 3 UI in Rust or C#** - lowest overhead, but far from Max's skills and slower to
  iterate on. Kept as the fallback if Tauri fails the criteria below.
- **`windows-sys` instead of `windows`** - thinner and faster to compile, but no safe wrappers or
  `Result` types. CLAUDE.md names `windows`; we keep it.
- **pnpm** - works fine; bun was chosen to match Max's other repos and for speed.

## When we would switch away from Tauri

Any of these, measured on real hardware, would reopen this decision:

- **Overlay latency:** on Windows 10, the time from creating (or showing a pre-created hidden) overlay
  window to it being confirmed visible can't meet the ~2 s rule (invariant 11) reliably. WebView2 cold
  start can take 2-3 s [needs prototype], so M6 measures this first.
- **Idle cost:** idle memory or CPU with the dashboard closed to tray (webview destroyed) is clearly
  above what §30 allows.
- **Engine independence:** any case where a hung or crashed webview delays unlocking.

## Consequences

- Two toolchains (Rust and bun/Node) on every dev machine and in CI.
- The engine is fully testable without a UI; `lock_smoke` exercises it without Tauri.
- WebView2 must be present; the NSIS installer uses `downloadBootstrapper` for machines without it
  (M11).
- Every new dependency still needs a one-line justification in its commit or an ADR.
