# ADR 0018 - Start with Windows and notifications

- Status: Accepted, 2026-10-18. Max approved the M10 plan (D1 option C, D2 option ii, D3-D11 as
  proposed) on 2026-10-17, and on 2026-10-18 the points settled during the build: "Start
  minimized" stays switchable when Start with Windows is off; the problem toasts follow the
  "Show notifications" switch; the debug-only `KEYCLEAN_E2E_START_MINIMIZED` and
  `KEYCLEAN_E2E_NOTIFICATIONS` variables; the Start with Windows switch showing on (with a note)
  while Task Manager has it turned off, so it can still remove the entry; a dev build hiding the
  "another copy" note; the extra roadmap item "idle cost recorded". Manual verification pending.
- Deciders: Max
- Related: M10; §29, §30, §31, §32 "General", §46; invariants 3, 4, 9, 12; privacy invariants;
  ADR 0001 (idle cost), 0016 (tray, window lifecycle), 0017 (global shortcut);
  `crates/keyclean-core/src/{startup,notify}.rs`, `crates/keyclean-win/src/autostart.rs`,
  `src-tauri/src/{general,notify}.rs`, `src/ui/settings/GeneralSettings.tsx`,
  `docs/research/m10-startup-notifications.md`, harness S49-S51. Deviates from the roadmap
  (official autostart plugin), §29 (defaults, "Show window on startup") and §31 ("Session
  started").

## Context

§29 asks KeyClean to start with Windows, minimized, both on by default. §31 asks for optional
notifications when a session starts, when it completes, and when there is a permission or
configuration problem. With autostart, KeyClean runs from sign-in to sign-out, so its idle cost
and its behaviour with no window open matter more than before.

The project guide prefers official Tauri plugins. The locked sources of
`tauri-plugin-autostart` 2.7.0 and its `auto-launch` 0.6.0 showed three problems on Windows
(research note): it tries the machine-wide `Run` key first and uses it when KeyClean is elevated;
it writes the exe path without quotes; and enabling rewrites Task Manager's `StartupApproved`
record, which undoes a user's "Disable" there. It also reports "enabled" whatever exe the entry
points to.

## Decision

### Start with Windows: one value of our own (D1, option C)

- **What is written:** `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, value `KeyClean`,
  type `REG_SZ`, data `"<exe path>" --autostart`. The path is quoted; a `\\?\` prefix is removed
  (`\\?\UNC\` becomes `\\`); the command line must fit 260 UTF-16 units [docs: "Run and RunOnce
  Registry Keys"]. Never the machine-wide key.
- **Task Manager's choice is read, never written.** A "Disable" in Task Manager → Startup apps
  shows as "Turned off in Task Manager", and the user turns it back on there. The format of
  `StartupApproved\Run` is undocumented (first byte odd = disabled); a value that can't be read
  counts as enabled.
- **The registry is the record.** Nothing about starting with Windows is in the settings file.
  Settings → General reads the registry every time it asks, so the switch always shows what
  Windows will do: on, off, turned off in Task Manager, or another copy of KeyClean (the entry
  starts a different exe, for example a dev build or an older install).
- **Writes only from the switch.** Nothing is written at startup. A program started from the
  `Run` key shouldn't write to it [docs, same page], and a "repair" at startup would undo the
  user's choices. Turning the switch on replaces another copy's entry; turning it off deletes
  the value.
- `keyclean-win` makes the registry calls (`RegGetValueW`, `RegSetKeyValueW`,
  `RegDeleteKeyValueW`); `keyclean-core` builds and reads the command line and decides the state,
  with unit tests. No new dependency.

### Defaults (D2, option ii)

- **Debug builds never write the entry.** The switch is disabled with "Only release builds of
  KeyClean can start with Windows.", so dev runs and the harness never point Windows at
  `target\debug`. A dev build doesn't show the "another copy" note, which asks to turn the
  switch on.
- **Release builds write it only when the user turns the switch on.** Nothing registers itself
  on first run, so a downloaded exe never adds itself to startup unasked.
- §29's "on by default" moves to the M11 installer: a "Start KeyClean when I sign in" checkbox,
  ticked by default. The uninstaller removes the `Run` value and KeyClean's `StartupApproved`
  value.

### Start minimized (D3)

- One setting, **Start minimized**, on by default (`startMinimized` in the settings file). §29's
  "Show window on startup" is the same choice reversed, so it isn't a second switch.
- It applies only to a start at sign-in (`--autostart`). A manual launch always opens the
  dashboard (ADR 0016).
- It stays in the tray only if the tray icon exists; without one, the dashboard opens, so there is
  never a KeyClean with no window and no icon. (tray-icon 0.25.1 doesn't fail when the taskbar
  isn't ready yet; it adds the icon when Explorer starts.)
- **A second launch with `--autostart`** (Windows starting KeyClean while it already runs) exits
  and leaves the running instance as it is. Any other second launch opens the dashboard, as
  before.

### Notifications (D5, D6, D7)

- **The official `tauri-plugin-notification` ~2.7**, called from Rust only. No window gets its
  permissions (a test checks the capability files). It adds a small script to every webview,
  the overlay included, that replaces `window.Notification`; with no permissions it can do
  nothing.
- **Which toasts:**

| When                                                             | Text                                                                  |
| ---------------------------------------------------------------- | --------------------------------------------------------------------- |
| A lock ran its full length                                       | "Cleaning complete. Keyboard unlocked." (mouse, or both, by targets)  |
| The safety limit ended it                                        | "The safety time limit was reached, so KeyClean unlocked your input." |
| Sleep, a locked computer, a switched session ended it            | "KeyClean unlocked your input because …"                              |
| At a start at sign-in: the start shortcut couldn't be registered | `error.shortcut_unavailable`                                          |
| At a start at sign-in: the engine didn't start                   | `error.engine_start`                                                  |
| The engine stopped and can't be restarted                        | `error.engine_stopped`                                                |

- **Never:** while a lock is starting or running (the overlay says it all, so §31's "Session
  started" isn't shown); while the dashboard is open (it shows the same); when the user ended
  the lock (Ctrl+Alt+K, Unlock, Quit, a lost overlay); when Windows signs out or shuts down;
  after an engine error, which opens the dashboard instead (ADR 0016); with "Show notifications"
  off (`notifications` in the settings file, on by default).
- **No key data:** a toast carries the end reason and what was locked. The log records
  `notification: requested (<kind>)`, or `notification: failed` if the plugin couldn't prepare
  the toast.
- **Where toasts come from:** Windows shows a desktop app's toasts only under an
  AppUserModelID with a Start menu shortcut [docs: "How to enable desktop toast notifications
  through an AppUserModelID"]. From `target\debug` or `target\release` the plugin uses Windows
  PowerShell's, so M10 tests toasts there, shown as "Windows PowerShell". An installed KeyClean
  uses `io.github.mbukator.keyclean`; M11's installer must give its Start menu shortcut that ID.
  A copied exe with no shortcut shows no toasts.
- **Failures are silent:** the plugin drops Windows' answer, so KeyClean can't tell a toast
  wasn't shown.
- **During a lock nothing is raised,** so a toast can't cover the overlay or take its focus. The
  end toast comes after the overlay is gone. Do Not Disturb and full-screen suppression are
  Windows' choice; a suppressed toast goes to the notification centre.

### Settings → General (D8, D9)

- At the top of Settings: Start with Windows (with its status line), Start minimized, Show
  notifications. Commands (main window only): `get_general_settings`,
  `set_start_with_windows`, `set_start_minimized`, `set_notifications`. A failed change keeps
  the entry as it was and shows the plain-language error with technical details (§46).
- Start minimized stays switchable when Start with Windows is off (pending Max's OK): it only
  matters at sign-in, and debug builds can still test it.
- Plain styling until Max's design arrives.

### Debug-only harness variables

`KEYCLEAN_E2E_START_MINIMIZED=0|1` and `KEYCLEAN_E2E_NOTIFICATIONS=0|1` replace the settings,
so the harness doesn't depend on Max's settings file. The harness turns notifications off for
every check except S51. Release builds don't contain them.

## Alternatives

- **`tauri-plugin-autostart` as it is (option A).** Official, but machine-wide when elevated,
  an unquoted path, and it overrides Task Manager. Rejected.
- **`auto-launch` directly, current user only, path quoted by us (option B).** Fixes two of the
  three, but still rewrites `StartupApproved` on enable. Rejected.
- **Register on a release build's first run (D2 option i).** Matches §29's default, but a
  portable exe would add itself to startup unasked. Rejected in favour of the installer.
- **Repair the entry at startup** (point it at the running exe). Undoes the user's choices and
  writes the `Run` key while running from it. Rejected.
- **A "Session started" toast.** Hidden by the overlay and likely suppressed while a full-screen
  window is in front; noise. Rejected.
- **Toasts for every end reason.** The user's own unlock needs no toast. Rejected.
- **Reorder M10 and M11** so toasts are tested with the installed AppUserModelID. Not needed:
  content and timing are testable from `target\debug`; attribution is an M11 check.

## Consequences

- With Start with Windows on, KeyClean runs from sign-in: two `keyclean.exe` processes, the tray
  icon and the start shortcut's registration. Idle cost is measured in M10.md.
- Windows may delay `Run` entries after sign-in [docs], so the start shortcut may not work for
  a while after sign-in.
- The engine's 5 s startup handshake runs during sign-in load; if it fails, the tray shows
  "Engine unavailable" and a toast says so. M10.md step 8 checks the working case; the
  failure toast is checked by code review only.
- A user who disabled KeyClean in Task Manager turns it back on there; KeyClean doesn't fight
  that choice.
- New dependency: `tauri-plugin-notification` (official plugin), bringing `notify-rust` and
  `tauri-winrt-notification` (plus macOS- and Linux-only crates, not built on Windows).
- M11 must give the Start menu shortcut the AppUserModelID, offer the startup checkbox, and
  remove the `Run` and `StartupApproved` values on uninstall.
