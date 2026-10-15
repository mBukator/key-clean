# ADR 0017 - Global start shortcut

- Status: Accepted, 2026-10-15. M9 plan approved by Max (D1-D10, all as recommended); after the
  M9 manual test Max accepted the points the plan didn't settle (the exact shortcut rules with
  Ctrl+Alt+Shift refused too, retrying on every dashboard open, the "unavailable" state before
  the first registration attempt, the tray showing the shortcut after a tab, the debug-only
  `KEYCLEAN_E2E_SHORTCUT` variables) and chose the elevated-window warning below. The warning
  awaits its manual check (M9.md step 13).
- Deciders: Max
- Related: M9; §12, §28, §30, §32 "Shortcuts", §46; invariants 3, 4, 8, 11; privacy invariants;
  ADR 0005 (elevated windows), 0007 (injected input), 0009 (engine process), 0014 (overlay gate),
  0016 (tray); `src-tauri/src/shortcut.rs`, `crates/keyclean-core/src/shortcut.rs`,
  `src/ui/settings/ShortcutSettings.tsx`, `docs/research/m9-shortcut.md`, harness S45-S48.

## Context

§28 asks for a shortcut that starts a lock without opening KeyClean. Until M9 a lock started only
from the dashboard or the tray. The shortcut is pressed with another app in front, by the same
keys the lock is about to block.

## Decision

### Mechanism

- **The official `tauri-plugin-global-shortcut` 2.4.1** (with `global-hotkey` 0.8.0), pinned to
  `~2.4`. On Windows it calls `RegisterHotKey` on a hidden window of its own and gets `WM_HOTKEY`
  [docs: locked source, `platform_impl/windows/mod.rs`]. Windows reports only that exact chord, so
  KeyClean sees no other key. It is neither a hook nor a Raw Input registration (invariant 4 and
  harness S42 hold), and `MOD_NOREPEAT` stops a held chord from firing again.
- After a press, the plugin starts a short-lived thread that checks every 50 ms whether the key is
  still down, to report its release. It ends with the release. KeyClean ignores releases.
- **Idle cost:** one registration and one more hidden window. No thread runs at idle.
- **Registered from KeyClean's own `setup`, not the plugin's builder.** The builder registers in
  the plugin's setup with `?`, so a chord another app holds could stop KeyClean from starting.
- **Every change runs off the main thread.** The plugin runs each registration on the main thread
  and waits for it, so a change on the main thread would wait for itself. The shortcut handler and
  the tray only read the shortcut's state, which is never locked across a plugin call (invariant
  3: nothing about a lock waits on this).
- **The plugin's window is guarded.** Plugins are set up before `setup`, and the window uses the
  exe's module handle, so the close guard covers it (`taskkill` without `/F` still exits; S42
  counts 4 helper windows).
- **No window gets the plugin's permissions** (its default set is empty anyway); a test checks
  the capability files, as for the store.

### The shortcut

- **Default Ctrl+Shift+K** (§28), on. It can be turned off (`null` in the settings file).
- **Allowed:** Shift plus Ctrl or Alt, plus a letter, a digit, or F1-F24 except F12. Checked in
  `keyclean-core` and again by the command, whatever the page sends.
- **Refused, each with its own message:** one modifier (Ctrl+C would stop working everywhere,
  because a registered chord reaches KeyClean alone); any Ctrl+Alt combination (Ctrl+Alt+K is the
  emergency chord, and AltGr arrives as Ctrl+Alt, so it would take characters such as @ and €);
  the Windows key (reserved for Windows) and F12 (reserved for debuggers) [docs: `RegisterHotKey`].
