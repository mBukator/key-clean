# KeyClean — Desktop App Specification

> **KeyClean** is a lightweight Windows utility for temporarily locking keyboard and pointing-device input so the user can safely clean, inspect, or work around their input devices without accidental input.

**Platform:** Windows only  
**Primary user:** The developer / owner  
**Distribution:** Open source  
**Primary language:** English  
**Future languages:** Russian → Ukrainian → major world languages  
**Current product stage:** Pre-development / specification

---

# 1. Product Vision

KeyClean should be a small, reliable Windows utility built around one simple action:

> **Lock input → clean the keyboard → unlock safely.**

The application should feel like a native utility rather than a large desktop suite.

The core experience should be:

1. Open KeyClean.
2. Choose what to lock.
3. Choose a duration.
4. Start the cleaning session.
5. A full-screen overlay clearly indicates that input is locked.
6. Clean the keyboard/device.
7. The timer expires or the user uses the emergency unlock shortcut.
8. Input is restored.

The application should prioritize:

- reliability
- safety
- simplicity
- low resource usage
- privacy
- open-source transparency
- excellent Windows integration

---

# 2. Product Principles

## 2.1 Small tool, not a system optimizer

KeyClean should **not** become a generic PC cleaner.

It is specifically an input-device utility.

Do not add unrelated functionality such as:

- registry cleaning
- disk cleaning
- antivirus
- RAM optimization
- startup optimization
- browser cleaning
- driver updating
- system tweaking

Those features are outside the product scope.

---

## 2.2 Safety before appearance

The most important part of KeyClean is not the UI.

It is making sure that the user can **always regain control of the computer**.

Every version must have:

- automatic timeout
- emergency unlock
- crash/failure recovery
- safe handling of sleep/wake
- safe handling of application termination
- safe handling of device disconnection

A beautiful UI is useless if the application can accidentally leave the keyboard locked.

---

## 2.3 No keylogging

KeyClean must not collect, store, transmit, or analyze normal user keystrokes.

The distinction is:

```text
Keyboard input
      ↓
Input interception
      ↓
If locked → discard/block
If unlocked → pass through normally
```

It should never become:

```text
Keyboard input
      ↓
Record
      ↓
Store
      ↓
Analyze
```

This is particularly important because an application with keyboard-level access naturally requires a high level of user trust.

---

# 3. Target Platform

## Windows only

Initial supported platform:

- Windows 10
- Windows 11
- x64

ARM64 support can be considered later.

The first implementation should be designed specifically around Windows APIs rather than attempting to create a cross-platform abstraction immediately.

This makes the first version simpler and allows Windows-specific behavior to be implemented correctly.

---

# 4. Application Name

## KeyClean

Working product name:

**KeyClean**

Possible executable:

```text
KeyClean.exe
```

Possible repository:

```text
keyclean
```

Possible package/project namespace:

```text
KeyClean
```

The name can be changed later if necessary, but development should use **KeyClean** for now.

---

# 5. Target Users

## Primary user

The primary user is the developer/owner.

This means the first release does **not** need to optimize every decision for a mass-market audience.

The goal is:

> Build something genuinely useful for daily personal use, then make it clean enough and documented enough to open-source.

---

## Secondary users

Once open-sourced, potential users include:

- mechanical keyboard users
- laptop users
- PC users who regularly clean keyboards
- IT technicians
- PC repair technicians
- people testing keyboards
- people troubleshooting stuck keys
- developers who need to temporarily disable input
- people with multiple keyboards connected
- streamers/content creators who need predictable input locking

---

# 6. Core Use Cases

## Use case 1 — Cleaning a laptop keyboard

User wants to clean their keyboard.

```text
Open KeyClean
      ↓
Keyboard: ON
Mouse: OFF
      ↓
Duration: 2 minutes
      ↓
Start
      ↓
Full-screen overlay
      ↓
Clean keyboard
      ↓
Timer reaches 00:00
      ↓
Keyboard automatically unlocks
```

---

## Use case 2 — Cleaning an external keyboard

