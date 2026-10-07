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

## Brand drafts

Chosen by Max on 2026-10-07 from images he generated; both may be committed.

| File                     | Shows                                                                              | Used for                                                  | Milestone |
| ------------------------ | ---------------------------------------------------------------------------------- | --------------------------------------------------------- | --------- |
| `brand/icon-draft.png`   | Satin-black keycap, three-quarter view, glowing amber K, amber glow at the base    | App icon at 32-256 px, installer, README, About           | M11       |
| `brand/banner-draft.png` | Dark keyboard from above, Ctrl, Alt and K lit amber, empty left side for the title | README banner and social preview, title added as SVG text | M12       |

- At 16 and 20 px the 3D key turns into a blob, so those `.ico` sizes get a hand-drawn flat
  version: rounded cap, amber K, amber base line.
- The tray icon is drawn in M8 from the same key: a flat outline with the K, in a light and a dark
  variant for the taskbar theme, plus a locked variant.
- Both PNGs are about 1.5 MB; compress them before they go into the README (M12).

## Mood references (local only)

Third-party images Max collected for later phases, kept in `references/` on his machine. They are
someone else's work, so the folder is git-ignored: use them for direction only, never copy or trace
them, and never ship them.

| File                | Shows                                                                         | For                                         |
| ------------------- | ----------------------------------------------------------------------------- | ------------------------------------------- |
| `keyboard-dark.jpg` | Dark keyboard render: soft key depth, quiet legends, a small LED on one key   | M13 visual keyboard (key shape and states)  |
| `arrow-keycaps.jpg` | Arrow keycaps drawn as a cap inside an outlined well ("ZEROLINE", AIGA entry) | App and tray icon, empty states, README art |

Notes from the review (2026-10-07):

- The keyboard is a Mac layout; the tester needs Windows keys (Ctrl, Win, Alt, PrtSc) and ANSI, ISO,
  laptop and full-size layouts. The fade at the edge is a photo effect: every key must stay readable.
- Keep KeyClean's warm near-black and amber (`#0a0908`, `#f5a54a`). Amber means pressed or locked;
  red only means a possible stuck key or an error. Each key state needs a second cue besides colour.
- If the keycap style is adopted, the overlay's Ctrl + Alt + K key caps change with it, so the
  overlay, the tester and the icon share one key drawing.

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
