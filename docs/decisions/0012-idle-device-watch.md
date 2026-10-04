# ADR 0012 - Watch for devices in the app with `CM_Register_Notification`

- Status: Accepted, 2026-10-04
- Deciders: Max (plan approved 2026-10-04)
- Related: M4; ADR 0004 (per-device strategy), ADR 0009 (engine process), ADR 0010 (hook liveness),
  ADR 0011 (engine restart); `crates/keyclean-win/src/device_watch.rs`, `src-tauri/src/devices.rs`;
  `docs/research/m4-devices.md`.

## Context

M4 needs a device list that updates by itself when something is plugged in or unplugged, also while
KeyClean is idle. M3's device notices (`WM_INPUT_DEVICE_CHANGE`) only exist during a lock, because
Raw Input is registered only then (invariant 4: at idle KeyClean sees no input). CLAUDE.md's
"Engine model" lists device notifications among the things the engine's hidden window handles.

## Decision

- **The watcher lives in the app process**, next to the enumeration that already runs there. It uses
  `CM_Register_Notification` for three device interface classes (keyboard, mouse, HID), which
  needs no window. Device interface notifications are not input registrations, so idle KeyClean
  still sees no input.
- **A notification only means "something changed".** The callback sends `()` on a channel and
  returns; it never calls `CM_Unregister_Notification` and does no work. A refresh thread blocks
  on the channel (no polling), waits for 400 ms of quiet (one plug-in fires several notifications,
  and names can lag behind the first), lists devices again, and emits an event only if the list
  changed.
- **No device names or ids cross the engine pipe**, as in M3. The engine, the wire protocol and
  ADR 0011's restart path are untouched, and the list stays live even if the engine is gone.
- The `unsafe` code stays in `keyclean-win` (`DeviceWatch`, which unregisters on drop). The pure
  classification (kind and Supported / Limited / Unsupported) is in `keyclean-core::devices`.
- During a lock, M3's engine-side keyboard notices keep working as before.

## Alternatives

- **`RegisterDeviceNotification` on the engine's hidden window.** Fits the CLAUDE.md wording, but it
  needs a new wire event, ties the list to the engine process (a dead engine means a stale list),
  and has to be unregistered before `DestroyWindow`.
- **Poll `GetRawInputDeviceList`.** Rejected: idle cost, and CLAUDE.md forbids polling loops.
- **Raw Input `RIDEV_DEVNOTIFY` at idle.** Rejected: it registers for input.

## Consequences

- CLAUDE.md's wording about device notifications on the engine window is now only true for the
  lock-time notices; the idle watcher is app-side.
- `CM_Register_Notification` needs Windows 8 or later; KeyClean targets Windows 10 and 11.
- If registration fails, the list still shows once and the window says it may be out of date
  (`error.device_watch`).
