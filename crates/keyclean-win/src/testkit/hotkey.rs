//! Holds a global hotkey the way another app would, so the harness can check what KeyClean does
//! when its start shortcut is taken (S48, ADR 0017).

use std::marker::PhantomData;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey, UnregisterHotKey,
};

/// Any id in the application range (0x0000-0xBFFF, `RegisterHotKey` docs).
const HOTKEY_ID: i32 = 0x4B43;

/// Ctrl+Shift+`vk`, registered for the calling thread until dropped. The thread's `WM_HOTKEY`
/// messages are never read; a press of the chord only goes nowhere.
pub struct HeldHotkey {
    /// Registered for the calling thread, so it must be unregistered there: not `Send`.
    _thread: PhantomData<*const ()>,
}

impl HeldHotkey {
    /// Registers Ctrl+Shift+`vk`. Fails if another program (KeyClean, for example) holds it.
    pub fn ctrl_shift(vk: u8) -> Result<Self, String> {
        // SAFETY: no window (the hotkey belongs to this thread); plain values otherwise.
        unsafe {
            RegisterHotKey(
                None,
                HOTKEY_ID,
                MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT,
                u32::from(vk),
            )
        }
        .map(|()| HeldHotkey {
            _thread: PhantomData,
        })
        .map_err(|e| format!("RegisterHotKey failed: {e}"))
    }
}

impl Drop for HeldHotkey {
    fn drop(&mut self) {
        // SAFETY: the hotkey was registered by this thread (the type isn't `Send`) with this id.
        let _ = unsafe { UnregisterHotKey(None, HOTKEY_ID) };
    }
}
