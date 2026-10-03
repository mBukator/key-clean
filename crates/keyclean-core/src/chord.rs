//! Emergency-chord detection (Ctrl+Alt+K, §17).
//!
//! The hook tracks the chord keys itself instead of asking Windows (`GetAsyncKeyState`), because
//! blocked events don't reliably update the system key state (invariant 7). The state is a single
//! byte so the hook can keep it without allocation or locks.
//!
//! Rules:
//! - Fires when any Ctrl, any Alt and K are all down, in any press order, on the key-down that
//!   completes the set. Auto-repeat (a key-down for a key already down) never fires.
//! - Left and right modifiers are tracked separately; releasing one Ctrl while the other is still
//!   held keeps Ctrl down.
//! - AltGr arrives as LCtrl + RAlt, so AltGr+K counts as Ctrl+Alt+K.
//! - Releasing a modifier before K is pressed cancels the chord.

/// A key as far as the chord detector cares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChordKey {
    /// Left Ctrl (also the fake LCtrl that precedes AltGr).
    LeftCtrl,
    /// Right Ctrl.
    RightCtrl,
    /// Left Alt.
    LeftAlt,
    /// Right Alt (AltGr on many layouts).
    RightAlt,
    /// The K key.
    K,
    /// Any other key.
    Other,
}

/// Press or release.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyDirection {
    /// Key-down (including auto-repeat).
    Down,
    /// Key-up.
    Up,
}

const LEFT_CTRL_BIT: u8 = 1 << 0;
const RIGHT_CTRL_BIT: u8 = 1 << 1;
const LEFT_ALT_BIT: u8 = 1 << 2;
const RIGHT_ALT_BIT: u8 = 1 << 3;
const K_BIT: u8 = 1 << 4;
const ALL_BITS: u8 = LEFT_CTRL_BIT | RIGHT_CTRL_BIT | LEFT_ALT_BIT | RIGHT_ALT_BIT | K_BIT;

impl ChordKey {
    const fn bit(self) -> u8 {
        match self {
            ChordKey::LeftCtrl => LEFT_CTRL_BIT,
            ChordKey::RightCtrl => RIGHT_CTRL_BIT,
            ChordKey::LeftAlt => LEFT_ALT_BIT,
            ChordKey::RightAlt => RIGHT_ALT_BIT,
            ChordKey::K => K_BIT,
            ChordKey::Other => 0,
        }
    }
}

/// Which chord keys are currently down. `Copy` and one byte wide.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChordState(u8);

impl ChordState {
    /// Nothing held.
    pub const fn new() -> Self {
        ChordState(0)
    }

    /// Restores a state previously obtained from [`ChordState::to_bits`]. Unknown bits are ignored.
    pub const fn from_bits(bits: u8) -> Self {
        ChordState(bits & ALL_BITS)
    }

    /// The raw state, for storing in an atomic.
    pub const fn to_bits(self) -> u8 {
        self.0
    }

    /// Marks `key` as already held, e.g. from a snapshot taken when the lock starts.
    #[must_use]
    pub const fn with_held(self, key: ChordKey) -> Self {
        ChordState(self.0 | key.bit())
    }

    /// Whether `key` is currently held.
    pub const fn is_held(self, key: ChordKey) -> bool {
        let bit = key.bit();
        bit != 0 && self.0 & bit != 0
    }

    const fn complete(self) -> bool {
        self.0 & (LEFT_CTRL_BIT | RIGHT_CTRL_BIT) != 0
            && self.0 & (LEFT_ALT_BIT | RIGHT_ALT_BIT) != 0
            && self.0 & K_BIT != 0
    }

