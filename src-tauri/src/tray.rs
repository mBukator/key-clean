//! The tray icon and its menu (§27, ADR 0016): the way back to KeyClean while no window is open.
//!
//! The icon is a keycap with a K: an outline in the taskbar's colour at idle, filled amber while
//! locked (`assets/tray/`, rasterized by `scripts/tray-icons.mjs`). It follows the taskbar's light
//! or dark theme and the display scaling, re-read only when Windows reports a setting change.
//!
//! The menu follows the engine status. It is rebuilt only when the status moves between idle and
//! locked (or the engine becomes available or not); during a lock the status line and the tooltip
//! show the countdown, updated on each countdown status. Nothing runs at idle.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use keyclean_win::keyclean_core::countdown::format_clock;
use keyclean_win::{EngineError, LockRequest, LockTargets};
use tauri::image::Image;
use tauri::include_image;
use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::settings::SettingsState;
use crate::{EngineSlot, StatusStore, dashboard, guard, i18n, overlay, update_status};

const TRAY_ID: &str = "keyclean";

/// Menu item ids.
mod item {
    pub const LOCK_KEYBOARD: &str = "lock-keyboard";
    pub const LOCK_SELECTED: &str = "lock-selected";
    pub const OPEN: &str = "open";
    pub const SETTINGS: &str = "settings";
    pub const UNLOCK: &str = "unlock";
    pub const SHOW_OVERLAY: &str = "show-overlay";
    pub const QUIT: &str = "quit";
}

/// What the tray shows, derived from the engine status (`StatusDto::tray_mode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayMode {
    Ready,
    /// Idle, but the last lock or lock request ended with an error.
    Problem,
    /// The engine isn't running; nothing can be locked.
    Unavailable,
    Starting,
    /// Locked, with this many whole seconds left.
    Cleaning(u64),
    Unlocking,
}

/// Which menu a mode uses; the menu is rebuilt only when this changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MenuKind {
    Idle { can_lock: bool },
    Locked,
}

impl TrayMode {
    fn menu_kind(self) -> MenuKind {
        match self {
            TrayMode::Ready | TrayMode::Problem => MenuKind::Idle { can_lock: true },
            TrayMode::Unavailable => MenuKind::Idle { can_lock: false },
            TrayMode::Starting | TrayMode::Cleaning(_) | TrayMode::Unlocking => MenuKind::Locked,
        }
    }

    fn status_text(self) -> String {
        match self {
            TrayMode::Ready => i18n::t("tray.status.ready"),
            TrayMode::Problem => i18n::t("tray.status.problem"),
            TrayMode::Unavailable => i18n::t("tray.status.unavailable"),
            TrayMode::Starting => i18n::t("tray.status.starting"),
            TrayMode::Cleaning(secs) => {
                i18n::t_with("tray.status.cleaning", &[("time", &format_clock(secs))])
            }
            TrayMode::Unlocking => i18n::t("tray.status.unlocking"),
        }
    }

    /// At most 127 characters: Windows truncates longer tooltips (`NOTIFYICONDATAW::szTip`).
    fn tooltip(self) -> String {
        match self {
            TrayMode::Ready => i18n::t("tray.tooltip.ready"),
            TrayMode::Problem => i18n::t("tray.tooltip.problem"),
            TrayMode::Unavailable => i18n::t("tray.tooltip.unavailable"),
            TrayMode::Starting => i18n::t("tray.tooltip.starting"),
            TrayMode::Cleaning(secs) => {
                i18n::t_with("tray.tooltip.cleaning", &[("time", &format_clock(secs))])
            }
            TrayMode::Unlocking => i18n::t("tray.tooltip.unlocking"),
        }
    }
}

/// Which drawing the icon uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Variant {
    /// Dark outline, for a light taskbar.
    IdleLight,
    /// White outline, for a dark taskbar.
    IdleDark,
    /// Filled amber, readable on either taskbar.
    Locked,
}

/// The tray icon sizes there are drawings for: a 16 px small icon at 100, 125, 150 and 200 %
/// display scaling.
const ICON_SIZES: [u32; 4] = [16, 20, 24, 32];

/// The smallest drawing that covers a 16 px icon at `scale`; the largest past 200 %.
fn icon_size(scale: f64) -> u32 {
    let wanted = 16.0 * scale;
    ICON_SIZES
        .into_iter()
        .find(|&size| f64::from(size) + 0.5 >= wanted)
        .unwrap_or(32)
}

