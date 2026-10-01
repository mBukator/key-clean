//! Pass/block decisions that leave no stuck keys (invariant 8).
//!
//! The hook keeps two sets of key codes:
//! - `os_down`: keys Windows believes are down, because their key-down reached it (keys held when
//!   the lock started, or pressed after the session ended).
//! - `swallowed`: keys physically down whose key-down was blocked.
//!
//! The release of a blocked press is blocked, and auto-repeat of a blocked press stays blocked even
//! after the session has ended, so Windows never sees half of a blocked keystroke. Every other
//! key-up passes, so a key Windows saw go down always comes back up. No `SendInput` is needed.
//!
//! Phases (the hook only exists outside `Idle`):
//! - [`Phase::Arming`]: hook installed, everything passes, `os_down` is tracked. The engine then
//!   adds a snapshot of held keys and switches to `Locked`. (Callbacks only run while the engine
//!   thread waits for messages, so in practice the snapshot does the work; the phase keeps the
//!   model correct if that ever changes.)
//! - [`Phase::Locked`]: every key-down is blocked.
//! - [`Phase::Draining`]: the session has ended; new presses pass, while repeats and releases of
//!   blocked presses stay blocked until `swallowed` is empty or the drain times out.
//! - [`Phase::Passthrough`]: the watchdog took over; everything passes.
//!
//! Everything here is O(1) and allocation-free, so it can run inside the hook callback.

use crate::chord::KeyDirection;

/// A set of 8-bit key codes (Windows virtual-key codes in practice). 32 bytes, `Copy`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeySet([u64; 4]);

impl KeySet {
    /// The empty set.
    pub const fn new() -> Self {
        KeySet([0; 4])
    }

    const fn slot(code: u8) -> (usize, u64) {
        ((code >> 6) as usize, 1u64 << (code & 63))
    }

    /// Whether `code` is in the set.
    pub const fn contains(&self, code: u8) -> bool {
        let (i, bit) = Self::slot(code);
        self.0[i] & bit != 0
    }

    /// Adds `code`.
    pub fn insert(&mut self, code: u8) {
        let (i, bit) = Self::slot(code);
        self.0[i] |= bit;
    }

    /// Removes `code`.
    pub fn remove(&mut self, code: u8) {
        let (i, bit) = Self::slot(code);
        self.0[i] &= !bit;
    }

    /// Whether the set is empty.
    pub const fn is_empty(&self) -> bool {
        self.0[0] == 0 && self.0[1] == 0 && self.0[2] == 0 && self.0[3] == 0
    }

    /// Number of codes in the set.
    pub const fn len(&self) -> u32 {
        self.0[0].count_ones()
            + self.0[1].count_ones()
            + self.0[2].count_ones()
            + self.0[3].count_ones()
    }
}

/// What the hook is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Phase {
    /// Installed, passing everything, learning which keys Windows sees as down.
    Arming = 1,
    /// Blocking every key-down.
    Locked = 2,
    /// Session over; releasing input without leaving unpaired events.
    Draining = 3,
    /// Watchdog override: pass everything.
    Passthrough = 4,
}

impl Phase {
    /// The phase for a stored byte. Anything unknown maps to `Passthrough`, the safe direction.
    pub const fn from_u8(value: u8) -> Phase {
        match value {
            1 => Phase::Arming,
            2 => Phase::Locked,
            3 => Phase::Draining,
            _ => Phase::Passthrough,
        }
    }
}

/// The hook's answer for one event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Let Windows have the event.
    Pass,
    /// Swallow the event.
    Block,
}

/// Both key sets the hook maintains.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyTracker {
    /// Keys Windows believes are down.
    pub os_down: KeySet,
    /// Keys physically down whose press was blocked.
    pub swallowed: KeySet,
}

impl KeyTracker {
    /// Nothing tracked.
    pub const fn new() -> Self {
        KeyTracker {
            os_down: KeySet::new(),
            swallowed: KeySet::new(),
        }
    }

