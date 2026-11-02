# M11 research - packaging (NSIS installer)

Notes behind ADR 0019. Tags: [docs] read in a primary source, [tested] seen on Max's machine or in
a build here, [assumption] not yet checked, [undocumented] observed behaviour with no
documentation.

## Tauri CLI 2.12.1's NSIS template

Source: `crates/tauri-bundler/src/bundle/windows/nsis/{installer.nsi,utils.nsh}` at tag
`tauri-cli-v2.12.1` (https://github.com/tauri-apps/tauri/tree/tauri-cli-v2.12.1). The CLI's
bundler is a workspace dependency at that tag, so this is the template the CLI embeds.

- With no template set, the generated script was saved; with our copy of the tag's file set as
  `bundle.windows.nsis.template` (unchanged), the generated script was identical apart from
  `ESTIMATEDSIZE` (the exe had been rebuilt) [tested].
- The build downloads NSIS 3.11 and `nsis_tauri_utils` 0.5.3 from GitHub and checks their hashes,
  once, into `%LOCALAPPDATA%\tauri\NSIS` [tested: build log].
- **Install mode `currentUser`:** default folder `$LOCALAPPDATA\${PRODUCTNAME}`
  (`%LOCALAPPDATA%\KeyClean`), `SetShellVarContext current`, `SHCTX` = HKCU,
  `RequestExecutionLevel user` [docs: template; tested: generated script]. The built installer's
  manifest says `requestedExecutionLevel level="asInvoker"` [tested]. NSIS's own default is
  `admin` [docs: NSIS manual 4.8.1.37], so the template's line matters.
- **Main binary:** `MAINBINARYNAME "keyclean"`, the cargo binary; installed as `keyclean.exe`
  [tested: generated script].
- **Publisher:** unset, `MANUFACTURER` is "github", the identifier's second part [docs: config
  schema; tested]. It names `HKCU\Software\<publisher>\<product>` (the install record) and the
  Apps & features publisher.
- **Start menu shortcut:** `$SMPROGRAMS\KeyClean.lnk` (no folder), then `SetLnkAppUserModelId`
  sets `PKEY_AppUserModel_ID` to `BUNDLEID` (the identifier) through `IPropertyStore` [docs:
  template, utils.nsh]. A desktop shortcut gets the same.
- **Desktop shortcut:** MUI's Finish-page "show readme" box, used as "Create desktop shortcut",
  ticked unless `MUI_FINISHPAGE_SHOWREADME_NOTCHECKED` [docs: MUI2 readme]; silent and passive
  installs always create one [docs: template].
- **Running app:** `CheckIfAppIsRunning` registers `$INSTDIR\keyclean.exe` with Restart Manager;
  if `RmGetList` reports processes it asks, then `RmShutdown(RmForceShutdown)` [docs: utils.nsh].
  The engine runs the same exe, so both processes count. The `PREINSTALL` and `PREUNINSTALL` hooks
  run before this check [docs: template].
- **Reinstall page:** over a different version the default is "Uninstall before installing". The
  old uninstaller gets `/UPDATE` only if this installer was itself started with `/UPDATE` (Tauri's
  updater), so a normal upgrade is a full uninstall [docs: `PageLeaveReinstall`]. Same version:
  "Add/Reinstall" installs over; "Uninstall" uninstalls and then continues installing [docs]. In
  passive mode the page calls its leave function with no radio buttons, whose state then reads as
  unticked: a passive install over the same version uninstalls it first [docs].
- **Uninstall:** deletes `HKCU\...\Run\${PRODUCTNAME}` by name unless `/UPDATE`; deletes the
  install record and `$APPDATA\${BUNDLEID}`, `$LOCALAPPDATA\${BUNDLEID}` only when "Delete the
  application data" is ticked (unticked by default) [docs: template].
