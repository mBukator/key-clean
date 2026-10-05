//! Synthesized mouse events and cursor helpers, tagged so the observer can tell them from a
//! person's mouse or touchpad.
//!
//! Injected mouse events pass through every low-level mouse hook with `LLMHF_INJECTED` set,
//! otherwise like real ones, so they exercise KeyClean's real pass/block path (it blocks injected
//! input during a lock, ADR 0013). They carry [`MOUSE_TAG`], not [`super::inject::TAG`]: the
//! observer swallows tagged buttons and wheel turns after counting them, while
//! [`super::inject::click`] (tagged with `TAG`) must keep reaching windows.

use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_MOUSE, MOUSE_EVENT_FLAGS, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSEEVENTF_XDOWN,
    MOUSEEVENTF_XUP, MOUSEINPUT, SendInput,
};
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, SetCursorPos};

use super::inject::InjectError;

/// `dwExtraInfo` value on every event this module sends ("KCMS").
pub const MOUSE_TAG: usize = 0x4B43_4D53;

/// A mouse button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    /// Left button.
    Left,
    /// Right button.
    Right,
    /// Middle button (wheel click).
    Middle,
    /// First side button (usually "back").
    X1,
    /// Second side button (usually "forward").
    X2,
}

/// One synthesized mouse event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseInput {
    /// A relative move by (`dx`, `dy`) mickeys (pointer speed and acceleration apply).
    Move {
        /// Horizontal movement.
        dx: i32,
        /// Vertical movement.
        dy: i32,
    },
    /// A button press.
    Down(Button),
    /// A button release.
    Up(Button),
    /// A vertical wheel turn; 120 is one notch.
    Wheel(i32),
    /// A horizontal wheel turn; 120 is one notch.
    HWheel(i32),
}

/// `XBUTTON1` / `XBUTTON2`, the `mouseData` values `SendInput` takes for the side buttons. (A
/// low-level hook receives the same numbers in the high word of its `mouseData` instead.)
const XBUTTON1: i32 = 1;
const XBUTTON2: i32 = 2;

fn button_flags(button: Button, down: bool) -> (MOUSE_EVENT_FLAGS, i32) {
    match (button, down) {
        (Button::Left, true) => (MOUSEEVENTF_LEFTDOWN, 0),
        (Button::Left, false) => (MOUSEEVENTF_LEFTUP, 0),
        (Button::Right, true) => (MOUSEEVENTF_RIGHTDOWN, 0),
        (Button::Right, false) => (MOUSEEVENTF_RIGHTUP, 0),
        (Button::Middle, true) => (MOUSEEVENTF_MIDDLEDOWN, 0),
        (Button::Middle, false) => (MOUSEEVENTF_MIDDLEUP, 0),
        (Button::X1, true) => (MOUSEEVENTF_XDOWN, XBUTTON1),
        (Button::X1, false) => (MOUSEEVENTF_XUP, XBUTTON1),
        (Button::X2, true) => (MOUSEEVENTF_XDOWN, XBUTTON2),
        (Button::X2, false) => (MOUSEEVENTF_XUP, XBUTTON2),
    }
}

fn to_input(event: MouseInput) -> INPUT {
    let (dx, dy, flags, data) = match event {
        MouseInput::Move { dx, dy } => (dx, dy, MOUSEEVENTF_MOVE, 0),
        MouseInput::Down(b) => {
            let (flags, data) = button_flags(b, true);
            (0, 0, flags, data)
        }
        MouseInput::Up(b) => {
            let (flags, data) = button_flags(b, false);
            (0, 0, flags, data)
        }
        MouseInput::Wheel(delta) => (0, 0, MOUSEEVENTF_WHEEL, delta),
        MouseInput::HWheel(delta) => (0, 0, MOUSEEVENTF_HWHEEL, delta),
    };
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                // A negative wheel delta is passed as its two's-complement bits, as Windows expects.
                mouseData: data as u32,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: MOUSE_TAG,
            },
        },
    }
}

/// Sends `events` in order as one batch, each tagged with [`MOUSE_TAG`].
pub fn send(events: &[MouseInput]) -> Result<(), InjectError> {
    let inputs: Vec<INPUT> = events.iter().copied().map(to_input).collect();
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

/// The cursor position in screen coordinates (physical pixels after
/// [`super::system::make_dpi_aware`]), or `None` if Windows refuses (e.g. on the secure desktop).
pub fn cursor_pos() -> Option<(i32, i32)> {
    let mut point = POINT::default();
    // SAFETY: `point` is writable for the duration of the call.
    unsafe { GetCursorPos(&mut point) }.ok()?;
    Some((point.x, point.y))
}

/// Puts the cursor at (`x`, `y`). Returns false if Windows refused.
pub fn set_cursor_pos(x: i32, y: i32) -> bool {
    // SAFETY: no preconditions; fails harmlessly (e.g. on the secure desktop).
    unsafe { SetCursorPos(x, y) }.is_ok()
}
