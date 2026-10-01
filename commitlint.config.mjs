/** @type {import("@commitlint/types").UserConfig} */
export default {
    extends: ["@commitlint/config-conventional"],
    rules: {
        "scope-enum": [
            2,
            "always",
            ["engine", "core", "app", "ui", "i18n", "docs", "ci", "repo", "deps"],
        ],
        "scope-empty": [2, "never"],
        "scope-max-length": [2, "always", 20],
        "header-max-length": [2, "always", 72],
        "subject-full-stop": [2, "never", "."],
        "body-empty": [2, "never"],
    },
};