```text
External keyboard connected

Select:
☑ Keychron Keyboard
☐ Mouse
☐ Other keyboard

Start
```

Only the selected device should be affected if device-specific blocking is supported.

---

## Use case 3 — Testing a keyboard

Later:

```text
Open KeyClean
      ↓
Keyboard Tester
      ↓
Press every key
      ↓
See which keys are detected
      ↓
Identify possible stuck/non-working keys
```

---

## Use case 4 — Emergency unlock

If something goes wrong:

```text
Keyboard locked
      ↓
User presses emergency shortcut
      ↓
KeyClean detects shortcut
      ↓
Input unlocked immediately
```

This must work without requiring the user to click a UI button.

---

# 7. MVP Scope

The first usable version should focus on these features:

### Required

- Windows 10/11 support
- keyboard lock
- keyboard unlock
- optional mouse lock
- optional trackpad/touchpad lock where technically applicable
- external keyboard detection
- device selection where technically possible
- timer
- automatic unlock
- emergency unlock shortcut
- hard safety timeout
- full-screen overlay
- Windows system tray
- global shortcut
- startup with Windows
- basic settings
- permission/error handling
- logging without keystroke contents
- crash/failure safety

### Not required for MVP

- keyboard remapping
- keyboard heatmaps
- advanced diagnostics
- cloud sync
- accounts
- telemetry
- analytics
- AI
- subscription system
- multi-platform support
- localization beyond English

---

# 8. Main Application Structure

The application should have four main areas:

```text
KeyClean
│
├── Clean
├── Devices
├── Test
└── Settings
```

For the MVP, **Clean** and **Settings** are the highest priority.

**Devices** can initially be simple.

**Test** can be implemented after the locking system is stable.

---

# 9. Main Dashboard

The main window should be intentionally minimal.

Concept:

```text
┌──────────────────────────────────────────────┐
│ KeyClean                              ─ □ × │
├──────────────────────────────────────────────┤
│                                              │
│                 Ready to clean               │
│                                              │
│              ┌────────────────┐              │
│              │                │              │
│              │     LOCK       │              │
│              │                │              │
│              └────────────────┘              │
│                                              │
│                                              │
│ Devices                                      │
│                                              │
│ Keyboard          ● Connected               │
│ Mouse             ● Connected               │
│                                              │
│ Duration                                     │
│                                              │
│  30 sec   1 min   2 min   5 min   Custom    │
│                     ●                        │
│                                              │
│                               ⚙ Settings     │
└──────────────────────────────────────────────┘
```

The user should understand the application's purpose immediately.

---

# 10. Clean Screen

The primary screen should expose only the options necessary to start a session.

## Device selection

Example:

```text
Devices to lock

☑ Keyboard
☑ Touchpad
☐ Mouse
```

For multiple devices:

```text
Keyboard

☑ Built-in Keyboard
☐ USB Keyboard
☐ Bluetooth Keyboard
```

---

# 11. Duration Selection

Initial presets:

```text
30 seconds
1 minute
2 minutes
5 minutes
```

Later:

```text
10 seconds
15 seconds
30 seconds
1 minute
2 minutes
5 minutes
10 minutes
Custom
```

The default should be:

```text
2 minutes
```

The custom duration should have a reasonable upper limit.

---

# 12. Starting a Cleaning Session

When the user presses **LOCK**, KeyClean should:

1. Validate that the required permissions/access are available.
2. Detect selected devices.
3. Prepare the native input-blocking layer.
4. Start the safety timer.
5. Activate the selected input locks.
6. Display the full-screen overlay.
7. Start the visible countdown.

The transition should be fast enough that the user perceives it as one action.

---

# 13. Full-Screen Overlay

The cleaning session should use a **full-screen overlay**.

This is the standard interaction model for KeyClean.

The overlay should cover the active display and clearly communicate:

- KeyClean is active
- input is locked
- remaining time
- which devices are locked
- emergency unlock shortcut

Example:

