//! The start shortcut (§28, ADR 0017): a chord that starts a lock from any app.
//!
//! Registered with Windows through the official global-shortcut plugin, which uses
//! `RegisterHotKey` [docs: global-hotkey 0.8.0 source]. That is not a hook and registers no Raw
//! Input: Windows tells KeyClean only when this exact chord is pressed, and KeyClean sees no other
//! key (invariant 4, privacy invariants). Idle cost is one registration; the plugin's short-lived
//! thread that waits for the key's release runs only while the chord is held.
//!
//! Rules:
//! - The registration changes only off the main thread: the plugin waits for the main thread to
//!   run each change, so a change on it could wait for itself. `applying` keeps two changes from
//!   interleaving; `inner` is never held while the plugin works, so the main thread (the tray, the
//!   shortcut handler) can always read it.
//! - A shortcut that can't be registered (another app holds it) is shown, never fatal: KeyClean
//!   starts as usual, and registering is tried again when the dashboard opens, when the user saves
//!   a shortcut, or asks to try again. Nothing polls.
//! - The shortcut stays registered during a lock. A press then is refused like a second tray lock.
//! - While the user picks a new shortcut in Settings, the current one is unregistered, so pressing
//!   it reaches the page instead of starting a lock.
//! - Logs say what happened to the shortcut, never which keys it uses (privacy invariants).

use std::str::FromStr;
use std::sync::Mutex;

use keyclean_win::LockTargets;
use keyclean_win::keyclean_core::notify::Toast;
use keyclean_win::keyclean_core::shortcut::{Key, Modifier, Shortcut, ShortcutAction};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{
    Code, GlobalShortcutExt, Modifiers, Shortcut as PluginShortcut, ShortcutEvent,
    ShortcutState as PressState,
};

use crate::bridge::ErrorDto;
use crate::settings::SettingsState;
use crate::{LockOrigin, MAIN_WINDOW, guard, i18n, lock_remembered, tray};

/// Name of the event emitted to the main window when the shortcut's state changes.
pub const SHORTCUT_EVENT: &str = "shortcut-changed";

/// The start shortcut's registration.
pub struct ShortcutState {
    inner: Mutex<Inner>,
    /// Held while the registration changes. Never taken on the main thread.
    applying: Mutex<()>,
    /// Debug builds only: the harness's shortcut and targets (`KEYCLEAN_E2E_SHORTCUT`).
    e2e: Option<E2eShortcut>,
}

#[derive(Default)]
struct Inner {
    /// The shortcut registered with Windows now.
    registered: Option<Shortcut>,
    /// Why the configured shortcut isn't registered.
    error: Option<ErrorDto>,
    /// The user is picking a new shortcut; nothing is registered meanwhile.
    capturing: bool,
}

#[derive(Clone, Copy)]
struct E2eShortcut {
    shortcut: Shortcut,
    targets: Option<LockTargets>,
}

/// What the dashboard shows about the shortcut. Mirrors `ShortcutInfo` in
/// `src/shared/types/engine.ts`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutDto {
    /// The configured shortcut as key caps (`["Ctrl", "Shift", "K"]`); empty when it is off.
    keys: Vec<String>,
    /// `keyboard` or `selected`.
    action: &'static str,
    /// `off`, `active`, `unavailable` or `capturing`.
    state: &'static str,
    /// Why it isn't registered, while `unavailable`.
    error: Option<ErrorDto>,
}

/// What the tray shows next to the item the shortcut does, while it is registered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrayHint {
    pub shortcut: Shortcut,
    pub action: ShortcutAction,
}

impl ShortcutState {
    pub fn new() -> Self {
        ShortcutState {
            inner: Mutex::new(Inner::default()),
            applying: Mutex::new(()),
            e2e: e2e_shortcut(),
        }
    }
}

/// The shortcut the settings (or the harness) ask for; `None` while it is off or being picked.
fn wanted(app: &AppHandle, state: &ShortcutState, capturing: bool) -> Option<Shortcut> {
    if capturing {
        return None;
    }
    match state.e2e {
        Some(e2e) => Some(e2e.shortcut),
        None => app.state::<SettingsState>().current().start_shortcut(),
    }
}

/// The plugin's form of a shortcut. Only Shift plus Ctrl or Alt, so never the Windows key.
fn to_plugin(shortcut: Shortcut) -> Option<PluginShortcut> {
    let modifier = match shortcut.modifier() {
        Modifier::Ctrl => Modifiers::CONTROL,
        Modifier::Alt => Modifiers::ALT,
    };
    let code = match shortcut.key() {
        Key::Letter(c) => format!("Key{}", char::from(c)),
        Key::Digit(c) => format!("Digit{}", char::from(c)),
        Key::Function(n) => format!("F{n}"),
    };
    let code = Code::from_str(&code).ok()?;
    Some(PluginShortcut::new(Some(modifier | Modifiers::SHIFT), code))
}

