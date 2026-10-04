# Design references

The target look for the MVP UI. Made in Claude Design from [`brief.md`](brief.md), the feature-only
brief (the brief gave no visual direction). These are screenshots only: there is no exported design
file, so exact fonts, colours and spacing have to be matched by eye and confirmed by Max.

Built in their milestones (`docs/ROADMAP.md`): the overlay in **M6**, the Clean screen in **M7**.

## What's here

| File                                 | Shows                                                                                                                                                                                                                                                                | Milestone |
| ------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------- |
| [`clean-ready.png`](clean-ready.png) | Clean screen, Ready state, light and dark: tabs (Clean, Devices, Test, Settings), `02:00` display with lock summary and the emergency shortcut, presets plus Custom, Keyboard / Mouse / Touchpad tiles (Touchpad marked Limited), Lock tile, the every-keyboard note | M7        |
| [`overlay.png`](overlay.png)         | Full-screen overlay at 1366x768: locked (`01:42`, segmented progress, Unlock now, Ctrl + Alt + K keys) and last seconds ("Unlocking in 4 seconds", accent colour)                                                                                                    | M6        |

## Not designed yet

Ask for these one screen at a time before or during their milestone:

- Clean: Starting, Locked, Unlocking, Session finished (each end reason), Custom duration input,
  engine-unavailable error, elevated-window warning, DEV CAP indicator.
- Overlay: with the mouse locked (no Unlock now button), large monitor.
- Devices, Test, Settings, tray menus, notifications, the shared error block.

## Notes for implementation

- The design uses a monospace face for the display and labels. Any font must be bundled with the app
  (KeyClean makes no network calls), so pick one with a licence that allows bundling.
- The overlay's **Unlock now** button only appears when the mouse is not locked (from M5 on).
- Additions beyond the spec that these screens assume (Custom duration, remembered choices, the dark
  overlay for spotting smudges, keeping the display awake) get an ADR in the milestone that builds them.
