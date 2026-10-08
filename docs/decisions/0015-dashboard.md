# ADR 0015 - MVP dashboard: custom duration, max-lock setting, remembered choices, display kept on

- Status: Accepted, 2026-10-08
- Deciders: Max (M7 plan settled 2026-10-07; hard-deadline wording option (b) chosen 2026-10-08)
- Related: M7; §9-§12, §19, §25, §32, §45, §46; invariants 3, 6, 9, 11; ADR 0004, 0005 (amended),
  0006, 0013, 0014; `crates/keyclean-core/src/settings.rs`, `crates/keyclean-win/src/display.rs`,
  `src-tauri/src/{settings,overlay,bridge}.rs`, `src/ui/{dashboard,settings,devices}/`,
  `src/shared/ui/`, `docs/research/m7-dashboard.md`. Deviates from §11 (presets only) and §32
  (Settings has one setting so far).

## Context

The Clean screen design (`docs/design/clean-ready.png`) assumes four things the spec doesn't define
or defines differently: a **Custom** duration, a user-set **maximum lock duration**, **remembered**
duration and targets, and the display **kept awake** during a lock. It also shows a dark overlay,
which M6 already ships. Each is an addition beyond the spec, so the project guide asks for an ADR.

Before M7 the app accepted only the four presets, and always sent the 30 min default max lock to the
engine.

## Decision

### Custom duration

- A lock can last from **10 s** up to the **max-lock setting**, in whole seconds. The presets stay
  (§11) and Custom sits next to them.
- `lock_input` checks the range in Rust (`keyclean_core::settings::check_duration`) and replaces
  the old preset-only check. The max lock comes from the settings file, never from the window, so
  the webview can't widen the limit (invariant 3).
- The engine's policy (`plan_lock`) still has the final word: it clamps to the max lock, to the
  60 min ceiling (ADR 0006) and, in debug builds, to the DEV CAP.
- The window checks the same range to show an inline message and keep Lock disabled. Rust checks
  again.

### Maximum lock duration setting

