# M10 research - start with Windows and notifications

Sources for ADR 0018. Tags: [docs] official documentation or the locked crate sources
(`~/.cargo/registry/src/*/`: tauri-plugin-autostart 2.7.0, auto-launch 0.6.0,
tauri-plugin-notification 2.7.0, notify-rust 4.18.2, tauri-winrt-notification 0.8.1, tray-icon
0.25.1, windows 0.62.2), [tested] measured on Max's machine, [assumption] not yet verified,
[undocumented] relied on without Microsoft documentation.

## The Run key

- `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` runs its programs each time this user
  signs in; `HKLM\...\Run` does so for every user. The data is a command line of at most 260
  characters. The order of several entries is indeterminate. [docs]
  https://learn.microsoft.com/en-us/windows/win32/setupapi/run-and-runonce-registry-keys
- "A program that is run from any of these keys should not write to the key during its
  execution." [docs, same page]
- "The system does not provide guarantees about how promptly the programs in the Run key are
  run"; Windows may delay them. [docs, same page]
- The keys are ignored in Safe Mode. [docs, same page]
- Task Manager → Startup apps records a "Disable" under
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run`, a binary value
  with the same name: first byte 2 or 6 when enabled, 3 or 7 when disabled, then a timestamp.
  [undocumented; checked in M10.md]
- Whether Windows keeps a `StartupApproved` value after its `Run` value is deleted (so a later
  entry with the same name starts out disabled) [assumption; M10.md step 11].

## Registry calls (windows 0.62.2)

- `RegSetKeyValueW` creates the subkey if it doesn't exist; for `REG_SZ` the data must be
  null-terminated and `cbData` must include the terminator. [docs]
  https://learn.microsoft.com/en-us/windows/win32/api/winreg/nf-winreg-regsetkeyvaluew
- `RegGetValueW` with no data buffer returns the size; a buffer that is too small gives
  `ERROR_MORE_DATA`; a missing value gives `ERROR_FILE_NOT_FOUND`. `RRF_NOEXPAND` keeps a
  `REG_EXPAND_SZ` as written. [docs]
  https://learn.microsoft.com/en-us/windows/win32/api/winreg/nf-winreg-reggetvaluew
- `RegDeleteKeyValueW` returns `ERROR_FILE_NOT_FOUND` for a missing value. [docs: windows
  0.62.2 binding; behaviour as for `RegGetValueW`, assumption]

## tauri-plugin-autostart 2.7.0 and auto-launch 0.6.0 (not used)

- The plugin builds `AutoLaunchBuilder` without setting a Windows mode; the default is
  `WindowsEnableMode::Dynamic`, which writes `HKLM\...\Run` first and falls back to HKCU only on
  `E_ACCESSDENIED`. An elevated KeyClean would start for every user. The plugin has no way to set
  the mode. [docs: `auto-launch-0.6.0/src/lib.rs`, `windows.rs`; plugin `src/lib.rs`]
- The value is `format!("{} {}", app_path, args.join(" "))` with `app_path` from
  `current_exe()`: the path isn't quoted, and the plugin can't override it. [docs: same files]
  An unquoted path with a space makes Windows try the shorter paths first (as for
  `CreateProcess`'s command line) [assumption].
- `enable()` also writes `StartupApproved\Run` with the "enabled" bytes, undoing a Task Manager
  "Disable". [docs: `windows.rs`]
- `is_enabled()` checks only that a value with the name exists (and Task Manager's bytes), not
  which exe it starts. [docs: `windows.rs`]
- The plugin's default permission set allows enable, disable and is_enabled. [docs:
  `permissions/default.toml`]

## tauri-plugin-notification 2.7.0

- Desktop `show()` builds a `notify_rust::Notification` and shows it on an async task, ignoring
  the result. A failed toast can't be detected. [docs: `src/desktop.rs`]
- On Windows the plugin sets the toast's AppUserModelID to the app identifier, except when the
  exe's folder ends in `target\debug` or `target\release`; then notify-rust uses
  `Toast::POWERSHELL_APP_ID`. [docs: `src/desktop.rs`; `notify-rust-4.18.2/src/windows.rs`]
- With no title, the product name is used. [docs: `src/desktop.rs`]
- Permission is always granted on desktop. [docs: `src/desktop.rs`]
- The plugin adds a JS init script to every webview that replaces `window.Notification`; on
  Windows it makes no IPC call at load. [docs: `src/lib.rs`, `src/init-iife.js`]
- No thread or window at idle; one task per toast. A click is reported only to listeners, and
  KeyClean registers none. [docs: `src/desktop.rs`]
- The default permission set allows notify and more; KeyClean grants none. [docs:
  `permissions/default.toml`]

## Toasts on Windows

- "Without a valid shortcut installed in the Start screen or in All Programs, you cannot raise a
  toast notification from a desktop app." The shortcut carries `System.AppUserModel.ID`; doing
  it in the installer is recommended. [docs]
  https://learn.microsoft.com/en-us/windows/win32/shell/enable-desktop-toast-with-appusermodelid
- So an installed KeyClean needs M11's Start menu shortcut with the AppUserModelID
  `io.github.mbukator.keyclean`. Whether Tauri's NSIS template sets it [assumption; M11].
- A toast banner doesn't take the keyboard focus [assumption; M10.md].
- Windows 11 turns on Do Not Disturb automatically while a full-screen app is in front, and
  queues toasts in the notification centre [assumption; M10.md]. KeyClean raises none during a
  lock anyway.
- Clicking a toast of an installed app with no COM activator may launch the app's shortcut
  [assumption; M11]; a second launch then opens the dashboard.

## tray-icon 0.25.1 at sign-in

- If `Shell_NotifyIconW(NIM_ADD)` fails because the taskbar isn't ready, `TrayIcon::new` still
  succeeds and keeps its window; the icon is added on `TaskbarCreated`. It fails only if its
  window can't be created. [docs: `platform_impl/windows/mod.rs`]

## Measurements

Filled in from M10.md.
