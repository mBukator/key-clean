# ADR 0003 - System-transition policy: any transition unlocks

- Status: Accepted, 2026-10-01
- Deciders: Max
- Related: invariant 9, §47; `docs/research/phase-0-windows-input.md` (B8, B10);
  `crates/keyclean-win/src/engine.rs`

## Context

A lock can be running when Windows sleeps, shuts down, logs off, locks the workstation, or switches
sessions. Trying to keep or restore a lock across these is complex and risky: the user may come back to
a machine they can't type on. §47 also requires that KeyClean never interferes with shutdown.

## Decision

Any of these ends the session **immediately, without the drain**, because the machine may go away
before a drain could finish:

| Event                                                    | Notification                                      | End reason                            |
| -------------------------------------------------------- | ------------------------------------------------- | ------------------------------------- |
| Sleep / hibernate                                        | `WM_POWERBROADCAST` + `PBT_APMSUSPEND`            | `SystemTransition(Suspend)`           |
| Shutdown, restart, logoff                                | `WM_QUERYENDSESSION`, `WM_ENDSESSION`             | `SystemTransition(EndSession)`        |
| Logoff (session notification)                            | `WTS_SESSION_LOGOFF`                              | `SystemTransition(EndSession)`        |
| Workstation lock (Win+L, Ctrl+Alt+Del → Lock, auto-lock) | `WTS_SESSION_LOCK`                                | `SystemTransition(SessionLock)`       |
| Fast user switching, RDP disconnect                      | `WTS_CONSOLE_DISCONNECT`, `WTS_REMOTE_DISCONNECT` | `SystemTransition(SessionDisconnect)` |

- `WM_QUERYENDSESSION` always returns TRUE: KeyClean **never blocks shutdown**.
- After resume, KeyClean is idle. No lock resumes, and no lock state is ever persisted, so nothing
  survives a restart.
- The notifications arrive at a **hidden top-level window** (`WS_EX_TOOLWINDOW`, never shown, off the
  taskbar) owned by the engine thread, the same thread that owns the hook. A message-only window would
  be simpler but doesn't receive broadcast messages such as `WM_POWERBROADCAST` and
  `WM_QUERYENDSESSION` [docs](https://learn.microsoft.com/en-us/windows/win32/winmsg/window-features#message-only-windows).
- Registrations: `RegisterSuspendResumeNotification(hwnd, DEVICE_NOTIFY_WINDOW_HANDLE)` and
  `WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION)`, both unregistered before the window
  is destroyed. If either fails, the engine reports a notice and keeps running.
- Time uses QPC (`Instant`), which keeps counting through sleep and ignores wall-clock changes. That
  matters less because suspend unlocks anyway, but it means a clock change can never stretch a lock.

In plain terms: if Windows is about to do something big, KeyClean lets go of the keyboard first and
asks no questions.

## Alternatives

- **Pause the lock and resume it after wake** - the user returns to a locked keyboard on the sign-in
  path, and the overlay may not be visible yet. Rejected.
- **Drain before unhooking on transitions** - could delay shutdown or still be running when the machine
  sleeps. Rejected for transitions; the drain is kept for normal endings.
- **Message-only window (`HWND_MESSAGE`)** - misses broadcasts. Rejected.
- **`PostThreadMessage` for commands** - thread messages are lost in modal loops; we post to the hidden
  window instead.

## Consequences

- A sleep or Win+L always ends the session; the user restarts it if they want.
- A key held during the transition may produce an unpaired key-up afterwards, which is harmless.
- Hardening and full testing of shutdown, restart and session switch are Phase 2 (M3); M1 covers Win+L
  and sleep by hand (M1 steps 6 and 12).
