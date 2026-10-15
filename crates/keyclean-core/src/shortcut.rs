//! The start shortcut (§28, ADR 0017): which chords may start a lock, and how they are written.
//!
//! A shortcut is Shift plus Ctrl or Alt, plus one key: a letter, a digit, or F1-F24 except F12.
//! The rules keep the shortcut from taking keys other apps need, because Windows hands a
//! registered chord to KeyClean alone:
//! - one modifier is refused: Ctrl+C, Alt+F and the like would stop working everywhere;
//! - Ctrl+Alt is refused in every form: Ctrl+Alt+K is the emergency chord, and AltGr arrives as
//!   Ctrl+Alt, so a Ctrl+Alt shortcut would take characters such as @ and € on many layouts;
//! - the Windows key is refused: its shortcuts are reserved for Windows [docs: `RegisterHotKey`];
//! - F12 is refused: it is reserved for the debugger [docs: `RegisterHotKey`].
//!
//! The text form is `Ctrl+Shift+K`: modifiers in the order Ctrl, Alt, Shift, then the key.
//! Parsing ignores case and modifier order. Only the chord is ever stored, never typed keys.

use std::fmt;

/// The default start shortcut (§28).
pub const DEFAULT_SHORTCUT: Shortcut = Shortcut {
    modifier: Modifier::Ctrl,
    key: Key::Letter(b'K'),
};

/// The modifier held with Shift.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modifier {
    Ctrl,
    Alt,
}

/// The shortcut's key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// `A`-`Z`, as an uppercase ASCII letter.
    Letter(u8),
    /// `0`-`9`, as an ASCII digit.
    Digit(u8),
    /// F1-F24 except F12.
    Function(u8),
}

/// A start shortcut that passed every rule: Ctrl+Shift or Alt+Shift, plus a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shortcut {
    modifier: Modifier,
    key: Key,
}

/// Why a shortcut was refused. Each has a message key in `locales/en/strings.json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShortcutError {
    /// Empty, or a part that isn't a known modifier or key.
    Unreadable,
    /// Modifiers only.
    NoKey,
    /// More than one non-modifier key.
    TwoKeys,
    /// A key other than a letter, digit or F-key.
    UnsupportedKey,
    /// F12, reserved for the debugger.
    ReservedKey,
    /// Ctrl+Alt+K, the emergency chord.
    Emergency,
    /// Ctrl and Alt together (AltGr).
    CtrlAlt,
    /// The Windows key.
    Win,
    /// Not Shift plus Ctrl or Alt.
    NeedsShift,
}

impl ShortcutError {
    /// The message key that explains this refusal to the user.
    pub fn message_key(self) -> &'static str {
        match self {
            ShortcutError::Unreadable => "shortcut.error.unreadable",
            ShortcutError::NoKey => "shortcut.error.noKey",
            ShortcutError::TwoKeys => "shortcut.error.twoKeys",
            ShortcutError::UnsupportedKey => "shortcut.error.unsupportedKey",
            ShortcutError::ReservedKey => "shortcut.error.reservedKey",
            ShortcutError::Emergency => "shortcut.error.emergency",
            ShortcutError::CtrlAlt => "shortcut.error.ctrlAlt",
            ShortcutError::Win => "shortcut.error.win",
            ShortcutError::NeedsShift => "shortcut.error.needsShift",
        }
    }
}

impl fmt::Display for ShortcutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ShortcutError::Unreadable => "not a shortcut",
            ShortcutError::NoKey => "no key besides the modifiers",
            ShortcutError::TwoKeys => "more than one key besides the modifiers",
            ShortcutError::UnsupportedKey => "not a letter, digit or F-key",
            ShortcutError::ReservedKey => "F12 is reserved for the debugger",
            ShortcutError::Emergency => "Ctrl+Alt+K is the emergency shortcut",
            ShortcutError::CtrlAlt => "Ctrl+Alt is AltGr on many layouts",
            ShortcutError::Win => "Windows key shortcuts are reserved for Windows",
            ShortcutError::NeedsShift => "needs Shift and Ctrl or Alt",
        })
    }
}

impl std::error::Error for ShortcutError {}

