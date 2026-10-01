//! Synthesized key events, tagged so the observer can tell them from a person's typing.
//!
//! Injected events pass through every low-level keyboard hook with `LLKHF_INJECTED` set, exactly
//! like real ones otherwise, so they exercise KeyClean's real pass/block path (which blocks
//! injected input during a lock, ADR 0007).

use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY,
};

/// `dwExtraInfo` value on every event this module sends ("KCLN").
pub const TAG: usize = 0x4B43_4C4E;

/// Virtual-key codes the harness uses.
pub mod vk {
    /// Tab.
    pub const TAB: u8 = 0x09;
    /// Escape.
    pub const ESCAPE: u8 = 0x1B;
    /// G.
    pub const G: u8 = 0x47;
    /// K.
    pub const K: u8 = 0x4B;
    /// X.
    pub const X: u8 = 0x58;
    /// Left Windows key.
    pub const LWIN: u8 = 0x5B;
    /// F13 — the first harmless probe key (types nothing, rarely bound).
    pub const F13: u8 = 0x7C;
    /// F24 — the last probe key.
    pub const F24: u8 = 0x87;
    /// Left Shift.
    pub const LSHIFT: u8 = 0xA0;
    /// Left Ctrl.
    pub const LCONTROL: u8 = 0xA2;
    /// Right Ctrl.
    pub const RCONTROL: u8 = 0xA3;
    /// Left Alt.
    pub const LMENU: u8 = 0xA4;
    /// Right Alt / AltGr.
    pub const RMENU: u8 = 0xA5;
    /// Volume up (a media key).
    pub const VOLUME_UP: u8 = 0xAF;
}

/// One key transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stroke {
    /// Virtual-key code.
    pub vk: u8,
    /// Key-down when true, key-up when false.
    pub down: bool,
}

impl Stroke {
    /// A key-down.
    pub const fn down(vk: u8) -> Self {
        Stroke { vk, down: true }
    }

    /// A key-up.
    pub const fn up(vk: u8) -> Self {
        Stroke { vk, down: false }
    }
}

/// Keys that live in the extended (E0) range.
fn is_extended(vk: u8) -> bool {
    matches!(
        vk,
        vk::LWIN | 0x5C | vk::RCONTROL | vk::RMENU | 0xAD..=0xB7 | 0x21..=0x28 | 0x2D | 0x2E
    )
}

/// Error from [`send`]: Windows inserted fewer events than requested (e.g. blocked by UIPI because
/// an elevated window has focus, or the secure desktop is active).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InjectError {
    /// Events requested.
    pub requested: usize,
    /// Events inserted.
    pub inserted: usize,
}

impl std::fmt::Display for InjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SendInput inserted {} of {} events",
            self.inserted, self.requested
        )
    }
}

impl std::error::Error for InjectError {}

/// Sends `strokes` in order as one batch.
pub fn send(strokes: &[Stroke]) -> Result<(), InjectError> {
    let inputs: Vec<INPUT> = strokes
        .iter()
        .map(|s| {
            let mut flags = KEYBD_EVENT_FLAGS(0);
            if !s.down {
                flags |= KEYEVENTF_KEYUP;
            }
            if is_extended(s.vk) {
                flags |= KEYEVENTF_EXTENDEDKEY;
            }
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(u16::from(s.vk)),
                        wScan: 0,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: TAG,
                    },
                },
            }
        })
        .collect();
    // SAFETY: `inputs` is a valid slice of fully initialized INPUT structs and `cbsize` is the
    // size of one INPUT, as SendInput requires.
    let inserted = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) } as usize;
    if inserted == inputs.len() {
        Ok(())
    } else {
        Err(InjectError {
            requested: inputs.len(),
            inserted,
        })
    }
}

/// Presses and releases `vk`.
pub fn tap(vk: u8) -> Result<(), InjectError> {
    send(&[Stroke::down(vk), Stroke::up(vk)])
}

/// Presses `keys` in order, then releases them in reverse order (a shortcut).
pub fn chord(keys: &[u8]) -> Result<(), InjectError> {
    let strokes: Vec<Stroke> = keys
        .iter()
        .map(|&k| Stroke::down(k))
        .chain(keys.iter().rev().map(|&k| Stroke::up(k)))
        .collect();
    send(&strokes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extended_keys() {
        assert!(is_extended(vk::LWIN));
        assert!(is_extended(vk::RMENU));
        assert!(is_extended(vk::VOLUME_UP));
        assert!(!is_extended(vk::LCONTROL));
        assert!(!is_extended(vk::F13));
        assert!(!is_extended(vk::K));
    }

    #[test]
    fn chord_releases_in_reverse() {
        let keys = [vk::LCONTROL, vk::LMENU, vk::K];
        let strokes: Vec<Stroke> = keys
            .iter()
            .map(|&k| Stroke::down(k))
            .chain(keys.iter().rev().map(|&k| Stroke::up(k)))
            .collect();
        assert_eq!(strokes[2], Stroke::down(vk::K));
        assert_eq!(strokes[3], Stroke::up(vk::K));
        assert_eq!(strokes[5], Stroke::up(vk::LCONTROL));
    }
}
