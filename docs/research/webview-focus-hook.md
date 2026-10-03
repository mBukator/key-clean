# M1 findings - WebView2 focus and the keyboard hook, and keys a lock can't block

Findings from Max's M1 manual runs on Windows 11 25H2 (2026-10-02) and the harness, after Phase 0
(`phase-0-windows-input.md`). Claims are tagged [docs], [tested] or [assumption].

## 1. The hook goes deaf while its own process's WebView2 window has focus

**Symptom.** With the engine thread inside the Tauri app, and the app's own window focused, Windows
intermittently stopped calling the engine's `WH_KEYBOARD_LL` hook during a lock [tested]:

| Check | Focus           | What happened                                   |
| ----- | --------------- | ----------------------------------------------- |
| F1    | KeyClean window | Win opened the Start menu (should be blocked)   |
| F2    | Notepad         | Win blocked                                     |
| F3    | KeyClean window | Ctrl+Alt+K needed several tries to end the lock |
| F4    | Notepad         | Ctrl+Alt+K ended the lock first time            |

**Experiments X1/X2** [tested]: the same hook running in a different process (`lock_smoke`) blocked
everything and the chord worked first time with the KeyClean window focused (X1), and with VS Code
or Chrome focused (X2). So other processes' WebView2/Chromium windows don't trigger it; only a
WebView2 window **owned by the hook's own process** does.

**Known issue.** tauri-apps/tauri#13919 and
[wudaming00/wh-keyboard-ll-chromium](https://github.com/wudaming00/wh-keyboard-ll-chromium) describe
the same behaviour. No root cause is published; the latter reports that installing a second
low-level hook can mask it [assumption, not reproduced here].

**Reproduction in the harness.** S14 starts the app with the debug autolock, focuses its window with
a synthesized mouse click, sends Ctrl+Alt+K, and reads the app log. It runs after the observer hook
is removed, so no second hook can mask the problem. On the in-process engine it failed: the session
ended with `UserRequest` (at app close), not `Emergency` [tested].

**Decision.** The engine moved to its own process, which has no WebView (ADR 0009). S14 and F1/F3 are
re-run to confirm.

## 2. Keys a user-mode hook can't block

| Keys                        | Result during a lock                                       | Status             |
| --------------------------- | ---------------------------------------------------------- | ------------------ |
| Ctrl+Alt+Del                | Security screen appears; lock continues after Cancel       | [docs], [tested]   |
| Win+L                       | Workstation locks; KeyClean ends with `SessionLock`        | [tested]           |
| Win+G                       | Gets through (Game Bar opens)                              | [tested], see note |
| Fn + brightness keys        | Get through                                                | [tested]           |
| Typing into an admin window | Gets through (fails open, ADR 0005); Notepad stays blocked | [tested]           |

Treat Win+G and Fn/brightness like Win+L: documented as unblockable, not bugs. Fn and brightness keys
are commonly handled by keyboard firmware or ACPI and never reach the hook [assumption, consistent with
the result].

**Note on Win+G.** Harness check S6 sends a synthesized Win+G and it is blocked, while the real key
got through. Either Game Bar sees the physical key some other way, or the real-key test ran with the
KeyClean window focused and hit the focus bug above. Re-check Win+G with Notepad focused after the
engine-process change; if it is then blocked, update this row.

## 3. Other Part B results (2026-10-02)

- Ctrl+Alt+Del then Cancel: the lock continued to its timer and printed `notice: DrainTimedOut`, as
  predicted (the Ctrl, Alt and Del releases happen on the secure desktop) [tested].
- Sleep: the window reported the lock as ended by the computer being locked (`SessionLock`), not
  `Suspend` [tested]. Probably Windows locks the session before it suspends, and that notification
  arrives first [assumption]. Either reason releases input, so this is acceptable.
- On-Screen Keyboard clicks: blocked during a lock (ADR 0007) [tested].
- Sticky Keys: not tested (disabled on the test machine).
