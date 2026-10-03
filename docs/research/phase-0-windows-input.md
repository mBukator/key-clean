# Phase 0 - Windows input research

Status: done 2026-09-30 (desk research), hardware items pending Max's M1 manual test.
Spec: §51 (Phase 0). Implementation: Milestone 1 (§62).

Every claim is tagged:

- **[docs]** - confirmed in official documentation (link given).
- **[tested]** - covered by a unit test in this repo (pure logic only; no test ever installs a hook).
- **[needs prototype]** - plausible, but only real hardware will tell. These are recorded through
  `docs/testing/manual/M1.md`.
- **[assumption]** - our best reading; not yet confirmed.

## Versions in use

| Component                      | Version | Notes                                    |
| ------------------------------ | ------- | ---------------------------------------- |
| `windows` crate                | 0.62.2  | All Win32 calls, only in `keyclean-win`  |
| `tauri`                        | 2.12.1  | App shell                                |
| `tauri-build`                  | 2.7.1   | Build script                             |
| `tauri-plugin-single-instance` | 2.5.2   | Invariant 12                             |
| Rust                           | 1.98.1  | Tauri plugins need Rust 1.90 or newer    |
| Test machine                   | Win 11  | 25H2, build 26200; Win 10 still untested |

## A. Input interception

### A1. Low-level keyboard hook (`WH_KEYBOARD_LL`)

- The hook callback runs on the thread that installed the hook, and only while that thread waits
  for messages. So the installing thread must run a message loop. [docs]
  https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc
- Install with `SetWindowsHookExW(WH_KEYBOARD_LL, proc, GetModuleHandleW(None), 0)`. Returning a
  nonzero value blocks the event. When `nCode < 0` the callback must call `CallNextHookEx`. [docs]
  https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowshookexw
- `LowLevelHooksTimeout` is capped at 1000 ms since Windows 10 1709. If the callback is slower,
  Windows skips it, and after repeated timeouts it removes the hook **silently**, with no
  notification. For KeyClean that means the lock fails open: typing suddenly works again. [docs]
  (same LowLevelKeyboardProc page, Remarks)
- Detecting silent removal needs a second input source, e.g. a Raw Input sink, compared against the
  hook's last-callback time (the pattern AltSnap uses). [needs prototype]

**What this means for KeyClean:** one dedicated engine thread owns the hook and pumps messages. The
callback is O(1) - atomics and fixed-size bit sets only (invariant 5). Silent removal fails open, which
is safe but unannounced; detection is deferred to Phase 2 (M3) by Max's decision.

### A2. What user mode cannot block

- Ctrl+Alt+Del is the Secure Attention Sequence, handled by Winlogon. No user-mode hook can block it.
  [docs] https://learn.microsoft.com/en-us/windows/win32/secauthn/initializing-winlogon
- Win+L is handled by Winlogon too. [assumption, recorded in M1.md]
- Other desktops (UAC prompt, lock screen, Ctrl+Alt+Del screen) don't run our hook. [docs]
  https://learn.microsoft.com/en-us/windows/win32/winstation/desktops
- Fn, brightness, firmware and power keys are often handled in the keyboard firmware or by ACPI and
  never reach Windows as keystrokes. [assumption]
- The Windows key can be swallowed by a low-level hook. [docs]
  https://learn.microsoft.com/en-us/windows/win32/dxtecharts/disabling-shortcut-keys-in-games
- Alt+Tab, Ctrl+Esc, Ctrl+Shift+Esc, Win+X, Win+G and media keys should be swallowable. (M1 found
  that the physical Win+G is not; see `webview-focus-hook.md`.)
  [needs prototype]
- Don't change Sticky Keys with `SPI_SETSTICKYKEYS`: the setting persists after the process exits,
  which would break invariant 2 (no persistent state). [docs] (same "disabling shortcut keys" page)

**What this means for KeyClean:** Ctrl+Alt+Del and Win+L are documented escape hatches, not bugs. Win+L
locks the workstation, which KeyClean treats as a system transition and unlocks (ADR 0003).

### A3. Key-state consistency (no stuck keys)

