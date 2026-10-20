//! Start with Windows and start minimized (§29, ADR 0018). Pure: the app reads the registry and
//! its own arguments and hands them here.
//!
//! KeyClean starts with Windows through a value named [`RUN_VALUE`] under the current user's
//! `Run` key, holding [`run_command`]: the quoted exe path and [`AUTOSTART_FLAG`]. The registry is
//! the only record of the choice (nothing in the settings file), so the setting and Windows can't
//! disagree. Nothing about a lock is involved: a launch at sign-in starts idle (invariant 9).

use std::borrow::Cow;

/// The argument the `Run` value passes, so the app knows Windows started it at sign-in.
pub const AUTOSTART_FLAG: &str = "--autostart";

/// The name of KeyClean's value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
pub const RUN_VALUE: &str = "KeyClean";

/// The longest command line a `Run` value may hold, in characters [docs: "Run and RunOnce
/// Registry Keys"].
pub const MAX_RUN_COMMAND: usize = 260;

/// How this process was started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Launch {
    /// By the user: Start menu, Explorer, a shortcut, a terminal.
    Manual,
    /// By Windows at sign-in, through the `Run` value.
    Autostart,
}

impl Launch {
    /// Reads the launch kind from the command line. The program name may be included.
    pub fn from_args<I, S>(args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        if args.into_iter().any(|arg| arg.as_ref() == AUTOSTART_FLAG) {
            Launch::Autostart
        } else {
            Launch::Manual
        }
    }
}

/// Whether the first instance opens the dashboard at startup. Only a launch at sign-in with
/// "Start minimized" on stays in the tray, and only once the tray icon exists: without it there
/// would be no window and no icon, so no way back to KeyClean.
pub fn opens_dashboard(launch: Launch, start_minimized: bool, tray_ok: bool) -> bool {
    !(launch == Launch::Autostart && start_minimized && tray_ok)
}

/// Whether a second launch opens the running instance's dashboard (ADR 0016). A second launch
/// at sign-in (KeyClean was already running) leaves it as it is.
pub fn second_launch_opens_dashboard(launch: Launch) -> bool {
    launch == Launch::Manual
}

/// Why no `Run` command can be built for an exe path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunCommandError {
    /// The path is empty or contains a double quote.
    InvalidPath,
    /// The command line would be longer than [`MAX_RUN_COMMAND`].
    TooLong,
}

impl std::fmt::Display for RunCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunCommandError::InvalidPath => f.write_str("the program path can't be used"),
            RunCommandError::TooLong => f.write_str("the program path is too long"),
        }
    }
}

impl std::error::Error for RunCommandError {}

/// The `Run` value for `exe`: the path in quotes, so a folder with a space can't be read as a
/// shorter program path, then [`AUTOSTART_FLAG`].
pub fn run_command(exe: &str) -> Result<String, RunCommandError> {
    let exe = plain_path(exe);
    if exe.trim().is_empty() || exe.contains('"') {
        return Err(RunCommandError::InvalidPath);
    }
    let command = format!("\"{exe}\" {AUTOSTART_FLAG}");
    // Windows counts UTF-16 units.
    if command.encode_utf16().count() > MAX_RUN_COMMAND {
        return Err(RunCommandError::TooLong);
    }
    Ok(command)
}

/// The program path in a `Run` command line: the quoted part, or, unquoted, everything up to the
/// first `.exe` followed by a space or the end. `None` if neither is found.
pub fn run_program(command: &str) -> Option<&str> {
    let command = command.trim_start();
    if let Some(rest) = command.strip_prefix('"') {
        let end = rest.find('"')?;
        let path = &rest[..end];
        return (!path.trim().is_empty()).then_some(path);
    }
    let lower = command.to_ascii_lowercase();
    let mut from = 0;
    while let Some(at) = lower[from..].find(".exe") {
        let end = from + at + ".exe".len();
        if lower[end..].is_empty() || lower[end..].starts_with(' ') {
            return Some(&command[..end]);
        }
        from = end;
    }
    None
}

/// Whether the `StartupApproved\Run` data for the value says the user turned it off in Task
/// Manager's Startup apps. The format is undocumented: the first byte is even (2 or 6) when
/// enabled and odd (3 or 7) when disabled. A missing or empty value counts as enabled, as for
/// Windows.
pub fn disabled_in_task_manager(approved: Option<&[u8]>) -> bool {
    approved
        .and_then(|bytes| bytes.first())
        .is_some_and(|first| first & 1 == 1)
}