fn icon_image(variant: Variant, size: u32) -> Image<'static> {
    match (variant, size) {
        (Variant::IdleLight, 16) => include_image!("icons/tray/idle-light-16.png"),
        (Variant::IdleLight, 20) => include_image!("icons/tray/idle-light-20.png"),
        (Variant::IdleLight, 24) => include_image!("icons/tray/idle-light-24.png"),
        (Variant::IdleLight, _) => include_image!("icons/tray/idle-light-32.png"),
        (Variant::IdleDark, 16) => include_image!("icons/tray/idle-dark-16.png"),
        (Variant::IdleDark, 20) => include_image!("icons/tray/idle-dark-20.png"),
        (Variant::IdleDark, 24) => include_image!("icons/tray/idle-dark-24.png"),
        (Variant::IdleDark, _) => include_image!("icons/tray/idle-dark-32.png"),
        (Variant::Locked, 16) => include_image!("icons/tray/locked-16.png"),
        (Variant::Locked, 20) => include_image!("icons/tray/locked-20.png"),
        (Variant::Locked, 24) => include_image!("icons/tray/locked-24.png"),
        (Variant::Locked, _) => include_image!("icons/tray/locked-32.png"),
    }
}

/// What the icon looks like for a status, a taskbar theme and a display scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Look {
    variant: Variant,
    size: u32,
}

impl Look {
    fn of(mode: TrayMode, light_taskbar: bool, scale: f64) -> Self {
        let variant = match (mode.menu_kind(), light_taskbar) {
            (MenuKind::Locked, _) => Variant::Locked,
            (MenuKind::Idle { .. }, true) => Variant::IdleLight,
            (MenuKind::Idle { .. }, false) => Variant::IdleDark,
        };
        Look {
            variant,
            size: icon_size(scale),
        }
    }

    fn image(self) -> Image<'static> {
        icon_image(self.variant, self.size)
    }
}

/// What the tray shows now. Only touched by [`sync`], on the main thread.
struct Shown {
    /// The mode whose status line and tooltip are showing; `None` while they couldn't be set.
    mode: Option<TrayMode>,
    /// The menu showing.
    menu: MenuKind,
    /// The menu's status line, updated in place while the menu stays the same.
    status: MenuItem<Wry>,
    /// The taskbar theme and display scale, re-read when Windows reports a setting change.
    light_taskbar: bool,
    scale: f64,
    look: Look,
}

fn primary_scale(app: &AppHandle) -> f64 {
    app.primary_monitor()
        .ok()
        .flatten()
        .map_or(1.0, |m| m.scale_factor())
}

/// What the tray shows. The icon itself stays with Tauri (`tray_by_id`): holding a clone here
/// would keep it alive past exit, and Windows would show a dead icon until it is hovered.
pub struct TrayState {
    shown: Mutex<Shown>,
    /// Set while [`sync`] runs. Changing the icon, menu or tooltip calls `Shell_NotifyIconW`,
    /// which waits on Explorer; while it waits, Windows may deliver a sent message (a setting
    /// change broadcast) to the tray window on this thread, which calls [`sync`] again. That
    /// nested call only sets `pending`, so `shown` is never locked twice on one thread.
    syncing: AtomicBool,
    /// Another pass is needed.
    pending: AtomicBool,
    /// The next pass re-reads the taskbar theme and the display scale.
    reread: AtomicBool,
}

fn tray_icon(app: &AppHandle) -> Option<TrayIcon> {
    app.tray_by_id(TRAY_ID)
}

/// Builds the tray icon. Runs in `setup`, on the main thread, before the close guard, so the
/// guard also covers the tray's hidden window (ADR 0016).
pub fn create(app: &AppHandle, mode: TrayMode) -> tauri::Result<()> {
    let (menu, status) = build_menu(app, mode)?;
    let light_taskbar = keyclean_win::taskbar_uses_light_theme();
    let scale = primary_scale(app);
    let look = Look::of(mode, light_taskbar, scale);
    let icon = TrayIconBuilder::with_id(TRAY_ID)
        .icon(look.image())
        .tooltip(mode.tooltip())
        .menu(&menu)
        // Left click opens the dashboard; right click shows the menu.
        .show_menu_on_left_click(false)
        .on_menu_event(on_menu)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                dashboard::open(tray.app_handle());
            }
        })
        .build(app)?;
    // The tray's hidden window is a top-level window of this (the main) thread, so it gets the
    // setting-change broadcasts. Not fatal if it can't: the icon keeps its first look.
    let handle = app.clone();
    let watching = icon.with_inner_tray_icon(move |inner| {
        keyclean_win::watch_setting_changes(inner.window_handle(), move || refresh(&handle))
    });
    if !matches!(watching, Ok(true)) {
        eprintln!("[keyclean] tray: not watching the taskbar theme");
    }
    app.manage(TrayState {
        shown: Mutex::new(Shown {
            mode: Some(mode),
            menu: mode.menu_kind(),
            status,
            light_taskbar,
            scale,
            look,
        }),
        syncing: AtomicBool::new(false),
        pending: AtomicBool::new(false),
        reread: AtomicBool::new(false),
    });
    Ok(())
}

