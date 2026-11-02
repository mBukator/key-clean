# ADR 0019 - Packaging: the NSIS installer and uninstaller

- Status: Accepted, 2026-11-02. Max approved the M11 plan (D1-D12 as recommended, publisher and
  copyright as proposed) on 2026-11-02. Pending Max's OK, settled during the build: the startup
  checkbox sits at the right of the install-location page, under the folder box (the page has no
  room below it); another version is installed over without its uninstaller, and a passive
  install never uninstalls first; "Uninstall" of the same version stays a clean reinstall; another
  folder during an upgrade leaves the old one; the startup check also covers the previous
  install's folder; the drafted 16-24 px flat icon art; `@tauri-apps/cli` pinned to an exact
  version; the harness's running-copy check and its refusal to run S52-S53 while KeyClean is
  installed; the draft's 99 % opaque keycap made opaque in the icon.
- Deciders: Max
- Related: M11; §3, §29, §41-§43, §55, §60 "Windows 10 works", "Windows 11 works"; invariants 1, 2,
  9, 12; ADR 0001 (WebView2), 0005 (`asInvoker`), 0009 (engine process), 0016 (tray), 0018
  (start with Windows, toasts); `src-tauri/windows/{installer.nsi,keyclean.nsh}`,
  `src-tauri/src/installer.rs`, `scripts/app-icon.mjs`, `assets/icon/`,
  `.github/workflows/installer.yml`, harness S52-S53, `docs/research/m11-packaging.md`,
  `docs/testing/manual/M11.md`. Deviates from the project guide's preference for official
  defaults: KeyClean ships its own copy of Tauri's NSIS template.

## Context

M11 packages KeyClean: an NSIS installer for the current user, the WebView2 bootstrapper, and an
uninstaller. M10 (ADR 0018) left three jobs for it: a Start menu shortcut carrying the
AppUserModelID `io.github.mbukator.keyclean` (without it an installed KeyClean shows no toasts), a
"Start KeyClean when I sign in" checkbox ticked by default (§29), and an uninstaller that removes
KeyClean's `Run` value and Task Manager's `StartupApproved` record of it.

Tauri CLI 2.12.1's template already does a lot (research note): a `currentUser` install to
`%LOCALAPPDATA%\KeyClean` with `RequestExecutionLevel user`, the shortcut's AppUserModelID, a
Restart Manager check that closes a running copy, and removal of the `Run` value. Four things in
it don't fit KeyClean:

1. **An upgrade is a full uninstall.** Over an older version, the default "Uninstall before
   installing" runs the old uninstaller without `/UPDATE`, which deletes the `Run` value. If the
   uninstaller also removed `StartupApproved` (as ADR 0018 promised), a "Disable" in Task Manager
   would be lost and the ticked checkbox would turn startup back on. The 0.1.0 uninstaller is
   what the next version runs during its upgrade, so this has to be right from the first release.
2. **It deletes `Run\KeyClean` by name,** even when the value starts another copy.
3. **It creates a desktop shortcut** by default, and always in passive or silent installs.
4. **It keeps the install record and the WebView2 cache** unless "Delete the application data"
   is ticked.

## Decision

### Our own copy of the template (D1)

- `src-tauri/windows/installer.nsi` is Tauri CLI 2.12.1's template, used through
  `bundle.windows.nsis.template`. Every change is marked `KEYCLEAN:`; KeyClean's logic is in
  `keyclean.nsh`, included through `installerHooks`.
- `@tauri-apps/cli` is pinned to `2.12.1` in `package.json`. A test in `src-tauri/src/installer.rs`
  fails when `bun.lock` names another version: then the new template is copied, the marked
  changes applied again, and the version updated.
- Unchanged from the template, the generated script was compared line by line with the default
  one: identical apart from the estimated size [tested].

### The startup checkbox (D2)

- "Start KeyClean when I sign in" on the install-location page.
- **Default:** ticked for a fresh install. When KeyClean is already installed: ticked only if the
  `Run` value starts this install, so an entry the user turned off stays off. The default is
  decided in `.onInit`, before the reinstall page can run the old uninstaller.
- **Ticked:** writes `HKCU\...\Run\KeyClean` = `"<install dir>\keyclean.exe" --autostart`, the
  string `keyclean_core::startup::run_command` builds (a test checks this, with a space in the
  path). Over 260 UTF-16 units, nothing is written and the install log says why.
