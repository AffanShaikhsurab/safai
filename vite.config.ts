/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

// Tauri expects a fixed dev port matching tauri.conf.json > build.devUrl
// (http://localhost:1420) and a static bundle in ../dist (frontendDist).
export default defineConfig({
  plugins: [solid()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // Don't reload the frontend when the Rust backend changes.
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    target: "esnext",
    outDir: "dist",
  },
  test: {
    // Unit tests cover pure logic (`src/lib/*.test.ts`). They run in `node`
    // rather than a DOM: nothing under test touches the document, and keeping
    // it that way is a useful pressure to leave logic extractable instead of
    // buried in components.
    environment: "node",
    include: ["src/**/*.test.ts"],
    // The dev server and the Rust target tree are not test sources.
    exclude: ["node_modules/**", "dist/**", "target/**", "src-tauri/**"],
    // Solid ships browser + server builds; tests import plain modules, so
    // resolve the standard (browser) condition to match production behaviour.
    server: { deps: { inline: [/solid-js/] } },
  },
});
