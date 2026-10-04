# Design brief for Claude Design

The feature-only brief the designs in this folder were made from (2026-10-03). It describes features,
screens, states and constraints, and leaves visual direction to the designer. Reuse it, or a section of
it, when asking for the screens that aren't designed yet.

---

Design the complete UI for KeyClean, a small open-source Windows 10/11 desktop utility. Below are the
features, screens, states and constraints. Visual style, layout and branding are yours to decide.

## What KeyClean is

KeyClean temporarily locks the keyboard (and optionally the mouse/touchpad) so the user can clean their
keyboard, laptop or screen without typing into apps or triggering shortcuts. It always gives control
back: every lock ends by a timer, an emergency shortcut, a hard safety limit, or when the app closes. It
is a single-purpose tool, not a system optimizer. There are no accounts, no sign-in, no cloud, no
telemetry, no ads and no upsells, so don't design any of those.

## Platform and constraints

- Windows 10 and 11 desktop app, in a normal resizable window plus a system tray icon. Built with
  React + Tailwind CSS v4 inside a Tauri (WebView2) window, so components should map cleanly to
  HTML/CSS.
- The app mostly lives in the system tray. The main window opens on demand.
- Every layout must tolerate text about 2x longer than English (future translations). No text baked
  into images.
- Theme setting: System / Light / Dark. Provide both light and dark versions.
- Accessible: keyboard navigable when not locked, visible focus, screen-reader labels, sufficient
  contrast, nothing conveyed by color alone.

## Main window: four areas

Clean (default), Devices, Test, Settings. Clean and Settings matter most.

### 1. Clean (the main screen)

Purpose obvious at a glance: "lock my keyboard so I can clean it".

- Primary action: Lock (starts a cleaning session).
- Duration choice: presets 30 seconds, 1 minute, 2 minutes (default), 5 minutes, plus Custom. Custom
  lets the user enter a length from 10 seconds up to the "Maximum lock duration" setting (default 30
  min, at most 60 min). An invalid custom value shows an inline message.
- What to lock: Keyboard (on by default) and Mouse (off by default). Touchpad follows the mouse choice
  and is marked "Limited" (see Devices). A clear note says every connected keyboard is locked during a
  session (there is no per-keyboard selection yet).
- The last used duration and lock choices are remembered and preselected next time.
- Always-visible reminder of the emergency unlock shortcut: Ctrl + Alt + K.
- States to design:
    - Ready (idle).
    - Starting (brief).
    - Locked: countdown in mm:ss, an "Unlock now" button usable with the mouse when the mouse isn't
      locked, and the emergency shortcut.
    - Unlocking (brief).
    - Session finished: "Cleaning complete. Your keyboard has been unlocked." after the timer; other
      end reasons get their own short message (below).
    - Engine unavailable: Lock is disabled and an error is shown.
    - Warning shown before locking when an administrator window (e.g. Task Manager) is in front:
      "KeyClean can't block typing into administrator windows" with the option to continue anyway.
- End reasons to show after a session:
    - the time ran out
    - you pressed the emergency shortcut
    - the safety time limit was reached
    - you unlocked it
    - the computer went to sleep
    - Windows was signing out or shutting down
    - the computer was locked
    - the session was switched
    - something went wrong, so input was released
- Development builds only: a small "DEV CAP" indicator (locks capped at 15 s) with a hint explaining
  it.

### 2. Full-screen overlay (shown during every lock)

Covers the screen while locked. The lock only starts once the overlay is visible.

- Dark, near-black full-screen background so dust, smudges and fingerprints on the display become
  visible. This is a feature: the session doubles as screen cleaning.
- Shows:
    - "KeyClean" and that input is locked
    - what is locked (Keyboard / Mouse / Touchpad)
    - the remaining time as a large countdown (mm:ss, ticking every second)
    - the emergency unlock shortcut, always visible: "Emergency unlock: Ctrl + Alt + K"
    - "Press the shortcut to unlock immediately."
- "Unlock now" button on the overlay only when the mouse is not locked.
- Clearly shows when the session is about to end (last few seconds), then disappears immediately on
  unlock.