- Settings → Safety → Maximum lock duration: **1-60 min**, default **30** (§19, §32; the ceiling is
  ADR 0006's). The new `set_max_lock` command clamps it, and only the main window may call it.
- Presets longer than the setting are disabled, with a hint that points to Settings.
- A running lock keeps the limit it started with.

### A lock as long as the max lock reads as complete (Max's option (b))

- Invariant 6 sets the hard deadline to `min(max_lock, session + 10 s)`. When a lock lasts exactly
  the max lock, both deadlines fall at the same instant. The engine checks the hard deadline first
  and reports `HardDeadline`, so the user would read "the safety time limit was reached" after a
  normal session.
- The app computes the lock's plan (`LockPlan::session_meets_hard_deadline`) when it requests the
  lock. If the plan says the two coincide, a `HardDeadline` end reads as `timeout`. This works the
  way `overlayLost` already does (ADR 0014).
- The engine, its exits and the deadlines are unchanged. Debug builds never hit this case: DEV CAP
  keeps 5 s between the 15 s session and the 20 s hard deadline.

### Remembered choices and the settings file

- The settings file holds only the max lock, plus the duration (including Custom) and targets of
  the last lock that was **requested** (the engine accepted the request; a failure it reports
  later doesn't undo it). Nothing about a running lock is written (invariant 9).
- Rust owns the file through the official `tauri-plugin-store` (2.5), used from Rust only. No
  capability grants a `store:` permission, and a test checks that.
- The file is `%APPDATA%\io.github.mbukator.keyclean\settings.json`. A missing or corrupt file, or a
  value of the wrong type, means defaults. Values out of range are clamped, and the remembered
  duration is always within the current max lock (`keyclean_core::settings`, unit-tested).

### Display kept on during a lock

- From the overlay's confirmation until its window record is cleared, the app holds a power request
  (`keyclean_win::DisplayRequest`): `PowerRequestDisplayRequired` **plus**
  `PowerRequestSystemRequired`. The PowerSetRequest docs say a display request needs a system
  request alongside it to keep the display on. The plan named only the display request.
- **It doesn't stop the sleep exit.** Windows ends power requests "upon user-initiated system sleep
  entry (power button, lid close or selecting Sleep from the Start menu)" [docs]. So the sleep exit
  of a lock (invariant 9) still works. Only idle sleep waits, and only for the lock, which lasts
  60 min at most.
- **It is released on every path.** Dropping the request releases it, on every path that clears the
  overlay: the lock ends, the overlay is lost, the engine refuses or dies, or the app exits. When
  the overlay is lost, it is released after the unlock request, so it never delays an unlock.
- **If the process dies,** Windows closes its handles [docs]. That the request ends with its handle
  is [assumption], checked in `docs/testing/manual/M7.md` with `powercfg /requests` (which needs
  admin [tested]).
- **A refused request** is only logged. It isn't a safety mechanism.

### Dark overlay

The near-black overlay (M6, ADR 0014) is a deliberate feature: dust, smudges and fingerprints show
up on a dark screen, so a session doubles as screen cleaning. No code change.

### The dashboard itself

- **Tabs:** Clean, Devices and Settings. **Test** stays hidden until M13. There are no placeholder
  tabs.
- **Target tiles:** Keyboard and Mouse are toggles (`aria-pressed`). Touchpad is a status tile you
  can't click. It mirrors Mouse ("Follows mouse"), is marked Limited (ADR 0004/0013), and appears
  only when a touchpad is detected.
- **Theme:** light and dark follow Windows (`prefers-color-scheme`), with tokens matching the
  overlay's near-black `#0a0908` and amber `#f5a54a`. There is no theme setting yet (§32
  Appearance comes later).
- **Fonts:** system faces only (Segoe UI, and Cascadia Mono or Consolas). KeyClean bundles no fonts
  and makes no network calls.
- **Window:** 760x600, minimum 420x520. Below 640 px the layout drops to one column, and it wraps
  text that is twice as long as English (§45).
- **Errors (§46):** one shared error block (`src/shared/ui/ErrorBlock.tsx`), a plain sentence plus
  "View technical details". It is used for request, engine, device and settings errors.
- **Elevated-window warning:** deferred to M9 (ADR 0005 amendment).

The tab, tile, theme, window and 1-60 min range choices were settled with Max in the M7 plan
(2026-10-07); the range follows §32's default and ADR 0006's ceiling.

## Alternatives

- **Presets only** (the spec's §11 MVP list). Rejected: the design asks for Custom, and the range
  check is no weaker than the preset check.
- **Settings in the webview's `localStorage`, or the store plugin's JavaScript API.** Rejected: the
  webview could then raise its own safety limit. Rust owning the file keeps invariant 3.
- **`serde_json` and `std::fs` instead of the plugin.** Workable. The project guide prefers official
  plugins where they exist, and the plugin resolves the app data folder.
- **For the max-length lock:**
    - **(a) cap Custom at max lock − 10 s.** Rejected by Max: the ceiling would read as an odd
      "29:50".
    - **(c) change the end reason in the engine.** Rejected: it touches the hook and watchdog end
      paths for a wording problem.
- **`SetThreadExecutionState(ES_DISPLAY_REQUIRED | ES_CONTINUOUS)`.** Rejected: it is tied to a
  thread, and the overlay's lifetime isn't. A power request object is an owned handle with a reason
  string that `powercfg /requests` shows.

## Consequences

- The app now writes one small file (no lock state). Deleting it restores the defaults.
- `lock_input` accepts any whole number of seconds in range, and the error details name the range.
- During a lock the display and the system stay on until the overlay goes, which is at most the
  60 min ceiling.
- `powercfg /requests` (admin) shows KeyClean during a lock, with the reason "KeyClean keeps the
  screen on during a cleaning session."
- New strings: tabs, panel labels, tiles, Custom, Settings → Safety. Strings the old shell used and
  nothing references any more were removed.
