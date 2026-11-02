//! Checks on the NSIS installer (M11, ADR 0019), which lives in `src-tauri/windows/`.
//!
//! The installer writes the start-at-sign-in value itself, with its own copy of Tauri's
//! template, so these tests keep it in step with `keyclean_core::startup` (ADR 0018),
//! `locales/en/strings.json`, `tauri.conf.json` and the Tauri CLI the template came from.

use keyclean_core::startup::{AUTOSTART_FLAG, MAX_RUN_COMMAND, RUN_VALUE, run_command};
use serde_json::Value;

const HOOKS: &str = include_str!("../windows/keyclean.nsh");
const TEMPLATE: &str = include_str!("../windows/installer.nsi");
const CONF: &str = include_str!("../tauri.conf.json");
const STRINGS: &str = include_str!("../../locales/en/strings.json");
const PACKAGE_JSON: &str = include_str!("../../package.json");
const BUN_LOCK: &str = include_str!("../../bun.lock");
const AUTOSTART_RS: &str = include_str!("../../crates/keyclean-win/src/autostart.rs");

/// The `@tauri-apps/cli` version `installer.nsi` was copied from.
const TEMPLATE_CLI: &str = "2.12.1";

/// The value of `!define <name> <value>` in keyclean.nsh, without quotes.
fn define(name: &str) -> &'static str {
    let prefix = format!("!define {name} ");
    let line = HOOKS
        .lines()
        .find(|l| l.starts_with(&prefix))
        .unwrap_or_else(|| panic!("keyclean.nsh doesn't define {name}"));
    line[prefix.len()..].trim().trim_matches('"')
}

/// The single-quoted NSIS string on the first keyclean.nsh line that contains `marker`.
fn quoted_after(marker: &str) -> &'static str {
    let line = HOOKS
        .lines()
        .find(|l| l.contains(marker))
        .unwrap_or_else(|| panic!("keyclean.nsh has no line with {marker}"));
    let start = line.find('\'').expect("an opening quote") + 1;
    let end = line.rfind('\'').expect("a closing quote");
    &line[start..end]
}

/// Expands the NSIS variables the installer's command line uses (`${dir}` is the folder
/// parameter of `KEYCLEAN_RUN_IS_OURS`).
fn expand(nsis: &str, install_dir: &str) -> String {
    nsis.replace("$INSTDIR", install_dir)
        .replace("${dir}", install_dir)
        .replace("${MAINBINARYNAME}", env!("CARGO_PKG_NAME"))
        .replace(
            "${KEYCLEAN_AUTOSTART_FLAG}",
            define("KEYCLEAN_AUTOSTART_FLAG"),
        )
}

fn conf() -> Value {
    serde_json::from_str(CONF).expect("tauri.conf.json parses")
}

#[test]
fn the_hooks_use_the_same_registry_value_as_the_app() {
    assert_eq!(define("KEYCLEAN_RUN_VALUE"), RUN_VALUE);
    assert_eq!(define("KEYCLEAN_AUTOSTART_FLAG"), AUTOSTART_FLAG);
    assert_eq!(
        define("KEYCLEAN_MAX_RUN_COMMAND"),
        MAX_RUN_COMMAND.to_string()
    );
    for key in ["KEYCLEAN_RUN_KEY", "KEYCLEAN_APPROVED_KEY"] {
        let path = define(key);
        assert!(
            AUTOSTART_RS.contains(&format!("r\"{path}\"")),
            "autostart.rs doesn't use {key} = {path}"
        );
    }
}

#[test]
fn the_installer_writes_what_run_command_builds() {
    // The installed exe is the cargo binary, not renamed (no `mainBinaryName`).
    assert!(conf().get("mainBinaryName").is_none());
    assert_eq!(env!("CARGO_PKG_NAME"), "keyclean");
    let written = quoted_after("${KEYCLEAN_AUTOSTART_FLAG}'");
    let ours = quoted_after("StrCpy $KeyCleanTmp2 '");
    // A folder with a space, as under a user name like "Ann Lee".
    for dir in [
        r"C:\Users\Ann Lee\AppData\Local\KeyClean",
        r"C:\Users\max\AppData\Local\KeyClean",
    ] {
        let exe = format!(r"{dir}\keyclean.exe");
        let expected = run_command(&exe).expect("a usable path");
        assert_eq!(expand(written, dir), expected);
        // The "is it ours" check matches the value the installer writes, and the one the
        // app's Settings switch writes.
        assert!(expected.starts_with(&expand(ours, dir)));
    }
}