- `KBDLLHOOKSTRUCT` has no repeat flag; an auto-repeat arrives as another key-down. [docs]
  https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-kbdllhookstruct
- The hook must swallow all four messages: `WM_KEYDOWN`, `WM_KEYUP`, `WM_SYSKEYDOWN`, `WM_SYSKEYUP`.
  [docs] (LowLevelKeyboardProc)
- Left and right modifiers are told apart by virtual-key code, or by the generic code plus
  `LLKHF_EXTENDED`. [docs] (KBDLLHOOKSTRUCT). KeyClean also matches K by its physical position
  (scan code), so the chord works on non-QWERTY layouts. [tested] `keys.rs` tests
  `left_right_specific_codes`, `generic_modifiers_use_the_extended_flag`,
  `k_by_virtual_key_or_physical_position`.
- AltGr is delivered as a fake Left Ctrl (scan 0x21D) followed by Right Alt. [docs]
  https://learn.microsoft.com/en-us/windows/win32/inputdev/about-keyboard-input#keystroke-message-flags
  KeyClean counts AltGr+K as Ctrl+Alt+K. [tested] `altgr_counts_as_ctrl_alt`,
  `altgr_fake_ctrl_is_left_ctrl`.
- Auto-repeat applies to modifiers too. After the chord fires, the user is still holding Ctrl+Alt+K,
  and those repeats must not reach Windows.
- `GetAsyncKeyState` is not reliable inside the hook for blocked keys (invariant 7), so the hook
  tracks modifier state itself. It is accurate _before_ anything is blocked, which is when the
  engine reads it once to seed the model.

**What this means for KeyClean:** the hook models what Windows believes is down (`os_down`) and what
it blocked (`swallowed`), and never blocks half of a keystroke Windows has seen. No `SendInput`
needed. Details in ADR 0002. [tested] `keystate.rs` tests, e.g.
`chord_then_auto_repeat_during_drain_reaches_nothing`, `key_held_at_lock_start_repeats_then_releases`,
`ctrl_alt_del_mid_lock_never_drains`, `mixed_typing_during_lock_leaves_nothing_stuck`.

### A4. UIPI and integrity levels

- User Interface Privilege Isolation stops a medium-integrity process from interacting with
  higher-integrity windows. [docs]
  https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-securityoverview
- A medium-integrity low-level hook likely neither sees nor blocks input while an elevated window
  (Task Manager, admin terminal) has focus. That fails open. [needs prototype]
- `uiAccess=true` would lift this, but it is meant for assistive technology and needs a signed binary
  installed in a secure location such as Program Files. [docs]
  https://learn.microsoft.com/en-us/windows/security/threat-protection/security-policy-settings/user-account-control-only-elevate-uiaccess-applications-that-are-installed-in-secure-locations

**What this means for KeyClean:** run `asInvoker`. Later, warn when the foreground window is elevated
and offer an optional relaunch as administrator (ADR 0005).

### A5. Injected input

- `LLKHF_INJECTED` marks events from `SendInput`, the On-Screen Keyboard, the touch keyboard,
  AutoHotkey and remapping tools. [docs] (KBDLLHOOKSTRUCT)
- Max's decision (2026-10-01): **block injected input during a lock**, using the same rules as physical
  input. A later "allow on-screen keyboard" toggle is noted. KeyClean itself injects nothing.

**What this means for KeyClean:** the hook doesn't look at `LLKHF_INJECTED` at all in M1. ADR 0007.

### A6. Device identity

- Enumerate with `GetRawInputDeviceList` and `GetRawInputDeviceInfoW` (`RIDI_DEVICENAME`,
  `RIDI_DEVICEINFO`). [docs]
  https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getrawinputdevicelist
- Friendly name: `CM_Get_Device_Interface_PropertyW(DEVPKEY_Device_InstanceId)` →
  `CM_Locate_DevNodeW` → `CM_Get_DevNode_PropertyW(DEVPKEY_Device_FriendlyName)`, falling back to
  `DEVPKEY_NAME`. [docs]
  https://learn.microsoft.com/en-us/windows/win32/api/cfgmgr32/nf-cfgmgr32-cm_get_devnode_propertyw