- The display is kept awake for the whole session (no dimming or sleep).
- Appears on the monitor the cursor is on. Other monitors are untouched for now; design for one
  monitor, but don't assume a specific resolution (from 1366x768 up to 4K, any scaling).
- Should feel calm and intentional, not alarming.
- Failure state: if the overlay can't be shown, nothing is locked and the main window shows an error.

### 3. Devices

Live list of detected input devices, updating when devices are plugged in or removed.

- Grouped: Keyboards, Mice, Touchpads.
- Each device: friendly name (or "Unnamed keyboard"), connection type where known (USB, Bluetooth,
  Internal), and a lock capability: Supported / Limited / Unsupported, with a short explanation of
  Limited ("touchpad gestures may still get through") and Unsupported.
- Empty states ("No keyboards detected"), and a notice when a device disconnects or reconnects during
  a lock ("Keyboard disconnected." - the lock carries on).
- Informational for now: no per-device toggles.

### 4. Test (Keyboard Tester, ships after the first release)

- Visual on-screen keyboard. Pressing a physical key highlights it in real time (pressed / released).
- Details for the most recent key: key name, state, scan code, virtual-key code, time since last
  detected.
- Stuck-key detection: "Possible stuck key: Shift - pressed for 4.2 seconds", without flagging normal
  long presses.
- Test report: keys detected count, stuck keys, keys that never responded (e.g. "F7 - no response"),
  with Copy report and Save report actions.
- Privacy is a feature here: a visible note that nothing typed is stored or sent. The tester shows live
  state only and keeps no history.
- Start/stop testing and reset.

### 5. Settings

- General: Start with Windows (on), Start minimized (on), Show notifications (on).
- Locking: Default duration (preset or custom), Lock keyboard (on), Lock mouse/touchpad (off), Keep
  display awake during a session (on), Remember last choices (on).
- Shortcuts:
    - Start lock: Ctrl + Shift + K. Configurable, with a "press new shortcut" capture state and an
      error if it's taken.
    - What the start shortcut does: Lock keyboard / Lock selected devices.
    - Emergency unlock: Ctrl + Alt + K, shown read-only.
- Safety: Maximum lock duration (default 30 min, 1-60 min), with a one-line explanation that it is a
  hard limit no lock can exceed.
- Appearance: Theme System / Light / Dark.
- Advanced: Developer diagnostics, Log level, Open logs folder, Reset settings (with confirmation).
  Logs contain session events only, never keystrokes; say so.
- About: version, license (Apache-2.0), source code link, privacy statement (no telemetry, no network,
  no keystroke storage).

### 6. System tray

Tray icon with idle and locked variants. Right-click menu:

- Idle: "KeyClean", status "Ready", Lock keyboard, Lock selected devices, Open KeyClean, Settings,
  Quit.
- During a session: "KeyClean", status "Cleaning - 01:42" (live), Unlock, Show overlay, Quit.
- Tooltip text for both states.

### 7. Notifications (Windows toasts, can be turned off)

- Session started: "Keyboard locked for 2 minutes."
- Session complete: "Cleaning complete. Keyboard unlocked."
- Problem: "KeyClean could not access the selected input device."
- Session ended early, for example by sleep or the safety limit.

### 8. Errors (shared pattern for every screen)

Every error is a plain-language sentence plus an expandable "View technical details" section with the
raw details. Never show a bare error code. Examples:

- "KeyClean couldn't start its input engine."
- "Windows didn't allow KeyClean to lock the keyboard. Nothing was locked."
- "Something went wrong during the lock, so KeyClean released your keyboard."
- "KeyClean's input engine has stopped. Restart KeyClean to lock again."
- "A lock is already in progress."
- "That lock duration isn't valid."

## Deliverables

- Every screen and state above, in light and dark.
- The overlay at a small laptop resolution and a large monitor resolution.
- The main window at its minimum size and with 2x-length text.
- Tray menus (both states) and the notifications.
- A component inventory: buttons, toggles, duration selector, device row with capability badge,
  countdown, error block with technical details, keyboard key, shortcut capture field.