/// Where starting with Windows stands, from what the registry holds and this process's exe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutostartState {
    /// No `Run` value.
    Off,
    /// The `Run` value starts this exe.
    On,
    /// The `Run` value exists, but the user turned it off in Task Manager's Startup apps.
    /// KeyClean doesn't override that choice; the user turns it back on there.
    DisabledInTaskManager,
    /// The `Run` value starts another copy of KeyClean (or a path that can't be read). Turning
    /// the setting on points it at this copy.
    OtherCopy,
}

impl AutostartState {
    /// What the registry says, for the exe at `current_exe`. `run_value` is KeyClean's `Run`
    /// value and `approved` its `StartupApproved\Run` data, each `None` when missing.
    pub fn read(run_value: Option<&str>, approved: Option<&[u8]>, current_exe: &str) -> Self {
        let Some(command) = run_value else {
            return AutostartState::Off;
        };
        if disabled_in_task_manager(approved) {
            return AutostartState::DisabledInTaskManager;
        }
        match run_program(command) {
            Some(program) if same_path(program, current_exe) => AutostartState::On,
            _ => AutostartState::OtherCopy,
        }
    }

    /// Whether the switch shows as on. A value Task Manager turned off doesn't run at sign-in.
    pub fn is_on(self) -> bool {
        self == AutostartState::On
    }
}

/// Windows paths ignore case. A `\\?\` prefix doesn't change the file.
fn same_path(a: &str, b: &str) -> bool {
    plain_path(a).to_lowercase() == plain_path(b).to_lowercase()
}