#[test]
fn the_installer_strings_match_strings_json() {
    let strings: Value = serde_json::from_str(STRINGS).expect("strings.json parses");
    let strings = strings.as_object().expect("an object");
    let mut seen = Vec::new();
    for line in HOOKS.lines().map(str::trim) {
        let Some(rest) = line.strip_prefix("LangString ") else {
            continue;
        };
        let (id, rest) = rest.split_once(' ').expect("an id");
        let text = rest
            .strip_prefix("${LANG_ENGLISH} ")
            .expect("English")
            .trim_matches('"');
        let name = id.strip_prefix("keyclean").expect("a keyclean id");
        let key = format!("installer.{}{}", name[..1].to_lowercase(), &name[1..]);
        assert_eq!(
            strings.get(&key).and_then(Value::as_str),
            Some(text),
            "{id} differs from {key}"
        );
        seen.push(key);
    }
    for (key, text) in strings.iter().filter(|(k, _)| k.starts_with("installer.")) {
        assert!(
            seen.contains(key),
            "{key} has no LangString in keyclean.nsh"
        );
        // NSIS would read these as variables or the end of the string.
        let text = text.as_str().expect("a string");
        assert!(
            !text.contains(['$', '"', '`']),
            "{key} has a character NSIS reads"
        );
    }
}

#[test]
fn the_template_matches_the_tauri_cli() {
    assert!(
        BUN_LOCK.contains(&format!(
            "\"@tauri-apps/cli\": [\"@tauri-apps/cli@{TEMPLATE_CLI}\""
        )),
        "@tauri-apps/cli changed: copy its NSIS template to src-tauri/windows/installer.nsi, \
         apply the KEYCLEAN: changes again and update TEMPLATE_CLI (ADR 0019)"
    );
    assert!(PACKAGE_JSON.contains(&format!("\"@tauri-apps/cli\": \"{TEMPLATE_CLI}\"")));
    assert!(TEMPLATE.contains(&format!("tauri-cli-v{TEMPLATE_CLI}")));
}

#[test]
fn the_template_only_calls_what_the_hooks_define() {
    let words = |text: &'static str, before: &'static str| {
        text.lines().filter_map(move |l| {
            let rest = &l[l.find(before)? + before.len()..];
            rest.split_whitespace().next()
        })
    };
    let mut calls = 0;
    for name in words(TEMPLATE, "!insertmacro ").filter(|n| n.starts_with("KEYCLEAN_")) {
        assert!(
            HOOKS.contains(&format!("!macro {name}")),
            "{name} isn't defined"
        );
        calls += 1;
    }
    for before in [
        "Call ",
        "MUI_PAGE_CUSTOMFUNCTION_SHOW ",
        "MUI_PAGE_CUSTOMFUNCTION_LEAVE ",
    ] {
        for name in words(TEMPLATE, before).filter(|n| n.starts_with("KeyClean")) {
            assert!(
                HOOKS.contains(&format!("Function {name}")),
                "{name} isn't defined"
            );
            calls += 1;
        }
    }
    // KEYCLEAN_DEFINITIONS, KEYCLEAN_REMOVE_STARTUP, the startup default and the checkbox's two
    // page functions.
    assert_eq!(calls, 5);
}

#[test]
fn the_hooks_have_the_names_the_template_calls() {
    // The template calls each hook only `!ifmacrodef`, so a misspelled one would build and
    // silently skip closing KeyClean or writing the startup entry.
    for hook in [
        "NSIS_HOOK_PREINSTALL",
        "NSIS_HOOK_POSTINSTALL",
        "NSIS_HOOK_PREUNINSTALL",
    ] {
        assert!(
            TEMPLATE.contains(&format!("!ifmacrodef {hook}\n")),
            "{hook}"
        );
        assert!(HOOKS.contains(&format!("!macro {hook}\n")), "{hook}");
    }
    assert!(TEMPLATE.contains("!include \"{{installer_hooks}}\""));
}

#[test]
fn every_installer_string_is_defined() {
    // A `$(name)` with no LangString shows as empty text, with only a build warning.
    for text in [HOOKS, TEMPLATE] {
        for (at, _) in text.match_indices("$(keyclean") {
            let rest = &text[at + 2..];
            let name = &rest[..rest.find(')').expect("a closing parenthesis")];
            assert!(
                HOOKS.contains(&format!("LangString {name} ")),
                "{name} has no LangString"
            );
        }
    }
}

#[test]
fn the_bundle_is_a_current_user_nsis_installer() {
    let conf = conf();
    let bundle = &conf["bundle"];
    assert_eq!(bundle["active"], true);
    assert_eq!(bundle["targets"], "nsis");
    let windows = &bundle["windows"];
    assert_eq!(
        windows["webviewInstallMode"]["type"],
        "downloadBootstrapper"
    );
    let nsis = &windows["nsis"];
    // No elevation (ADR 0005): the template asks for admin rights only for perMachine.
    assert_eq!(nsis["installMode"], "currentUser");
    assert_eq!(nsis["template"], "windows/installer.nsi");
    assert_eq!(nsis["installerHooks"], "windows/keyclean.nsh");
    // The single-instance window keyclean.nsh closes is named after the identifier.
    assert!(HOOKS.contains("\"${BUNDLEID}-sic\""));
    assert_eq!(conf["identifier"], "io.github.mbukator.keyclean");
}
