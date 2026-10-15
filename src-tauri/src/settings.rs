//! The settings file (§32, ADR 0015): the maximum lock duration, the last started lock's
//! duration and targets, and the start shortcut with what it locks (ADR 0017). Rust owns it through `tauri-plugin-store`; no window is granted the
//! plugin's commands, so the webview can only ask through this app's own commands.
//!
//! The rules (ranges, defaults, clamping) live in `keyclean_core::settings`. A missing or corrupt
//! file loads as defaults [docs: tauri-plugin-store 2.5.0 source, `StoreBuilder::build` ignores a
//! failed load]. Nothing about a running lock is written (invariant 9).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use keyclean_win::LockTargets;
use keyclean_win::keyclean_core::settings::{Settings, StoredSettings, StoredShortcut};
use keyclean_win::keyclean_core::shortcut::{Shortcut, ShortcutAction};
use serde_json::Value;
use tauri::{AppHandle, Wry};
use tauri_plugin_store::{Store, StoreBuilder};

use crate::guard;

/// The file name, under the app data folder.
const FILE: &str = "settings.json";

const MAX_LOCK_MINUTES: &str = "maxLockMinutes";
const LAST_DURATION_SECS: &str = "lastDurationSecs";
const LAST_KEYBOARD: &str = "lastKeyboard";
const LAST_MOUSE: &str = "lastMouse";
/// `"Ctrl+Shift+K"` and the like, or `null` when the shortcut is off.
const START_SHORTCUT: &str = "startShortcut";
/// `"keyboard"` or `"selected"`.
const SHORTCUT_ACTION: &str = "shortcutAction";

/// The settings in effect, and the file they are written to.
pub struct SettingsState {
    settings: Mutex<Settings>,
    /// `None` if the store couldn't be opened; the settings then last until the app exits.
    store: Option<Arc<Store<Wry>>>,
}

impl SettingsState {
    /// Loads the settings file, or defaults.
    pub fn load(app: &AppHandle) -> Self {
        let store = match StoreBuilder::new(app, FILE).disable_auto_save().build() {
            Ok(store) => Some(store),
            Err(e) => {
                eprintln!("[keyclean] settings: couldn't open the settings file: {e}");
                None
            }
        };
        let stored = store
            .as_ref()
            .map_or_else(StoredSettings::default, |store| {
                let get = |key: &str| store.get(key);
                StoredSettings {
                    max_lock_minutes: get(MAX_LOCK_MINUTES).as_ref().and_then(Value::as_u64),
                    last_duration_secs: get(LAST_DURATION_SECS).as_ref().and_then(Value::as_u64),
                    last_keyboard: get(LAST_KEYBOARD).as_ref().and_then(Value::as_bool),
                    last_mouse: get(LAST_MOUSE).as_ref().and_then(Value::as_bool),
                    start_shortcut: get(START_SHORTCUT).as_ref().and_then(read_shortcut),
                    shortcut_action: get(SHORTCUT_ACTION)
                        .as_ref()
                        .and_then(Value::as_str)
                        .and_then(ShortcutAction::parse),
                }
            });
        SettingsState {
            settings: Mutex::new(Settings::from_stored(stored)),
            store,
        }
    }

    /// The settings in effect.
    pub fn current(&self) -> Settings {
        *guard(&self.settings)
    }

    /// Changes the maximum lock duration (clamped to 1-60 min) and saves it.
    pub fn set_max_lock_minutes(&self, minutes: u64) -> Settings {
        self.update(|s| s.set_max_lock_minutes(minutes))
    }

    /// Remembers the choices of a lock that started, and saves them.
    pub fn remember_lock(&self, duration: Duration, targets: LockTargets) -> Settings {
        self.update(|s| s.remember_lock(duration, targets))
    }

    /// Changes the start shortcut (`None` turns it off) and saves it.
    pub fn set_start_shortcut(&self, shortcut: Option<Shortcut>) -> Settings {
        self.update(|s| s.set_start_shortcut(shortcut))
    }

    /// Changes what the start shortcut locks and saves it.
    pub fn set_shortcut_action(&self, action: ShortcutAction) -> Settings {
        self.update(|s| s.set_shortcut_action(action))
    }

    fn update(&self, change: impl FnOnce(&mut Settings)) -> Settings {
        let mut settings = guard(&self.settings);
        change(&mut settings);
        let updated = *settings;
        if let Some(store) = &self.store {
            let stored = updated.to_stored();
            let put = |key: &str, value: Option<Value>| {
                if let Some(value) = value {
                    store.set(key, value);
                }
            };
            put(MAX_LOCK_MINUTES, stored.max_lock_minutes.map(Value::from));
            put(
                LAST_DURATION_SECS,
                stored.last_duration_secs.map(Value::from),
            );
            put(LAST_KEYBOARD, stored.last_keyboard.map(Value::from));
            put(LAST_MOUSE, stored.last_mouse.map(Value::from));
            put(START_SHORTCUT, stored.start_shortcut.map(write_shortcut));
            put(
                SHORTCUT_ACTION,
                stored.shortcut_action.map(|a| Value::from(a.as_str())),
            );
            // Written while the settings are held, so two saves can't land out of order.
            if let Err(e) = store.save() {
                eprintln!("[keyclean] settings: couldn't save: {e}");
            }
        }
        updated
    }
}

/// `null` is off; a string that breaks the shortcut rules counts as missing (the default).
fn read_shortcut(value: &Value) -> Option<StoredShortcut> {
    match value {
        Value::Null => Some(StoredShortcut::Off),
        Value::String(text) => Shortcut::parse(text).ok().map(StoredShortcut::On),
        _ => None,
    }
}

fn write_shortcut(stored: StoredShortcut) -> Value {
    match stored {
        StoredShortcut::Off => Value::Null,
        StoredShortcut::On(shortcut) => Value::from(shortcut.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_values_round_trip() {
        for stored in [
            StoredShortcut::Off,
            StoredShortcut::On(Shortcut::parse("Alt+Shift+F5").unwrap()),
        ] {
            assert_eq!(read_shortcut(&write_shortcut(stored)), Some(stored));
        }
    }

    #[test]
    fn a_broken_shortcut_counts_as_missing() {
        for value in [
            Value::from("Ctrl+Alt+K"),
            Value::from("Ctrl+C"),
            Value::from(""),
            Value::from(3),
            Value::from(false),
        ] {
            assert_eq!(read_shortcut(&value), None, "{value}");
        }
    }
}
