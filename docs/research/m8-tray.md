# M8 research - system tray

What the tray and the new window lifecycle rely on, how each claim was checked, and what is still
open. Decisions are in ADR 0016. Tags: [docs] from official documentation or the locked crate
sources (tauri 2.12.1, tauri-runtime-wry 2.12.1, tauri-utils 2.10.1, tauri-macros 2.7.1, tao
0.37.1, tray-icon 0.25.1, muda 0.20.0, windows 0.62.2), [tested] measured on Max's machine,
[assumption] not yet verified.

## Raw Input in the app process

- tao registers its event-target window for Raw Input from the generic mouse and keyboard usages
  when the event loop is created, with `DeviceEventFilter::default()`, which is `Unfocused`
  (`RIDEV_DEVNOTIFY`: input while a window of the process has focus). [docs: tao `event_loop.rs`
  `EventLoop::new`, `raw_input.rs` `register_all_mice_and_keyboards_for_raw_input`,
  `event_loop.rs` `DeviceEventFilter`]
- `DeviceEventFilter::Always` registers with `RIDEV_REMOVE` and a null target, which removes the
  registration. [docs: tao `raw_input.rs`;
  [RAWINPUTDEVICE](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-rawinputdevice)]
- `tauri::Builder::device_event_filter` is applied in `Builder::build`, before `setup` runs, so the
  count logged in `setup` already reflects it. [docs: tauri 2.12.1 `app.rs`]