- **Unticked:** deletes the `Run` value only if it starts this install (or the previous
  install's folder, when the user picked another).
- `StartupApproved` is never written, so a Task Manager "Disable" outlives any rewrite (tested in
  M10). Passive and silent installs use the same defaults.

### Upgrades and uninstall (D1, D4)

- **Another version is installed over.** The reinstall page is skipped and the old uninstaller
  isn't run: the files are replaced, and the startup entry, its Task Manager record, the
  shortcuts and the settings stay. Cancelling before the files are copied leaves the old version
  as it was.
- **The same version** gets the template's page: "Add/Reinstall" installs over; "Uninstall" is a
  clean reinstall: a real uninstall (below, Task Manager's record included), then this installer
  carries on, with the checkbox as it was before. A passive install (`/P`) has no page and
  installs over (the template's passive path read the missing radio buttons as "Uninstall").
- **Another folder during an upgrade** isn't a move: the old folder stays, and a KeyClean
  running from it isn't closed (the template behaves the same). To move KeyClean, uninstall it
  first.
- A real uninstall removes: the `Run` value if it starts this install, and then
  `StartupApproved\Run\KeyClean` (also when no `Run` value is left, as an orphan); the Start menu
  shortcut; the install folder; the Apps & features entry; `HKCU\Software\mBukator\KeyClean` (the
  install record); `%LOCALAPPDATA%\io.github.mbukator.keyclean` (WebView2's cache). Another copy's
  `Run` value and record stay.
- `%APPDATA%\io.github.mbukator.keyclean` (the settings) goes only when "Also delete my KeyClean
  settings" is ticked; unticked by default, so a reinstall keeps them.
- The WebView2 runtime stays: it's a shared Windows component.
- **No lock state to remove:** no settings key holds lock state; hooks die with the process
  (ADR 0002); nothing writes a Scancode Map, disables a device, or installs a driver or service;
  there are no log files. S53 checks the Scancode Map is unchanged.

### A running KeyClean (D3)

- Before files are replaced or removed: if Restart Manager finds this install's `keyclean.exe` running (the app or its engine), the
  installer asks "KeyClean is running and will be closed..." (not in silent or passive mode).
- It then posts `WM_CLOSE` to the single-instance plugin's window (`<identifier>-sic`), which
  KeyClean guards so that it exits cleanly (fail-safe matrix): the engine ends any lock and both
  processes exit. It waits up to 10 s.
- Whatever still runs is left to the template's Restart Manager check, which asks again
  (interactive installs only) and then ends it; cancelling there stops the install with KeyClean
  running. Either way input is released (invariant 1: process death).
- No app code changes.

### Desktop shortcut, publisher, WebView2 (D5-D7)

- The Finish page's "Create desktop shortcut" is unticked, and passive or silent installs create
  none.
- Publisher `mBukator`, copyright `Copyright 2026 mBukator`, homepage the GitHub repository.
  (Unset, the publisher would be "github", the identifier's second part.)
- WebView2 `downloadBootstrapper`: the installer is about 1.9 MB (1.78 MiB) [tested] and downloads Microsoft's
  bootstrapper only when WebView2 is missing. Offline without WebView2, it stops with a message
  before copying files [docs: template]. KeyClean itself still makes no network calls.

### App icon (D8)

`src-tauri/icons/icon.ico` holds 16, 20, 24, 32, 40, 48, 64 and 256 px, built by
`bun scripts/app-icon.mjs`: 32 px and up from `docs/design/brand/icon-draft.png`; 16-24 px from
flat SVGs in `assets/icon/` (rounded cap, amber K, amber base line), drafted for Max to redraw.

### Signing, single instance, Windows 10, CI (D9-D12)

- **Unsigned for now.** A downloaded installer shows SmartScreen's "Windows protected your PC"
  (More info → Run anyway). Options for M12: SignPath Foundation (free for open-source projects)
  or Azure Artifact Signing [assumption: terms to check then].
- **Single instance across copies:** an installed KeyClean shares the identifier with dev builds,
  so a running one takes over dev and harness launches. Every harness run first checks that no
  `keyclean.exe` runs and names the path if one does. A separate dev identifier was rejected: it
  would split the settings file.
- **Windows 10:** a Hyper-V VM (basic VMConnect session, not the RDP-based enhanced one, which
  ends locks on disconnect) with Windows 10 22H2. Latency measured there is labelled as a VM's.
- **CI:** `installer.yml`, manual trigger only, builds the installer and uploads it as an
  artifact. Automatic release builds come with M12.

## Alternatives

- **Template hooks only (D1 b).** Tracks upstream changes, but the upgrade fix needs an
  environment marker set from a GUI-init function, the checkbox would go on the Welcome page, and
  another copy's `Run` value would have to be saved and restored around the template's delete.
  Rejected as fragile.
- **The checkbox on the Finish page or on a page of its own.** The Finish page is skipped if
  closed; a page of its own costs a click. Rejected.
- **Run the old uninstaller with `/UPDATE /P` on an upgrade** (the first build). Keeps the
  startup entry, but a cancel after it ran (or a failed WebView2 download) would leave the
  entry and the shortcuts pointing at a removed exe, with no Apps & features entry to clean them
  up. Rejected after review.
- **Pass `/UPDATE` for every uninstall the installer starts.** "Uninstall" of the same version
  would then keep the startup entry. Rejected.
- **`embedBootstrapper` / `offlineInstaller`.** About 1.8 MB / 127 MB more for machines without
  WebView2, which Windows 11 always has. Rejected.
- **Delete the settings by default.** A reinstall would lose them. Rejected.

## Consequences

- KeyClean maintains a template copy: about 930 lines to re-sync per Tauri CLI update, guarded by
  a test.
- `bun tauri build` now makes `target\release\bundle\nsis\KeyClean_<version>_x64-setup.exe`.
- An installed KeyClean shows toasts as KeyClean (M10's carry-over), checked in M11.md.
- A running installed copy must be quit before dev runs and the harness; the harness says so.
- The uninstaller's `Run` check compares the path as the installer wrote it; a value the app wrote
  from a short (8.3) path wouldn't match and would stay [assumption: Windows launches the
  shortcut's long path].