    /// Feeds one key event. Returns `true` exactly when this event completes the chord.
    pub fn on_key(&mut self, key: ChordKey, direction: KeyDirection) -> bool {
        let bit = key.bit();
        if bit == 0 {
            return false;
        }
        match direction {
            KeyDirection::Down => {
                let repeat = self.0 & bit != 0;
                self.0 |= bit;
                !repeat && self.complete()
            }
            KeyDirection::Up => {
                self.0 &= !bit;
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ChordKey::*;
    use super::KeyDirection::{Down, Up};
    use super::*;

    /// Feeds a sequence and returns the index of every event that fired.
    fn fire_points(state: &mut ChordState, events: &[(ChordKey, KeyDirection)]) -> Vec<usize> {
        events
            .iter()
            .enumerate()
            .filter_map(|(i, &(k, d))| state.on_key(k, d).then_some(i))
            .collect()
    }

    #[test]
    fn left_ctrl_left_alt_k() {
        let mut s = ChordState::new();
        assert_eq!(
            fire_points(&mut s, &[(LeftCtrl, Down), (LeftAlt, Down), (K, Down)]),
            vec![2]
        );
    }

    #[test]
    fn every_left_right_combination_fires() {
        for ctrl in [LeftCtrl, RightCtrl] {
            for alt in [LeftAlt, RightAlt] {
                let mut s = ChordState::new();
                assert_eq!(
                    fire_points(&mut s, &[(ctrl, Down), (alt, Down), (K, Down)]),
                    vec![2],
                    "{ctrl:?} + {alt:?}"
                );
            }
        }
    }

    #[test]
    fn altgr_counts_as_ctrl_alt() {
        // AltGr is delivered as a fake LCtrl followed by RAlt.
        let mut s = ChordState::new();
        assert_eq!(
            fire_points(&mut s, &[(LeftCtrl, Down), (RightAlt, Down), (K, Down)]),
            vec![2]
        );
    }

    #[test]
    fn any_press_order_fires_on_the_completing_key() {
        let mut s = ChordState::new();
        assert_eq!(
            fire_points(&mut s, &[(K, Down), (LeftCtrl, Down), (LeftAlt, Down)]),
            vec![2]
        );
        let mut s = ChordState::new();
        assert_eq!(
            fire_points(&mut s, &[(LeftAlt, Down), (K, Down), (RightCtrl, Down)]),
            vec![2]
        );
    }

    #[test]
    fn key_repeat_fires_only_once() {
        let mut s = ChordState::new();
        let events = [
            (LeftCtrl, Down),
            (LeftCtrl, Down), // repeat
            (LeftAlt, Down),
            (K, Down),
            (K, Down),       // repeat
            (K, Down),       // repeat
            (LeftAlt, Down), // repeat
        ];
        assert_eq!(fire_points(&mut s, &events), vec![3]);
    }

    #[test]
    fn repeating_modifier_never_fires_by_itself() {
        // K held first, then Ctrl and Alt auto-repeat: only the first Alt-down completes it.
        let mut s = ChordState::new();
        let events = [
            (K, Down),
            (LeftCtrl, Down),
            (LeftCtrl, Down),
            (LeftAlt, Down),
            (LeftAlt, Down),
            (LeftCtrl, Down),
        ];
        assert_eq!(fire_points(&mut s, &events), vec![3]);
    }

    #[test]
    fn releasing_a_modifier_mid_chord_cancels() {
        let mut s = ChordState::new();
        let events = [(LeftCtrl, Down), (LeftAlt, Down), (LeftCtrl, Up), (K, Down)];
        assert_eq!(fire_points(&mut s, &events), Vec::<usize>::new());
        assert!(!s.is_held(LeftCtrl));
        assert!(s.is_held(LeftAlt));
        assert!(s.is_held(K));
    }

    #[test]
    fn other_side_modifier_keeps_the_chord_alive() {
        let mut s = ChordState::new();
        let events = [
            (LeftCtrl, Down),
            (RightCtrl, Down),
            (LeftCtrl, Up),
            (LeftAlt, Down),
            (K, Down),
        ];
        assert_eq!(fire_points(&mut s, &events), vec![4]);
    }

    #[test]
    fn other_keys_are_ignored() {
        let mut s = ChordState::new();
        let events = [
            (LeftCtrl, Down),
            (Other, Down),
            (LeftAlt, Down),
            (Other, Up),
            (Other, Down),
            (K, Down),
        ];
        assert_eq!(fire_points(&mut s, &events), vec![5]);
        assert!(!s.is_held(Other));
    }

    #[test]
    fn ctrl_k_or_alt_k_alone_never_fires() {
        let mut s = ChordState::new();
        assert!(fire_points(&mut s, &[(LeftCtrl, Down), (K, Down)]).is_empty());
        let mut s = ChordState::new();
        assert!(fire_points(&mut s, &[(RightAlt, Down), (K, Down)]).is_empty());
    }

    #[test]
    fn fires_again_after_full_release() {
        let mut s = ChordState::new();
        let events = [
            (LeftCtrl, Down),
            (LeftAlt, Down),
            (K, Down),
            (K, Up),
            (K, Down),
        ];
        assert_eq!(fire_points(&mut s, &events), vec![2, 4]);
    }

    #[test]
    fn seeded_modifiers_from_lock_start_count() {
        // Ctrl+Alt were already held when the lock began (snapshot), then K is pressed.
        let mut s = ChordState::new().with_held(LeftCtrl).with_held(LeftAlt);
        assert!(s.on_key(K, Down));
    }

    #[test]
    fn seeded_k_with_modifiers_does_not_fire_without_a_new_press() {
        // Everything held at lock start: no event has completed the chord yet.
        let mut s = ChordState::new()
            .with_held(LeftCtrl)
            .with_held(LeftAlt)
            .with_held(K);
        // A repeat of K must not fire.
        assert!(!s.on_key(K, Down));
        // Releasing and pressing K again does.
        assert!(!s.on_key(K, Up));
        assert!(s.on_key(K, Down));
    }

    #[test]
    fn bits_round_trip() {
        let s = ChordState::new().with_held(RightCtrl).with_held(K);
        assert_eq!(ChordState::from_bits(s.to_bits()), s);
        assert_eq!(ChordState::from_bits(0xFF).to_bits(), ALL_BITS);
    }
}
