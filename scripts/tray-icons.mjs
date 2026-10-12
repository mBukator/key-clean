// Rasterizes the tray icon sources (assets/tray/*.svg) to the PNGs the app embeds
// (src-tauri/icons/tray/<variant>-<size>.png). Run it after changing an SVG:
//
//     bun scripts/tray-icons.mjs
//
// The PNGs are committed, so building the app never needs this script or its dev dependency.
// Sizes are the tray icon at 100, 125, 150 and 200 % display scaling (16 px small icon).

import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { Resvg } from "@resvg/resvg-js";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const sources = join(root, "assets", "tray");
const output = join(root, "src-tauri", "icons", "tray");
const variants = ["idle-light", "idle-dark", "locked"];
const sizes = [16, 20, 24, 32];

mkdirSync(output, { recursive: true });
for (const variant of variants) {
    const svg = readFileSync(join(sources, `${variant}.svg`), "utf8");
    for (const size of sizes) {
        const png = new Resvg(svg, { fitTo: { mode: "width", value: size } })
            .render()
            .asPng();
        const file = join(output, `${variant}-${String(size)}.png`);
        writeFileSync(file, png);
        console.log(`${file} (${String(png.length)} bytes)`);
    }
}