```text
┌────────────────────────────────────────────────────────┐
│                                                        │
│                                                        │
│                                                        │
│                       KeyClean                         │
│                                                        │
│                  Keyboard Locked                      │
│                                                        │
│                       01:42                            │
│                                                        │
│                  Touchpad Locked                      │
│                                                        │
│                                                        │
│          Emergency unlock: Ctrl + Alt + K             │
│                                                        │
│                                                        │
└────────────────────────────────────────────────────────┘
```

The overlay should feel calm and intentional rather than alarming.

---

# 14. Overlay Behavior

The overlay should:

- cover the appropriate display
- remain visible for the session
- update the countdown
- not interfere with the emergency unlock shortcut
- not allow accidental interaction with underlying applications
- clearly show when the session is ending
- disappear immediately when unlocking

If multiple monitors are supported, decide whether the overlay appears on:

### Option A — Active display only

Simpler.

### Option B — All displays

More secure and consistent.

The initial implementation can use active-display-only behavior if that is substantially easier, with multi-monitor behavior added later.

---

# 15. Countdown

Example:

```text
02:00
```

Then:

```text
01:59
01:58
01:57
...
00:03
00:02
00:01
00:00
```

The countdown must be based on elapsed time rather than simply decrementing a variable once per second.

Use a monotonic/high-resolution time source so that:

- sleep/wake
- system clock changes
- timer drift

do not produce incorrect results.

---

# 16. Automatic Unlock

At `00:00`:

1. Stop input interception.
2. Restore selected devices.
3. Remove the overlay.
4. Reset session state.
5. Return to the main window/tray state.
6. Optionally show a notification.

Example notification:

> **Cleaning complete**  
> Your input devices have been unlocked.

---

# 17. Emergency Unlock

Emergency unlock is mandatory.

Recommended initial shortcut:

```text
Ctrl + Alt + K
```

This can later become configurable.

The shortcut should work while the normal keyboard input is blocked.

The implementation must ensure that the emergency shortcut is handled by the low-level/native input layer before normal input suppression.

---

# 18. Emergency Unlock UI

The overlay should display:

```text
Emergency unlock

Ctrl + Alt + K
```

Optional:

```text
Press the shortcut to unlock immediately.
```

Do not hide the emergency shortcut.

---

# 19. Safety Timeout

The normal timer is not the only safety mechanism.

Add an independent maximum lock duration.

Example:

```text
Maximum safety lock: 30 minutes
```

If anything goes wrong with the normal session timer, the safety system should still release the lock.

Architecture:

```text
Normal timer
     │
     ├── expires → unlock
     │
     └── failure
            ↓
      safety timeout
            ↓
          unlock
```

The safety timeout should be implemented independently enough that a bug in the UI/session timer cannot leave the user permanently locked.

---

# 20. Input Lock Engine

This is the technical core of KeyClean.

Conceptually:

```text
Windows input
      ↓
Native input interception
      ↓
┌─────────────────────────────┐
│ Is KeyClean locked?         │
├─────────────────────────────┤
│ NO → pass input normally    │
│ YES                         │
│   ├─ emergency shortcut     │
│   │     → unlock            │
│   └─ other input            │
│         → block             │
└─────────────────────────────┘
```

The input engine should not depend on the UI being responsive.

---

# 21. Keyboard Locking

Required behavior:

When locked:

- letters are blocked
- numbers are blocked
- function keys are blocked
- modifiers are blocked
- navigation keys are blocked
- shortcuts are blocked
- Windows key is blocked
- Alt/Tab behavior is blocked
- Ctrl/Alt combinations are blocked

Exception:

- emergency unlock shortcut
- any system-level escape mechanism deliberately supported by KeyClean

The exact Windows implementation should be chosen after testing the relevant low-level input APIs and their limitations.

---

# 22. Mouse Locking

Mouse locking is optional in the MVP but should be architecturally supported.

Possible behaviors:

### Full mouse lock

- left click blocked
- right click blocked
- middle click blocked
- mouse buttons blocked
- wheel blocked
- movement blocked

### Partial mouse lock

Only clicks are blocked.

For cleaning purposes, full blocking is more consistent.

---

# 23. Touchpad / Precision Touchpad

Windows touchpads can be treated differently depending on hardware and driver implementation.

