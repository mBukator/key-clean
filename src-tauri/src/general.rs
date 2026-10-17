//! Settings → General (§32, ADR 0018): start with Windows, start minimized, show notifications.
//!
//! Starting with Windows is read from the registry each time the page asks, never stored, so the
//! switch can't drift from what Windows will do. Only the switch writes it, and only in release
//! builds: a debug build would point the entry at `target\debug`.

use keyclean_win::keyclean_core::settings::Settings;
use keyclean_win::keyclean_core::startup::AutostartState;
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::bridge::ErrorDto;
use crate::settings::SettingsState;

/// What Settings → General shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneralDto {
    /// `on`, `off`, `disabledInTaskManager`, `otherCopy`, or `unknown` when it couldn't be read.
    start_with_windows: &'static str,
    /// Whether this build may change it (release builds only).
    start_with_windows_available: bool,
    start_minimized: bool,
    notifications: bool,
    /// Why the startup entry couldn't be read or changed.
    error: Option<ErrorDto>,
}

impl GeneralDto {
    fn new(state: Result<AutostartState, ErrorDto>, settings: &Settings) -> Self {
        let (start_with_windows, error) = match state {
            Ok(state) => (state_name(state), None),
            Err(e) => ("unknown", Some(e)),
        };
        GeneralDto {
            start_with_windows,
            start_with_windows_available: available(),
            start_minimized: settings.start_minimized(),
            notifications: settings.notifications(),
            error,
        }
    }

    fn with_error(mut self, error: ErrorDto) -> Self {
        self.error = Some(error);
        self
    }
}

fn state_name(state: AutostartState) -> &'static str {
    match state {
        AutostartState::Off => "off",
        AutostartState::On => "on",
        AutostartState::DisabledInTaskManager => "disabledInTaskManager",
        AutostartState::OtherCopy => "otherCopy",
    }
}

/// Release builds only, so a dev build never registers `target\debug`.
fn available() -> bool {
    !cfg!(debug_assertions)
}

/// This exe's path, as the startup entry holds it. `read` picks the error's message: checking or
/// changing the entry.
fn current_exe(read: bool) -> Result<String, ErrorDto> {
    let exe = std::env::current_exe()
        .map_err(|e| ErrorDto::autostart(read, format!("couldn't find KeyClean's path: {e}")))?;
    exe.to_str()
        .map(str::to_owned)
        .ok_or_else(|| ErrorDto::autostart(read, "KeyClean's path isn't valid Unicode".to_owned()))
}

fn read_state() -> Result<AutostartState, ErrorDto> {
    let exe = current_exe(true)?;
    keyclean_win::autostart_state(&exe).map_err(|e| ErrorDto::autostart(true, e.to_string()))
}

fn current(app: &AppHandle) -> GeneralDto {
    GeneralDto::new(read_state(), &app.state::<SettingsState>().current())
}

/// What Settings → General shows, with the startup entry read from the registry now.
#[tauri::command]
pub fn get_general_settings(app: AppHandle) -> GeneralDto {
    current(&app)
}

/// Turns starting with Windows on (for this exe, replacing another copy's entry) or off, then
/// reads the registry back. Refused in debug builds. A failure leaves the entry as it was and
/// comes back in the returned settings' `error`.
#[tauri::command]
pub fn set_start_with_windows(app: AppHandle, on: bool) -> GeneralDto {
    if !available() {
        return current(&app).with_error(ErrorDto::autostart_dev_build());
    }
    let changed = if on {
        current_exe(false).and_then(|exe| {
            keyclean_win::enable_autostart(&exe)
                .map_err(|e| ErrorDto::autostart(false, e.to_string()))
        })
    } else {
        keyclean_win::disable_autostart().map_err(|e| ErrorDto::autostart(false, e.to_string()))
    };
    eprintln!(
        "[keyclean] start with windows: {}",
        match (&changed, on) {
            (Ok(()), true) => "turned on",
            (Ok(()), false) => "turned off",
            (Err(_), _) => "change failed",
        }
    );
    let dto = current(&app);
    match changed {
        Ok(()) => dto,
        Err(e) => dto.with_error(e),
    }
}

/// Changes whether a start at sign-in stays in the tray, and saves it.
#[tauri::command]
pub fn set_start_minimized(app: AppHandle, on: bool) -> GeneralDto {
    app.state::<SettingsState>().set_start_minimized(on);
    current(&app)
}

/// Turns Windows notifications on or off, and saves it.
#[tauri::command]
pub fn set_notifications(app: AppHandle, on: bool) -> GeneralDto {
    app.state::<SettingsState>().set_notifications(on);
    current(&app)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_camel_case() {
        let dto = GeneralDto::new(
            Ok(AutostartState::DisabledInTaskManager),
            &Settings::default(),
        );
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["startWithWindows"], "disabledInTaskManager");
        assert_eq!(json["startWithWindowsAvailable"], !cfg!(debug_assertions));
        assert_eq!(json["startMinimized"], true);
        assert_eq!(json["notifications"], true);
        assert!(json["error"].is_null());
    }

    #[test]
    fn a_failed_read_shows_as_unknown_with_the_error() {
        let dto = GeneralDto::new(
            Err(ErrorDto::autostart(true, "x".to_owned())),
            &Settings::default(),
        );
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["startWithWindows"], "unknown");
        assert_eq!(json["error"]["messageKey"], "error.autostart_read");
    }

    #[test]
    fn every_state_has_a_name() {
        for state in [
            AutostartState::Off,
            AutostartState::On,
            AutostartState::DisabledInTaskManager,
            AutostartState::OtherCopy,
        ] {
            assert!(!state_name(state).is_empty());
        }
    }
}
