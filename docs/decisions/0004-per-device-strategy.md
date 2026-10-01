# ADR 0004 - Per-device strategy: lock all keyboards for now

- Status: Accepted, 2026-10-01
- Deciders: Max
- Related: §23-§26, §39, §58; `docs/research/phase-0-windows-input.md` (A6, A7);
  `crates/keyclean-win/src/devices.rs`

## Context

The spec lets users pick which device to lock (§25), for example lock the laptop keyboard while an
external one keeps working. The low-level hook, however, doesn't say which keyboard an event came from.

## Decision

- **M1 and the MVP lock all keyboards.** The hook is global.
- The **device list is informational**: KeyClean enumerates keyboards with Raw Input
  (`GetRawInputDeviceList`, `GetRawInputDeviceInfoW`) and the configuration manager for friendly names,
  and shows name and id only. Listing doesn't register for input, so idle KeyClean still sees no
  keystrokes (invariant 4).
- **Per-device locking waits for V1 (M14)**, and only after a prototype proves it can be done safely.
- **Touchpad capability model** (§23), used from M4/M5:
    - **Supported:** keyboards and mice;
    - **Limited:** precision touchpads (their input arrives as mouse input, can't be told apart from a
      mouse in the hook, and gestures may bypass it);
    - **Unsupported:** Fn/firmware keys, Ctrl+Alt+Del, pen and touchscreen.

## Why per-device is hard

To block one keyboard, the hook would need to know which device each event came from. The only source is
Raw Input (`WM_INPUT`), which arrives separately. Matching the two depends on their ordering, which
Windows doesn't document [needs prototype]. Making the hook wait for the matching `WM_INPUT` would break
the O(1) rule (invariant 5) and risk Windows silently removing the hook.

## Alternatives

- **Interception driver** - a kernel filter driver that does identify devices. Rejected: it needs a
  driver install and a reboot, it is LGPL for non-commercial use with a commercial licence otherwise,
  and a driver failure can leave input broken system-wide. It also violates invariant 2.
- **Correlate Raw Input and hook events now** - unproven ordering, breaks O(1). Deferred to a V1
  prototype.
- **Disable the selected device** - persistent, forbidden by invariant 2.

## Consequences

- The UI must say plainly that every keyboard is locked ("Every connected keyboard is locked during a
  session.").
- §25 device selection is reduced to "keyboard on/off" (and later "mouse on/off") until V1.
- A disconnect or reconnect doesn't change what is blocked (fail-safe matrix).
- V1 needs a prototype that measures hook vs `WM_INPUT` ordering on several machines before any design.
