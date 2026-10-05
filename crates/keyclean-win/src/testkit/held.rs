//! Which keys and mouse buttons Windows believes are held — the stuck-key check.
//!
//! `GetAsyncKeyState` is accurate outside a hook callback, and this is only ever called from the
//! harness's main thread.

use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

/// Whether Windows currently considers `vk` held down.
pub fn is_held(vk: u8) -> bool {
    // SAFETY: GetAsyncKeyState has no preconditions; the high bit means "currently down".
    unsafe { GetAsyncKeyState(i32::from(vk)) < 0 }
}

/// Mouse buttons by virtual-key code: VK_LBUTTON, VK_RBUTTON, VK_MBUTTON, VK_XBUTTON1/2.
const MOUSE_BUTTONS: [(u8, &str); 5] = [
    (0x01, "left button"),
    (0x02, "right button"),
    (0x04, "middle button"),
    (0x05, "X1 button"),
    (0x06, "X2 button"),
];

/// Names of the mouse buttons Windows currently considers held. `GetAsyncKeyState` reports
/// physical buttons (swapped buttons aren't mapped), which doesn't matter for a stuck check that
/// looks at all of them.
pub fn held_mouse_buttons() -> Vec<&'static str> {
    MOUSE_BUTTONS
        .iter()
        .filter(|(vk, _)| is_held(*vk))
        .map(|(_, name)| *name)
        .collect()
}
