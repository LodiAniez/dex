import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri points the webview at a fixed dev port, and its CLI output must not be
// cleared by Vite. The Rust side is rebuilt by Tauri, so Vite ignores it.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    // WebView2 is evergreen Chromium, so modern output is safe.
    target: "es2022",
    outDir: "dist",
  },
});
