import strings from "../../../locales/en/strings.json";

export type StringKey = keyof typeof strings;

/**
 * Returns the English string for `key`, replacing `{name}` placeholders with `vars`.
 * Unknown placeholders are left as-is.
 */
export function t(key: StringKey, vars?: Record<string, string | number>): string {
    const template: string = strings[key];
    if (!vars) {
        return template;
    }
    return template.replace(/\{(\w+)\}/g, (match, name: string) => {
        const value = vars[name];
        return value === undefined ? match : String(value);
    });
}