/// Registers `shortcut` with its handler. Off the main thread only (see the module docs).
fn register(app: &AppHandle, shortcut: Shortcut) -> Result<(), ErrorDto> {
    let plugin = to_plugin(shortcut)
        .ok_or_else(|| ErrorDto::shortcut_unavailable("the key has no plugin code".to_owned()))?;
    app.global_shortcut()
        .on_shortcut(plugin, move |app, _, event: ShortcutEvent| {
            // Only the press acts; the release arrives from the plugin's own thread.
            if event.state == PressState::Pressed {
                on_press(app, shortcut);
            }
        })
        .map_err(|e| ErrorDto::shortcut_unavailable(safe_details("register", &e.to_string())))
}

/// The plugin's error without the shortcut's keys. global-hotkey 0.8.0 prints the hotkey (its key
/// name) in some errors, and keys never go in logs, errors or events (privacy invariants). Keeps
/// what helps: whether it was already registered (the text of that version's
/// `AlreadyRegistered`), or the OS error code.
fn safe_details(action: &str, error: &str) -> String {
    if error.contains("already registered") {
        return format!(
            "couldn't {action} the shortcut: another program has already registered it"
        );
    }
    let code = error.find("os error ").and_then(|at| {
        let digits: String = error[at + "os error ".len()..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        digits.parse::<u32>().ok()
    });
    match code {
        Some(code) => {
            format!("couldn't {action} the shortcut: Windows refused it (os error {code})")
        }
        None => format!("couldn't {action} the shortcut: Windows refused it"),
    }
}

/// The shortcut as the tray shows it, with the modifier names from the strings file.
pub fn label(shortcut: Shortcut) -> String {
    shortcut
        .parts()
        .iter()
        .map(|part| match part.as_str() {
            "Ctrl" => i18n::t("overlay.key.ctrl"),
            "Alt" => i18n::t("overlay.key.alt"),
            "Shift" => i18n::t("shortcut.key.shift"),
            key => key.to_owned(),
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// Unregisters `shortcut`. A failure is only logged: the handler ignores a shortcut that isn't
/// the registered one, so a leftover registration starts nothing.
fn unregister(app: &AppHandle, shortcut: Shortcut) {
    let Some(plugin) = to_plugin(shortcut) else {
        return;
    };
    if let Err(e) = app.global_shortcut().unregister(plugin) {
        eprintln!("[keyclean] {}", safe_details("unregister", &e.to_string()));
    }
}

/// Brings the registration in line with the settings: unregisters a shortcut no longer wanted and
/// tries to register the wanted one. Off the main thread, with `applying` held.
fn reconcile(app: &AppHandle, state: &ShortcutState) {
    let (registered, capturing) = {
        let inner = guard(&state.inner);
        (inner.registered, inner.capturing)
    };
    let want = wanted(app, state, capturing);
    if registered == want {
        guard(&state.inner).error = None;
        return;
    }
    if let Some(old) = registered {
        unregister(app, old);
        guard(&state.inner).registered = None;
    }
    let error = want.and_then(|new| match register(app, new) {
        Ok(()) => {
            eprintln!("[keyclean] shortcut: registered");
            guard(&state.inner).registered = Some(new);
            None
        }
        Err(e) => {
            eprintln!("[keyclean] shortcut: unavailable");
            Some(e)
        }
    });
    guard(&state.inner).error = error;
}

/// Runs `change` with `applying` held, off the main thread, then tells the dashboard and the tray.
/// `None` if the change didn't finish (its thread panicked).
async fn change<T: Send + 'static>(
    app: &AppHandle,
    change: impl FnOnce(&AppHandle, &ShortcutState) -> T + Send + 'static,
) -> Option<T> {
    let handle = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<ShortcutState>();
        let _applying = guard(&state.applying);
        change(&handle, &state)
    })
    .await;
    announce(app);
    result
        .inspect_err(|e| eprintln!("[keyclean] shortcut: change failed: {e}"))
        .ok()
}

/// Registers the configured shortcut. Called once from `setup`; the work runs once the event
/// loop does, because the plugin waits for the main thread.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        change(&app, reconcile).await;
        // With no window open after a start at sign-in, only a notification can say the
        // shortcut isn't working (ADR 0018).
        let unavailable = app
            .try_state::<ShortcutState>()
            .is_some_and(|state| guard(&state.inner).error.is_some());
        if unavailable && crate::started_hidden(&app) {
            crate::notify::problem(&app, Toast::ShortcutUnavailable);
        }
    });
}