KeyClean should not assume every touchpad can be independently disabled at the same low level as a keyboard.

The device-management layer should therefore expose capability information:

```text
Built-in Keyboard
Locking: Supported

Precision Touchpad
Locking: Supported / Unsupported / Limited

USB Mouse
Locking: Supported
```

If a device cannot be safely controlled, KeyClean should say so instead of pretending it can.

---

# 24. Device Detection

The Devices screen should show currently detected input devices.

Example:

```text
Connected Devices

Keyboard
────────────────────────────
● Built-in Keyboard
  HID Keyboard Device

● Keychron K2
  USB

Mouse
────────────────────────────
● Logitech Mouse
  USB

Touchpad
────────────────────────────
● Precision Touchpad
  Internal
```

The exact device information depends on what Windows exposes.

---

# 25. Device Selection

Eventually:

```text
Devices to lock

☑ Built-in Keyboard
☑ Precision Touchpad
☐ Keychron K2
☐ Logitech Mouse
```

This solves an important real-world problem:

> A user may want to clean one keyboard while continuing to use another.

---

# 26. Device Connection Changes

KeyClean must handle devices appearing/disappearing during a session.

Example:

```text
Keyboard locked
      ↓
USB keyboard disconnected
      ↓
KeyClean updates device state
      ↓
No crash
```

If a device reconnects during a lock session, KeyClean should follow a clearly defined policy.

Initial recommendation:

> Do not automatically lock newly connected devices unless the user explicitly enabled "lock newly detected matching devices."

This avoids surprising behavior.

Later, add:

```text
☑ Lock newly connected matching devices
```

---

# 27. System Tray

KeyClean should run from the Windows system tray.

Tray icon:

```text
⌨
```

Right-click:

```text
KeyClean

● Ready

Lock Keyboard
Lock Selected Devices

──────────────

Open KeyClean
Settings

──────────────

Quit
```

During a session:

```text
KeyClean

● Cleaning — 01:42

Unlock
Open Overlay

──────────────

Quit
```

The tray should provide a reliable secondary way to control the application.

---

# 28. Global Shortcut

Allow the user to start a cleaning session without opening the main window.

Example:

```text
Ctrl + Shift + K
```

Configuration:

```text
Global shortcut

[ Ctrl + Shift + K ]

Action:

○ Lock keyboard
● Lock selected devices
```

This is a convenience feature and should be implemented after the core lock engine works.

---

# 29. Startup

Settings:

```text
Start with Windows       [ON]
Start minimized          [ON]
Show window on startup   [OFF]
```

Recommended default:

```text
Start with Windows: ON
Start minimized: ON
```

The application should consume minimal resources while idle.

---

# 30. Idle Resource Usage

KeyClean should be lightweight.

When idle:

- no continuous polling loop
- no unnecessary CPU usage
- no unnecessary background network activity
- no cloud services
- no telemetry
- no key recording

The application should essentially wait for:

- user interaction
- device changes
- global shortcut
- system events

---

# 31. Notifications

Optional Windows notifications:

### Session started

> Keyboard locked for 2 minutes.

### Session complete

> Cleaning complete. Keyboard unlocked.

### Permission/configuration problem

> KeyClean could not access the selected input device.

Notifications should be configurable.

---

# 32. Settings

## General

```text
Start with Windows        ON
Start minimized           ON
Show notifications        ON
```

## Locking

```text
Default duration          2 minutes

Keyboard                  ON
Mouse                     OFF
Touchpad                  ON
```

## Shortcuts

```text
Start lock                Ctrl + Shift + K
Emergency unlock          Ctrl + Alt + K
```

## Safety

```text
Maximum lock duration     30 minutes
```

## Appearance

```text
Theme
● System
○ Light
○ Dark
```

## Advanced

```text
Developer diagnostics
Log level
Open logs folder
Reset settings
```

---

# 33. Keyboard Tester

This is a post-MVP feature.

The tester should show a visual keyboard.

Example:

