//! Mapping from low-level hook key data to the chord detector's keys. Pure, so it is unit-tested.
//!
//! Nothing here stores or reports which key was pressed; the result is used inside the hook
//! callback and then discarded.

use keyclean_core::chord::ChordKey;

/// Generic Ctrl (some sources send this instead of the left/right code).
pub(crate) const VK_CONTROL: u32 = 0x11;
/// Generic Alt.
pub(crate) const VK_MENU: u32 = 0x12;
/// The K key.
pub(crate) const VK_K: u32 = 0x4B;
/// Left Ctrl.
pub(crate) const VK_LCONTROL: u32 = 0xA2;
/// Right Ctrl.
pub(crate) const VK_RCONTROL: u32 = 0xA3;
/// Left Alt.
pub(crate) const VK_LMENU: u32 = 0xA4;
/// Right Alt / AltGr.
pub(crate) const VK_RMENU: u32 = 0xA5;
/// Scan code of the physical key labelled K on a US layout. Accepted as K too, so the emergency
/// chord also works where the layout moves the K virtual key (e.g. Dvorak).
pub(crate) const SCAN_K: u32 = 0x25;

/// Classifies one hook event for the chord detector.
pub(crate) fn chord_key(vk: u32, scan: u32, extended: bool) -> ChordKey {
    match vk {
        VK_LCONTROL => ChordKey::LeftCtrl,
        VK_RCONTROL => ChordKey::RightCtrl,
        VK_LMENU => ChordKey::LeftAlt,
        VK_RMENU => ChordKey::RightAlt,
        VK_CONTROL if extended => ChordKey::RightCtrl,
        VK_CONTROL => ChordKey::LeftCtrl,
        VK_MENU if extended => ChordKey::RightAlt,
        VK_MENU => ChordKey::LeftAlt,
        VK_K => ChordKey::K,
        _ if scan == SCAN_K && !extended => ChordKey::K,
        _ => ChordKey::Other,
    }
}

/// The virtual-key codes whose "held" state seeds the chord detector when a lock starts.
pub(crate) const CHORD_SEED_KEYS: [(u32, ChordKey); 5] = [
    (VK_LCONTROL, ChordKey::LeftCtrl),
    (VK_RCONTROL, ChordKey::RightCtrl),
    (VK_LMENU, ChordKey::LeftAlt),
    (VK_RMENU, ChordKey::RightAlt),
    (VK_K, ChordKey::K),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn left_right_specific_codes() {
        assert_eq!(chord_key(VK_LCONTROL, 0x1D, false), ChordKey::LeftCtrl);
        assert_eq!(chord_key(VK_RCONTROL, 0x1D, true), ChordKey::RightCtrl);
        assert_eq!(chord_key(VK_LMENU, 0x38, false), ChordKey::LeftAlt);
        assert_eq!(chord_key(VK_RMENU, 0x38, true), ChordKey::RightAlt);
    }

    #[test]
    fn generic_modifiers_use_the_extended_flag() {
        assert_eq!(chord_key(VK_CONTROL, 0x1D, false), ChordKey::LeftCtrl);
        assert_eq!(chord_key(VK_CONTROL, 0x1D, true), ChordKey::RightCtrl);
        assert_eq!(chord_key(VK_MENU, 0x38, false), ChordKey::LeftAlt);
        assert_eq!(chord_key(VK_MENU, 0x38, true), ChordKey::RightAlt);
    }

    #[test]
    fn altgr_fake_ctrl_is_left_ctrl() {
        // AltGr sends LCtrl with scan code 0x21D before RAlt.
        assert_eq!(chord_key(VK_LCONTROL, 0x21D, false), ChordKey::LeftCtrl);
    }

    #[test]
    fn k_by_virtual_key_or_physical_position() {
        assert_eq!(chord_key(VK_K, SCAN_K, false), ChordKey::K);
        assert_eq!(chord_key(VK_K, 0x2F, false), ChordKey::K); // VK_K elsewhere on the layout
        assert_eq!(chord_key(0x54, SCAN_K, false), ChordKey::K); // Dvorak: physical K is 'T'
        assert_eq!(chord_key(0x54, SCAN_K, true), ChordKey::Other);
    }

    #[test]
    fn everything_else_is_other() {
        for vk in [0x41u32, 0x5B, 0x09, 0x1B, 0x70, 0xA0, 0xA1, 0x2E] {
            assert_eq!(chord_key(vk, 0x10, false), ChordKey::Other, "{vk:#x}");
        }
    }
}
