# ADR 0014 - Full-screen overlay: shown first, watched during the lock

- Status: Accepted, 2026-10-07
- Deciders: Max (plan approved 2026-10-06: monitor under the cursor, any overlay loss ends the lock,
  a desktop switch ends it too, Unlock now hidden while the mouse is locked, system monospace font,
  main window back after unlock, heartbeat)
- Related: M6; §12-§18, §45; invariants 3 and 11; ADR 0001 (stack, overlay latency criterion),
  ADR 0009 (engine process); `crates/keyclean-core/src/overlay.rs`, `src-tauri/src/overlay.rs`,
  `crates/keyclean-win/src/desktop.rs`, `src/ui/overlay/`, `docs/research/m6-overlay.md`.
  Deviates from §12 (order of steps).

## Context

Invariant 11: from M6 on, input is never locked without a visible overlay. If the overlay isn't
confirmed visible within about 2 s, nothing is locked and the user sees an error.

Invariant 3: the engine owns the lock, and a hung, crashed or closed webview must never affect
unlocking.

§12 lists "activate the selected input locks" before "display the full-screen overlay". If the lock
came first, a slow or failed overlay would leave the user locked with nothing on screen, which is
what invariant 11 forbids.

## Decision

### Order: overlay first, then the lock

1. The app (not the engine) picks the monitor under the cursor, falling back to the primary monitor,
   and to a maximized window if Windows reports no monitor at all (Max's decision for M6; all
   monitors later) [docs: `AppHandle::cursor_position`, `monitor_from_point`, tauri 2.12.1].
2. It builds the overlay window hidden: no decorations or shadow, not resizable, not in the taskbar,
   always on top, focused, background `#0a0908` so nothing flashes white. Each attempt gets its own
   label, `overlay-<n>`.
3. It sets position, then size, in physical pixels to the monitor's full rect (taskbar included),
   then shows and focuses it. It never uses `fullscreen(true)`.
4. The overlay page (`overlay.html`, its own small Vite entry) renders its locked screen, waits two
   animation frames and checks `document.visibilityState === "visible"`. Then it calls
   `overlay_ready(attempt)`.
5. Only then does the app ask the engine to lock. The 2 s budget (`CONFIRM_BUDGET`) is counted in Rust
   from the lock request on a monotonic clock. A late confirmation, or one for an earlier attempt,
   never locks. No confirmation means the window is closed, nothing is locked, and the window shows
   "The lock screen didn't appear in time, so nothing was locked."

`lock_input` is an async command because building a window in a synchronous command deadlocks on
Windows [docs: tauri 2.12.1 `WebviewWindowBuilder::new`]. The debug harness autolock goes through
the same path, once the event loop is ready.

The engine and its wire protocol are unchanged. The overlay is the app's business.

### During the lock: any loss ends the lock (Max's decision)

On every countdown status (once a second, only during a lock) the app checks the overlay on its main
thread:

| Loss          | How it's seen                                                                                                                                                                                                                           |
| ------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Closed        | the window's close request or destruction (Alt+F4 in a mouse-only lock, `WM_CLOSE`, `taskkill`), seen at once                                                                                                                           |
| Hidden        | `is_visible()` is false                                                                                                                                                                                                                 |
| Minimized     | `is_minimized()` (for example Show desktop in a mouse-only lock)                                                                                                                                                                        |
| Other desktop | `IVirtualDesktopManager::IsWindowOnCurrentVirtualDesktop` is false (a four-finger touchpad swipe can switch desktops) [docs](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-ivirtualdesktopmanager) |
| Silent        | the page answers every status with `overlay_tick`; 3 s without one (`HEARTBEAT_LIMIT`) means it crashed or hung                                                                                                                         |

Any of these makes the app ask the engine to unlock. A close request on the overlay (Alt+F4 or
`WM_CLOSE`) also exits KeyClean, like closing the main window (Max's decision, 2026-10-07, after
harness S23 showed that `taskkill` without `/F` reached only the overlay, so the app kept running).
Otherwise the last lock reads "ended because its
lock screen was closed or hidden" (`overlayLost`). A webview can end a lock early this way, but it
can never keep one going or delay an unlock, so invariant 3 holds: every engine exit works without
the overlay. A getter that fails, or a desktop check Windows can't answer, doesn't count as a loss.