    /// Decides one key event in `phase` and updates the sets.
    pub fn decide(&mut self, phase: Phase, code: u8, direction: KeyDirection) -> Verdict {
        match (phase, direction) {
            (Phase::Passthrough, _) => Verdict::Pass,

            (Phase::Arming, KeyDirection::Down) => {
                self.os_down.insert(code);
                Verdict::Pass
            }
            (Phase::Arming, KeyDirection::Up) => {
                self.os_down.remove(code);
                Verdict::Pass
            }

            (Phase::Locked, KeyDirection::Down) => {
                if !self.os_down.contains(code) {
                    self.swallowed.insert(code);
                }
                Verdict::Block
            }
            (Phase::Draining, KeyDirection::Down) => {
                if self.swallowed.contains(code) {
                    Verdict::Block
                } else {
                    self.os_down.insert(code);
                    Verdict::Pass
                }
            }

            // Only the release of a blocked press is blocked. Every other key-up passes: an
            // unpaired key-up is harmless, while a blocked one could leave a key stuck down.
            (Phase::Locked | Phase::Draining, KeyDirection::Up) => {
                if self.swallowed.contains(code) {
                    self.swallowed.remove(code);
                    Verdict::Block
                } else {
                    self.os_down.remove(code);
                    Verdict::Pass
                }
            }
        }
    }