- Registrations are per process (`GetRegisteredRawInputDevices` reports the caller's), which is why
  harness S22, run in the harness's own process, never saw the app's. [docs](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getregisteredrawinputdevices)
- The app now logs `app raw input registrations: N`; harness S42 asserts 0. [tested
  2026-10-12: S42 logged 0]

## Windows and exit

- `WindowConfig::create` (default `true`): with `false`, Tauri doesn't build the window at startup,
  and `WebviewWindowBuilder::from_config` builds it later. [docs: tauri-utils 2.10.1 `config.rs`;
  tauri 2.12.1 `webview_window.rs`]
- Building a window "deadlocks when used in a synchronous command or event handlers" on Windows.
  [docs: tauri 2.12.1 `WebviewWindowBuilder::new`]
- `RunEvent::ExitRequested { code, api }`: `code` is `None` when the last window closed and
  `Some` after `AppHandle::exit`. `api.prevent_exit()` keeps the event loop running; it is ignored
  only for a restart. [docs: tauri 2.12.1 `app.rs`]
- `run_on_main_thread` runs the task at once when called on the main thread, and queues it
  otherwise. [docs: tauri-runtime-wry 2.12.1 `send_user_message`]
- A relative `WebviewUrl::App` path is joined to the app URL with `Url::join`, so a query string
  (`index.html?tab=settings`) survives. [docs: tauri 2.12.1 `manager/webview.rs`]

## Tray icon (tray-icon 0.25.1 on Windows)

- Each tray icon has a hidden top-level window of class `tray_icon_app`, created with
  `WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_TOOLWINDOW` and no parent. Its
  window class is registered with the module handle of the image it is linked into
  (`__ImageBase`), which is `keyclean.exe`. [docs: `platform_impl/windows/mod.rs`, `util.rs`]
- Its window procedure handles `WM_DESTROY` (frees its data) and passes `WM_CLOSE` to
  `DefWindowProcW`, which destroys the window. The icon would stay in the notification area until
  the mouse passes over it. [docs: same file]
- It re-adds the icon when Explorer restarts (`TaskbarCreated`). [docs: same file]
- muda creates no window of its own on Windows; it subclasses the tray's window for the menu.
  [docs: muda 0.20.0, no `CreateWindowEx`]
- Tauri's tray builder, `set_icon`, `set_menu` and `set_tooltip` run on the main thread
  (`run_item_main_thread!`). [docs: tauri 2.12.1 `tray/mod.rs`, `menu/mod.rs`]
- `TrayIcon::with_inner_tray_icon` gives the tray-icon object, whose `window_handle()` is that
  hidden window. [docs: tauri 2.12.1 `tray/mod.rs`, tray-icon `lib.rs`]
- The tooltip is `NOTIFYICONDATAW::szTip`, 128 characters including the terminator.
  [docs](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ns-shellapi-notifyicondataw)
- `tauri::include_image!` decodes a PNG at compile time into raw RGBA, so the `image-png` feature
  isn't needed. [docs: tauri-macros 2.7.1 `include_image`]

## Closing windows

- `taskkill` without `/F` asks a process to close with `WM_CLOSE`. In M6 (harness S23) it reached
  only the overlay; before M6 it reached the hidden helpers. So it can't be relied on to reach every
  window. [tested 2026-10-02 and 2026-10-07]
- The close button, Alt+F4 and the window menu send `WM_SYSCOMMAND` with `SC_CLOSE`, and
  `DefWindowProc` turns that into `WM_CLOSE`. The low four bits of `wParam` are used by Windows and
  must be masked. [docs](https://learn.microsoft.com/en-us/windows/win32/menurc/wm-syscommand)
- tao passes `SC_CLOSE` on to `DefWindowProc` and raises `CloseRequested` on `WM_CLOSE`.
  [docs: tao `event_loop.rs`]
- Closing from the taskbar thumbnail or its context menu also sends `SC_CLOSE`. [tested 2026-10-12:
  M8.md step 13, KeyClean went to the tray with the dashboard guard active]
- Alt+F4 while the WebView2 content has focus reaches the window as `SC_CLOSE`. [tested
  2026-10-12: M8.md step 13]
- Asking Tauri for the dashboard's handle (`WebviewWindow::hwnd`) from a main-thread task, right
  after `from_config(...).build()`, failed with "the underlying handle is not available", and a
  listener added with `on_window_event` never ran. Why isn't known; the overlay's identical calls
  work. KeyClean finds its webview windows with `EnumThreadWindows` instead. [tested 2026-10-12:
  harness S44 and S12 logs]
- That task runs once WebView2 has started on the main thread, about a second after the window
  first answers messages, so a bare `WM_CLOSE` in that first second leaves KeyClean in the tray.
  [tested 2026-10-12: S44 and S12 before they waited for the guard]

## Taskbar theme

- Microsoft documents only the app colour mode (`UISettings.GetColorValue`, `ColorValuesChanged`).
  [docs](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/ui/apply-windows-themes)
- The taskbar follows the "Choose your Windows mode" setting, stored as `SystemUsesLightTheme`
  (DWORD, 1 = light) under `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize`.
  [undocumented; tested 2026-10-12: M8.md step 15]
- A colour mode change broadcasts `WM_SETTINGCHANGE` to top-level windows, hidden ones included.
  [tested 2026-10-12: M8.md step 15, the icon switched with no restart] KeyClean re-reads the value on every
  `WM_SETTINGCHANGE` rather than matching its `lParam` string.
- `RegNotifyChangeKeyValue` is the documented fallback: one notification per call, re-armed after
  each, on a persistent thread. Not used. [docs](https://learn.microsoft.com/en-us/windows/win32/api/winreg/nf-winreg-regnotifychangekeyvalue)

## Latency with no webview open

From M8 the dashboard is closed at idle, so a tray lock starts WebView2 cold with no other webview
alive. Harness S41 with `--no-dashboard` measures it, with a 16 s pause after each lock so
WebView2 has exited (S43 saw it go 735-803 ms after the dashboard closed). [tested 2026-10-12]

| Machine                      | Build                                  | Mode         | Cold   | Warm p50 | Warm p95 | Max     |
| ---------------------------- | -------------------------------------- | ------------ | ------ | -------- | -------- | ------- |
| Windows 11 25H2, ASUS laptop | debug, `custom-protocol` (harness S41) | M6 dashboard | 393 ms | 260 ms   | 283 ms   |         |
| Windows 11 25H2, ASUS laptop | debug, `custom-protocol` (harness S41) | no webview   | 862 ms | 826 ms   | 1540 ms  | 1540 ms |
| Windows 11 25H2, ASUS laptop | release, tray locks (M8.md step 20)    | no webview   |        |          |          |         |

With no webview, every lock pays for WebView2's start: about three times the M6 warm numbers, and
the worst debug lock left 460 ms of the 2 s budget. Harness S34, which locks while the app and its
dashboard are still starting, confirmed after 1970 ms. The release measurement (step 20) decides
whether a pre-created hidden overlay gets an ADR.

## Idle cost

- No Raw Input registration in the app process: S42 logged 0. [tested 2026-10-12]
- No WebView2 process after the dashboard closes: S43, gone 735-803 ms after the close.
  [tested 2026-10-12]
- Both `keyclean.exe` at 0 % CPU with the dashboard closed (M8.md step 3). Memory: pending.
  [tested 2026-10-12]

## Open

- Whether Windows 11 shows tray icons on a second monitor's taskbar (the only way to the lock menu
  with the mouse when the overlay covers the main taskbar).
