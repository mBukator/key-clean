//! S52-S53 (`--installer`): the NSIS installer (M11, ADR 0019).
//!
//! S52 installs silently into a temporary folder whose path has a space and checks what the
//! installer wrote. S53 starts the installed KeyClean (idle, in the tray), uninstalls silently
//! and checks that KeyClean was closed and nothing is left. No input lock is engaged.
//!
//! Both change the current user's Start menu, `Run` key and Apps & features list, so they refuse
//! to run when KeyClean is installed or another copy owns the `Run` value.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};
use std::thread::sleep;
use std::time::{Duration, Instant};

use keyclean_core::startup::run_command;

use crate::report::Report;
use crate::scenarios::{Outcome, keyclean_processes};

const RUN_KEY: &str = r"HKCU:\Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVED_KEY: &str =
    r"HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const UNINSTALL_KEY: &str = r"HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\KeyClean";
const MACHINE_UNINSTALL_KEY: &str =
    r"HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\KeyClean";
/// The installer's record of its folder: `Software\<publisher>\<product name>`.
const INSTALL_RECORD_KEY: &str = r"HKCU:\Software\mBukator\KeyClean";
const IDENTIFIER: &str = "io.github.mbukator.keyclean";
const PUBLISHER: &str = "mBukator";
/// Where S52 installs: a folder name with a space, as under a user name like "Ann Lee".
const INSTALL_FOLDER: &str = "KeyClean e2e install";

/// Runs S52-S53 and adds them to `report`. `Err` is a setup problem: nothing was installed.
pub fn run(setup: Option<PathBuf>, report: &mut Report) -> Result<(), String> {
    let setup = match setup {
        Some(path) => path,
        None => newest_setup()?,
    };
    if !setup.is_file() {
        return Err(format!("no installer at {}", setup.display()));
    }
    if test_path(UNINSTALL_KEY)? {
        return Err(
            "KeyClean is installed. Uninstall it first (Settings > Apps): these checks install \
             their own copy and would replace its Start menu shortcut and startup entry."
                .into(),
        );
    }
    if let Some(value) = reg_value(RUN_KEY, "KeyClean")? {
        return Err(format!(
            "another copy of KeyClean starts at sign-in ({value}). Turn Start with Windows off \
             in that copy first: these checks would replace its entry."
        ));
    }
    println!("Installer: {}", setup.display());

    let dir = std::env::temp_dir().join(INSTALL_FOLDER);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .map_err(|e| format!("couldn't clear {}: {e}", dir.display()))?;
    }
    let before = Snapshot::take()?;

    print!("S52 The installer installs for this user, with the startup entry ... ");
    let installed = install(&setup, &dir);
    println!("{}", label(&installed));
    let can_uninstall = dir.join("uninstall.exe").is_file();
    report.add(
        "S52",
        "M11 1",
        "The installer installs for this user, with the startup entry",
        installed,
    );

    print!("S53 Uninstalling closes KeyClean and leaves nothing behind ... ");
    let uninstalled = if can_uninstall {
        uninstall(&dir, &before)
    } else {
        Outcome::Skip("S52 didn't install an uninstaller".into())
    };
    println!("{}", label(&uninstalled));
    report.add(
        "S53",
        "M11 9",
        "Uninstalling closes KeyClean and leaves nothing behind",
        uninstalled,
    );
    if dir.exists() || test_path(UNINSTALL_KEY).unwrap_or(false) {
        eprintln!(
            "warning: the test install is still there ({}). Uninstall KeyClean from Settings > \
             Apps.",
            dir.display()
        );
    }
    Ok(())
}

fn label(outcome: &Outcome) -> &'static str {
    match outcome {
        Outcome::Pass(_) => "PASS",
        Outcome::Fail(_) => "FAIL",
        Outcome::Skip(_) => "SKIP",
    }
}

/// The newest `KeyClean_*_x64-setup.exe` that `bun tauri build` made.
fn newest_setup() -> Result<PathBuf, String> {
    let nsis = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.parent()?.join(r"release\bundle\nsis")))
        .ok_or("couldn't find the target folder")?;
    let entries = std::fs::read_dir(&nsis).map_err(|e| {
        format!(
            "no installer in {} ({e}); build one with `bun tauri build`, or pass its path",
            nsis.display()
        )
    })?;
    entries
        .filter_map(Result::ok)
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            name.starts_with("keyclean_") && name.ends_with("_x64-setup.exe")
        })
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
        .map(|e| e.path())
        .ok_or_else(|| format!("no KeyClean_*_x64-setup.exe in {}", nsis.display()))
}

/// What must not change from before the install to after the uninstall.
struct Snapshot {
    settings: bool,
    scancode_map: String,
    approved: bool,
}

