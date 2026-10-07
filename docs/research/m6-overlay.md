# M6 research - full-screen overlay

What the overlay relies on, how each claim was checked, and what is still open. Decisions are in
ADR 0014. Tags: [docs] from official documentation or the locked crate sources (tauri 2.12.1, tao
0.37.1, wry 0.57.0), [tested] measured on Max's machine, [assumption] not yet verified.

## Window creation and placement

- Building a window from a synchronous command or an event handler deadlocks on Windows, so
  `lock_input` is an async command. [docs: `WebviewWindowBuilder::new` doc comment, tauri 2.12.1]
- Synchronous commands run on the main thread unless marked async.
  [docs](https://v2.tauri.app/develop/calling-rust/)
- `AppHandle::cursor_position()` and `AppHandle::monitor_from_point(x, y)` give the monitor under the
  cursor in physical pixels; `Monitor::position()` and `size()` are its full rect, taskbar included.
  [docs: tauri 2.12.1 `app.rs`]
- The window is built with `visible(false)`, then `set_position` and `set_size` in physical pixels,
  then `show()`. Position goes first because moving onto a monitor with another scale factor can
  resize the window. [assumption: checked by eye on one monitor only; mixed DPI is M6 optional step 9]
- For an undecorated window, the outer rect equals the size set. [assumption: harness S34 compares
  the overlay's `GetWindowRect` with the monitor rect and prints both on a mismatch]
- `background_color` paints the window before the page loads; the page uses the same `#0a0908`, so
  nothing flashes. [assumption until Max looks at it, M6 step 2]
- `visible_on_all_workspaces` has no Windows implementation in tao 0.37.1, so the overlay can't be
  pinned to every virtual desktop. [docs: tao source, no Windows code path]
- `focusable(false)` maps to `WS_EX_NOACTIVATE` in tao. KeyClean doesn't use it: the overlay takes
  focus so typed keys in a mouse-only lock go to it. [docs: tao `window_state.rs`]

## Events and threads

- A global `emit` reaches every JS listener, including ones registered with
  `getCurrentWebviewWindow().listen`, so one status event serves the main window and the overlay.
  [docs: tauri 2.12.1 `event/listener.rs`, `match_any_or_filter`]
- tao initializes COM (single-threaded apartment, `OleInitialize`) on its window thread, so the
  virtual-desktop check runs there. [docs: tao `platform_impl/windows/window.rs`]
- `IVirtualDesktopManager` (Windows 10 and later) has `IsWindowOnCurrentVirtualDesktop`,
  `GetWindowDesktopId` and `MoveWindowToDesktop`. Its documentation names no restriction on which
  windows `MoveWindowToDesktop` may move; the widely reported "own process only" rule is unverified.
  [docs](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-ivirtualdesktopmanager),
  [docs](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ivirtualdesktopmanager-movewindowtodesktop)
- Two animation frames after the locked screen renders, it has been painted. [assumption: the usual
  browser reading of `requestAnimationFrame`; WebView2 throttles frames in a hidden window, so the
  page also checks `document.visibilityState`]

## Build

- A debug `cargo build -p keyclean` loads the UI from `devUrl` (`http://localhost:1420`), because
  tauri-build sets the `dev` cfg when the `custom-protocol` feature is off. With
  `--features tauri/custom-protocol` the exe embeds `dist/`. [docs: tauri-build `is_dev`; tested
  2026-10-07: the overlay bundle's hashed name is in the exe only with the feature]
- So the harness has always run against a dead dev URL for the main window. That didn't matter until
  the overlay had to confirm.

## Focus and the hook (ADR 0009)

- Windows skipped a `WH_KEYBOARD_LL` hook while a WebView2 window of the hook's own process had focus.
  The hook now lives in the engine process, which has no WebView, so the focused overlay in the app
  process can't silence it. [tested in M1 with the main window: S14, F1/F3; for the overlay:
  assumption until S14 and M6 step 8 run]

## Latency (ADR 0001 criterion)

Measured from the lock request (in Rust, monotonic) to the page's confirmation. The app logs
`overlay: built X ms, shown Y ms` and `confirmed visible N ms` for every lock (session events only).
Harness S34 reports one launch; opt-in S41 runs ten locks in one app run: the first (cold, while the
main window's WebView2 is also starting) and the nine after it (warm).

| Machine                      | Build                                  | Cold (first lock) | Warm p50 | Warm p95 | Max    |
| ---------------------------- | -------------------------------------- | ----------------- | -------- | -------- | ------ |
| Windows 11 25H2, ASUS laptop | debug, `custom-protocol` (harness S41) | 393 ms            | 260 ms   | 283 ms   | 283 ms |
| Windows 11 25H2, ASUS laptop | release (`bun tauri build`), 3 locks   | 253 ms            | 250 ms   | -        | 253 ms |
| Windows 10                   | -                                      | no machine        |          |          |        |

[tested 2026-10-07] Every lock confirmed well inside the 2 s budget on Windows 11: the slowest was
the first lock of harness S34 (769 ms, while the app itself was still starting), the cold S41 lock
took 393 ms, and the release build 216-253 ms. So no pre-created window is needed, and ADR 0001's
overlay criterion is met on Windows 11. Windows 10 is still open.

Open: from M8 the dashboard is closed at idle, so the first lock will start WebView2 cold without a
main window. If that misses the budget, a pre-created hidden overlay needs its own ADR.

## Touchpad gestures over the overlay (ELAN1203)

From M5: two-finger scrolling, pinch, and three- and four-finger swipes never pass the hook. What
they do to the overlay is M6 step 6:

| Gesture                      | Expected under ADR 0014                    | Result                                                                                                                                                             |
| ---------------------------- | ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Three-finger swipe up        | Task View over the overlay; lock goes on   | as expected: Task View over the overlay, lock ran to the timer [tested]                                                                                            |
| Three-finger swipe down      | if it minimizes the overlay, the lock ends | not minimized: the other monitor showed the desktop, the taskbar appeared over the overlay, the lock ran to the timer [tested]                                     |
| Four-finger swipe left/right | with two desktops, the lock ends           | the other monitor showed the second desktop, but the overlay stayed on its monitor and the lock ran to the timer [tested; why Windows kept the overlay is unknown] |

With two monitors, these gestures changed only the monitor without the overlay. The taskbar showing
over the overlay after Show desktop is a known limitation: the taskbar is itself always on top. The
lock still holds and the countdown stays visible above it.