```text
┌─────────────────────────────────────────────┐
│ Keyboard Tester                             │
│                                             │
│ Esc  F1 F2 F3 F4 F5 F6 F7 F8 F9 F10 F11 F12│
│                                             │
│ Tab   Q W E R T Y U I O P                  │
│ Caps   A S D F G H J K L                   │
│ Shift   Z X C V B N M                      │
│ Ctrl Alt       Space       Alt Ctrl        │
│                                             │
└─────────────────────────────────────────────┘
```

Pressing a physical key highlights the corresponding visual key.

---

# 34. Key Tester Information

For each detected key:

```text
Key
A

State
Pressed / Released

Scan code
...

Virtual key
...

Last detected
...
```

Do not store a history of what the user typed.

The tester only needs to process events in real time.

---

# 35. Stuck-Key Detection

The diagnostic system should detect suspicious states.

Example:

```text
Possible stuck key

Key:
Shift

State:
Pressed

Duration:
4.2 seconds
```

This can help identify hardware problems.

The detection should avoid falsely reporting normal long key presses as hardware failures.

---

# 36. Keyboard Test Report

Later:

```text
Keyboard Test Results

✓ 103 keys detected
✓ No stuck keys detected

Potential issues:
F7 — no response
```

Possible actions:

```text
Copy report
Save report
```

---

# 37. Profiles

Post-MVP.

Example profiles:

## Quick Clean

```text
Keyboard ✓
Touchpad ✓
Mouse ✗
Duration 30 seconds
```

## Normal Clean

```text
Keyboard ✓
Touchpad ✓
Mouse ✗
Duration 2 minutes
```

## Deep Clean

```text
Keyboard ✓
Touchpad ✓
Mouse ✓
Duration 5 minutes
```

The user can create custom profiles later.

---

# 38. Key Remapping

**Not MVP.**

Potential future feature:

```text
Key Remapping

Caps Lock → Ctrl
Right Alt → Menu
F1 → Volume Down
```

This requires a separate input-transformation subsystem and should not complicate the initial locking implementation.

---

# 39. Advanced Device Management

Future functionality could include:

- per-device lock rules
- device aliases
- preferred devices
- device capabilities
- reconnect behavior
- newly connected device policies

Example:

```text
Keychron K2

Type:
Keyboard

Connection:
USB

Status:
Connected

Lock support:
Yes
```

---

# 40. Privacy Requirements

KeyClean should be privacy-first.

The application should not:

- record typed text
- store keystrokes
- transmit keystrokes
- upload keyboard activity
- create user profiles from typing
- require an online account
- require cloud connectivity

A future privacy statement should clearly explain:

> KeyClean does not record or transmit the contents of keyboard input. During a lock session, selected input is blocked rather than recorded.

---

# 41. Open Source Strategy

KeyClean will be open-source.

The repository should include:

```text
README.md
LICENSE
CONTRIBUTING.md
SECURITY.md
CODE_OF_CONDUCT.md
CHANGELOG.md
```

Potential repository structure:

```text
keyclean/
│
├── src/
│   ├── app/
│   ├── core/
│   ├── ui/
│   ├── platform/
│   │   └── windows/
│   └── shared/
│
├── tests/
│
├── docs/
│
├── assets/
│
├── scripts/
│
├── README.md
├── LICENSE
├── CONTRIBUTING.md
├── SECURITY.md
├── CODE_OF_CONDUCT.md
└── CHANGELOG.md
```

---

# 42. Open-Source Documentation

The README should immediately explain:

```text
# KeyClean

A lightweight Windows utility for safely locking
keyboard and pointing-device input while cleaning
or testing your devices.
```

Then:

- screenshots
- features
- installation
- usage
- keyboard shortcuts
- troubleshooting
- development setup
- architecture
- contributing
- license

---

# 43. License

Choose an open-source license before publishing.

Potential choices:

- MIT
- Apache-2.0
- GPL-3.0

The choice depends on how permissive you want redistribution and derivative works to be.

For a small personal utility where simplicity is important, MIT is one straightforward option, but the final license should be a deliberate project decision.

---

# 44. Localization Strategy

## Phase 1

English only.

All user-facing strings must already be externalized rather than hard-coded throughout the UI.

