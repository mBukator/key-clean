# ADR 0013 - Mouse and touchpad lock with a second low-level hook

- Status: Accepted, 2026-10-05
- Deciders: Max (plan approved 2026-10-05: keyboard-only default, swallow the completing chord key,
  read only the Raw Input header, a mouse liveness miss ends the lock)
- Related: M5; §22, §23; ADR 0002 (blocking mechanism), ADR 0004 (per-device strategy), ADR 0007
  (injected input), ADR 0010 (hook liveness, amended here); `crates/keyclean-core/src/{mouse,keystate,policy}.rs`,
  `crates/keyclean-win/src/{hook,engine,raw_input,keys,foreground}.rs`.

## Context

§22 asks for a full mouse lock: no movement, no clicks, no wheel. §23 asks KeyClean to be honest about
touchpads, which Windows may handle in ways a lock can't reach. Until M5 only the keyboard was locked.

ADR 0002 limits blocking to user-mode low-level hooks, whose effect ends with the process. ADR 0004
says a hook can't tell devices apart, so the choice is "every mouse and touchpad" on or off.

## Decision

### What is locked

- A lock has **targets**: keyboard, mouse, or both (`LockTargets`). A lock with neither is rejected
  before anything is installed. The window selects the keyboard only by default; locking the mouse is
  opt-in because it removes the clickable **Unlock now** button.
- The mouse is locked with a **`WH_MOUSE_LL` hook on the same engine thread** as the keyboard hook,
  installed only when the mouse is a target. Windows calls it in the context of the installing thread,
  through that thread's message loop [docs](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelmouseproc).
  Returning nonzero stops the event reaching any window [docs, same page].
- While locked the mouse hook blocks movement, every button (left, right, middle, both side buttons),
  both wheels, and any mouse message it doesn't recognise. `WM_MOUSEHWHEEL` isn't in the documented
  list of hook messages but is treated as a wheel turn [assumption].
- Injected mouse input (`SendInput`, remote tools) is blocked like physical input, as ADR 0007
  decided for keys. The hook doesn't look at `LLMHF_INJECTED`.

### Every exit still works (invariant 1)

- **The keyboard hook is installed for every lock**, also a mouse-only one, because Ctrl+Alt+K is
  detected inside it. In a mouse-only lock it lets keys through.
- **The completing chord key is swallowed** in a mouse-only lock (`keystate::keyboard_phase`). The
  hook decides each key the moment it arrives and KeyClean never re-sends keys (ADR 0002), so Ctrl
  and Alt have already reached the app when K completes the chord. Swallowing K keeps an app shortcut
  or an AltGr+K character from firing in the focused window. Its repeats and its release are blocked
  until it is let go, the same drain as in a keyboard lock. Whichever key completes the chord is the
  one swallowed (the chord fires in any press order).
- **Both hooks enforce the hard deadline** on every event (invariant 6). In a mouse-only lock nobody
  may be typing, so the mouse hook alone must be able to release input at the deadline. The watchdog
  is unchanged.
- Timer, process death and system transitions need nothing new: the session timer and transitions
  end the session, and Windows removes both hooks when the process dies.
- Install order: keyboard hook, then mouse hook. Both pass everything while arming. If the mouse hook
  fails, the keyboard hook is removed again and the user sees "Nothing was locked."

### No stuck buttons (invariant 8)

Buttons follow the key model of ADR 0002 (`mouse::ButtonTracker`): a blocked press swallows its
release, every other release passes, and a press Windows already has down isn't swallowed. The
tracker learns only from hook events. Nothing is read at lock start: buttons don't auto-repeat, a
button held at lock start isn't swallowed so its release passes anyway, and `GetAsyncKeyState`
reports physical buttons, which is wrong for swapped buttons [docs](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getasynckeystate).
The drain after a lock waits for held buttons as well as held keys.

### Lost-hook check for the mouse (amends ADR 0010)

- When the mouse is locked, the engine also registers **mouse Raw Input** (usage page 1, usage 2)
  with the same flags, only during the lock.
- To tell a keyboard `WM_INPUT` from a mouse one, the engine now calls
  **`GetRawInputData(RID_HEADER)`**, which copies only the `RAWINPUTHEADER`: device type, size,
  device handle and `wParam` [docs](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getrawinputdata).
  It never asks for `RID_INPUT`, so no key, button, wheel or movement data is read. This replaces
  ADR 0010's "never calls `GetRawInputData`". Only the type is used; nothing is stored.
- Each hook has its own check, with the ADR 0010 rule: a mouse `WM_INPUT` arms a 250 ms timer, and the
  mouse hook must have been called around that time. If not, the lock ends with
  `error.mouse_hook_lost` ("Windows stopped KeyClean's mouse lock early."), a false alarm being the
  safe direction (Max's decision).
- The elevated-window exception uses the **window under the cursor** for the mouse, because mouse
  input goes there, not to the foreground window. If that window is elevated and KeyClean isn't, the
  lock stays on and the window warns once.

### Devices

- Device notices during a lock say whether a keyboard or a mouse (including a touchpad's mouse
  interface) changed. Mice are only reported when the mouse is locked.
- **Touchpads are Limited** (ADR 0004). A precision touchpad's pointer movement, taps and clicks
  are blocked. Two-finger scrolling, pinch, and three- and four-finger swipes are not: they never
  pass the hook [tested 2026-10-05, ELAN1203, M5 step 7]. The touchpad note in the window names those
  gestures instead of claiming the touchpad is fully locked.

## Alternatives

- **Pass the whole chord in a mouse-only lock.** Simpler, but Ctrl+Alt+K would reach the focused app
  and could fire a shortcut or type an AltGr character. Rejected.
- **Skip the keyboard hook in a mouse-only lock.** Removes the chord, leaving three exits. Rejected
  (invariant 1).
- **Register mice on a second hidden window** so no `GetRawInputData` call is needed. Rejected: a
  second window in the engine for one bit of information the header gives directly, and Raw Input's
  one-window-per-device-class rule would have to be relied on.
- **No liveness check for the mouse.** A mouse hook Windows removes would fail open in silence, the
  bug class M3 fixed for the keyboard. Rejected.
- **Partial mouse lock (clicks only, §22).** Not built: full blocking is what cleaning needs (§22 says
  so). Can be added later as a setting.
- **`BlockInput`, disabling the touchpad, or a filter driver.** Forbidden by invariant 2.

## Consequences

- With both targets locked, the only exits a person can reach are Ctrl+Alt+K and the timer (plus
  Ctrl+Alt+Del and the other unblockable system escapes). The hard deadline and process death still
  apply.
- In a mouse-only lock, the keyboard still works, so the window's **Unlock now** button can be reached
  with Tab and Enter [tested 2026-10-05, M5 step 8].
- A touchpad gesture that produces raw mouse input without a hook call would end the lock as
  "mouse lock stopped early". The harness's `--mouse-diag` measures this before the manual test; if
  it happens, the rule is tuned with that data.
- The engine reads one more piece of Raw Input (the header) during a lock. It still reads no input
  contents, and nothing is registered while idle (invariant 4).
