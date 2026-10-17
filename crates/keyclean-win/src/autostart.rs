//! Start with Windows through the current user's `Run` key (§29, ADR 0018).
//!
//! - **One value, current user only:** `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`,
//!   named [`RUN_VALUE`], holding the quoted exe path and `--autostart`
//!   ([`keyclean_core::startup::run_command`]). Windows runs it each time this user signs in
//!   [docs: "Run and RunOnce Registry Keys"]. Never the machine-wide key, even when KeyClean
//!   runs elevated, so KeyClean never starts for every user. (Elevated with another account's
//!   credentials, `HKEY_CURRENT_USER` is that account's, so the entry lands there.)
//! - **Task Manager's choice is read, never written.** Task Manager → Startup apps records a
//!   user's "Disable" under `...\Explorer\StartupApproved\Run` [undocumented]. KeyClean reads it
//!   to show the state, and never overwrites it, so turning the setting on can't undo it.
//! - **Writes only on the user's request** (the Settings switch). A program started from the
//!   `Run` key shouldn't write to it while it runs [docs: same page], so nothing here runs at
//!   startup except a read.
//! - It is persistent configuration, not lock state: a launch at sign-in starts idle
//!   (invariant 9).

use keyclean_core::startup::{AutostartState, RUN_VALUE, RunCommandError, run_command};
use windows::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_SUCCESS, WIN32_ERROR,
};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_BINARY, REG_EXPAND_SZ, REG_NONE, REG_SZ, REG_VALUE_TYPE, RRF_NOEXPAND,
    RRF_RT_ANY, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::core::{HSTRING, PCWSTR, w};

const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const APPROVED_KEY: PCWSTR =
    w!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run");

/// The largest value KeyClean reads back, in bytes. A `Run` command line holds at most 260
/// characters; anything far larger isn't KeyClean's.
const MAX_READ: u32 = 64 * 1024;

/// Why starting with Windows couldn't be read or changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutostartError {
    /// The exe path can't go into a `Run` value.
    Path(RunCommandError),
    /// A registry call failed with this Windows error code.
    Registry { action: &'static str, code: u32 },
}

impl std::fmt::Display for AutostartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AutostartError::Path(e) => write!(f, "{e}"),
            AutostartError::Registry { action, code } => {
                write!(f, "couldn't {action} (Windows error {code})")
            }
        }
    }
}

impl std::error::Error for AutostartError {}

/// Where starting with Windows stands for the exe at `current_exe`. Only reads.
pub fn autostart_state(current_exe: &str) -> Result<AutostartState, AutostartError> {
    let run = read_value(RUN_KEY, RUN_VALUE).map_err(|code| AutostartError::Registry {
        action: "read the startup entry",
        code: code.0,
    })?;
    // Task Manager's record is only a hint: if it can't be read, the entry counts as enabled.
    let approved = read_value(APPROVED_KEY, RUN_VALUE).ok().flatten();
    Ok(state_from_values(run, approved, current_exe))
}

/// The state from the raw values: KeyClean's `Run` value and its `StartupApproved\Run` data,
/// each with its registry type, `None` when missing.
fn state_from_values(
    run: Option<(REG_VALUE_TYPE, Vec<u8>)>,
    approved: Option<(REG_VALUE_TYPE, Vec<u8>)>,
    current_exe: &str,
) -> AutostartState {
    let command = run.map(|(kind, data)| match kind {
        // Not a string, so not KeyClean's: reads as another program.
        REG_SZ | REG_EXPAND_SZ => utf16_string(&data),
        _ => String::new(),
    });
    // Task Manager writes binary data; anything else is ignored (enabled).
    let approved = approved.and_then(|(kind, data)| (kind == REG_BINARY).then_some(data));
    AutostartState::read(command.as_deref(), approved.as_deref(), current_exe)
}

/// Makes Windows start the exe at `current_exe` with `--autostart` when this user signs in,
/// replacing any earlier entry (another copy's included).
pub fn enable_autostart(current_exe: &str) -> Result<(), AutostartError> {
    let command = run_command(current_exe).map_err(AutostartError::Path)?;
    let data: Vec<u16> = command.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = u32::try_from(data.len() * size_of::<u16>())
        .map_err(|_| AutostartError::Path(RunCommandError::TooLong))?;
    // SAFETY: the key and value names are static null-terminated strings; `data` is a
    // null-terminated UTF-16 string that outlives the call, and `bytes` is its size in bytes
    // including the terminator, as REG_SZ requires [docs: RegSetKeyValueW].
    let result = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            &HSTRING::from(RUN_VALUE),
            REG_SZ.0,
            Some(data.as_ptr().cast()),
            bytes,
        )
    };
    check(result, "write the startup entry")
}

/// Removes KeyClean's `Run` value. Already gone counts as done. Task Manager's record is left
/// alone; Windows ignores it without the value.
pub fn disable_autostart() -> Result<(), AutostartError> {
    // SAFETY: the key and value names are null-terminated strings that outlive the call.
    let result =
        unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, &HSTRING::from(RUN_VALUE)) };
    if result == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    check(result, "remove the startup entry")
}