impl Key {
    fn parse(part: &str) -> Result<Key, ShortcutError> {
        let upper = part.to_ascii_uppercase();
        let bytes = upper.as_bytes();
        match bytes {
            [c @ b'A'..=b'Z'] => Ok(Key::Letter(*c)),
            [c @ b'0'..=b'9'] => Ok(Key::Digit(*c)),
            [b'F', rest @ ..]
                if !rest.is_empty()
                    && !rest.starts_with(b"0")
                    && rest.iter().all(u8::is_ascii_digit) =>
            {
                match upper[1..].parse::<u8>() {
                    Ok(12) => Err(ShortcutError::ReservedKey),
                    Ok(n @ 1..=24) => Ok(Key::Function(n)),
                    _ => Err(ShortcutError::UnsupportedKey),
                }
            }
            _ => Err(ShortcutError::UnsupportedKey),
        }
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Key::Letter(c) | Key::Digit(c) => write!(f, "{}", char::from(c)),
            Key::Function(n) => write!(f, "F{n}"),
        }
    }
}

impl Shortcut {
    /// Reads `Ctrl+Shift+K` and the like, and applies the rules. Case and the order of the parts
    /// don't matter; spaces around the parts are ignored. `Win`, `Windows`, `Meta` and `Super`
    /// name the Windows key, and `Control` names Ctrl.
    pub fn parse(text: &str) -> Result<Shortcut, ShortcutError> {
        let (mut ctrl, mut alt, mut shift, mut win) = (false, false, false, false);
        let mut key: Option<Result<Key, ShortcutError>> = None;
        for part in text.split('+').map(str::trim) {
            if part.is_empty() {
                return Err(ShortcutError::Unreadable);
            }
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => ctrl = true,
                "alt" => alt = true,
                "shift" => shift = true,
                "win" | "windows" | "meta" | "super" => win = true,
                _ if key.is_some() => return Err(ShortcutError::TwoKeys),
                _ => key = Some(Key::parse(part)),
            }
        }
        let key = match key {
            None => return Err(ShortcutError::NoKey),
            Some(key) => key,
        };
        if win {
            return Err(ShortcutError::Win);
        }
        if ctrl && alt {
            return Err(if key == Ok(Key::Letter(b'K')) && !shift {
                ShortcutError::Emergency
            } else {
                ShortcutError::CtrlAlt
            });
        }
        let key = key?;
        let modifier = match (ctrl, alt, shift) {
            (true, false, true) => Modifier::Ctrl,
            (false, true, true) => Modifier::Alt,
            _ => return Err(ShortcutError::NeedsShift),
        };
        Ok(Shortcut { modifier, key })
    }

    /// Ctrl or Alt; Shift is always held too.
    pub fn modifier(self) -> Modifier {
        self.modifier
    }

    /// The key pressed with the modifiers.
    pub fn key(self) -> Key {
        self.key
    }

    /// The parts as shown on key caps: `["Ctrl", "Shift", "K"]`.
    pub fn parts(self) -> [String; 3] {
        let modifier = match self.modifier {
            Modifier::Ctrl => "Ctrl",
            Modifier::Alt => "Alt",
        };
        [
            modifier.to_owned(),
            "Shift".to_owned(),
            self.key.to_string(),
        ]
    }
}

impl fmt::Display for Shortcut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [modifier, shift, key] = self.parts();
        write!(f, "{modifier}+{shift}+{key}")
    }
}

/// What the start shortcut locks (§28).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShortcutAction {
    /// The keyboard only.
    Keyboard,
    /// The remembered targets, as the dashboard last locked them (§28's default).
    #[default]
    Selected,
}