/// Ends a capture the dashboard left open when it closed. Runs on the main thread, so the work is
/// handed to a task.
pub fn dashboard_closed(app: &AppHandle) {
    let capturing = app
        .try_state::<ShortcutState>()
        .is_some_and(|state| guard(&state.inner).capturing);
    if capturing {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            change(&app, |app, state| {
                guard(&state.inner).capturing = false;
                reconcile(app, state);
            })
            .await;
        });
    }
}

/// The shortcut was pressed. Runs on the main thread, inside the plugin's window procedure, so
/// the lock goes through a task.
fn on_press(app: &AppHandle, shortcut: Shortcut) {
    let state = app.state::<ShortcutState>();
    let current = {
        let inner = guard(&state.inner);
        (!inner.capturing).then_some(inner.registered).flatten()
    };
    if current != Some(shortcut) {
        return;
    }
    let settings = app.state::<SettingsState>().current();
    let targets = state
        .e2e
        .and_then(|e2e| e2e.targets)
        .unwrap_or_else(|| settings.shortcut_targets());
    eprintln!("[keyclean] shortcut: lock requested");
    lock_remembered(app, targets, LockOrigin::Shortcut);
}

/// The dashboard's view of the shortcut.
pub fn snapshot(app: &AppHandle) -> ShortcutDto {
    let state = app.state::<ShortcutState>();
    let settings = app.state::<SettingsState>().current();
    let inner = guard(&state.inner);
    let configured = state
        .e2e
        .map(|e2e| e2e.shortcut)
        .or(settings.start_shortcut());
    let (status, error) = match configured {
        _ if inner.capturing => ("capturing", None),
        None => ("off", None),
        Some(_) if inner.error.is_some() => ("unavailable", inner.error.clone()),
        Some(shortcut) if inner.registered == Some(shortcut) => ("active", None),
        // Not tried yet (startup), or about to change.
        Some(_) => ("unavailable", None),
    };
    ShortcutDto {
        keys: configured.map_or_else(Vec::new, |s| s.parts().to_vec()),
        action: settings.shortcut_action().as_str(),
        state: status,
        error,
    }
}

/// What the tray shows next to the item the shortcut does: only while it is registered.
pub fn tray_hint(app: &AppHandle) -> Option<TrayHint> {
    let state = app.try_state::<ShortcutState>()?;
    let registered = guard(&state.inner).registered?;
    let action = match state.e2e.and_then(|e2e| e2e.targets) {
        Some(_) => ShortcutAction::Selected,
        None => app.state::<SettingsState>().current().shortcut_action(),
    };
    Some(TrayHint {
        shortcut: registered,
        action,
    })
}

/// Tells the dashboard and the tray that the shortcut changed.
fn announce(app: &AppHandle) {
    let _ = app.emit_to(MAIN_WINDOW, SHORTCUT_EVENT, snapshot(app));
    tray::show(app);
}

#[tauri::command]
pub fn get_shortcut(app: AppHandle) -> ShortcutDto {
    snapshot(&app)
}

/// Saves a new start shortcut (`None` turns it off) and registers it. The text is checked again
/// here, whatever the page sent. If the new shortcut can't be registered, the old one stays in
/// place and nothing is saved. Ends a capture.
#[tauri::command]
pub async fn set_shortcut(
    app: AppHandle,
    shortcut: Option<String>,
) -> Result<ShortcutDto, ErrorDto> {
    let wanted = shortcut
        .map(|text| Shortcut::parse(&text).map_err(ErrorDto::shortcut_refused))
        .transpose()?;
    change(&app, move |app, state| {
        let registered = {
            let mut inner = guard(&state.inner);
            inner.capturing = false;
            inner.registered
        };
        if let Some(new) = wanted.filter(|new| registered != Some(*new)) {
            if let Err(e) = register(app, new) {
                eprintln!("[keyclean] shortcut: the new one is unavailable");
                // Puts the old one back if the capture unregistered it.
                reconcile(app, state);
                return Err(e);
            }
            eprintln!("[keyclean] shortcut: registered");
            if let Some(old) = registered {
                unregister(app, old);
            }
            guard(&state.inner).registered = Some(new);
        }
        app.state::<SettingsState>().set_start_shortcut(wanted);
        // Unregisters the old one when turned off, and clears any error.
        reconcile(app, state);
        Ok(())
    })
    .await
    .unwrap_or_else(|| {
        Err(ErrorDto::shortcut_unavailable(
            "the change didn't finish".to_owned(),
        ))
    })?;
    Ok(snapshot(&app))
}

/// Saves what the shortcut locks: `keyboard` or `selected`.
#[tauri::command]
pub fn set_shortcut_action(app: AppHandle, action: String) -> Result<ShortcutDto, ErrorDto> {
    let action =
        ShortcutAction::parse(&action).ok_or_else(|| ErrorDto::shortcut_refused_action(&action))?;
    app.state::<SettingsState>().set_shortcut_action(action);
    announce(&app);
    Ok(snapshot(&app))
}