/// A system setting changed: the next pass re-reads the taskbar theme and the display scale.
/// Runs on the main thread, in the tray window's procedure.
fn refresh(app: &AppHandle) {
    if let Some(tray) = app.try_state::<TrayState>() {
        tray.reread.store(true, Ordering::SeqCst);
    }
    sync(app);
}

/// Brings the tray up to date with the status. Never waits for the main thread, so the event
/// relay can't stall behind it; the update runs there and reads the status then, so updates
/// scheduled from two threads can't leave an older one showing.
pub fn show(app: &AppHandle) {
    let handle = app.clone();
    if let Err(e) = app.run_on_main_thread(move || sync(&handle)) {
        eprintln!("[keyclean] tray: couldn't schedule an update: {e}");
    }
}

/// Runs passes until nothing more is pending. Main thread only; a nested call (see
/// [`TrayState::syncing`]) leaves its work to the pass already running.
fn sync(app: &AppHandle) {
    let Some(tray) = app.try_state::<TrayState>() else {
        return;
    };
    tray.pending.store(true, Ordering::SeqCst);
    if tray.syncing.swap(true, Ordering::SeqCst) {
        return;
    }
    while tray.pending.swap(false, Ordering::SeqCst) {
        sync_once(app, &tray);
    }
    tray.syncing.store(false, Ordering::SeqCst);
}

/// One pass: the menu, the status line and tooltip, then the icon. Whatever fails is tried
/// again on the next pass (the next status or setting change).
fn sync_once(app: &AppHandle, tray: &TrayState) {
    let (Some(store), Some(icon)) = (app.try_state::<StatusStore>(), tray_icon(app)) else {
        return;
    };
    // Released before the tray's own lock; nothing takes them the other way.
    let mode = guard(&store.0).tray_mode();
    let mut shown = guard(&tray.shown);
    if tray.reread.swap(false, Ordering::SeqCst) {
        shown.light_taskbar = keyclean_win::taskbar_uses_light_theme();
        shown.scale = primary_scale(app);
    }
    let kind = mode.menu_kind();
    if kind != shown.menu {
        let built = build_menu(app, mode).and_then(|(menu, status)| {
            icon.set_menu(Some(menu))?;
            Ok(status)
        });
        match built {
            // Recorded only once the new menu is up, so the countdown never goes to a menu that
            // isn't shown, and a failed swap is tried again.
            Ok(status) => {
                shown.menu = kind;
                shown.status = status;
                shown.mode = None;
            }
            Err(e) => eprintln!("[keyclean] tray: couldn't change the menu: {e}"),
        }
    }
    if shown.menu == kind && shown.mode != Some(mode) {
        let updated = shown
            .status
            .set_text(mode.status_text())
            .and_then(|()| icon.set_tooltip(Some(mode.tooltip())));
        match updated {
            Ok(()) => shown.mode = Some(mode),
            Err(e) => eprintln!("[keyclean] tray: couldn't update the status: {e}"),
        }
    }
    let look = Look::of(mode, shown.light_taskbar, shown.scale);
    if look != shown.look {
        match icon.set_icon(Some(look.image())) {
            Ok(()) => shown.look = look,
            Err(e) => eprintln!("[keyclean] tray: couldn't change the icon: {e}"),
        }
    }
}

