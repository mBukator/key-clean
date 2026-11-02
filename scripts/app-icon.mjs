// Builds the app icon (src-tauri/icons/icon.ico) and the bundle PNGs from the brand sources:
//
// - docs/design/brand/icon-draft.png, Max's satin-black keycap, for 32 px and up;
// - assets/icon/flat-<size>.svg, the flat keycap, for 16, 20 and 24 px, where the 3D key turns
//   into a blob.
//
// Run it after changing a source:
//
//     bun scripts/app-icon.mjs
//
// The outputs are committed, so building the app never needs this script or its dev dependency.
// Entries below 256 px are 32-bit bitmaps and the 256 px entry is a PNG: the usual .ico layout,
// which Explorer, the taskbar and the installer all read.

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { Resvg } from "@resvg/resvg-js";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const draft = join(root, "docs", "design", "brand", "icon-draft.png");
const flatSources = join(root, "assets", "icon");
const output = join(root, "src-tauri", "icons");

// 16-48 px cover Explorer and the taskbar at 100-200 % scaling; 64 and 256 px the large views.
const icoSizes = [16, 20, 24, 32, 40, 48, 64, 256];
const flatSizes = new Set([16, 20, 24]);
const pngFiles = { "32x32.png": 32, "128x128.png": 128, "128x128@2x.png": 256 };

const draftPng = readFileSync(draft);

/** Renders `svg` at `size` px wide. */
function render(svg, size) {
    return new Resvg(svg, { fitTo: { mode: "width", value: size } }).render();
}

/** The PNG's width, read from its IHDR chunk. */
function pngWidth(png) {
    return png.readUInt32BE(16);
}

// The draft's keycap has alpha 253, not 255, so the icon would show a trace of what's behind it.
// Scaling alpha by 255/248 makes it opaque and leaves the glow's soft edge almost as it was.
const opaqueFilter =
    `<filter id="opaque" color-interpolation-filters="sRGB"><feComponentTransfer>` +
    `<feFuncA type="linear" slope="${String(255 / 248)}"/></feComponentTransfer></filter>`;

/**
 * The draft scaled to `size` px. Each step at most halves the image, so the resampling never
 * skips source pixels (one big step from 1254 px aliases the K's edges). The first step also
 * applies `opaqueFilter`.
 */
function fromDraft(size) {
    let png = draftPng;
    let width = pngWidth(png);
    let image = null;
    while (width > size) {
        const next = Math.max(size, Math.ceil(width / 2));
        const filter = png === draftPng ? ' filter="url(#opaque)"' : "";
        const svg =
            `<svg xmlns="http://www.w3.org/2000/svg" width="${String(width)}" ` +
            `height="${String(width)}" viewBox="0 0 ${String(width)} ${String(width)}">` +
            `<defs>${opaqueFilter}</defs>` +
            `<image width="${String(width)}" height="${String(width)}"${filter} ` +
            `href="data:image/png;base64,${png.toString("base64")}"/></svg>`;
        image = render(svg, next);
        png = image.asPng();
        width = next;
    }
    if (image === null) throw new Error(`the draft is smaller than ${String(size)} px`);
    return image;
}

function iconImage(size) {
    if (flatSizes.has(size)) {
        const svg = readFileSync(join(flatSources, `flat-${String(size)}.svg`), "utf8");
        return render(svg, size);
    }
    return fromDraft(size);
}

/**
 * A 32-bit .ico bitmap entry: a BITMAPINFOHEADER with double height (colour plus mask), the
 * BGRA rows bottom-up with straight alpha, then an all-zero AND mask (the alpha does the work).
 */
function bitmapEntry(image) {
    const size = image.width;
    const rgba = image.pixels; // premultiplied
    const maskRow = Math.ceil(size / 32) * 4;
    const out = Buffer.alloc(40 + size * size * 4 + maskRow * size);
    out.writeUInt32LE(40, 0);
    out.writeInt32LE(size, 4);
    out.writeInt32LE(size * 2, 8);
    out.writeUInt16LE(1, 12);
    out.writeUInt16LE(32, 14);
    out.writeUInt32LE(size * size * 4 + maskRow * size, 20);
    let at = 40;
    for (let y = size - 1; y >= 0; y--) {
        for (let x = 0; x < size; x++) {
            const i = (y * size + x) * 4;
            const a = rgba[i + 3];
            const straight = (c) =>
                a === 0 ? 0 : Math.min(255, Math.round((c * 255) / a));
            out[at++] = straight(rgba[i + 2]);
            out[at++] = straight(rgba[i + 1]);
            out[at++] = straight(rgba[i]);
            out[at++] = a;
        }
    }
    return out;
}

function ico(entries) {
    const header = Buffer.alloc(6 + 16 * entries.length);
    header.writeUInt16LE(0, 0);
    header.writeUInt16LE(1, 2);
    header.writeUInt16LE(entries.length, 4);
    let offset = header.length;
    entries.forEach(({ size, data }, n) => {
        const at = 6 + 16 * n;
        header.writeUInt8(size >= 256 ? 0 : size, at);
        header.writeUInt8(size >= 256 ? 0 : size, at + 1);
        header.writeUInt16LE(1, at + 4);
        header.writeUInt16LE(32, at + 6);
        header.writeUInt32LE(data.length, at + 8);
        header.writeUInt32LE(offset, at + 12);
        offset += data.length;
    });
    return Buffer.concat([header, ...entries.map((e) => e.data)]);
}

const entries = icoSizes.map((size) => {
    const image = iconImage(size);
    const data = size >= 256 ? image.asPng() : bitmapEntry(image);
    return { size, data };
});
const icoFile = join(output, "icon.ico");
const icoData = ico(entries);
writeFileSync(icoFile, icoData);
console.log(`${icoFile} (${icoSizes.join(", ")} px; ${String(icoData.length)} bytes)`);

for (const [name, size] of Object.entries(pngFiles)) {
    const png = fromDraft(size).asPng();
    const file = join(output, name);
    writeFileSync(file, png);
    console.log(`${file} (${String(png.length)} bytes)`);
}
