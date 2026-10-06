# ADR 0010 - Detect a lost keyboard hook with a session-scoped Raw Input sink

- Status: Accepted, 2026-10-04. Amended by ADR 0013 (2026-10-05): the check also covers the mouse
  hook, and the engine reads each `WM_INPUT`'s header (`RID_HEADER`, device type only) to tell
  keyboard from mouse. It still never reads input contents.
- Deciders: Max (elevated-window policy, 2026-10-04)
- Related: `crates/keyclean-core/src/liveness.rs`, `crates/keyclean-win/src/{engine,hook,raw_input,foreground}.rs`,
  `docs/research/m3-safety.md`, fail-safe matrix, harness checks S17, S20, S22, M3 steps 6 and 7.
  Builds on ADR 0002 (blocking mechanism), ADR 0004 (all keyboards locked) and ADR 0009.

## Context

Windows silently removes a low-level hook whose callback is too slow (`LowLevelHooksTimeout`), and
it can skip a callback without removing the hook. Either way the lock fails open: typing works again
while KeyClean still shows "Locked". Until M3 nothing noticed. The engine needs a second view of the
keyboard to tell "the hook is blocking" from "the hook is gone".

§26 and §47 also ask the engine to notice keyboards being connected and disconnected during a lock,
which needs device notifications.

## Decision

- **During a session only**, the engine registers keyboard Raw Input (usage page 1, usage 6) on its
  hidden window with `RIDEV_INPUTSINK | RIDEV_DEVNOTIFY`, right after installing the hook. It removes
  the registration when the session finishes and when the engine stops. At idle there is no
  registration (invariant 4; checked by harness S22).
- **Privacy.** The engine counts each `WM_INPUT` and passes it to `DefWindowProcW`. It never calls
  `GetRawInputData`, so no key data is read.
- **Hook side.** Each hook callback stores the current QPC tick in an atomic: one store, still O(1)
  (invariant 5).
- **The rule.** On a `WM_INPUT` while locked, the engine records the time and checks 250 ms later.
  The hook is alive if it was called no more than 100 ms before that `WM_INPUT`, or at any time
  after it (`keyclean_core::liveness::hook_alive`). The rule uses timestamps, not counts, because
  some keys (Pause, fake shifts) produce more raw messages than hook calls. It holds whether or not
  blocked keys produce `WM_INPUT`, and whichever of the two arrives first. Neither is documented.
- **When the check fails:**
    - **An elevated window has focus** (or its process can't be inspected) **and KeyClean isn't
      elevated itself**: keys reaching admin windows is the known gap from M1 step 11, not a lost
      hook. An elevated KeyClean sees that input, so for it a miss counts as a lost hook. The lock stays on and the UI
      warns once per session: "Typing reaches administrator windows during a lock." (Max's
      decision.)
    - **No window has focus:** inconclusive. The next keystroke triggers a new check.
    - **Otherwise:** the session ends with `EngineError` and `error.hook_lost`: "Windows stopped
      KeyClean's keyboard lock early. Your keyboard works again."
- **If registration fails,** the lock goes ahead without the check, and a notice says so. This
  follows the precedent of the power-notification notice.
- **Device changes.** The same registration delivers `WM_INPUT_DEVICE_CHANGE`. The engine compares
  each one with the keyboard handles it listed at lock start, and reports `DeviceChanged(Removed)` or
  `DeviceChanged(Arrived)` with no names. Name lookups happen in the app, never on the engine
  thread. As ADR 0004 says, the hook is global, so the lock continues and a newly connected keyboard
  is blocked too. The UI says so. This replaces §26's suggestion of not locking new devices, which a
  global hook can't follow.

## Alternatives

- **Compare raw and hook event counts.** Rejected: Pause and fake-shift sequences give extra raw
  messages and would end good locks.
- **Probe the hook with a tagged `SendInput` key now and then.** Rejected: it polls during a lock and
  injects input. If the hook is gone, the probe key lands in the user's window.
- **Keep Raw Input registered at idle for device notifications.** Rejected: idle KeyClean must see no
  input (invariant 4). M4's live device list will need its own idle-safe mechanism.
- **End the lock on any leak, elevated window or not.** Max chose against it: clicking into Task
  Manager mid-clean would then unlock every other window too.

## Consequences

- A lost hook now ends the lock within about a quarter of a second of the first key that gets
  through, and the user is told, instead of failing open in silence.
- A false positive ends a lock early, which is the safe direction.
- During a lock KeyClean holds a second input registration. The engine reads neither its contents
  nor any key data.
- **Elevated windows, measured on Max's machine.** M3 step 7 (2026-10-04) found typing into an
  elevated window **blocked** by the hook, unlike M1 step 11 (2026-10-02, before ADR 0009), so the
  warning didn't show. It stays as a fallback for setups where keys do reach elevated windows.
- **Known limitation: the Xbox Game Bar.** Win+G opens it even during a lock, physical or synthesized,
  because the Game Bar acts on it outside the hook. While its overlay is open, Windows delivers no Raw
  Input to KeyClean at all. [tested 2026-10-04: harness S25; every other system shortcut leaves the
  check working.] So a lost hook can't be detected until the overlay closes.
    - The lock itself is unaffected: the hook still blocks, and the timer, Ctrl+Alt+K and the hard
      deadline still end it.
    - The check stays silent rather than misfiring.
    - It takes a hook failure and an open overlay at the same time to matter. Accepted rather than
      worked around. M3 step 11 checks the physical case.