fn check(result: WIN32_ERROR, action: &'static str) -> Result<(), AutostartError> {
    if result == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(AutostartError::Registry {
            action,
            code: result.0,
        })
    }
}

/// A value under `HKCU\<subkey>`: its type and raw data, `None` if it (or the key) is missing.
/// Environment strings aren't expanded. A value larger than [`MAX_READ`] isn't KeyClean's: it
/// comes back as `REG_NONE` with no data, which reads as another program.
fn read_value(
    subkey: PCWSTR,
    name: &str,
) -> Result<Option<(REG_VALUE_TYPE, Vec<u8>)>, WIN32_ERROR> {
    let name = HSTRING::from(name);
    // The value can change between asking its size and reading it; try a few times.
    for _ in 0..3 {
        let mut size = 0u32;
        // SAFETY: the names are null-terminated strings that outlive the call; with no data
        // buffer, Windows only writes the size to `size` [docs: RegGetValueW].
        let result = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                subkey,
                &name,
                RRF_RT_ANY | RRF_NOEXPAND,
                None,
                None,
                Some(&raw mut size),
            )
        };
        if result == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if result != ERROR_SUCCESS {
            return Err(result);
        }
        if size > MAX_READ {
            return Ok(Some((REG_NONE, Vec::new())));
        }
        let mut data = vec![0u8; size as usize];
        let mut kind = REG_VALUE_TYPE(0);
        // SAFETY: `data` is valid for `size` bytes of writes and `size` holds its length, as
        // RegGetValueW requires; `kind` is valid for a write.
        let result = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                subkey,
                &name,
                RRF_RT_ANY | RRF_NOEXPAND,
                Some(&raw mut kind),
                Some(data.as_mut_ptr().cast()),
                Some(&raw mut size),
            )
        };
        match result {
            ERROR_SUCCESS => {
                data.truncate(size as usize);
                return Ok(Some((kind, data)));
            }
            ERROR_FILE_NOT_FOUND => return Ok(None),
            ERROR_MORE_DATA => continue,
            other => return Err(other),
        }
    }
    Err(ERROR_MORE_DATA)
}

/// A registry string's bytes as text, up to the first null. An odd trailing byte is ignored.
fn utf16_string(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .take_while(|&unit| unit != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(text: &str) -> Vec<u8> {
        text.encode_utf16().flat_map(u16::to_le_bytes).collect()
    }

    #[test]
    fn registry_strings_stop_at_the_null() {
        let mut data = bytes(r#""C:\Jane Doe\keyclean.exe" --autostart"#);
        data.extend([0, 0, b'x', 0]);
        assert_eq!(
            utf16_string(&data),
            r#""C:\Jane Doe\keyclean.exe" --autostart"#
        );
    }

    #[test]
    fn registry_strings_survive_odd_lengths_and_no_null() {
        let mut data = bytes("abc");
        assert_eq!(utf16_string(&data), "abc");
        data.push(7);
        assert_eq!(utf16_string(&data), "abc");
        assert_eq!(utf16_string(&[]), "");
    }

    use windows::Win32::System::Registry::REG_DWORD;

    const EXE: &str = r"C:\Users\Jane Doe\AppData\Local\KeyClean\keyclean.exe";

    fn ours() -> Option<(REG_VALUE_TYPE, Vec<u8>)> {
        Some((REG_SZ, bytes(&run_command(EXE).unwrap())))
    }

    #[test]
    fn our_string_value_reads_as_on() {
        assert_eq!(state_from_values(ours(), None, EXE), AutostartState::On);
        let expand = Some((REG_EXPAND_SZ, bytes(&run_command(EXE).unwrap())));
        assert_eq!(state_from_values(expand, None, EXE), AutostartState::On);
        assert_eq!(state_from_values(None, None, EXE), AutostartState::Off);
    }

    #[test]
    fn a_value_of_another_type_reads_as_another_program() {
        for kind in [REG_DWORD, REG_BINARY, REG_NONE] {
            assert_eq!(
                state_from_values(Some((kind, vec![1, 0, 0, 0])), None, EXE),
                AutostartState::OtherCopy,
                "{kind:?}"
            );
        }
    }

    #[test]
    fn only_binary_task_manager_data_counts() {
        let disabled = Some((REG_BINARY, vec![3; 12]));
        assert_eq!(
            state_from_values(ours(), disabled, EXE),
            AutostartState::DisabledInTaskManager
        );
        let not_binary = Some((REG_DWORD, vec![3, 0, 0, 0]));
        assert_eq!(
            state_from_values(ours(), not_binary, EXE),
            AutostartState::On
        );
    }

    #[test]
    fn errors_read_plainly() {
        let e = AutostartError::Registry {
            action: "write the startup entry",
            code: 5,
        };
        assert_eq!(
            e.to_string(),
            "couldn't write the startup entry (Windows error 5)"
        );
        assert_eq!(
            AutostartError::Path(RunCommandError::TooLong).to_string(),
            "the program path is too long"
        );
    }
}
