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
  test: {
    // Tests sit beside the code they test, as the Rust side does.
    include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
    // No jsdom: what is worth testing here is pure — key parsing, layout
    // geometry, store reducers. A component needing a DOM is a sign the logic
    // should come out of the component first.
    environment: "node",
  },
});
