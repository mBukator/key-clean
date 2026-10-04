//! Keyboard Raw Input registration, for the duration of a lock only (invariant 4).
//!
//! The registration serves two purposes:
//! - the hook-liveness check: the engine counts `WM_INPUT` messages and compares their timing with
//!   the hook's last call. It never calls `GetRawInputData`, so no key data is read;
//! - device arrival/removal notifications (`WM_INPUT_DEVICE_CHANGE`, via `RIDEV_DEVNOTIFY`).
//!
//! [docs] `RIDEV_INPUTSINK` delivers input while the caller isn't in the foreground and requires
//! `hwndTarget`; `RIDEV_REMOVE` requires `hwndTarget` to be NULL, or registration fails
//! (<https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-rawinputdevice>).
//! [docs] Only one window per raw input device class per process receives raw input: the one from
//! the last `RegisterRawInputDevices` call
//! (<https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registerrawinputdevices>).
//! The engine process registers nothing else, so this is the only registration.

use std::mem::size_of;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::{
    RAWINPUTDEVICE, RIDEV_DEVNOTIFY, RIDEV_INPUTSINK, RIDEV_REMOVE, RegisterRawInputDevices,
};

/// HID usage page "Generic Desktop Controls".
const USAGE_PAGE_GENERIC: u16 = 0x01;
/// HID usage "Keyboard" on the generic desktop page.
const USAGE_KEYBOARD: u16 = 0x06;

/// Registers `hwnd` for keyboard Raw Input in the background, plus device change notifications.
/// Engine thread only, during a lock.
pub(crate) fn register(hwnd: HWND) -> windows::core::Result<()> {
    let device = RAWINPUTDEVICE {
        usUsagePage: USAGE_PAGE_GENERIC,
        usUsage: USAGE_KEYBOARD,
        dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
        hwndTarget: hwnd,
    };
    // SAFETY: `device` is a fully initialized RAWINPUTDEVICE that lives for the call, and the size
    // passed is its exact size. `hwnd` is the engine's own window, as RIDEV_INPUTSINK requires.
    unsafe { RegisterRawInputDevices(&[device], size_of::<RAWINPUTDEVICE>() as u32) }
}

/// Removes the keyboard registration, so the engine receives no raw input while idle.
pub(crate) fn remove() -> windows::core::Result<()> {
    let device = RAWINPUTDEVICE {
        usUsagePage: USAGE_PAGE_GENERIC,
        usUsage: USAGE_KEYBOARD,
        dwFlags: RIDEV_REMOVE,
        // RIDEV_REMOVE requires a NULL target window.
        hwndTarget: HWND::default(),
    };
    // SAFETY: as in `register`; the target is NULL as RIDEV_REMOVE requires.
    unsafe { RegisterRawInputDevices(&[device], size_of::<RAWINPUTDEVICE>() as u32) }
}