/// The path without a `\\?\` long-path prefix: `\\?\C:\x` is `C:\x`, and `\\?\UNC\srv\x` is
/// `\\srv\x`.
fn plain_path(path: &str) -> Cow<'_, str> {
    if let Some(share) = path.strip_prefix(r"\\?\UNC\") {
        Cow::Owned(format!(r"\\{share}"))
    } else {
        Cow::Borrowed(path.strip_prefix(r"\\?\").unwrap_or(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXE: &str = r"C:\Users\Jane Doe\AppData\Local\KeyClean\keyclean.exe";

    #[test]
    fn launch_reads_the_flag_anywhere() {
        assert_eq!(Launch::from_args(Vec::<String>::new()), Launch::Manual);
        assert_eq!(Launch::from_args([EXE]), Launch::Manual);
        assert_eq!(Launch::from_args([EXE, "--autostart"]), Launch::Autostart);
        assert_eq!(Launch::from_args(["--autostart"]), Launch::Autostart);
        assert_eq!(Launch::from_args([EXE, "--AUTOSTART"]), Launch::Manual);
        assert_eq!(Launch::from_args([EXE, "--autostart=1"]), Launch::Manual);
    }

    #[test]
    fn only_a_minimized_sign_in_launch_with_a_tray_stays_hidden() {
        use Launch::{Autostart, Manual};
        for tray_ok in [true, false] {
            for minimized in [true, false] {
                assert!(opens_dashboard(Manual, minimized, tray_ok));
            }
        }
        assert!(!opens_dashboard(Autostart, true, true));
        assert!(opens_dashboard(Autostart, false, true));
        assert!(
            opens_dashboard(Autostart, true, false),
            "no tray icon: never start hidden"
        );
    }

    #[test]
    fn a_second_launch_at_sign_in_opens_nothing() {
        assert!(second_launch_opens_dashboard(Launch::Manual));
        assert!(!second_launch_opens_dashboard(Launch::Autostart));
    }

    #[test]
    fn run_command_quotes_the_path() {
        assert_eq!(run_command(EXE).unwrap(), format!("\"{EXE}\" --autostart"));
        assert_eq!(
            run_command(&format!(r"\\?\{EXE}")).unwrap(),
            format!("\"{EXE}\" --autostart")
        );
    }

    #[test]
    fn run_command_keeps_network_paths_runnable() {
        assert_eq!(
            run_command(r"\\?\UNC\srv\share\keyclean.exe").unwrap(),
            r#""\\srv\share\keyclean.exe" --autostart"#
        );
        let ours = run_command(r"\\srv\share\keyclean.exe").unwrap();
        assert_eq!(
            AutostartState::read(Some(&ours), None, r"\\?\UNC\srv\share\keyclean.exe"),
            AutostartState::On
        );
    }

    #[test]
    fn the_length_limit_counts_utf16_units() {
        // `C:\`, `\keyclean.exe`, the quotes, a space and the flag around the filler.
        let overhead = 16 + 2 + 1 + AUTOSTART_FLAG.len();
        let units = MAX_RUN_COMMAND - overhead;
        // Each broom is two UTF-16 units but one `char`.
        let fits = format!(r"C:\{}\keyclean.exe", "\u{1F9F9}".repeat(units / 2));
        assert!(run_command(&fits).is_ok());
        let over = format!(r"C:\{}\keyclean.exe", "\u{1F9F9}".repeat(units / 2 + 1));
        assert_eq!(run_command(&over), Err(RunCommandError::TooLong));
    }

    #[test]
    fn run_command_refuses_bad_paths() {
        assert_eq!(run_command(""), Err(RunCommandError::InvalidPath));
        assert_eq!(run_command("  "), Err(RunCommandError::InvalidPath));
        assert_eq!(
            run_command(r#"C:\a"b\keyclean.exe"#),
            Err(RunCommandError::InvalidPath)
        );
        let long = format!(r"C:\{}\keyclean.exe", "a".repeat(240));
        assert_eq!(run_command(&long), Err(RunCommandError::TooLong));
    }

    #[test]
    fn run_command_fits_exactly_at_the_limit() {
        // Quotes, a space and the flag around the path.
        let overhead = 2 + 1 + AUTOSTART_FLAG.len();
        // `C:\` and `\keyclean.exe` around the filler.
        let fill = MAX_RUN_COMMAND - overhead - r"C:\\keyclean.exe".len();
        let path = format!(r"C:\{}\keyclean.exe", "a".repeat(fill));
        assert_eq!(run_command(&path).unwrap().len(), MAX_RUN_COMMAND);
        let longer = format!(r"C:\{}\keyclean.exe", "a".repeat(fill + 1));
        assert_eq!(run_command(&longer), Err(RunCommandError::TooLong));
    }

    #[test]
    fn run_program_reads_quoted_and_unquoted_paths() {
        assert_eq!(run_program(&run_command(EXE).unwrap()), Some(EXE));
        assert_eq!(
            run_program(r"C:\Tools\KeyClean.EXE --autostart"),
            Some(r"C:\Tools\KeyClean.EXE")
        );
        assert_eq!(
            run_program(r"C:\My Tools\keyclean.exe"),
            Some(r"C:\My Tools\keyclean.exe")
        );
        assert_eq!(
            run_program(r"C:\a.exe.d\keyclean.exe --x"),
            Some(r"C:\a.exe.d\keyclean.exe")
        );
        assert_eq!(run_program(r#""C:\unterminated"#), None);
        assert_eq!(run_program(r#""" --autostart"#), None);
        assert_eq!(run_program("keyclean"), None);
        assert_eq!(run_program(""), None);
    }

    #[test]
    fn task_manager_state_follows_the_first_byte() {
        assert!(!disabled_in_task_manager(None));
        assert!(!disabled_in_task_manager(Some(&[])));
        assert!(!disabled_in_task_manager(Some(&[2, 0, 0, 0])));
        assert!(!disabled_in_task_manager(Some(&[6, 0, 0, 0])));
        assert!(disabled_in_task_manager(Some(&[3, 0, 0, 0, 1, 2])));
        assert!(disabled_in_task_manager(Some(&[7, 0, 0, 0])));
    }

    #[test]
    fn state_from_the_registry() {
        let ours = run_command(EXE).unwrap();
        let enabled: &[u8] = &[2; 12];
        let disabled: &[u8] = &[3; 12];
        assert_eq!(AutostartState::read(None, None, EXE), AutostartState::Off);
        assert_eq!(
            AutostartState::read(None, Some(disabled), EXE),
            AutostartState::Off
        );
        assert_eq!(
            AutostartState::read(Some(&ours), None, EXE),
            AutostartState::On
        );
        assert_eq!(
            AutostartState::read(Some(&ours), Some(enabled), EXE),
            AutostartState::On
        );
        assert_eq!(
            AutostartState::read(Some(&ours), Some(disabled), EXE),
            AutostartState::DisabledInTaskManager
        );
        assert_eq!(
            AutostartState::read(
                Some(r#""D:\dev\target\debug\keyclean.exe" --autostart"#),
                None,
                EXE
            ),
            AutostartState::OtherCopy
        );
        assert_eq!(
            AutostartState::read(Some("garbage"), None, EXE),
            AutostartState::OtherCopy
        );
    }

    #[test]
    fn paths_compare_without_case_or_long_path_prefix() {
        let ours = run_command(EXE).unwrap();
        assert_eq!(
            AutostartState::read(Some(&ours), None, &EXE.to_uppercase()),
            AutostartState::On
        );
        assert_eq!(
            AutostartState::read(Some(&ours), None, &format!(r"\\?\{EXE}")),
            AutostartState::On
        );
    }

    #[test]
    fn only_on_counts_as_on() {
        assert!(AutostartState::On.is_on());
        for state in [
            AutostartState::Off,
            AutostartState::DisabledInTaskManager,
            AutostartState::OtherCopy,
        ] {
            assert!(!state.is_on(), "{state:?}");
        }
    }
}
