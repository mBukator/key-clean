//! Pass/block decisions for mouse and touchpad input (§22) that leave no stuck buttons
//! (invariant 8).
//!
//! The mouse hook sorts each event into a [`MouseEvent`] and asks [`ButtonTracker::decide`]. While
//! `Locked`, everything is blocked: movement, every button and both wheels, plus any mouse message
//! the hook doesn't recognise. Once the session ends, movement and wheel turns pass again at once.
//!
//! Buttons follow the key rule from [`crate::keystate`]: a blocked press swallows its release, and
//! every other release passes, so Windows never sees a button go down without coming back up.
//! Unlike the keyboard, nothing is seeded at lock start. A button held when the lock begins isn't
//! swallowed, so its release passes anyway, and mouse buttons don't auto-repeat. (Seeding from
//! `GetAsyncKeyState` would also be wrong with swapped buttons: it reports physical buttons.)
//!
//! Everything here is O(1) and allocation-free, so it can run inside the hook callback. No
//! coordinates or wheel amounts are kept.

use crate::chord::KeyDirection;
use crate::keystate::{Phase, Verdict};

/// A mouse button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    /// Left button (or a touchpad tap).
    Left,
    /// Right button.
    Right,
    /// Middle button or wheel click.
    Middle,
    /// First side button (usually "back").
    X1,
    /// Second side button (usually "forward").
    X2,
}

impl MouseButton {
    const fn bit(self) -> u8 {
        match self {
            MouseButton::Left => 1 << 0,
            MouseButton::Right => 1 << 1,
            MouseButton::Middle => 1 << 2,
            MouseButton::X1 => 1 << 3,
            MouseButton::X2 => 1 << 4,
        }
    }
}

/// One mouse event, as far as the lock cares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseEvent {
    /// The cursor moved.
    Move,
    /// The vertical or horizontal wheel turned (including touchpad scrolling that Windows turns into
    /// wheel messages).
    Wheel,
    /// A button was pressed or released.
    Button(MouseButton, KeyDirection),
    /// A mouse message the hook doesn't recognise. Blocked while locked, like everything else.
    Other,
}

/// Two button masks, one byte each, `Copy`: the same model as
/// [`KeyTracker`](crate::keystate::KeyTracker), learned from hook events only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ButtonTracker {
    /// Buttons Windows believes are down, because the hook passed their press.
    os_down: u8,
    /// Buttons whose press was blocked and whose release hasn't arrived yet.
    swallowed: u8,
}

impl ButtonTracker {
    /// Nothing tracked.
    pub const fn new() -> Self {
        ButtonTracker {
            os_down: 0,
            swallowed: 0,
        }
    }

    /// Decides one mouse event in `phase` and updates the button masks.
    pub fn decide(&mut self, phase: Phase, event: MouseEvent) -> Verdict {
        // Movement, wheels and unknown messages: blocked only while locked.
        let MouseEvent::Button(button, direction) = event else {
            return if phase == Phase::Locked {
                Verdict::Block
            } else {
                Verdict::Pass
            };
        };
        let swallowed = self.swallowed & button.bit() != 0;
        match (phase, direction) {
            (Phase::Passthrough, _) => Verdict::Pass,
            // Blocked. A press of a button Windows already has down (it passed before the lock)
            // isn't swallowed, so its release still passes.
            (Phase::Locked, KeyDirection::Down) => {
                if self.os_down & button.bit() == 0 {
                    self.swallowed |= button.bit();
                }
                Verdict::Block
            }
            // Draining: a second press of a button whose blocked press is still down would reach
            // Windows without its own release; keep it blocked. New presses pass.
            (Phase::Draining, KeyDirection::Down) if swallowed => Verdict::Block,
            // Only the release of a blocked press is blocked. Every other release passes: an
            // unpaired release is harmless, while a blocked one could leave a button held down.
            (Phase::Locked | Phase::Draining, KeyDirection::Up) if swallowed => {
                self.swallowed &= !button.bit();
                Verdict::Block
            }
            // Arming, a new press while draining, or any other release: Windows gets it.
            _ => {
                self.passed(button, direction);
                Verdict::Pass
            }
        }
    }

    fn passed(&mut self, button: MouseButton, direction: KeyDirection) {
        match direction {
            KeyDirection::Down => self.os_down |= button.bit(),
            KeyDirection::Up => self.os_down &= !button.bit(),
        }
    }