impl ShortcutAction {
    /// The settings file's value: `keyboard` or `selected`.
    pub fn as_str(self) -> &'static str {
        match self {
            ShortcutAction::Keyboard => "keyboard",
            ShortcutAction::Selected => "selected",
        }
    }

    /// Reads [`ShortcutAction::as_str`]'s values; anything else is `None`.
    pub fn parse(text: &str) -> Option<ShortcutAction> {
        match text {
            "keyboard" => Some(ShortcutAction::Keyboard),
            "selected" => Some(ShortcutAction::Selected),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(text: &str) -> String {
        Shortcut::parse(text).unwrap().to_string()
    }

    fn err(text: &str) -> ShortcutError {
        Shortcut::parse(text).unwrap_err()
    }

    #[test]
    fn default_is_ctrl_shift_k() {
        assert_eq!(DEFAULT_SHORTCUT.to_string(), "Ctrl+Shift+K");
        assert_eq!(Shortcut::parse("Ctrl+Shift+K"), Ok(DEFAULT_SHORTCUT));
    }

    #[test]
    fn parsing_ignores_case_order_and_spaces() {
        assert_eq!(ok("shift+ctrl+k"), "Ctrl+Shift+K");
        assert_eq!(ok(" Control + SHIFT + k "), "Ctrl+Shift+K");
        assert_eq!(ok("Shift+Alt+7"), "Alt+Shift+7");
        assert_eq!(ok("alt+shift+f5"), "Alt+Shift+F5");
    }

    #[test]
    fn accepted_keys() {
        for key in ["A", "Z", "0", "9", "F1", "F11", "F13", "F24"] {
            assert_eq!(
                ok(&format!("Ctrl+Shift+{key}")),
                format!("Ctrl+Shift+{key}")
            );
        }
    }

    #[test]
    fn text_form_round_trips() {
        for text in ["Ctrl+Shift+K", "Alt+Shift+1", "Ctrl+Shift+F24"] {
            let shortcut = Shortcut::parse(text).unwrap();
            assert_eq!(Shortcut::parse(&shortcut.to_string()), Ok(shortcut));
        }
    }

    #[test]
    fn parts_are_the_key_caps() {
        assert_eq!(DEFAULT_SHORTCUT.parts(), ["Ctrl", "Shift", "K"]);
        assert_eq!(
            Shortcut::parse("Alt+Shift+F3").unwrap().parts(),
            ["Alt", "Shift", "F3"]
        );
    }

    #[test]
    fn unsupported_and_reserved_keys() {
        assert_eq!(err("Ctrl+Shift+F12"), ShortcutError::ReservedKey);
        for key in [
            "F0", "F25", "F01", "Space", "Tab", "Esc", "/", "Enter", "KeyK", "Ä",
        ] {
            assert_eq!(
                err(&format!("Ctrl+Shift+{key}")),
                ShortcutError::UnsupportedKey,
                "{key}"
            );
        }
    }

    #[test]
    fn one_modifier_is_not_enough() {
        for text in ["Ctrl+C", "Alt+F", "Shift+K", "K", "Ctrl+F5", "Alt+1"] {
            assert_eq!(err(text), ShortcutError::NeedsShift, "{text}");
        }
    }

    #[test]
    fn the_emergency_chord_is_refused_by_name() {
        assert_eq!(err("Ctrl+Alt+K"), ShortcutError::Emergency);
        assert_eq!(err("alt+ctrl+k"), ShortcutError::Emergency);
    }

    #[test]
    fn ctrl_alt_is_refused_in_every_form() {
        for text in [
            "Ctrl+Alt+Q",
            "Ctrl+Alt+Shift+K",
            "Ctrl+Alt+Shift+2",
            "Ctrl+Alt+F5",
        ] {
            assert_eq!(err(text), ShortcutError::CtrlAlt, "{text}");
        }
    }

    #[test]
    fn the_windows_key_is_refused() {
        for text in [
            "Win+Shift+K",
            "Ctrl+Win+Shift+K",
            "Meta+Shift+K",
            "Super+Alt+Shift+1",
        ] {
            assert_eq!(err(text), ShortcutError::Win, "{text}");
        }
    }

    #[test]
    fn malformed_text_is_refused() {
        assert_eq!(err(""), ShortcutError::Unreadable);
        assert_eq!(err("Ctrl++K"), ShortcutError::Unreadable);
        assert_eq!(err("Ctrl+Shift+"), ShortcutError::Unreadable);
        assert_eq!(err("Ctrl+Shift"), ShortcutError::NoKey);
        assert_eq!(err("Ctrl+Shift+K+L"), ShortcutError::TwoKeys);
    }

    #[test]
    fn every_error_has_a_message_key() {
        for e in [
            ShortcutError::Unreadable,
            ShortcutError::NoKey,
            ShortcutError::TwoKeys,
            ShortcutError::UnsupportedKey,
            ShortcutError::ReservedKey,
            ShortcutError::Emergency,
            ShortcutError::CtrlAlt,
            ShortcutError::Win,
            ShortcutError::NeedsShift,
        ] {
            assert!(e.message_key().starts_with("shortcut.error."));
        }
    }

    #[test]
    fn action_round_trips_and_defaults_to_selected() {
        assert_eq!(ShortcutAction::default(), ShortcutAction::Selected);
        for action in [ShortcutAction::Keyboard, ShortcutAction::Selected] {
            assert_eq!(ShortcutAction::parse(action.as_str()), Some(action));
        }
        assert_eq!(ShortcutAction::parse("mouse"), None);
    }
}