impl Snapshot {
    fn take() -> Result<Snapshot, String> {
        Ok(Snapshot {
            settings: settings_dir().is_some_and(|d| d.exists()),
            scancode_map: powershell(
                r"(Get-ItemProperty -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Control\Keyboard Layout' -Name 'Scancode Map' -ErrorAction SilentlyContinue).'Scancode Map' -join ','",
            )?,
            approved: reg_value(APPROVED_KEY, "KeyClean")?.is_some(),
        })
    }
}

fn install(setup: &Path, dir: &Path) -> Outcome {
    let run = || -> Result<String, String> {
        // `/D=` must come last and unquoted, even with a space in it [docs: NSIS 3.2]. Up to
        // 5 minutes: without WebView2 the installer downloads it.
        let mut installer = Command::new(setup);
        installer.arg("/S").raw_arg(format!("/D={}", dir.display()));
        let status = run_bounded(installer, "the installer", Duration::from_secs(300))?;
        if !status.success() {
            return Err(format!("the installer exited with {status}"));
        }
        let mut problems = Vec::new();
        let exe = dir.join("keyclean.exe");
        for file in [&exe, &dir.join("uninstall.exe")] {
            if !file.is_file() {
                problems.push(format!("{} is missing", file.display()));
            }
        }
        let expected = run_command(&exe.to_string_lossy()).map_err(|e| e.to_string())?;
        match reg_value(RUN_KEY, "KeyClean")? {
            Some(value) if value == expected => {}
            other => problems.push(format!("Run\\KeyClean is {other:?}, not {expected:?}")),
        }
        let aumid = powershell(
            "$f = (New-Object -ComObject Shell.Application).Namespace([Environment]::GetFolderPath('Programs')); \
             $i = $f.ParseName('KeyClean.lnk'); if ($i) { 'lnk:' + $i.ExtendedProperty('System.AppUserModel.ID') }",
        )?;
        if aumid != format!("lnk:{IDENTIFIER}") {
            problems.push(format!(
                "the Start menu shortcut's AppUserModelID is {aumid:?}, not {IDENTIFIER} \
                 (empty: no shortcut)"
            ));
        }
        let entry = powershell(&format!(
            "$k = Get-ItemProperty -LiteralPath '{UNINSTALL_KEY}' -ErrorAction SilentlyContinue; \
             if ($k) {{ $k.Publisher + '|' + $k.DisplayName + '|' + $k.InstallLocation }}"
        ))?;
        let expected_entry = format!("{PUBLISHER}|KeyClean|\"{}\"", dir.display());
        if entry != expected_entry {
            problems.push(format!(
                "the Apps & features entry is {entry:?}, not {expected_entry:?}"
            ));
        }
        if test_path(MACHINE_UNINSTALL_KEY)? {
            problems.push("a machine-wide Apps & features entry exists".into());
        }
        if desktop_shortcut()?.exists() {
            problems.push("a desktop shortcut was created".into());
        }
        if problems.is_empty() {
            Ok(format!(
                "installed to {}; Run\\KeyClean = {expected}; shortcut ID {IDENTIFIER}; \
                 publisher {PUBLISHER}; no desktop shortcut, no machine-wide entry",
                dir.display()
            ))
        } else {
            Err(problems.join("; "))
        }
    };
    run().into()
}

fn uninstall(dir: &Path, before: &Snapshot) -> Outcome {
    let run = || -> Result<String, String> {
        // KeyClean as Windows starts it at sign-in: idle, usually in the tray.
        let _app = KillOnDrop(
            Command::new(dir.join("keyclean.exe"))
                .arg("--autostart")
                .spawn()
                .map_err(|e| format!("couldn't start the installed KeyClean: {e}"))?,
        );
        let mut problems = Vec::new();
        if !poll(Duration::from_secs(20), || {
            keyclean_processes().is_ok_and(|n| n == 2)
        }) {
            // Uninstall anyway, so the test install doesn't stay behind.
            problems.push("the installed KeyClean didn't start its engine within 20 s".into());
        }
        let started = Instant::now();
        let mut uninstaller = Command::new(dir.join("uninstall.exe"));
        uninstaller.arg("/S");
        let status = run_bounded(uninstaller, "the uninstaller", Duration::from_secs(60))?;
        if !status.success() {
            return Err(format!("the uninstaller exited with {status}"));
        }
        // The uninstaller copies itself to %TEMP% and returns at once; its work is done when
        // nothing it removes is left.
        let mut left = Vec::new();
        let done = poll(Duration::from_secs(60), || {
            left = leftovers(dir).unwrap_or_else(|e| vec![e]);
            left.is_empty()
        });
        let took = started.elapsed();
        if !done {
            problems.push(format!("after 60 s: {}", left.join("; ")));
        }
        poll(Duration::from_secs(5), || {
            keyclean_processes().is_ok_and(|n| n == 0)
        });
        let processes = keyclean_processes()?;
        if processes != 0 {
            problems.push(format!(
                "{processes} keyclean.exe process(es) still running"
            ));
        }
        // The settings stay unless the user ticks the box, which a silent uninstall doesn't.
        if before.settings && !settings_dir().is_some_and(|d| d.exists()) {
            problems.push("the settings folder was deleted".into());
        }
        let after = Snapshot::take()?;
        if after.scancode_map != before.scancode_map {
            problems.push("the Scancode Map changed".into());
        }
        if problems.is_empty() {
            Ok(format!(
                "KeyClean closed and uninstalled in {:.1} s; no Run value{}, shortcut, install \
                 record or WebView2 data left; settings {}; Scancode Map unchanged",
                took.as_secs_f64(),
                if before.approved {
                    " (an orphaned StartupApproved value was removed too)"
                } else {
                    " or StartupApproved value"
                },
                if before.settings {
                    "kept"
                } else {
                    "never existed"
                },
            ))
        } else {
            Err(problems.join("; "))
        }
    };
    run().into()
}