/// The user starts picking a new shortcut: the current one is unregistered meanwhile.
#[tauri::command]
pub async fn begin_shortcut_capture(app: AppHandle) -> ShortcutDto {
    change(&app, |app, state| {
        guard(&state.inner).capturing = true;
        reconcile(app, state);
    })
    .await;
    snapshot(&app)
}

/// The user stopped picking without saving: the current shortcut is registered again.
#[tauri::command]
pub async fn end_shortcut_capture(app: AppHandle) -> ShortcutDto {
    change(&app, |app, state| {
        guard(&state.inner).capturing = false;
        reconcile(app, state);
    })
    .await;
    snapshot(&app)
}

/// Tries again to register a shortcut that was unavailable. The dashboard calls it when it opens
/// and from the Try again button. Also ends a capture a reloaded page left open: the page that
/// started it is gone.
#[tauri::command]
pub async fn retry_shortcut(app: AppHandle) -> ShortcutDto {
    change(&app, |app, state| {
        guard(&state.inner).capturing = false;
        reconcile(app, state);
    })
    .await;
    snapshot(&app)
}

/// Debug builds only: `KEYCLEAN_E2E_SHORTCUT=<shortcut>` uses that shortcut instead of the
/// settings file's, without saving it, and `KEYCLEAN_E2E_SHORTCUT_TARGETS=keyboard|mouse|all`
/// what it locks, so the harness doesn't depend on Max's settings (ADR 0008).
#[cfg(debug_assertions)]
fn e2e_shortcut() -> Option<E2eShortcut> {
    let shortcut = Shortcut::parse(&std::env::var("KEYCLEAN_E2E_SHORTCUT").ok()?).ok()?;
    let targets = match std::env::var("KEYCLEAN_E2E_SHORTCUT_TARGETS")
        .ok()
        .as_deref()
    {
        Some("keyboard") => Some(LockTargets::KEYBOARD),
        Some("mouse") => Some(LockTargets {
            keyboard: false,
            mouse: true,
        }),
        Some("all") => Some(LockTargets::ALL),
        _ => None,
    };
    Some(E2eShortcut { shortcut, targets })
}

#[cfg(not(debug_assertions))]
fn e2e_shortcut() -> Option<E2eShortcut> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_allowed_key_has_a_plugin_code() {
        let mut keys: Vec<String> = ('A'..='Z').chain('0'..='9').map(String::from).collect();
        keys.extend((1..=24).filter(|n| *n != 12).map(|n| format!("F{n}")));
        for key in keys {
            for modifier in ["Ctrl", "Alt"] {
                let shortcut = Shortcut::parse(&format!("{modifier}+Shift+{key}")).unwrap();
                assert!(to_plugin(shortcut).is_some(), "{shortcut}");
            }
        }
    }

    #[test]
    fn plugin_errors_lose_the_key_name() {
        // As global-hotkey 0.8.0 prints them.
        let taken = "HotKey already registered: HotKey { mods: Modifiers(SHIFT | CONTROL), key: \
                     KeyK, id: 1234 }";
        let details = safe_details("register", taken);
        assert!(details.contains("already registered"), "{details}");
        let os = "Access is denied. (os error 5)";
        assert!(safe_details("register", os).ends_with("(os error 5)"));
        let unknown = "Unable to register hotkey: Unknown VKCode for KeyK";
        let unregister = "Failed to unregister hotkey: HotKey { key: KeyK }";
        for text in [taken, os, unknown, unregister] {
            for action in ["register", "unregister"] {
                let details = safe_details(action, text);
                assert!(
                    !details.contains("KeyK") && !details.contains("SHIFT"),
                    "{details}"
                );
            }
        }
    }

    #[test]
    fn tray_label_uses_the_strings_file() {
        assert_eq!(
            label(Shortcut::parse("Ctrl+Shift+K").unwrap()),
            "Ctrl+Shift+K"
        );
        assert_eq!(
            label(Shortcut::parse("Alt+Shift+F3").unwrap()),
            "Alt+Shift+F3"
        );
    }

    #[test]
    fn plugin_shortcut_has_the_same_modifiers_and_key() {
        let plugin = to_plugin(Shortcut::parse("Ctrl+Shift+K").unwrap()).unwrap();
        assert_eq!(plugin.mods, Modifiers::CONTROL | Modifiers::SHIFT);
        assert_eq!(plugin.key, Code::KeyK);
        let plugin = to_plugin(Shortcut::parse("Alt+Shift+F24").unwrap()).unwrap();
        assert_eq!(plugin.mods, Modifiers::ALT | Modifiers::SHIFT);
        assert_eq!(plugin.key, Code::F24);
    }
}
