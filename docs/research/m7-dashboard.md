# M7 research - MVP dashboard

What the dashboard relies on, how each claim was checked, and what is still open. Decisions are in
ADR 0015. Tags: [docs] from official documentation or the locked crate sources (tauri 2.12.1,
tauri-plugin-store 2.5.0, windows 0.62.2), [tested] measured on Max's machine, [assumption] not yet
verified.

## Keeping the display on

- `PowerCreateRequest` returns a power request object. When it is no longer needed,
  `CloseHandle` "frees the handle and cleans up the object".
  [docs](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-powercreaterequest)
- `PowerRequestDisplayRequired` keeps the display on without user input. The docs add that a
  `PowerRequestSystemRequired` "must be taken in addition" to keep the display on and the system
  out of sleep, so KeyClean takes both.
  [docs](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-powersetrequest)
- "Power requests are terminated upon user-initiated system sleep entry (power button, lid close or
  selecting Sleep from the Start menu)". So the request can't stop a sleep the user asks for, and
  the sleep exit of a lock still works. [docs, same page; checked again in M7.md step 8]
- On Modern Standby systems on battery, system and execution requests end 5 minutes after the
  sleep timeout has passed. Display-on during a long lock on battery may therefore end early.
  [docs, same page] Not a safety issue.
- When a process ends, all its handles are closed.
  [docs](https://learn.microsoft.com/en-us/windows/win32/procthread/terminating-a-process) That the
  request ends with its last handle is [assumption]; M7.md step 7 checks it after a kill.
- `powercfg /requests` lists active requests, but only from an elevated prompt. Run from a normal
  PowerShell it prints "This command requires administrator privileges". [tested, 2026-10-08] The
  harness runs without admin, so it can't check the request (see "Harness" below).
- In windows 0.62.2, `PowerCreateRequest` turns `INVALID_HANDLE_VALUE` into an `Err`, and
  `POWER_REQUEST_CONTEXT_VERSION` (0) lives behind the `Win32_System_SystemServices` feature.
  `display.rs` defines the constant locally instead of enabling that feature. [docs: windows source]
- The unit test `display::tests::acquire_and_release` takes and drops a real request. It touches no
  input. [tested: `cargo test -p keyclean-win display`]

## Settings file (tauri-plugin-store 2.5.0)

- Version 2.5.0 requires tauri `^2.12`. The 3.0 alphas target tauri 3 and don't apply.
  [docs: crates.io index]
- `StoreBuilder::build` ignores a failed load, whether the file is missing or unparsable. The store
  then starts empty, and KeyClean's core fills in defaults. [docs: `store.rs`, `build_inner`]
- `save()` writes with `fs::write`, which isn't atomic. A crash mid-write can leave a corrupt file,
  which loads as defaults next time. Acceptable for three values.
  [docs: `store.rs`, `StoreInner::save`]
- The plugin saves every store again on `RunEvent::Exit`. KeyClean also saves explicitly on each
  change and disables auto-save, so no debounce thread runs. [docs: `lib.rs`, `Builder::build`]
- The path resolves against `BaseDirectory::AppData`, which is
  `%APPDATA%\io.github.mbukator.keyclean\settings.json`. [docs: `resolve_store_path`, tauri path API]
- The plugin registers JavaScript commands, but a window can call them only with a `store:`
  permission in its capability. Neither capability grants one; `no_window_can_use_the_store_plugin`
  checks this. [docs: Tauri capabilities; tested by unit test]
- Tauri initializes plugins before the app's `setup` runs, so `StoreBuilder::new` in `setup` finds
  the plugin's state. [docs: tauri 2.12.1 `app.rs`, `initialize_plugins`]
- `set_max_lock` is a synchronous command, so its file write runs on the main thread. It is a few
  bytes, written only when the user saves the setting. [docs](https://v2.tauri.app/develop/calling-rust/)

## A lock as long as the max lock

- `Session::expired` checks the hard deadline before the session deadline. When they are equal,
  the end reason is `HardDeadline`. [docs: `keyclean-core/src/session.rs`; unit test
  `hard_deadline_wins_after_a_long_gap`]
- The app now shows that end as `timeout` (ADR 0015, option (b)). This is covered by unit tests:
    - `session_meets_hard_deadline_only_at_max_lock`
    - `a_full_length_lock_at_its_hard_deadline_reads_as_timeout`
    - `a_shorter_lock_at_its_hard_deadline_keeps_the_reason`
    - `a_full_length_lock_keeps_other_end_reasons`
- Debug builds can't reach this case: DEV CAP keeps the session at 15 s and the hard deadline at
  20 s. M7.md Part B checks it in a release build.

## Harness

The M7 plan asked for harness scenarios where OS behaviour changes. **None were added.**

- **Custom duration.** The harness locks through the debug autolock, which already takes any length
  from 1 to 15 s. S35 runs a 3 s lock that isn't a preset. [docs: `e2e_autolock` in
  `src-tauri/src/lib.rs`; `overlay_timer_end` in `crates/keyclean-e2e/src/scenarios.rs`] The new range check sits in `lock_input`,
  which only the window calls; it is unit-tested in core (`range_is_ten_seconds_to_max_lock` and
  others), and the engine and timer are unchanged.
- **Display request.** Reading active power requests needs admin. [tested] M7.md step 7 checks it
  instead, from an elevated PowerShell.

## Dashboard

- The UI was checked in a browser against `clean-ready.png`, with the Tauri IPC mocked
  (`@tauri-apps/api/mocks`). No app, engine or hook ran. Checked:
    - Ready, light and dark, matches the design.
    - Custom with an invalid value shows its message.
    - At 420 px the layout is one column.
    - The arrow keys move between tabs.
    - Settings validation works.
      [tested in a browser, 2026-10-08; Max still checks it in WebView2]
- Fonts are system faces only. Cascadia Mono ships with Windows 11; Windows 10 falls back to
  Consolas. [assumption for Windows 10, which isn't available to test]

## Open

- The restyled Starting, Locked, Unlocking, Session finished, Custom, engine-unavailable, DEV CAP and
  Settings screens wait for Max's designs. Each works now with plain styling.
- Display-on is unverified on Windows 10 and on Modern Standby running on battery.