- **What it locks:** "Lock selected devices" by default (§28's selected option), or "Lock
  keyboard". Selected devices are the remembered targets; the duration is the remembered one. Like
  a tray lock, a shortcut lock doesn't change the remembered choices.
- **Settings file** (Rust only): `startShortcut` (`"Ctrl+Shift+K"` or `null`) and
  `shortcutAction` (`"keyboard"` or `"selected"`). A missing or invalid value loads the default.
  Only the chord is stored, never typed keys.

### Pressing it

- **A press locks through the overlay gate**, exactly like a tray lock (invariant 11). The
  handler runs on the main thread inside the plugin's window procedure, so the lock starts from a
  task.
- **During a lock the shortcut stays registered.** A press then is refused like a second tray
  lock (`AlreadyActive`), silently. In a keyboard lock the hook probably blocks the key before
  Windows looks for hotkeys [assumption]; in a mouse-only lock keys pass, and S47 checks the
  refusal. Unregistering for the lock would risk a failed re-registration afterwards.
- **A shortcut lock that fails** (engine unavailable, overlay not confirmed) stores the error like
  a tray lock and **opens the dashboard**: unlike the tray, nothing else would show that the press
  did nothing.
- **Keys held at the lock start** are covered by the engine's existing drain (invariant 8): it
  reads the held keys when the hook goes in, blocks their repeats, and lets their releases
  through. Usually the chord is released while the overlay opens, before the hook exists. S45 and
  S46 check both cases.
- The default shares K with the emergency chord: holding Ctrl+Shift+K through the lock start and
  then pressing Alt would end the lock. That fails safe.

### When another app holds it

- KeyClean starts as usual and logs `shortcut: unavailable`. Settings shows the plain-language
  error with technical details and **Try again**; the Clean screen says it isn't working; the tray
  shows no shortcut.
- It is tried again when the dashboard opens, when the user saves a shortcut, and on Try again.
  Nothing polls.
- The plugin reports every failure as text, so KeyClean doesn't tell "taken" from other OS errors;
  the message says another app _may_ be using it, and the details carry the OS error.
- A new shortcut that can't be registered leaves the old one registered, and nothing is saved.

### Picking a new shortcut

- **Change** unregisters the current shortcut until the pick ends, so pressing it reaches the
  page instead of starting a lock. Esc, losing focus, Cancel, saving, and the dashboard closing
  (Rust watches for that) all end the pick and register the shortcut again.
- The page reads key presses only while picking and sends only the resulting chord. Nothing is
  logged or stored besides the saved chord (privacy invariants).

### Where it shows

- The Clean screen's dark panel: "Start shortcut: Ctrl + Shift + K" under the emergency shortcut,
  or off, or not working.
- The tray menu: the shortcut right-aligned after the item it does, as plain text after a tab
  (muda renders accelerators the same way, but this registers none) [docs: muda 0.20.0 source].
- Plain styling until Max's designs arrive.

### Elevated window (ADR 0005)

Measured first (M9.md step 11, 2026-10-15): in a keyboard lock, letters typed into an
administrator terminal appeared there, and the engine's in-lock notice didn't fire [tested]. The
notice relies on the Raw Input sink seeing a key the hook missed (ADR 0010); Windows apparently
keeps that input from both [assumption]. So the gap isn't limited to a shortcut press with an
administrator window in front: clicking into one during a keyboard-only lock reaches it too.

Decision (Max, 2026-10-15):

- **During a lock, the app checks once per countdown second whether the window in front is
  elevated** (the engine's existing `is_elevated_above_us`: the foreground window's process token,
  and a process KeyClean can't open counts as elevated). It runs on the main thread with the
  overlay check that already follows each countdown status, so nothing new runs at idle and no
  hook or thread is added.
- **If it is, and the keyboard is locked, the overlay and the dashboard show "Typing reaches
  administrator windows during a lock."**, once per lock, through the existing notice. A
  mouse-only lock doesn't warn: keys pass there anyway.
- This covers a shortcut press with an administrator window in front (unless the overlay takes the
  focus first) and clicking into one later. No confirmation dialog: it would defeat the shortcut.
- "Relaunch as administrator" is deferred: ADR 0005 requires its own review against every
  invariant.

### Logs

`shortcut: registered`, `shortcut: unavailable`, `shortcut: lock requested`: what happened, never
which keys (privacy invariants). global-hotkey prints the hotkey's key name in some errors, so the
technical details keep only "already registered" or the OS error code, never the plugin's text.

### Letters follow the layout

`RegisterHotKey` takes a virtual-key code, and letter codes follow the keyboard layout. The capture
field therefore reads letters from the layout (on AZERTY the key labelled A gives A), and digits and
F-keys from their position. The modifier names shown come from the strings file.

## Alternatives

- **Off until the user sets one.** Avoids taking Ctrl+Shift+K from VS Code (delete line) and
  Firefox (console) on first run. Rejected by Max in favour of §28's default (D1).
- **"Lock keyboard" as the default action.** Gentler for an accidental press (the mouse can still
  click Unlock now). Rejected: §28 selects "Lock selected devices" (D2).
- **Unregister during a lock.** Rejected: the re-registration after the lock could fail.
- **A low-level keyboard hook at idle to watch for the chord.** Breaks invariant 4 (no hook at
  idle) and sees every key. Rejected.
- **A confirmation dialog when an administrator window is in front.** Defeats the shortcut;
  decided only after the measurement.

## Consequences

- At idle, KeyClean holds one hotkey registration; the app's hidden windows are four.
- During a keyboard lock, typing into an administrator window still gets through (ADR 0005's known
  fail-open case); the user is now told within about a second.
- Ctrl+Shift+K stops reaching other apps while KeyClean runs (VS Code's delete line, Firefox's
  console). The user can change or turn it off.
- A lock can now start with another app in front; the overlay takes the focus as for a tray lock.
- Harness S45-S48 cover the shortcut; if Windows doesn't fire a hotkey for synthesized keys, they
  fail with "shortcut not seen" and the manual test carries the check.
- New dependency: `tauri-plugin-global-shortcut` (official plugin, as the project guide asks),
  bringing `global-hotkey` and `keyboard-types` (plus Linux-only `x11rb`, not built on Windows).
