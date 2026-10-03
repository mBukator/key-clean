// Lints pull request titles in CI. A title has no body, so body-empty is off.
import base from "./commitlint.config.mjs";

/** @type {import("@commitlint/types").UserConfig} */
export default {
    ...base,
    rules: {
        ...base.rules,
        "body-empty": [0],
    },
};
