# M5 research - mouse and touchpad lock

What KeyClean relies on to lock mice and touchpads, how each claim was checked, and what is still
open. Decisions are in ADR 0013. Tags: [docs] from Microsoft Learn, [tested] measured on Max's
machine, [assumption] not yet verified.

## The mouse hook

- `WH_MOUSE_LL` is called "every time a new mouse input event is about to be posted into a thread
  input queue", in the context of the thread that installed it, through that thread's message loop.
  [docs](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelmouseproc)
- Returning nonzero "prevents the system from passing the message to the rest of the hook chain or
  the target window procedure". [docs, same page] So blocking a `WM_MOUSEMOVE` stops the cursor
  [assumption until harness S26 measures it with `GetCursorPos`].
- The same `LowLevelHooksTimeout` rule as the keyboard hook applies: at most 1 s since Windows 10
  1709, and a hook that times out is silently removed. [docs, same page] KeyClean's callback only
  reads atomics and a thread-local value (invariant 5).
- `wParam` is one of `WM_LBUTTONDOWN/UP`, `WM_MOUSEMOVE`, `WM_MOUSEWHEEL`, `WM_RBUTTONDOWN/UP`,
  `WM_MBUTTONDOWN/UP`, `WM_XBUTTONDOWN/UP`. [docs, same page] `WM_MOUSEHWHEEL` (horizontal wheel) isn't
  listed; KeyClean treats it as a wheel turn and blocks any unknown message while locked
  [assumption that it is delivered; harness S26 probes it].
- `MSLLHOOKSTRUCT.mouseData` holds the X button in its high word (`XBUTTON1` = 1, `XBUTTON2` = 2).
  `LLMHF_INJECTED` marks injected events; KeyClean ignores it, so injected mouse input is blocked
  too (ADR 0007). [docs](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-msllhookstruct)
  The cursor position `pt` is never read.

## Buttons and stuck state

- `GetAsyncKeyState` works for mouse buttons but reports the **physical** buttons, not the logical
  ones after a left/right swap. [docs](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getasynckeystate)
  So the button tracker doesn't read it at lock start; it learns only from hook events (ADR 0013).
- Mouse buttons don't auto-repeat, so a button held at lock start needs no snapshot: its release isn't
  swallowed and reaches Windows. [assumption, by design; covered by `mouse` unit tests]

## Raw Input for the lost-hook check

- `GetRawInputData` with `RID_HEADER` returns only the `RAWINPUTHEADER` (type, size, device handle,
  `wParam`); `RID_INPUT` would return the data. [docs](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getrawinputdata)
  KeyClean asks only for the header.
- Mouse Raw Input uses usage page 1, usage 2 (Generic Desktop, Mouse), with
  `RIDEV_INPUTSINK | RIDEV_DEVNOTIFY` [docs](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-rawinputdevice).
  KeyClean registers it only during a mouse lock (design choice, ADR 0013).
- Injected mouse input produces mouse `WM_INPUT` too [assumption; harness S31 depends on it and
  prints the count when it fails].

## Touchpads

- Max's machine lists the ELAN1203 precision touchpad and the Logitech receiver's digitizer
  collection as touchpads, and their mouse interfaces as mice [tested 2026-10-05, `print_devices`:
  4 keyboards, 2 mice, 2 touchpads].
- A precision touchpad's cursor movement, taps and clicks arrive as mouse input and reach the hook
  [assumption; M5 step 7].
- Two-finger scrolling, pinch, and three- and four-finger swipes may be handled by the shell or the
  driver without passing the hook [assumption; M5 steps 2 and 7 record which].
- Open risk: if a gesture produces raw mouse input without a hook call, the mouse lost-hook check
  ends the lock early. `--mouse-diag` (harness S33) counts these misses with the check in report-only
  mode.

## Results

To fill in from Part A and M5 steps 2 and 7:

| Question                                        | Result                                                                                               |
| ----------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| Cursor frozen by blocking `WM_MOUSEMOVE`        | Yes [tested 2026-10-05, harness S26]                                                                 |
| Horizontal wheel reaches the hook               | Yes, injected [tested 2026-10-05: the harness startup check found every probe kind, S26 blocked all] |
| Injected mouse input produces mouse `WM_INPUT`  | Yes [tested 2026-10-05, S31: 15 messages]                                                            |
| Touchpad move / tap / click blocked             |                                                                                                      |
| Two-finger scroll blocked                       |                                                                                                      |
| Three- and four-finger swipes blocked           |                                                                                                      |
| Pinch blocked                                   |                                                                                                      |
| Gestures cause liveness misses (`--mouse-diag`) | Touchpad: no, 0 misses in 1405 raw messages over 15 s [tested 2026-10-05, S33]                       |
| Notepad menu bar after Ctrl+Alt+K (mouse-only)  |                                                                                                      |
