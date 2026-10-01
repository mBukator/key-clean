# ADR 0002 - Blocking mechanism: user-mode low-level keyboard hook

- Status: Accepted, 2026-10-01
- Deciders: Max
- Related: invariants 2, 5, 7, 8; `docs/research/phase-0-windows-input.md` (A1-A3);
  `crates/keyclean-core/src/keystate.rs`, `crates/keyclean-win/src/hook.rs`

## Context

KeyClean must stop keystrokes reaching Windows during a lock, and must always give control back. The
blocking mechanism decides what happens when KeyClean crashes, hangs or is killed. Windows offers
several ways to block input; most of them outlive the process.

## Decision

Use **only a user-mode `WH_KEYBOARD_LL` hook**, installed on a dedicated engine thread for the duration
of a session and removed afterwards (invariant 4).

In plain terms: Windows calls our function for every key event before any app sees it, and our function
says "pass" or "block". When our process dies, Windows forgets the function, so the keyboard comes back
on its own. Nothing is written to disk or the registry, and no device is touched.

We never use `BlockInput`, SetupAPI / Device Manager / `pnputil` device disabling, drivers (including
Interception), or the `Scancode Map` registry value.

### Keeping keys balanced (no stuck keys)

Windows tracks which keys are down. If it sees a key go down but never up, that key stays "stuck" (for
example, Shift stays held after the lock). So the hook keeps two 256-bit sets
(`keyclean_core::keystate::KeyTracker`):

- `os_down` - keys Windows believes are down (their key-down reached Windows);
- `swallowed` - keys physically down whose key-down we blocked.

The hook moves through four phases, stored in an atomic byte:

| Phase       | Key-down                                                         | Key-up                                                       |
| ----------- | ---------------------------------------------------------------- | ------------------------------------------------------------ |
| Arming      | Pass, add to `os_down`                                           | Pass, remove from `os_down`                                  |
| Locked      | Block; if not in `os_down`, add to `swallowed`                   | Block only if in `swallowed` (and remove it); otherwise pass |
| Draining    | Block if in `swallowed` (a repeat of a blocked press); else pass | Same as Locked                                               |
| Passthrough | Pass                                                             | Pass                                                         |

- **Arming:** the hook is installed and passes everything. The engine then adds a one-time
  `GetAsyncKeyState` snapshot of held keys to `os_down` (accurate at this point because nothing has been
  blocked yet) and seeds the chord detector, then switches to Locked.
- **Locked:** every key-down is blocked, including repeats of keys held at lock start. Only the release
  of a blocked press is blocked; every other key-up passes, because an unpaired key-up is harmless while
  a blocked one could leave a key stuck.
- **Draining:** the session has ended (chord, timer, hard deadline) but the hook stays until every key
  in `swallowed` is released. The drain gives up once no blocked key has produced an event for **2 s**
  (`DRAIN_IDLE_TIMEOUT`, which covers key-ups that never arrive, e.g. after Ctrl+Alt+Del), and after
  **30 s** at most (`DRAIN_MAX`), and never past the hard deadline: there the hook (and the watchdog)
  switch to passthrough and release everything. Auto-repeat of a held key keeps it alive, so the user's still-held
  Ctrl+Alt+K never repeats into Windows however long it's held. New presses pass normally throughout.
- **Passthrough:** the watchdog's override; everything passes.

The emergency chord (Ctrl+Alt+K, either side, AltGr counts) is detected inside the hook from its own
modifier tracking, never from `GetKeyState` (invariant 7). K is also matched by physical position, so
the chord works on any layout.

**No `SendInput`.** We never inject fake key-ups to "fix" state; the balanced model makes them
unnecessary, and injected events could themselves be blocked or reach the wrong window.

## Alternatives

- **`BlockInput`** - blocks keyboard and mouse together, and Ctrl+Alt+Del cancels it. Forbidden by
  invariant 2.
- **Disabling devices (SetupAPI, Device Manager, `pnputil`)** - survives a crash; a crash mid-lock
  leaves the keyboard disabled. Forbidden.
- **Scancode Map** - persistent registry state and needs a reboot. Forbidden.
- **Interception or another filter driver** - per-device blocking, but kernel code, reboot, licence and
  a much worse failure mode. Rejected (ADR 0004).
- **Raw Input with `RIDEV_NOLEGACY`** - only affects our own window, doesn't block other apps.
- **Fixing stuck keys with `SendInput` key-ups after unlock** - racy and can type into the wrong window.

## Consequences

- Fail-safe by construction: process death, thread death and hook timeout all release input.
- The same property means Windows can silently remove a slow hook (fails open). The callback must stay
  O(1); detection is Phase 2 (M3).
- Unblockable by design: Ctrl+Alt+Del, Win+L, other desktops (UAC, lock screen), and likely firmware
  keys. These are documented escape hatches.
- A key that keeps auto-repeating past the drain's limit (30 s, or the hard deadline if sooner) after a lock ends (something resting on the
  keyboard) reaches Windows once the drain gives up. That is what is physically happening anyway.
- The pass/block rules are pure functions in `keyclean-core`, fully unit-tested.