- Listing devices does not register for input, so KeyClean still sees no keystrokes while idle
  (invariant 4). [docs] (GetRawInputDeviceList)
- The low-level hook doesn't say which device an event came from. Matching it to a `WM_INPUT` event
  depends on their ordering, which is undocumented, and waiting for `WM_INPUT` inside the hook would
  break O(1). [needs prototype]
- The Interception driver can tell devices apart, but it is a kernel driver (reboot needed, LGPL or
  commercial licence) and violates invariant 2. Rejected.

**What this means for KeyClean:** M1 and the MVP lock all keyboards; the device list is informational
(ADR 0004).

### A7. Precision touchpads

- Precision Touchpad input reaches applications as mouse input; in raw input `hDevice` may be 0.
  [docs] https://learn.microsoft.com/en-us/windows-hardware/design/component-guidelines/touchpad-windows-precision-touchpad-collection
- The low-level mouse hook can't tell a touchpad from a mouse, and system gestures may bypass it.
  [needs prototype]

**What this means for KeyClean:** capability model for §23 - **Supported**: keyboards and mice;
**Limited**: precision touchpads; **Unsupported**: Fn/firmware keys, Ctrl+Alt+Del, pen and touchscreen.

## B. System events and failure safety

### B8. Which window receives system notifications

- Message-only windows don't receive broadcast messages. [docs]
  https://learn.microsoft.com/en-us/windows/win32/winmsg/window-features#message-only-windows
  So the engine uses a hidden top-level window with `WS_EX_TOOLWINDOW` (keeps it off the taskbar),
  on the same thread as the hook.
- `RegisterSuspendResumeNotification(hwnd, DEVICE_NOTIFY_WINDOW_HANDLE)` delivers
  `PBT_APMSUSPEND` / `PBT_APMRESUMEAUTOMATIC`. [docs]
  https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registersuspendresumenotification
- `WM_QUERYENDSESSION`: return TRUE (never block shutdown) and unlock right away. Apps have about 5 s
  before Windows shows the "this app is preventing shutdown" screen. [docs]
  https://learn.microsoft.com/en-us/windows/win32/shutdown/wm-queryendsession
- `WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION)` delivers
  `WTS_SESSION_LOCK`, `WTS_SESSION_LOGOFF`, `WTS_CONSOLE_DISCONNECT`, `WTS_REMOTE_DISCONNECT`. Unregister
  before destroying the window. [docs]
  https://learn.microsoft.com/en-us/windows/win32/api/wtsapi32/nf-wtsapi32-wtsregistersessionnotification
- Device arrival/removal (`WM_INPUT_DEVICE_CHANGE`) needs `RIDEV_DEVNOTIFY` and a `hwndTarget`. [docs]
  https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-input-device-change (M4)

**What this means for KeyClean:** one hidden window, one thread, all transitions unlock immediately
(ADR 0003).

### B9. Hook cleanup when things go wrong

- Windows removes a process's hooks when the process ends, however it ends (normal exit, crash,
  `Stop-Process -Force`, Task Manager). [docs]
  https://learn.microsoft.com/en-us/windows/win32/winmsg/about-hooks
- When only the thread that installed the hook exits, its hooks are freed too. [assumption]
- A thread that stops pumping messages stalls keyboard input for up to `LowLevelHooksTimeout` (1 s)
  per event, then Windows removes the hook (A1). [docs]
- A panic that tries to unwind out of an `extern "system"` function aborts the process (Rust 1.81
  and newer). [docs] https://doc.rust-lang.org/nomicon/ffi.html#ffi-and-unwinding
- KeyClean builds with `panic = "abort"` in dev and release, so any panic ends the process and
  Windows releases input. The hook keeps `catch_unwind` as a defence in case a build ever unwinds.

**What this means for KeyClean:** process death is the one exit that can't fail. The watchdog uses it
as its last resort (ADR 0006).

### B10. Monotonic time

- `std::time::Instant` uses `QueryPerformanceCounter` on Windows. It is monotonic, ignores wall-clock
  changes, and keeps counting through sleep. [docs]
  https://doc.rust-lang.org/std/time/struct.Instant.html and
  https://learn.microsoft.com/en-us/windows/win32/sysinfo/acquiring-high-resolution-time-stamps