    /// Whether every blocked press has been released, so the hook can be removed.
    pub const fn drained(&self) -> bool {
        self.swallowed.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chord::KeyDirection::{Down, Up};

    // Virtual-key codes, for readability only.
    const CTRL: u8 = 0xA2;
    const ALT: u8 = 0xA4;
    const K: u8 = 0x4B;
    const A: u8 = 0x41;
    const SHIFT: u8 = 0xA0;
    const DEL: u8 = 0x2E;

    /// Simulates Windows' view of the keyboard from the events the hook lets through, and fails on
    /// any unpaired event.
    #[derive(Default)]
    struct OsView {
        down: KeySet,
        delivered_downs: u32,
    }

    impl OsView {
        fn deliver(&mut self, code: u8, direction: KeyDirection) {
            match direction {
                Down => {
                    self.down.insert(code);
                    self.delivered_downs += 1;
                }
                Up => {
                    // Unpaired key-ups are harmless to Windows; only stuck downs matter.
                    self.down.remove(code);
                }
            }
        }
    }

    struct Rig {
        tracker: KeyTracker,
        os: OsView,
    }

    impl Rig {
        fn new() -> Self {
            Rig {
                tracker: KeyTracker::new(),
                os: OsView::default(),
            }
        }

        /// A key held before the hook existed: Windows already saw its key-down.
        fn held_before_hook(&mut self, code: u8) {
            self.os.deliver(code, Down);
        }

        /// The engine's snapshot of held keys at the end of Arming.
        fn snapshot(&mut self) {
            for code in 0..=u8::MAX {
                if self.os.down.contains(code) {
                    self.tracker.os_down.insert(code);
                }
            }
        }

        fn send(&mut self, phase: Phase, code: u8, direction: KeyDirection) -> Verdict {
            let verdict = self.tracker.decide(phase, code, direction);
            if verdict == Verdict::Pass {
                self.os.deliver(code, direction);
            }
            verdict
        }
    }

    #[test]
    fn key_set_basics() {
        let mut s = KeySet::new();
        assert!(s.is_empty());
        for code in [0u8, 1, 63, 64, 127, 128, 200, 255] {
            s.insert(code);
            assert!(s.contains(code));
        }
        assert_eq!(s.len(), 8);
        s.remove(64);
        assert!(!s.contains(64));
        assert!(s.contains(63));
        assert_eq!(s.len(), 7);
    }

    #[test]
    fn phase_round_trip_and_unknown_is_passthrough() {
        for p in [
            Phase::Arming,
            Phase::Locked,
            Phase::Draining,
            Phase::Passthrough,
        ] {
            assert_eq!(Phase::from_u8(p as u8), p);
        }
        assert_eq!(Phase::from_u8(0), Phase::Passthrough);
        assert_eq!(Phase::from_u8(99), Phase::Passthrough);
    }

    #[test]
    fn locked_blocks_every_down_and_its_up() {
        let mut rig = Rig::new();
        for code in [
            A, K, CTRL, ALT, SHIFT, 0x5B, /* LWin */
            0x09, /* Tab */
            0x70, /* F1 */
        ] {
            assert_eq!(rig.send(Phase::Locked, code, Down), Verdict::Block);
            assert_eq!(rig.send(Phase::Locked, code, Down), Verdict::Block); // repeat
            assert_eq!(rig.send(Phase::Locked, code, Up), Verdict::Block);
        }
        assert_eq!(rig.os.delivered_downs, 0);
        assert!(rig.tracker.drained());
    }

    #[test]
    fn chord_then_auto_repeat_during_drain_reaches_nothing() {
        let mut rig = Rig::new();
        rig.snapshot();
        // Ctrl+Alt+K pressed while locked; the K-down completes the chord.
        rig.send(Phase::Locked, CTRL, Down);
        rig.send(Phase::Locked, ALT, Down);
        assert_eq!(rig.send(Phase::Locked, K, Down), Verdict::Block);

        // Session ends. The user is still holding all three; Windows auto-repeats them.
        for _ in 0..5 {
            assert_eq!(rig.send(Phase::Draining, K, Down), Verdict::Block);
            assert_eq!(rig.send(Phase::Draining, ALT, Down), Verdict::Block);
            assert_eq!(rig.send(Phase::Draining, CTRL, Down), Verdict::Block);
        }
        assert!(!rig.tracker.drained());

        // Release in any order.
        assert_eq!(rig.send(Phase::Draining, K, Up), Verdict::Block);
        assert_eq!(rig.send(Phase::Draining, CTRL, Up), Verdict::Block);
        assert!(!rig.tracker.drained());
        assert_eq!(rig.send(Phase::Draining, ALT, Up), Verdict::Block);

        assert!(rig.tracker.drained());
        assert_eq!(
            rig.os.delivered_downs, 0,
            "Ctrl+Alt+K must not reach the focused app"
        );
        assert!(rig.os.down.is_empty(), "no stuck keys");
    }

    #[test]
    fn key_held_at_lock_start_repeats_then_releases() {
        // E.g. Shift from a start shortcut, held across the lock.
        let mut rig = Rig::new();
        rig.held_before_hook(SHIFT);
        rig.snapshot();

        for _ in 0..10 {
            assert_eq!(rig.send(Phase::Locked, SHIFT, Down), Verdict::Block);
        }
        assert!(rig.tracker.drained(), "a held key is not a blocked press");

        // Its release must reach Windows, or Shift stays stuck.
        assert_eq!(rig.send(Phase::Locked, SHIFT, Up), Verdict::Pass);
        assert!(rig.os.down.is_empty());
    }

    #[test]
    fn key_pressed_during_arming_is_tracked_without_snapshot() {
        // The race the Arming phase closes: pressed after the hook went in, before the snapshot.
        let mut rig = Rig::new();
        assert_eq!(rig.send(Phase::Arming, CTRL, Down), Verdict::Pass);
        assert_eq!(rig.send(Phase::Locked, CTRL, Down), Verdict::Block);
        assert_eq!(rig.send(Phase::Locked, CTRL, Up), Verdict::Pass);
        assert!(rig.os.down.is_empty());
    }

    #[test]
    fn key_released_during_arming_is_forgotten() {
        let mut rig = Rig::new();
        rig.held_before_hook(A);
        assert_eq!(rig.send(Phase::Arming, A, Up), Verdict::Pass);
        rig.snapshot(); // A is no longer down, so the snapshot doesn't add it.
        assert_eq!(rig.send(Phase::Locked, A, Down), Verdict::Block);
        assert_eq!(rig.send(Phase::Locked, A, Up), Verdict::Block);
        assert!(rig.os.down.is_empty());
    }

    #[test]
    fn ctrl_alt_del_mid_lock_never_drains() {
        // Ctrl+Alt+Del switches to the secure desktop; the hook never sees the key-ups.
        let mut rig = Rig::new();
        rig.snapshot();
        rig.send(Phase::Locked, CTRL, Down);
        rig.send(Phase::Locked, ALT, Down);
        rig.send(Phase::Locked, DEL, Down);
        // Session ends (e.g. timer). No ups arrive.
        assert!(!rig.tracker.drained());
        assert_eq!(rig.tracker.swallowed.len(), 3);
        // The engine's DRAIN_TIMEOUT then removes the hook; Windows never saw those keys go down,
        // so there's nothing stuck on its side.
        assert!(rig.os.down.is_empty());
    }

    #[test]
    fn new_press_during_drain_passes_normally() {
        let mut rig = Rig::new();
        rig.snapshot();
        rig.send(Phase::Locked, K, Down); // still held after the session ends
        assert_eq!(rig.send(Phase::Draining, A, Down), Verdict::Pass);
        assert_eq!(rig.send(Phase::Draining, A, Down), Verdict::Pass); // its repeat too
        assert_eq!(rig.send(Phase::Draining, A, Up), Verdict::Pass);
        assert_eq!(rig.send(Phase::Draining, K, Up), Verdict::Block);
        assert!(rig.tracker.drained());
        assert!(rig.os.down.is_empty());
    }

    #[test]
    fn re_press_after_release_during_drain_passes() {
        let mut rig = Rig::new();
        rig.snapshot();
        rig.send(Phase::Locked, K, Down);
        rig.send(Phase::Draining, K, Up); // blocked, clears swallowed
        assert_eq!(rig.send(Phase::Draining, K, Down), Verdict::Pass);
        assert_eq!(rig.send(Phase::Draining, K, Up), Verdict::Pass);
        assert!(rig.os.down.is_empty());
    }

    #[test]
    fn up_for_a_key_never_seen_going_down_passes() {
        // A key-down that slipped past the hook install but wasn't in the snapshot yet.
        let mut rig = Rig::new();
        rig.os.deliver(A, Down); // Windows saw it go down; the tracker didn't.
        assert_eq!(rig.send(Phase::Locked, A, Up), Verdict::Pass);
        assert!(rig.os.down.is_empty(), "the key must not stick");
        assert_eq!(rig.send(Phase::Draining, SHIFT, Up), Verdict::Pass);
    }

    #[test]
    fn passthrough_passes_everything() {
        let mut tracker = KeyTracker::new();
        assert_eq!(tracker.decide(Phase::Passthrough, A, Down), Verdict::Pass);
        assert_eq!(tracker.decide(Phase::Passthrough, A, Up), Verdict::Pass);
        assert_eq!(tracker.decide(Phase::Passthrough, K, Up), Verdict::Pass);
    }

    #[test]
    fn mixed_typing_during_lock_leaves_nothing_stuck() {
        let mut rig = Rig::new();
        rig.held_before_hook(SHIFT);
        rig.snapshot();
        // Cleaning: random presses, overlapping, with repeats.
        let script: &[(u8, KeyDirection)] = &[
            (A, Down),
            (K, Down),
            (A, Down),
            (SHIFT, Down),
            (A, Up),
            (DEL, Down),
            (SHIFT, Up),
            (K, Up),
        ];
        for &(code, dir) in script {
            rig.send(Phase::Locked, code, dir);
        }
        // Session ends with DEL still held.
        assert_eq!(rig.send(Phase::Draining, DEL, Down), Verdict::Block);
        assert_eq!(rig.send(Phase::Draining, DEL, Up), Verdict::Block);
        assert!(rig.tracker.drained());
        assert!(rig.os.down.is_empty());
    }
}
