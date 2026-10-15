import { t } from "../localization/t";

/** A key cap of the start shortcut as shown: modifier names from the strings file. */
function keyLabel(key: string): string {
    switch (key) {
        case "Ctrl":
            return t("overlay.key.ctrl");
        case "Alt":
            return t("overlay.key.alt");
        case "Shift":
            return t("shortcut.key.shift");
        default:
            return key;
    }
}

/** The start shortcut's key caps (`["Ctrl", "Shift", "K"]`) as one line: "Ctrl + Shift + K". */
export function shortcutLabel(keys: readonly string[]): string {
    return keys.map(keyLabel).join(` ${t("shortcut.key.plus")} `);
}