Example:

```text
"lock_keyboard"
"unlock_keyboard"
"cleaning_complete"
"emergency_unlock"
"settings"
```

Do not build localization yet, but build the architecture so adding languages later does not require rewriting the UI.

---

## Phase 2

Russian

```text
en
ru
```

---

## Phase 3

Ukrainian

```text
en
ru
uk
```

---

## Phase 4

Major world languages

Potential future languages:

- English
- Chinese
- Hindi
- Spanish
- French
- Arabic
- Portuguese
- Bengali
- Russian
- Japanese
- Punjabi
- German
- Indonesian
- Korean
- Vietnamese
- Italian
- Turkish
- Ukrainian
- Polish
- Dutch

The final language list should be based on actual project usage and contributor interest rather than trying to translate everything immediately.

---

# 45. Localization Architecture

Example:

```text
locales/
├── en/
│   └── strings.json
├── ru/
│   └── strings.json
└── uk/
    └── strings.json
```

Example:

```json
{
  "clean.start": "Start Cleaning",
  "clean.locked": "Keyboard Locked",
  "clean.remaining": "Time Remaining",
  "clean.unlock": "Unlock",
  "settings.title": "Settings"
}
```

Avoid building the UI around string lengths that only work in English.

Allow translated strings to be substantially longer.

---

# 46. Error Handling

Every failure should produce an understandable message.

Bad:

```text
Error 0x00000005
```

Better:

```text
KeyClean could not access this input device.

Check that the device is connected and try again.
```

Advanced details can be available under:

```text
View technical details
```

---

# 47. Important Failure Cases

The application must be tested against:

### Permission failure

```text
Cannot access required Windows input functionality.
```

### Device disconnected

```text
Keyboard disconnected.
```

### Device reconnect

Device state is refreshed.

### Application crash

Input lock should not remain active indefinitely.

### Forced application termination

Input lock should have a safe cleanup mechanism.

### Windows shutdown

Input interception must not interfere with shutdown.

### Windows sleep

State must be restored safely after wake.

### Windows restart

No persistent lock state should survive an application restart.

### Timer failure

Safety timeout must still release input.

---

# 48. Session State Machine

Use an explicit state machine.

```text
                 ┌──────────────┐
                 │     IDLE     │
                 └──────┬───────┘
                        │
                      START
                        │
                        ▼
                 ┌──────────────┐
                 │  STARTING    │
                 └──────┬───────┘
                        │
                  initialization
                        │
                        ▼
                 ┌──────────────┐
                 │    LOCKED    │
                 └──────┬───────┘
                        │
             ┌──────────┼───────────┐
             │          │           │
          timeout   emergency    failure
             │          │           │
             └──────────┼───────────┘
                        ▼
                 ┌──────────────┐
                 │  UNLOCKING   │
                 └──────┬───────┘
                        │
                        ▼
                 ┌──────────────┐
                 │     IDLE     │
                 └──────────────┘
```

Do not let UI state alone determine whether the keyboard is locked.

The native/core state must be authoritative.

---

# 49. Recommended Architecture

Separate the application into layers:

```text
┌─────────────────────────────┐
│             UI              │
│ Dashboard / Overlay / Tests │
└──────────────┬──────────────┘
               │
┌──────────────▼──────────────┐
│         Application         │
│ Session / Timer / Profiles  │
└──────────────┬──────────────┘
               │
┌──────────────▼──────────────┐
│            Core             │
│ Lock Manager / Devices      │
│ Permission / Safety         │
└──────────────┬──────────────┘
               │
┌──────────────▼──────────────┐
│       Windows Native        │
│ Input / HID / System APIs   │
└─────────────────────────────┘
```

This separation is especially useful for an open-source project because contributors can understand where platform-specific code belongs.

---

# 50. Suggested Project Structure

