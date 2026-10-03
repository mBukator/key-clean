//! Rust-side string lookup for user-facing text (tray, notifications).
//!
//! Reads the same `locales/en/strings.json` the UI uses, so there is a single source
//! for every user-facing string.

use std::collections::HashMap;
use std::sync::OnceLock;

const EN_STRINGS: &str = include_str!("../../locales/en/strings.json");

fn strings() -> &'static HashMap<String, String> {
    static STRINGS: OnceLock<HashMap<String, String>> = OnceLock::new();
    STRINGS.get_or_init(|| serde_json::from_str(EN_STRINGS).unwrap_or_default())
}

/// Returns the English string for `key`, or the key itself when it is missing.
pub fn t(key: &str) -> String {
    strings()
        .get(key)
        .cloned()
        .unwrap_or_else(|| key.to_owned())
}

/// Like [`t`], replacing `{name}` placeholders with the matching value from `vars`.
/// Placeholders without a value are left as-is, matching the UI's `t()`.
pub fn t_with(key: &str, vars: &[(&str, &str)]) -> String {
    vars.iter().fold(t(key), |text, (name, value)| {
        text.replace(&format!("{{{name}}}"), value)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_are_replaced() {
        // Unknown keys fall back to the key itself, which makes a convenient template here.
        assert_eq!(
            t_with("{n} of {total}", &[("n", "2"), ("total", "5")]),
            "2 of 5"
        );
        assert_eq!(t_with("{n} of {total}", &[("n", "2")]), "2 of {total}");
        assert_eq!(t_with("app.title", &[("n", "2")]), "KeyClean");
    }

    #[test]
    fn strings_json_parses() {
        let parsed: Result<HashMap<String, String>, _> = serde_json::from_str(EN_STRINGS);
        assert!(parsed.is_ok());
    }

    #[test]
    fn every_value_is_non_empty() {
        let parsed: HashMap<String, String> = serde_json::from_str(EN_STRINGS).unwrap();
        assert!(!parsed.is_empty());
        for (key, value) in &parsed {
            assert!(!value.trim().is_empty(), "empty string for key {key}");
        }
    }

    #[test]
    fn known_key_resolves() {
        assert_eq!(t("app.title"), "KeyClean");
    }

    #[test]
    fn unknown_key_returns_key() {
        assert_eq!(t("does.not.exist"), "does.not.exist");
    }
}
