//! Which keys Windows believes are held — the stuck-key check.
//!
//! `GetAsyncKeyState` is accurate outside a hook callback, and this is only ever called from the
//! harness's main thread.

use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

/// Whether Windows currently considers `vk` held down.
pub fn is_held(vk: u8) -> bool {
    // SAFETY: GetAsyncKeyState has no preconditions; the high bit means "currently down".
    unsafe { GetAsyncKeyState(i32::from(vk)) < 0 }
}
