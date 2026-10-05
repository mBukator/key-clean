//! Raw Input registration, for the duration of a lock only (invariant 4): keyboards for every
//! lock, mice too when the mouse is locked.
//!
//! The registration serves two purposes:
//! - the hook-liveness check: the engine compares the timing of `WM_INPUT` messages with each
//!   hook's last call. To tell a keyboard message from a mouse message it reads only the
//!   `RAWINPUTHEADER` (`RID_HEADER`): the device type and handle. It never asks for `RID_INPUT`,
//!   so no key, button, wheel or movement data is read (ADR 0010, amended in ADR 0013);
//! - device arrival/removal notifications (`WM_INPUT_DEVICE_CHANGE`, via `RIDEV_DEVNOTIFY`).
//!
//! [docs] `RIDEV_INPUTSINK` delivers input while the caller isn't in the foreground and requires
//! `hwndTarget`; `RIDEV_REMOVE` requires `hwndTarget` to be NULL, or registration fails
//! (<https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-rawinputdevice>).
//! [docs] Only one window per raw input device class per process receives raw input: the one from
//! the last `RegisterRawInputDevices` call
//! (<https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registerrawinputdevices>).
//! The engine process registers nothing else, so these are the only registrations.
//! [docs] `GetRawInputData` with `RID_HEADER` copies only the header of the `RAWINPUT` structure
//! (<https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getrawinputdata>).

use std::mem::size_of;

use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::UI::Input::{
    GetRawInputData, HRAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RID_HEADER, RIDEV_DEVNOTIFY,
    RIDEV_INPUTSINK, RIDEV_REMOVE, RIM_TYPEKEYBOARD, RIM_TYPEMOUSE, RegisterRawInputDevices,
};

/// HID usage page "Generic Desktop Controls".
const USAGE_PAGE_GENERIC: u16 = 0x01;
/// HID usage "Mouse" on the generic desktop page.
const USAGE_MOUSE: u16 = 0x02;
/// HID usage "Keyboard" on the generic desktop page.
const USAGE_KEYBOARD: u16 = 0x06;

/// Which kind of device a raw input message came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RawSource {
    Keyboard,
    Mouse,
}

/// Registers `hwnd` for keyboard Raw Input in the background, plus device change notifications,
/// and for mouse Raw Input too when `mouse` is true. Engine thread only, during a lock.
pub(crate) fn register(hwnd: HWND, mouse: bool) -> windows::core::Result<()> {
    let flags = RIDEV_INPUTSINK | RIDEV_DEVNOTIFY;
    apply(&devices(mouse, |usage| RAWINPUTDEVICE {
        usUsagePage: USAGE_PAGE_GENERIC,
        usUsage: usage,
        dwFlags: flags,
        hwndTarget: hwnd,
    }))
}

/// Removes the registrations [`register`] made (pass the same `mouse`), so the engine receives no
/// raw input while idle.
pub(crate) fn remove(mouse: bool) -> windows::core::Result<()> {
    apply(&devices(mouse, |usage| RAWINPUTDEVICE {
        usUsagePage: USAGE_PAGE_GENERIC,
        usUsage: usage,
        dwFlags: RIDEV_REMOVE,
        // RIDEV_REMOVE requires a NULL target window.
        hwndTarget: HWND::default(),
    }))
}

/// The keyboard entry, and the mouse entry when `mouse` is true.
fn devices(mouse: bool, make: impl Fn(u16) -> RAWINPUTDEVICE) -> Vec<RAWINPUTDEVICE> {
    let mut list = vec![make(USAGE_KEYBOARD)];
    if mouse {
        list.push(make(USAGE_MOUSE));
    }
    list
}

fn apply(devices: &[RAWINPUTDEVICE]) -> windows::core::Result<()> {
    // SAFETY: every entry is a fully initialized RAWINPUTDEVICE that lives for the call, and the
    // size passed is the exact element size. Targets are the engine's own window for a
    // registration (as RIDEV_INPUTSINK requires) and NULL for a removal (as RIDEV_REMOVE requires).
    unsafe { RegisterRawInputDevices(devices, size_of::<RAWINPUTDEVICE>() as u32) }
}

/// The source of the `WM_INPUT` whose `lParam` is `lparam`, read from its header only. `None` for
/// a HID source or if the header can't be read.
pub(crate) fn source(lparam: LPARAM) -> Option<RawSource> {
    let mut header = RAWINPUTHEADER::default();
    let mut size = size_of::<RAWINPUTHEADER>() as u32;
    // SAFETY: `lparam` is the HRAWINPUT of the WM_INPUT being handled. `header` is a writable
    // RAWINPUTHEADER and `size` is its exact size, so RID_HEADER copies at most that many bytes.
    let copied = unsafe {
        GetRawInputData(
            HRAWINPUT(lparam.0 as *mut _),
            RID_HEADER,
            Some((&raw mut header).cast()),
            &mut size,
            size_of::<RAWINPUTHEADER>() as u32,
        )
    };
    if copied == u32::MAX || copied == 0 {
        return None;
    }
    match header.dwType {
        t if t == RIM_TYPEKEYBOARD.0 => Some(RawSource::Keyboard),
        t if t == RIM_TYPEMOUSE.0 => Some(RawSource::Mouse),
        _ => None,
    }
}