fn build_menu(app: &AppHandle, mode: TrayMode) -> tauri::Result<(Menu<Wry>, MenuItem<Wry>)> {
    let entry = |id: &str, key: &str, enabled: bool| {
        MenuItem::with_id(app, id, i18n::t(key), enabled, None::<&str>)
    };
    let title = MenuItem::new(app, i18n::t("app.title"), false, None::<&str>)?;
    let status = MenuItem::new(app, mode.status_text(), false, None::<&str>)?;
    let separator = || PredefinedMenuItem::separator(app);
    let menu = match mode.menu_kind() {
        MenuKind::Idle { can_lock } => Menu::with_items(
            app,
            &[
                &title,
                &status,
                &separator()?,
                &entry(item::LOCK_KEYBOARD, "tray.lockKeyboard", can_lock)?,
                &entry(item::LOCK_SELECTED, "tray.lockSelected", can_lock)?,
                &separator()?,
                &entry(item::OPEN, "tray.open", true)?,
                &entry(item::SETTINGS, "tray.settings", true)?,
                &separator()?,
                &entry(item::QUIT, "tray.quit", true)?,
            ],
        )?,
        MenuKind::Locked => Menu::with_items(
            app,
            &[
                &title,
                &status,
                &separator()?,
                &entry(item::UNLOCK, "tray.unlock", true)?,
                &entry(item::SHOW_OVERLAY, "tray.showOverlay", true)?,
                &separator()?,
                &entry(item::QUIT, "tray.quit", true)?,
            ],
        )?,
    };
    Ok((menu, status))
}

/// Runs on the main thread. Anything that builds a window goes through an async task.
fn on_menu(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        item::LOCK_KEYBOARD => lock(app, Some(LockTargets::KEYBOARD)),
        item::LOCK_SELECTED => lock(app, None),
        item::OPEN => dashboard::open(app),
        item::SETTINGS => dashboard::open_on(app, dashboard::Tab::Settings),
        item::UNLOCK => {
            if let Some(engine) = guard(&app.state::<EngineSlot>().0).as_ref()
                && let Err(e) = engine.unlock()
            {
                eprintln!("[keyclean] tray: unlock failed: {}", e.details());
            }
        }
        item::SHOW_OVERLAY => overlay::bring_to_front(app),
        // Exits with a code, so the tray rule in `run` doesn't keep KeyClean running. The exit
        // ends any lock (`UserRequest`).
        item::QUIT => crate::exit_app(app),
        _ => {}
    }
}

/// Locks from the tray with the remembered duration, and `targets` or the remembered ones.
/// Goes through the overlay like every lock (invariant 11). Doesn't change the remembered
/// choices: the menu item names what it locks (ADR 0016).
fn lock(app: &AppHandle, targets: Option<LockTargets>) {
    let settings = app.state::<SettingsState>().current();
    let request = LockRequest {
        duration: settings.last_duration(),
        max_lock: settings.max_lock(),
        targets: targets.unwrap_or_else(|| settings.last_targets()),
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = overlay::lock(app.clone(), request).await {
            eprintln!("[keyclean] tray: lock failed: {}", e.details);
            // A lock already starting or running isn't a problem to report.
            if !e.is(&EngineError::AlreadyActive) {
                update_status(&app, |status| status.set_request_error(e));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_size_covers_the_scale() {
        assert_eq!(icon_size(1.0), 16);
        assert_eq!(icon_size(1.25), 20);
        assert_eq!(icon_size(1.5), 24);
        assert_eq!(icon_size(1.75), 32);
        assert_eq!(icon_size(2.0), 32);
        assert_eq!(icon_size(3.0), 32);
    }

    #[test]
    fn locked_looks_the_same_on_both_taskbars() {
        for light in [true, false] {
            assert_eq!(
                Look::of(TrayMode::Cleaning(5), light, 1.0).variant,
                Variant::Locked
            );
        }
        assert_eq!(
            Look::of(TrayMode::Ready, true, 1.0).variant,
            Variant::IdleLight
        );
        assert_eq!(
            Look::of(TrayMode::Unavailable, false, 1.0).variant,
            Variant::IdleDark
        );
    }

    #[test]
    fn every_drawing_is_its_size() {
        for variant in [Variant::IdleLight, Variant::IdleDark, Variant::Locked] {
            for size in ICON_SIZES {
                let image = icon_image(variant, size);
                assert_eq!((image.width(), image.height()), (size, size), "{variant:?}");
            }
        }
    }

    #[test]
    fn tooltips_fit_the_tray() {
        for mode in [
            TrayMode::Ready,
            TrayMode::Problem,
            TrayMode::Unavailable,
            TrayMode::Starting,
            TrayMode::Cleaning(3_600),
            TrayMode::Unlocking,
        ] {
            assert!(mode.tooltip().chars().count() < 128, "{mode:?}");
        }
    }
}
