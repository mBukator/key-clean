//! What the tray icon needs from Windows: whether the taskbar is light or dark, and a signal when
//! that (or the display scaling) may have changed (ADR 0016).
//!
//! - Windows documents only the *app* colour mode (`UISettings`), not the taskbar's
//!   [docs](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/ui/apply-windows-themes).
//!   The taskbar follows the "Windows mode" setting, stored as the DWORD `SystemUsesLightTheme`
//!   under `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize` (1 = light)
//!   [assumption: undocumented, widely relied on; checked in M8.md]. Missing means dark, the
//!   taskbar's look before Windows 10 1903 had a light mode.
//! - Windows broadcasts `WM_SETTINGCHANGE` to top-level windows when a system setting changes
//!   [docs: `WM_SETTINGCHANGE`]; a colour mode change is one of them [assumption, checked in
//!   M8.md]. [`watch_setting_changes`] subclasses one hidden top-level window (the tray's) and calls
//!   back on each one, and on `WM_DISPLAYCHANGE` and `WM_DPICHANGED` (a display scale change
//!   [assumption, checked in M8.md]). The callback re-reads what it needs, so it doesn't depend
//!   on the undocumented `"ImmersiveColorSet"` string in `lParam`. Nothing polls.

use std::ffi::c_void;
use std::sync::OnceLock;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    WM_DISPLAYCHANGE, WM_DPICHANGED, WM_NCDESTROY, WM_SETTINGCHANGE,
};
use windows::core::w;

/// Subclass id; any constant unique to this module.
const SUBCLASS_ID: usize = 0x4B43_5442;

static ON_CHANGE: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

/// Whether the taskbar uses the light theme. `false` (dark) if Windows doesn't say.
pub fn taskbar_uses_light_theme() -> bool {
    let mut value = 0u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: `value` and `size` are valid for writes, and `size` holds the buffer's byte length
    // as RegGetValueW requires; RRF_RT_REG_DWORD makes it fail rather than write another type.
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut value).cast()),
            Some(&raw mut size),
        )
    };
    result.is_ok() && value == 1
}

/// Calls `on_change` on the window's thread after each `WM_SETTINGCHANGE`, `WM_DISPLAYCHANGE`
/// or `WM_DPICHANGED` that reaches `hwnd`, a hidden top-level window of the calling thread. Returns whether the
/// window was subclassed. Only the first callback is kept.
pub fn watch_setting_changes(
    hwnd: *mut c_void,
    on_change: impl Fn() + Send + Sync + 'static,
) -> bool {
    let _ = ON_CHANGE.set(Box::new(on_change));
    // SAFETY: the caller passes a window of the calling thread, as SetWindowSubclass requires;
    // `settings_subclass` lives for the whole program. A stale handle makes the call fail.
    unsafe { SetWindowSubclass(HWND(hwnd), Some(settings_subclass), SUBCLASS_ID, 0) }.as_bool()
}

/// The subclass procedure.
///
/// # Safety
/// Called by Windows only, with valid subclass-procedure arguments.
unsafe extern "system" fn settings_subclass(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    _data: usize,
) -> LRESULT {
    if msg == WM_NCDESTROY {
        // SAFETY: removing our own subclass from a window of this thread, as the docs ask
        // before the window is destroyed.
        let _ = unsafe { RemoveWindowSubclass(hwnd, Some(settings_subclass), id) };
    }
    // SAFETY: default processing with the original arguments, before the callback, so the window
    // has handled the message when the callback changes the tray icon.
    let result = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
    if matches!(msg, WM_SETTINGCHANGE | WM_DISPLAYCHANGE | WM_DPICHANGED)
        && let Some(on_change) = ON_CHANGE.get()
    {
        on_change();
    }
    result
}
