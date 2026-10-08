//! The settings file (§32, ADR 0015): the maximum lock duration and the last started lock's
//! duration and targets. Rust owns it through `tauri-plugin-store`; no window is granted the
//! plugin's commands, so the webview can only ask through this app's own commands.
//!
//! The rules (ranges, defaults, clamping) live in `keyclean_core::settings`. A missing or corrupt
//! file loads as defaults [docs: tauri-plugin-store 2.5.0 source, `StoreBuilder::build` ignores a
//! failed load]. Nothing about a running lock is written (invariant 9).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use keyclean_win::LockTargets;
use keyclean_win::keyclean_core::settings::{Settings, StoredSettings};
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
            // Written while the settings are held, so two saves can't land out of order.
            if let Err(e) = store.save() {
                eprintln!("[keyclean] settings: couldn't save: {e}");
            }
        }
        updated
    }
}