/// What the uninstaller removes and is still there.
fn leftovers(dir: &Path) -> Result<Vec<String>, String> {
    let mut left = Vec::new();
    if dir.exists() {
        left.push(format!("{} is still there", dir.display()));
    }
    if test_path(UNINSTALL_KEY)? {
        left.push("the Apps & features entry is still there".into());
    }
    if let Some(value) = reg_value(RUN_KEY, "KeyClean")? {
        left.push(format!("Run\\KeyClean is still there ({value})"));
    }
    if reg_value(APPROVED_KEY, "KeyClean")?.is_some() {
        left.push("StartupApproved\\Run\\KeyClean is still there".into());
    }
    let shortcut = powershell(
        "Test-Path -LiteralPath (Join-Path ([Environment]::GetFolderPath('Programs')) 'KeyClean.lnk')",
    )?;
    if shortcut != "False" {
        left.push("the Start menu shortcut is still there".into());
    }
    if test_path(INSTALL_RECORD_KEY)? {
        left.push(format!("{INSTALL_RECORD_KEY} is still there"));
    }
    if let Some(cache) = local_data_dir().filter(|d| d.exists()) {
        left.push(format!("{} is still there", cache.display()));
    }
    Ok(left)
}

/// Kills the child when dropped, so a failed check never leaves a process behind.
struct KillOnDrop(Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

/// Runs `command` and waits at most `limit` for it to exit; kills it after that.
fn run_bounded(mut command: Command, what: &str, limit: Duration) -> Result<ExitStatus, String> {
    let mut child = KillOnDrop(
        command
            .spawn()
            .map_err(|e| format!("couldn't run {what}: {e}"))?,
    );
    let until = Instant::now() + limit;
    loop {
        if let Some(status) = child
            .0
            .try_wait()
            .map_err(|e| format!("couldn't wait for {what}: {e}"))?
        {
            return Ok(status);
        }
        if Instant::now() >= until {
            return Err(format!("{what} didn't finish within {} s", limit.as_secs()));
        }
        sleep(Duration::from_millis(250));
    }
}

fn settings_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|d| Path::new(&d).join(IDENTIFIER))
}

fn local_data_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|d| Path::new(&d).join(IDENTIFIER))
}

fn desktop_shortcut() -> Result<PathBuf, String> {
    let desktop = powershell("[Environment]::GetFolderPath('Desktop')")?;
    Ok(Path::new(&desktop).join("KeyClean.lnk"))
}

fn poll(timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
    let until = Instant::now() + timeout;
    loop {
        if done() {
            return true;
        }
        if Instant::now() >= until {
            return false;
        }
        sleep(Duration::from_millis(250));
    }
}

fn test_path(key: &str) -> Result<bool, String> {
    Ok(powershell(&format!("Test-Path -LiteralPath '{key}'"))? == "True")
}

/// A registry value as text, or `None` when it doesn't exist.
fn reg_value(key: &str, name: &str) -> Result<Option<String>, String> {
    let out = powershell(&format!(
        "$v = Get-ItemProperty -LiteralPath '{key}' -Name '{name}' -ErrorAction SilentlyContinue; \
         if ($v) {{ 'value:' + $v.'{name}' }}"
    ))?;
    Ok(out.strip_prefix("value:").map(str::to_owned))
}

/// Runs a read-only PowerShell script and returns its trimmed output; a failed script is an
/// error, so a missing answer is never read as "absent". The script must not contain double
/// quotes, which would need escaping on the command line.
fn powershell(script: &str) -> Result<String, String> {
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .map_err(|e| format!("couldn't run powershell: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "powershell failed ({}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}
