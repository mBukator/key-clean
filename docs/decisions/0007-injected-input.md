# ADR 0007 - Injected input is blocked during a lock

- Status: Accepted, 2026-10-01. Extended to the mouse by ADR 0013 (2026-10-05): injected mouse input
  is blocked the same way; the mouse hook doesn't look at `LLMHF_INJECTED`.
- Deciders: Max (decision 2026-10-01)
- Related: `docs/research/phase-0-windows-input.md` (A5); M1 step 17

## Context

Not all keystrokes come from a physical keyboard. Windows marks synthesized events with
`LLKHF_INJECTED` [docs](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-kbdllhookstruct):
they come from `SendInput`, the On-Screen Keyboard, the touch keyboard, AutoHotkey, remapping tools and
some remote-control software. During a lock, KeyClean has to decide whether these pass or are blocked.

## Decision

- **Block injected input during a lock**, using exactly the same rules as physical input. The hook
  doesn't look at `LLKHF_INJECTED` at all in M1.
- **KeyClean itself injects nothing** (no `SendInput`, see ADR 0002).
- A future setting, **"Allow on-screen keyboard during a lock"**, is noted for when accessibility needs
  come up. It would pass events with `LLKHF_INJECTED` set, and needs its own review (any app could then
  type during a lock).

In plain terms: during a cleaning session nothing types, whether it's a real key, the On-Screen
Keyboard, or a script.

## Alternatives

- **Pass injected input** - lets the On-Screen Keyboard and remote tools work, but also lets remappers
  and macro tools type while the user is wiping the keys, which defeats the purpose. Rejected as the
  default.
- **Pass only known accessibility sources** - the hook can't tell sources apart from `LLKHF_INJECTED`
  alone. Deferred to the future toggle.

## Consequences

- During a lock, the On-Screen Keyboard and the touch keyboard do nothing (M1 step 17 records this).
- Remote-desktop input into the locked machine is likely blocked too [assumption].
- Users who rely on injected input for accessibility need the future toggle.