```text
keyclean/
│
├── src/
│   │
│   ├── app/
│   │   ├── application-state
│   │   ├── session-manager
│   │   ├── timer-manager
│   │   └── notification-manager
│   │
│   ├── core/
│   │   ├── lock-manager
│   │   ├── safety-manager
│   │   ├── device-manager
│   │   ├── permission-manager
│   │   └── settings-manager
│   │
│   ├── platform/
│   │   └── windows/
│   │       ├── input/
│   │       ├── devices/
│   │       ├── permissions/
│   │       ├── system/
│   │       └── startup/
│   │
│   ├── ui/
│   │   ├── dashboard/
│   │   ├── overlay/
│   │   ├── devices/
│   │   ├── tester/
│   │   └── settings/
│   │
│   └── shared/
│       ├── types/
│       ├── constants/
│       ├── localization/
│       └── utilities/
│
├── tests/
│
├── docs/
│
├── assets/
│
├── scripts/
│
├── README.md
├── LICENSE
├── CONTRIBUTING.md
├── SECURITY.md
├── CODE_OF_CONDUCT.md
└── CHANGELOG.md
```

---

# 51. Development Roadmap

## Phase 0 — Windows input research

Before building the UI, investigate and prototype:

- Windows low-level keyboard input
- Windows low-level mouse input
- HID device enumeration
- device identification
- multiple keyboards
- multiple mice
- device arrival/removal
- emergency shortcut handling
- process termination behavior
- sleep/wake
- shutdown/restart
- privilege requirements
- security implications

### Deliverable

A tiny technical prototype:

```text
Detect keyboard
       ↓
Lock keyboard
       ↓
Emergency unlock
       ↓
Unlock keyboard
```

No polished UI yet.

---

# 52. Phase 1 — Core Lock Engine

Implement:

- keyboard interception
- input blocking
- emergency unlock
- clean unlock
- device state
- timer
- safety timeout
- error handling

### Goal

The locking system must be reliable before building the rest of the application.

---

# 53. Phase 2 — Safety

Implement and test:

- automatic timeout
- independent safety timeout
- crash recovery
- forced termination
- Windows shutdown
- Windows restart
- sleep/wake
- device disconnect
- device reconnect
- timer drift
- system clock changes

### Goal

It should be difficult to create a situation where the user loses control of the machine.

---

# 54. Phase 3 — MVP UI

Build:

- dashboard
- device selection
- duration selector
- Lock button
- full-screen overlay
- countdown
- unlock state
- error messages

The visual design should remain simple.

---

# 55. Phase 4 — Windows Integration

Add:

- system tray
- global shortcut
- start with Windows
- notifications
- Windows-native packaging
- installer
- uninstaller

---

# 56. Phase 5 — Open-Source Release

Prepare:

- README
- screenshots
- installation instructions
- development instructions
- architecture documentation
- contributing guide
- security policy
- license
- issue templates
- feature-request template
- bug-report template
- changelog

Then publish the first public release.

---

# 57. Phase 6 — Keyboard Diagnostics

Add:

- keyboard tester
- visual keyboard
- key detection
- stuck-key detection
- scan-code information
- diagnostic report

This is the first major expansion beyond cleaning.

---

# 58. Phase 7 — Advanced Device Management

Add:

- individual device locking
- device profiles
- reconnect behavior
- device aliases
- capability detection
- per-device settings

---

# 59. Phase 8 — Optional Advanced Features

Only after the core product is stable:

- key remapping
- advanced profiles
- hardware information
- battery status
- diagnostic reports
- automation
- custom lock modes

---

# 60. MVP Definition of Done

KeyClean MVP is complete when all of the following work reliably:

## Application

- [ ] Launches successfully
- [ ] Runs in system tray
- [ ] Has a simple dashboard
- [ ] Can start a cleaning session

## Input

- [ ] Keyboard can be locked
- [ ] Keyboard can be unlocked
- [ ] Selected input is blocked
- [ ] Emergency shortcut still works
- [ ] Input returns to normal after unlock

## Timer

- [ ] Preset durations work
- [ ] Countdown is accurate
- [ ] Automatic unlock works
- [ ] Safety timeout works

## Overlay

- [ ] Full-screen overlay appears
- [ ] Countdown is visible
- [ ] Locked devices are visible
- [ ] Emergency shortcut is visible
- [ ] Overlay disappears after unlock

