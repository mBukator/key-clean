# ADR 0005 - Privilege level: run as the invoking user (asInvoker)

- Status: Accepted, 2026-10-01. Amended in M7 (2026-10-08): the warning moves to M9.
- Deciders: Max
- Related: `docs/research/phase-0-windows-input.md` (A4); fail-safe matrix "Elevated window focused";
  M1 step 11

## Context

Windows separates apps by integrity level (User Interface Privilege Isolation, UIPI). A normal app runs
at medium integrity; apps "run as administrator" run at high integrity. A medium-integrity hook likely
neither sees nor blocks keystrokes going to an elevated window such as Task Manager or an admin terminal
[needs prototype]. So during a lock, typing into an elevated window may still work.

## Decision

- KeyClean runs **`asInvoker`** - with the user's normal rights, no UAC prompt.
- **No `uiAccess`.** That manifest flag lets an app interact with elevated windows, but it is meant for
  assistive technology, requires a signed binary installed in a secure location such as Program Files
  [docs](https://learn.microsoft.com/en-us/windows/security/threat-protection/security-policy-settings/user-account-control-only-elevate-uiaccess-applications-that-are-installed-in-secure-locations),
  and would make KeyClean a more powerful (and riskier) program than it needs to be.
- An elevated foreground window is treated as a **known fail-open case**: input to it may get through.
  This is safe (the user keeps control) but imperfect for cleaning.
- Later (MVP UI, M7 onwards): warn when the foreground window is elevated, and optionally offer
  "Relaunch as administrator" for users who want full coverage.

In plain terms: KeyClean won't ask for admin rights. If an admin window is in front, it may not be able
to block that window, and it will tell you.

## Alternatives

- **Always run elevated** - UAC prompt on every start, autostart becomes harder, and any bug runs with
  admin rights. Rejected as a default.
- **`uiAccess=true`** - signing and install-location requirements, intended for accessibility tools.
  Rejected.
- **Refuse to lock while an elevated window exists** - too restrictive; Task Manager is often open.

## Consequences

- No UAC prompt, simple installer (NSIS `currentUser`).
- M1 step 11 records the real behavior; the result decides how prominent the warning must be.
- An optional elevated mode would need its own review against every invariant before it ships.

## Amendment (M7, 2026-10-08): the warning moves to M9

M7 was to warn before locking when an elevated window is in front. From the dashboard that can't
happen: clicking Lock makes KeyClean the foreground window, and the overlay takes the focus when
the lock starts. (Later in a lock, focus can still reach an elevated window on another monitor, or by
Alt+Tab in a mouse-only lock; the in-lock notices below cover that.) An elevated window can only be in front when a lock starts without the dashboard, which
is what the global shortcut (M9) adds. So the warning, and the optional "Relaunch as
administrator", move to M9.

M7 only shows the engine's existing in-lock notices (`notice.elevated_window`,
`notice.elevated_window_chord` and `notice.elevated_window_mouse`) in the new dashboard. M3 step 7
found that typing into an admin window was **blocked** once the engine ran in its own process
(ADR 0009). The fail-open case above may therefore be rarer than first assumed; M9 measures it again
before deciding how prominent the warning must be.
