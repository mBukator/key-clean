//! Whether a window is on the current virtual desktop, for the overlay rule (ADR 0014).
//!
//! A four-finger touchpad swipe can switch virtual desktops during a lock, which no user-mode hook
//! can stop. The overlay window stays on the desktop it was shown on, so the user would look at a
//! locked machine without the overlay. The app asks this once per countdown tick during a lock and
//! ends the lock when the answer is no.

use std::ffi::c_void;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CLSCTX_LOCAL_SERVER, CoCreateInstance};
use windows::Win32::UI::Shell::{IVirtualDesktopManager, VirtualDesktopManager};

/// `Some(true)` if the top-level window `hwnd` (a raw window handle) is on the active virtual
/// desktop, `Some(false)` if it is on another one, `None` if Windows couldn't say (no shell, COM
/// not initialized on this thread, or the window is gone).
///
/// Must run on a thread where COM is initialized; the app calls it on its main (window) thread,
/// which the windowing library initializes for COM. Uses
/// [`IVirtualDesktopManager::IsWindowOnCurrentVirtualDesktop`](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ivirtualdesktopmanager-iswindowoncurrentvirtualdesktop).
pub fn is_on_current_desktop(hwnd: *mut c_void) -> Option<bool> {
    let hwnd = HWND(hwnd);
    if hwnd.is_invalid() {
        return None;
    }
    // SAFETY: creating the shell's documented COM class has no memory-safety preconditions; a
    // failure (including COM not initialized on this thread) is an Err.
    let manager: IVirtualDesktopManager = unsafe {
        CoCreateInstance(
            &VirtualDesktopManager,
            None,
            CLSCTX_INPROC_SERVER | CLSCTX_LOCAL_SERVER,
        )
    }
    .ok()?;
    // SAFETY: `manager` is a live interface pointer; an invalid or closed window is an Err, not
    // undefined behaviour.
    let on_current = unsafe { manager.IsWindowOnCurrentVirtualDesktop(hwnd) }.ok()?;
    Some(on_current.as_bool())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_null_window_is_unknown() {
        assert_eq!(is_on_current_desktop(std::ptr::null_mut()), None);
    }
}