- `QueryUnbiasedInterruptTime` excludes sleep, so it is avoided. [docs]
  https://learn.microsoft.com/en-us/windows/win32/api/realtimeapiset/nf-realtimeapiset-queryunbiasedinterrupttime
- The hook stores the hard deadline as raw QPC ticks in an `AtomicU64`.
- Commands enter the engine with `PostMessageW` to the hidden window, not `PostThreadMessageW`:
  thread messages are lost while a modal loop runs. [docs]
  https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-postthreadmessagew

**What this means for KeyClean:** a clock change can't stretch a lock, and suspend unlocks anyway.

## C. Tauri 2

### C11. App shell and plugins

- Tray-only later: `app.windows: []`, and in `RunEvent::ExitRequested` call `prevent_exit()` only when
  `code` is `None`. [docs] https://v2.tauri.app/learn/system-tray/
- Create windows from `async_runtime::spawn`, because building a window inside a sync command handler
  can deadlock on Windows. [docs] https://v2.tauri.app/develop/calling-rust/
- single-instance is Rust-only, must be registered first, and must not `expect()` a window. [docs]
  https://v2.tauri.app/plugin/single-instance/
- Notifications (toasts) only work in installed builds. [docs] https://v2.tauri.app/plugin/notification/
- Capabilities: list custom commands in `tauri_build::AppManifest::commands`, give each window its own
  capability file, read real `allow-*` ids from `gen/schemas/acl-manifests.json`. [docs]
  https://v2.tauri.app/security/capabilities/
- `emit_to(label)` needs a listener scoped to that window. [docs]
  https://v2.tauri.app/develop/calling-frontend/
- Bundle: NSIS with `installMode: currentUser` and WebView2 `downloadBootstrapper`. [docs]
  https://v2.tauri.app/distribute/windows-installer/
- bun: `beforeDevCommand: "bun run dev"`, run with `bun tauri dev`. CI: `oven-sh/setup-bun@v2`,
  `dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2`. [docs] https://v2.tauri.app/start/

### C12. Full-screen overlay (M6)

- Build the window hidden, then `set_position` / `set_size` with physical monitor bounds, then
  `show()`. `fullscreen(true)` is buggy on secondary monitors
  ([tauri#11307](https://github.com/tauri-apps/tauri/issues/11307)), and mixed DPI has issues
  ([tauri#6784](https://github.com/tauri-apps/tauri/issues/6784)). [docs]
- Confirm visibility with an acknowledgement from the webview after two `requestAnimationFrame`
  callbacks. [assumption]
- A WebView2 cold start can take 2-3 s, which conflicts with the ~2 s visible rule (invariant 11).
  [needs prototype]
- Max's decision: overlay on **the monitor under the cursor first**, all monitors later. Show the
  overlay, wait for its ack, then engage the lock.

**What this means for KeyClean:** M6 must measure create-to-visible latency on Windows 10. If it can't
meet the rule even with a pre-created hidden window, revisit Tauri (ADR 0001).

## Needs hardware verification

Each item is a "record what you see" step in `docs/testing/manual/M1.md`.

| Item                                                         | M1.md step   |
| ------------------------------------------------------------ | ------------ |
| Win, Alt+Tab, Ctrl+Esc, Win+X swallowed                      | 5            |
| Ctrl+Shift+Esc, Win+G swallowed                              | 5            |
| Media/volume and Fn/brightness keys                          | 5            |
| Win+L locks the workstation and ends the lock (SessionLock)  | 6            |
| Ctrl+Alt+Del reachable; what happens after Cancel            | 7            |
| Keys held at lock start leave nothing stuck                  | 9            |
| Elevated window focused during a lock (expected: fails open) | 11           |
| Sleep during a lock (idle after wake)                        | 12           |
| AltGr+K on an AltGr layout                                   | 15           |
| Sticky Keys prompt while Shift is swallowed                  | 16           |
| On-Screen Keyboard clicks during a lock (injected input)     | 17           |
| Chord auto-repeat after unlock                               | 3            |
| Windows 10 behavior (all of the above)                       | M1 on Win 10 |