    /// Whether every blocked press has been released, so the hook can be removed.
    pub const fn drained(&self) -> bool {
        self.swallowed == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chord::KeyDirection::{Down, Up};
    use MouseButton::{Left, Middle, Right, X1, X2};

    const ALL_BUTTONS: [MouseButton; 5] = [Left, Right, Middle, X1, X2];

    /// Windows' view of the buttons, from the events the hook lets through.
    #[derive(Default)]
    struct Rig {
        tracker: ButtonTracker,
        os_down: u8,
        delivered_presses: u32,
    }

    impl Rig {
        fn send(&mut self, phase: Phase, event: MouseEvent) -> Verdict {
            let verdict = self.tracker.decide(phase, event);
            if verdict == Verdict::Pass
                && let MouseEvent::Button(button, direction) = event
            {
                match direction {
                    Down => {
                        self.os_down |= button.bit();
                        self.delivered_presses += 1;
                    }
                    Up => self.os_down &= !button.bit(),
                }
            }
            verdict
        }

        fn click(&mut self, phase: Phase, button: MouseButton) -> (Verdict, Verdict) {
            (
                self.send(phase, MouseEvent::Button(button, Down)),
                self.send(phase, MouseEvent::Button(button, Up)),
            )
        }
    }

    #[test]
    fn locked_blocks_movement_wheel_unknown_and_every_click() {
        let mut rig = Rig::default();
        for event in [MouseEvent::Move, MouseEvent::Wheel, MouseEvent::Other] {
            assert_eq!(rig.send(Phase::Locked, event), Verdict::Block);
        }
        for button in ALL_BUTTONS {
            assert_eq!(
                rig.click(Phase::Locked, button),
                (Verdict::Block, Verdict::Block)
            );
        }
        assert_eq!(rig.delivered_presses, 0);
        assert!(rig.tracker.drained());
    }

    #[test]
    fn arming_and_passthrough_pass_everything() {
        for phase in [Phase::Arming, Phase::Passthrough] {
            let mut rig = Rig::default();
            for event in [MouseEvent::Move, MouseEvent::Wheel, MouseEvent::Other] {
                assert_eq!(rig.send(phase, event), Verdict::Pass);
            }
            for button in ALL_BUTTONS {
                assert_eq!(rig.click(phase, button), (Verdict::Pass, Verdict::Pass));
            }
            assert!(rig.tracker.drained());
            assert_eq!(rig.os_down, 0);
        }
    }

    #[test]
    fn movement_and_wheel_pass_as_soon_as_the_lock_ends() {
        let mut rig = Rig::default();
        rig.send(Phase::Locked, MouseEvent::Button(Left, Down)); // still held
        for event in [MouseEvent::Move, MouseEvent::Wheel, MouseEvent::Other] {
            assert_eq!(rig.send(Phase::Draining, event), Verdict::Pass);
        }
        assert!(!rig.tracker.drained());
    }

    #[test]
    fn button_held_at_lock_start_releases_normally() {
        // E.g. the user is still holding the touchpad button when the lock begins.
        let mut rig = Rig::default();
        assert_eq!(
            rig.send(Phase::Arming, MouseEvent::Button(Left, Down)),
            Verdict::Pass
        );
        assert_eq!(
            rig.send(Phase::Locked, MouseEvent::Button(Left, Up)),
            Verdict::Pass,
            "Windows saw the press, so it must see the release"
        );
        assert_eq!(rig.os_down, 0);
        assert!(rig.tracker.drained());
    }

    #[test]
    fn second_press_of_a_button_windows_has_down_is_not_swallowed() {
        // Pressed during arming (Windows saw it), then a second press with no release between
        // (e.g. injected input, or a lost release). Swallowing it would block the real release.
        let mut rig = Rig::default();
        rig.send(Phase::Arming, MouseEvent::Button(Left, Down));
        assert_eq!(
            rig.send(Phase::Locked, MouseEvent::Button(Left, Down)),
            Verdict::Block
        );
        assert!(rig.tracker.drained());
        assert_eq!(
            rig.send(Phase::Locked, MouseEvent::Button(Left, Up)),
            Verdict::Pass
        );
        assert_eq!(rig.os_down, 0, "no stuck button");
    }

    #[test]
    fn button_held_across_the_end_of_the_lock_drains() {
        let mut rig = Rig::default();
        rig.send(Phase::Locked, MouseEvent::Button(Right, Down));
        rig.send(Phase::Locked, MouseEvent::Button(X2, Down));
        assert!(!rig.tracker.drained());
        // Session ends; the releases arrive later and stay blocked.
        assert_eq!(
            rig.send(Phase::Draining, MouseEvent::Button(Right, Up)),
            Verdict::Block
        );
        assert!(!rig.tracker.drained());
        assert_eq!(
            rig.send(Phase::Draining, MouseEvent::Button(X2, Up)),
            Verdict::Block
        );
        assert!(rig.tracker.drained());
        assert_eq!(rig.delivered_presses, 0);
        assert_eq!(rig.os_down, 0);
    }

    #[test]
    fn new_click_during_drain_passes() {
        let mut rig = Rig::default();
        rig.send(Phase::Locked, MouseEvent::Button(Left, Down)); // still held
        assert_eq!(
            rig.click(Phase::Draining, Middle),
            (Verdict::Pass, Verdict::Pass)
        );
        // A second press of the held button would be unpaired; it stays blocked.
        assert_eq!(
            rig.send(Phase::Draining, MouseEvent::Button(Left, Down)),
            Verdict::Block
        );
        assert_eq!(
            rig.send(Phase::Draining, MouseEvent::Button(Left, Up)),
            Verdict::Block
        );
        assert!(rig.tracker.drained());
        assert_eq!(
            rig.click(Phase::Draining, Left),
            (Verdict::Pass, Verdict::Pass)
        );
        assert_eq!(rig.os_down, 0);
    }

    #[test]
    fn release_never_seen_going_down_passes() {
        let mut rig = Rig::default();
        for phase in [Phase::Locked, Phase::Draining] {
            for button in ALL_BUTTONS {
                assert_eq!(
                    rig.send(phase, MouseEvent::Button(button, Up)),
                    Verdict::Pass
                );
            }
        }
    }

    #[test]
    fn buttons_are_tracked_separately() {
        let mut rig = Rig::default();
        for button in ALL_BUTTONS {
            rig.send(Phase::Locked, MouseEvent::Button(button, Down));
        }
        for (i, button) in ALL_BUTTONS.into_iter().enumerate() {
            assert!(!rig.tracker.drained(), "{i} released so far");
            assert_eq!(
                rig.send(Phase::Draining, MouseEvent::Button(button, Up)),
                Verdict::Block
            );
        }
        assert!(rig.tracker.drained());
        assert_eq!(rig.os_down, 0);
    }
}