Pinning the overlay to every virtual desktop isn't possible: Tauri's `visible_on_all_workspaces` has
no Windows implementation in tao 0.37.1 [docs: tao source]. Moving the overlay after the user
(`MoveWindowToDesktop`) would need a prototype, so a desktop switch ends the lock instead (Max's
decision).

Task View (three-finger swipe up) and other system UI drawn over the overlay don't remove it. The lock
goes on, and the overlay is visible again when they close. This is documented, not fought.

### End of the lock

The overlay is destroyed (not hidden) at the first `Unlocking` or `Idle` status, which is when input
other than held keys is free again (§14 "disappear immediately"). It is also destroyed when the
engine refuses the request, when the engine process dies, and when the overlay is lost. Then the main
window comes back to the front (§16). Closing the main window exits the app even while an overlay is
open.

### Focus and topmost

The overlay takes focus, so in a mouse-only lock typed keys reach the overlay, not an app hidden
underneath (§14). The ADR 0009 focus problem can't return: the hook lives in the engine process,
which has no WebView, so a focused overlay in the app process doesn't affect it. Harness check S14
now clicks the overlay and sends Ctrl+Alt+K while the overlay has focus.

Not fought: Alt+Tab in a mouse-only lock moves focus away (the overlay stays on top and visible), and
another always-on-top window can cover part of it.

### What the overlay shows

The countdown with segmented progress, the locked devices as the engine reports them, and the
emergency shortcut. The last 5 seconds read "Unlocking in N seconds" in the accent colour. A debug
build shows DEV CAP, and the latest notice is shown during the lock.

| Lock               | Unlock now                      | Extra line                   |
| ------------------ | ------------------------------- | ---------------------------- |
| Keyboard           | shown, clickable                | -                            |
| Mouse and touchpad | hidden (Max's decision, design) | touchpad gestures still work |
| Both               | hidden                          | touchpad gestures still work |

The font is the system monospace stack (Cascadia Mono, then Consolas); nothing is bundled. Text
wraps for longer translations (§45).

### Idle cost

The overlay is created per lock and destroyed after it. At idle there is no overlay window and no
check runs. A pre-created hidden window would only be added, with its own ADR, if the measured
create-to-confirmed latency can't meet the 2 s rule (ADR 0001). See `docs/research/m6-overlay.md`.

## Alternatives

- **Lock first, then show the overlay (§12 as written).** A slow or failed overlay leaves the user
  locked with nothing on screen. Rejected (invariant 11).
- **Keep the lock when the overlay goes away, and document it.** Breaks invariant 11. Rejected.
- **Re-show the overlay once, end the lock only if that fails.** Fewer early endings from touchpad
  gestures, but more states to test. Max chose the simpler rule; it can be revisited with data from
  the manual test.
- **Follow the user to another virtual desktop.** Needs a prototype of `MoveWindowToDesktop`.
  Deferred.
- **A pre-created hidden overlay window.** Costs a WebView at idle. Only if the measurements need it.
- **Non-focusable overlay (`WS_EX_NOACTIVATE`).** Keeps focus where it was, but in a mouse-only lock
  typed keys would go to a hidden app. Rejected.
- **Moving the overlay rule into the engine.** The engine would wait on a webview, which invariant 3
  forbids. Rejected.

## Consequences

- A lock starts a little later: the time the overlay takes to confirm (measured in the research note).
- A touchpad gesture that minimizes the overlay or switches desktops ends the lock early. That is the
  safe direction; the user can start again.
- The harness exe must embed the UI (`cargo build -p keyclean --features tauri/custom-protocol`):
  a plain debug build loads the UI from a dev server, so its overlay never confirms. The harness skips
  its app checks on an exe without the current UI.
- Windows 10 latency is not measured yet (no Windows 10 machine). The ADR 0001 criterion stays open
  for Windows 10 until M11 or a VM.
