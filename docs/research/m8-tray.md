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
- The app now logs `app raw input registrations: N`; harness S42 asserts 0. [assumption until S42
  runs]

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
- Closing from the taskbar thumbnail or its context menu also sends `SC_CLOSE`. [assumption; M8.md]
- Alt+F4 while the WebView2 content has focus reaches the window as `SC_CLOSE`. [assumption;
  M8.md]

## Taskbar theme

- Microsoft documents only the app colour mode (`UISettings.GetColorValue`, `ColorValuesChanged`).
  [docs](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/ui/apply-windows-themes)
- The taskbar follows the "Choose your Windows mode" setting, stored as `SystemUsesLightTheme`
  (DWORD, 1 = light) under `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize`.
  [assumption: undocumented, widely used; M8.md step]
- A colour mode change broadcasts `WM_SETTINGCHANGE` to top-level windows, hidden ones included.
  [assumption: widely observed; M8.md step] KeyClean re-reads the value on every
  `WM_SETTINGCHANGE` rather than matching its `lParam` string.
- `RegNotifyChangeKeyValue` is the documented fallback: one notification per call, re-armed after
  each, on a persistent thread. Not used. [docs](https://learn.microsoft.com/en-us/windows/win32/api/winreg/nf-winreg-regnotifychangekeyvalue)

## Latency with no webview open

From M8 the dashboard is closed at idle, so a tray lock starts WebView2 cold with no other webview
alive. Harness S41 with `--no-dashboard` measures it. [pending Max's run]

| Machine                      | Build                                  | Mode         | Cold   | Warm p50 | Warm p95 | Max |
| ---------------------------- | -------------------------------------- | ------------ | ------ | -------- | -------- | --- |
| Windows 11 25H2, ASUS laptop | debug, `custom-protocol` (harness S41) | M6 dashboard | 393 ms | 260 ms   | 283 ms   |     |
| Windows 11 25H2, ASUS laptop | debug, `custom-protocol` (harness S41) | no webview   |        |          |          |     |

## Idle cost

[pending Max's run: M8.md step 3 (Task Manager), harness S42 (no Raw Input registration) and
S43 (no WebView2 process after the dashboard closes)]

## Open

- Whether Windows 11 shows tray icons on a second monitor's taskbar (the only way to the lock menu
  with the mouse when the overlay covers the main taskbar).