- **WebView2:** the `WebView2` section runs before `Install`. It reads EdgeUpdate's `pv` for
  WebView2's app GUID (HKLM, both views, and HKCU); if missing, `downloadBootstrapper` fetches
  `https://go.microsoft.com/fwlink/p/?LinkId=2124703` and runs it with `/silent /install`; a
  failed download or install aborts with a message [docs: template]. Sizes per the config schema:
  `embedBootstrapper` about +1.8 MB, `offlineInstaller` about +127 MB [docs].
- **Hooks:** `installerHooks` is included at the top, before `BUNDLEID`, `MAINBINARYNAME` and the
  languages are defined; `NSIS_HOOK_PREINSTALL`, `POSTINSTALL`, `PREUNINSTALL`, `POSTUNINSTALL`
  are inserted into the sections [docs: template]. So anything in the hooks file that uses those
  defines must be a macro expanded later, and variables must be declared before the template's
  pages use them [tested: makensis error].

## MUI's install-location page

`IDD_DIR` in NSIS's `Contrib/UIs/modern.rc` is 300 x 140 dialog units: intro text 0,0 300x60;
group box `IDC_SELDIRTEXT` (1020) 0,70 300x35 with the folder edit and Browse button; "Space
required" 0,115 and "Space available" 0,125, both 150 wide; a hidden spare checkbox `IDC_CHECK1`
at 180,110 118x10 [docs: https://github.com/kichik/nsis/blob/master/Contrib/UIs/modern.rc]. So
a full-width checkbox under the folder box would overlap the space texts; KeyClean's goes at
158,109 to 300,131, right of them, two lines high. How it looks is checked in M11.md
[assumption until then].

## The single-instance window

`tauri-plugin-single-instance` 2.5.2 names its window class `<identifier>-sic` (with
`_<semver>` only under its `semver` feature, which KeyClean doesn't enable) [docs:
`src/platform_impl/windows.rs`, `Cargo.toml`]. KeyClean subclasses that window so `WM_CLOSE`
exits the app cleanly (fail-safe matrix; harness S23).

## Shortcuts and AppUserModelIDs

- A desktop app's toast needs a Start menu shortcut with its AppUserModelID [docs:
  https://learn.microsoft.com/en-us/windows/win32/shell/enable-desktop-toast-with-appusermodelid].
- `(New-Object -ComObject Shell.Application).Namespace(<folder>).ParseName(<lnk>).ExtendedProperty('System.AppUserModel.ID')`
  reads a shortcut's ID without changing anything: it showed Office's IDs on Max's machine
  [tested]. `Get-StartApps` lists explicit IDs too [tested].

## Registry

- `Run` and `RunOnce`: the command line is at most 260 characters [docs: "Run and RunOnce
  Registry Keys"]. NSIS's `StrLen` counts UTF-16 units in the Unicode build [docs: NSIS manual].
- `StartupApproved\Run` is undocumented (ADR 0018); a "Disable" there outlives a rewritten `Run`
  value [tested in M10].
- LogicLib's `==` is `StrCmp`, which ignores case [docs: NSIS manual 4.9.4.19].

## Code signing

Unsigned for M11. A downloaded unsigned installer gets SmartScreen's "Windows protected your PC"
[assumption: not tried; a locally built file has no mark of the web]. For M12: SignPath
Foundation (free signing for open-source projects) or Azure Artifact Signing [assumption: terms
not checked].

## Windows 10

No Windows 10 machine. Plan: Hyper-V on Max's Windows 11 Pro with Windows 10 22H2 [assumption:
the ISO is still offered after October 2025's end of support]; VMConnect's basic session, since
the enhanced session is RDP and KeyClean ends a lock on a remote disconnect (ADR 0003). Overlay
latency measured there is a VM's, not hardware's.

## Measurements

| What                                 | Value                        | Source                     |
| ------------------------------------ | ---------------------------- | -------------------------- |
| Installer size (default template)    | 1.70 MiB                     | build, 2026-10-22 [tested] |
| Installer size (KeyClean's template) | 1.78 MiB (new icon, 1.87 MB) | build, 2026-11-03 [tested] |
| Installer manifest                   | `asInvoker`                  | built exe [tested]         |
| makensis warnings                    | none (`-V3`)                 | 2026-11-03 [tested]        |
