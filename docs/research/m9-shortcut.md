# M9 research - global start shortcut

Sources for ADR 0017. Tags: [docs] official documentation or the locked crate sources,
[tested] measured on Max's machine, [assumption] not yet verified.

## RegisterHotKey

- `RegisterHotKey(hWnd, id, fsModifiers, vk)` defines a system-wide hotkey; Windows posts
  `WM_HOTKEY` to that window's queue when it is pressed. It typically fails if the keystrokes are
  already registered by another hotkey. [docs]
  https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registerhotkey
- `MOD_NOREPEAT` stops auto-repeat from sending more `WM_HOTKEY` messages. [docs, same page]
- Shortcuts with the Windows key are reserved for the operating system, and F12 is reserved for
  the debugger at all times. [docs, same page]
- `MOD_CONTROL`, `MOD_ALT` and `MOD_SHIFT` accept either the left or the right key. [docs, same
  page] So AltGr, which arrives as LCtrl+RAlt, matches a Ctrl+Alt hotkey. [assumption, from the
  AltGr behaviour noted in the project guide's invariant 7]
- Whether `WM_HOTKEY` fires for keys synthesized with `SendInput` isn't documented. Harness
  S45-S47 depend on it and say "shortcut not seen" if it doesn't. [assumption]
- Whether a low-level hook that blocks a key-down keeps Windows from matching the hotkey isn't
  documented. Expected, since the hook runs before the event is processed further. [assumption]
- Whether the hotkey fires while an administrator window is in front: M9.md step 11.
  [needs measurement]

## tauri-plugin-global-shortcut 2.4.1 and global-hotkey 0.8.0

All from the locked sources in `~/.cargo/registry/src/*/`.

- Windows backend (`global-hotkey-0.8.0/src/platform_impl/windows/mod.rs`): a hidden top-level
  window of class `global_hotkey_app` (`WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED |
WS_EX_TOOLWINDOW`, no parent), with the module handle from `__ImageBase`. `RegisterHotKey`
  always with `MOD_NOREPEAT`; `ERROR_HOTKEY_ALREADY_REGISTERED` becomes
  `Error::AlreadyRegistered`. No `SetWindowsHookEx` and no `RegisterRawInputDevices` in the crate.
  [docs]
- On `WM_HOTKEY` the window procedure sends a Pressed event, then spawns a thread that calls
  `GetAsyncKeyState` on the hotkey's key every 50 ms and sends Released once it reads 0. The
  window passes everything else, `WM_CLOSE` included, to `DefWindowProcW`. [docs]
- The plugin's `Builder::with_shortcuts` registers in the plugin's setup with `?`, so one failed
  registration fails the setup. KeyClean registers from its own `setup` instead. [docs:
  `tauri-plugin-global-shortcut-2.4.1/src/lib.rs`]
- `register`, `on_shortcut`, `unregister` and `unregister_all` run on the main thread through
  `run_on_main_thread` and block the caller on a channel until it is done. [docs, same file]
  Tauri runs such a task at once when called on the main thread
  (`tauri-runtime-wry-2.12.1/src/lib.rs`, `send_user_message`), and through the event loop
  otherwise. [docs]
- Plugin errors are flattened to text (`Error::GlobalHotkey(String)`), so "already registered"
  can't be told apart from other OS errors without matching the text. [docs:
  `tauri-plugin-global-shortcut-2.4.1/src/error.rs`]
- The handler for a shortcut runs from the global-hotkey event handler: on the main thread for
  Pressed (inside the window procedure), on the release thread for Released. The plugin doesn't
  hold its shortcut map while a handler runs. [docs]
- The plugin's default permission set is empty. [docs: `permissions/default.toml`]
- Plugins are initialized in `Builder::build` (`app.manager.initialize_plugins`), before the
  app's `setup` runs from `App::run`. So the hotkey window exists when `setup` calls
  `guard_helper_windows`. [docs: `tauri-2.12.1/src/app.rs`; harness S42 counts the windows]

## muda 0.20.0 (tray menu text)

- On Windows an item with an accelerator gets the text `"{text}\t{accelerator}"`; the tab makes
  Windows right-align the rest. Plain text with a tab renders the same and registers no
  accelerator table entry. [docs: `muda-0.20.0/src/platform_impl/windows/mod.rs`]

## Conflicts worth knowing

- Ctrl+Shift+K is VS Code's "Delete line" and Firefox's web console. While KeyClean holds it,
  those apps don't receive it. [assumption, from those apps' default keymaps]

## Measurements

To be filled in from M9.md:

- Harness S45-S48 (step 0).
- Elevated window (step 11): does the hotkey fire, does the overlay take the focus, does typing
  reach the administrator window.
