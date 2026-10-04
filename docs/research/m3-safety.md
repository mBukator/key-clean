# M3 safety research notes

Findings behind the M3 safety hardening: hook-liveness detection, device changes during a lock, the
`taskkill` hang, engine restart, and clock changes. Claims are tagged [docs], [tested] or
[assumption]; "[needs prototype]" means the M3 harness or manual test settles it.

## Raw Input as a second input source

- `RAWINPUTDEVICE`: `RIDEV_INPUTSINK` delivers input even when the caller isn't in the foreground,
  and needs `hwndTarget`. `RIDEV_REMOVE` stops a registration and needs `hwndTarget` = NULL. [docs]
  https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-rawinputdevice
- `RegisterRawInputDevices`: only one window per process gets raw input for a device class (the last
  registration wins). `RIDEV_DEVNOTIFY` is needed for `WM_INPUT_DEVICE_CHANGE`. [docs]
  https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registerrawinputdevices
- `WM_INPUT`: with `RIM_INPUT` the window must call `DefWindowProc` so the system can clean up.
  `RIM_INPUTSINK` marks background input. [docs]
  https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-input
- `WM_INPUT_DEVICE_CHANGE`: `wParam` is `GIDC_ARRIVAL` (1) or `GIDC_REMOVAL` (2), `lParam` the
  device handle; return 0. [docs]
  https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-input-device-change
- `GetRegisteredRawInputDevices` with a NULL buffer returns -1 (`ERROR_INSUFFICIENT_BUFFER`) and
  writes the number of registrations to its count argument; the harness uses that count to check
  that idle KeyClean has none (invariant 4). [docs]
  https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getregisteredrawinputdevices
- Whether Windows replays `GIDC_ARRIVAL` for devices already connected when a window registers is
  not documented. The engine compares against a snapshot taken at lock start, so it works either
  way. [assumption]
- Whether a key a low-level hook blocks still produces `WM_INPUT`, and which of the two comes first,
  is not documented. The liveness rule works in every combination (ADR 0010). [needs prototype]
- Whether `SendInput` keys produce `WM_INPUT` decides whether harness checks S17 and S20 can work at
  all. Their failure message says whether the engine saw any `WM_INPUT`. [needs prototype]
- Typing into an elevated window during a lock:
    - **M1 step 11 (2026-10-02, engine in the app process):** the keys got through.
    - **M3 step 7 (2026-10-04, engine in its own process, ADR 0009):** the hook blocked them, so
      the elevated-window warning had nothing to report.
    - [tested, both] Why they differ is unknown. The engine process is the obvious change, but
      that's an [assumption]. The warning stays in place as a fallback for machines or windows
      where keys do get through.

## The Xbox Game Bar blinds the Raw Input sink

- Win+G opens the Game Bar during a lock, synthesized as well as physical, although the hook blocks
  both key events. The harness's observer hook never sees them. [tested 2026-10-04, photo of S6]
- While the overlay is open, Windows delivers no `WM_INPUT` to KeyClean's `RIDEV_INPUTSINK` window,
  although the registration is in place. The foreground window stays the same (here the Claude
  desktop app). [tested: S25. The baseline, Win, Alt+Tab, Ctrl+Esc, Win+X, Volume up and
  Ctrl+Shift+Esc all left the check working; only Win+G broke it.]
- Why is undocumented. A plausible cause is that the Game Bar overlay takes raw keyboard input
  exclusively while open [assumption].
- Consequence: the lost-hook check is blind while the overlay is open (ADR 0010, "Known
  limitation"). The hook keeps blocking.

## Silent hook removal

- On Windows 7 and later, a low-level hook whose callback exceeds `LowLevelHooksTimeout` is removed
  silently, with no notification; the timeout is capped at 1000 ms since Windows 10 1709. [docs]
  https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc
- Whether one timeout is enough for Windows to remove the hook, or only repeated ones, is not clear
  from the docs. Opt-in harness check S20 records it. [needs prototype]

## Foreground elevation

- `GetForegroundWindow` can return NULL, for example while focus changes or the desktop switches.
  [docs] https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getforegroundwindow
- `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)`, `OpenProcessToken(TOKEN_QUERY)` and
  `GetTokenInformation(TokenElevation)` tell whether a process is elevated. [docs]
  https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-gettokeninformation
- A standard process may be refused an elevated process or its token, so KeyClean treats "can't open
  it" as elevated. [assumption]
- The gap only exists when the foreground process is elevated and KeyClean isn't. An elevated
  KeyClean, or a machine with UAC off where every admin process is elevated, gets that input
  through its hook, so the engine compares the two elevations. [assumption, from UIPI's
  integrity-level rule]

## `taskkill` without `/F`

- Without `/F`, `taskkill` asks a process to end instead of terminating it. For a GUI process that
  means `WM_CLOSE` to its top-level windows. [assumption; tested 2026-10-02 in the first S11
  version, which made the UI hang]
  https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/taskkill
- tao 0.37.1 creates a hidden "Tao Thread Event Target" window on the main thread and passes its
  `WM_CLOSE` to `DefWindowProcW`, which destroys it. The event loop can't run without it. [tested:
  source `tao/src/platform_impl/windows/event_loop.rs`]
- tauri-plugin-single-instance 2.5.2 creates a hidden `{identifier}-sic` window during plugin setup,
  also passing `WM_CLOSE` to `DefWindowProcW`. [tested: source `src/platform_impl/windows.rs`]
- `SetWindowSubclass` can't subclass a window across threads, so the guard runs in Tauri's `setup`
  on the main thread, which owns both windows. [docs]
  https://learn.microsoft.com/en-us/windows/win32/api/commctrl/nf-commctrl-setwindowsubclass
- It needs Common Controls 6, which the manifest embedded by tauri-build requests. [tested: source
  tauri-build 2.7.1 `src/windows-app-manifest.xml`]

## Clock changes and timer drift

- Deadlines are `Instant`s, which use `QueryPerformanceCounter` on Windows and don't follow
  wall-clock changes. [docs] https://doc.rust-lang.org/std/time/struct.Instant.html
- `SetTimer` takes a relative timeout; the session timer is re-armed if it fires early, and a late
  fire is bounded by the hard deadline. That a system clock change doesn't move a `SetTimer` is an
  [assumption], checked by M3 step 5.
- The watchdog waits with `Condvar::wait_timeout` for the time left until an `Instant`, then
  compares `Instant`s again, so a clock change can't move it either. [docs] (same `Instant` page)
