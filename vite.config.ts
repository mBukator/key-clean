import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

const host = process.env.TAURI_DEV_HOST;

// https://v2.tauri.app/start/frontend/vite/
export default defineConfig({
    plugins: [react(), tailwindcss()],
    // Keep Rust errors visible in the terminal.
    clearScreen: false,
    server: {
        // Tauri expects a fixed port and fails if it is taken.
        port: 1420,
        strictPort: true,
        host: host ?? false,
        hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
        watch: {
            // Tauri watches the Rust side itself.
            ignored: ["**/src-tauri/**"],
        },
    },
    envPrefix: ["VITE_", "TAURI_ENV_*"],
    build: {
        // WebView2 on Windows 10/11 is Chromium-based.
        target: "chrome105",
        minify: !process.env.TAURI_ENV_DEBUG,
        sourcemap: Boolean(process.env.TAURI_ENV_DEBUG),
    },
});