## Windows

- [ ] Windows 10 works
- [ ] Windows 11 works
- [ ] Startup behavior works
- [ ] System tray works
- [ ] Notifications work

## Safety

- [ ] Forced app termination tested
- [ ] Sleep/wake tested
- [ ] Shutdown tested
- [ ] Restart tested
- [ ] Device disconnect tested
- [ ] Device reconnect tested

## Privacy

- [ ] No keystrokes stored
- [ ] No typed content stored
- [ ] No unnecessary telemetry
- [ ] No account required
- [ ] No cloud dependency

---

# 61. What to Build First

The actual development priority should be:

```text
1. Windows input prototype
        ↓
2. Keyboard lock/unlock
        ↓
3. Emergency unlock
        ↓
4. Safety timeout
        ↓
5. Automatic timer
        ↓
6. Crash/failure recovery
        ↓
7. Device detection
        ↓
8. Full-screen overlay
        ↓
9. Main dashboard
        ↓
10. System tray
        ↓
11. Global shortcut
        ↓
12. Startup with Windows
        ↓
13. Packaging
        ↓
14. Open-source release
        ↓
15. Keyboard diagnostics
        ↓
16. Advanced device management
```

Do **not** start with the UI.

The most technically risky part of KeyClean is the Windows input-locking and recovery mechanism. Prove that first.

---

# 62. First Development Milestone

The first milestone should be extremely small:

> **Press a button → keyboard input stops → press the emergency shortcut → keyboard input immediately works again.**

If this does not work reliably, nothing else matters yet.

Once this works, build the timer.

Then:

```text
Lock
  ↓
Timer
  ↓
Automatic unlock
```

Then add the full-screen overlay.

Then build the polished application around the already-working core.

---

# 63. Final Product Direction

KeyClean should ultimately feel like:

> **A tiny Windows utility that does one potentially annoying job perfectly.**

The core experience should remain:

```text
             KEYCLEAN

        ┌───────────────┐
        │     LOCK      │
        └───────────────┘

      Keyboard    ✓
      Touchpad    ✓
      Mouse       ○

      Duration
      30s  1m  2m  5m

              ↓

        FULL-SCREEN
          OVERLAY

             01:42

       INPUT LOCKED

     Ctrl + Alt + K
       Emergency Unlock

              ↓

          00:00

        UNLOCKED
```

The application can grow into a broader **Windows keyboard/input utility**, but the cleaning workflow should remain the simplest and most recognizable part of the product.

---

# 64. Feature Priority Summary

| Feature | Priority | Version |
|---|---:|---|
| Keyboard locking | P0 | MVP |
| Keyboard unlocking | P0 | MVP |
| Emergency unlock | P0 | MVP |
| Safety timeout | P0 | MVP |
| Automatic timer | P0 | MVP |
| Full-screen overlay | P0 | MVP |
| Device detection | P0 | MVP |
| Error handling | P0 | MVP |
| Crash/failure recovery | P0 | MVP |
| System tray | P1 | MVP |
| Global shortcut | P1 | MVP |
| Start with Windows | P1 | MVP |
| Mouse locking | P1 | MVP |
| Touchpad locking | P1 | MVP |
| Individual device selection | P1 | V1 |
| Keyboard tester | P1 | V1 |
| Stuck-key detection | P1 | V1 |
| Keyboard visualizer | P1 | V1 |
| Profiles | P2 | V1/V2 |
| Advanced device management | P2 | V2 |
| Key remapping | P2 | V2 |
| Hardware diagnostics | P2 | V2 |
| Battery information | P3 | Future |
| Automation | P3 | Future |
| Cloud sync | P4 | Not planned |
| Accounts | P4 | Not planned |
| Telemetry | P4 | Not planned |
| macOS/Linux support | P4 | Not planned initially |
| AI features | P4 | Not planned |

---

# 65. Initial Product Rule

When deciding whether to add a feature, ask:

> **Does this make KeyClean better at safely controlling, cleaning, or diagnosing input devices on Windows?**

If the answer is no, it probably does not belong in KeyClean.
