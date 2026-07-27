import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "path";
import { readFileSync } from "fs";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

const pkg = JSON.parse(readFileSync(resolve(__dirname, "package.json"), "utf-8"));

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react()],

  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },

  // Multi-page build: main app + detached band visualizer window
  build: {
    rollupOptions: {
      input: {
        main: resolve(__dirname, "index.html"),
        band: resolve(__dirname, "band.html"),
      },
      output: {
        // Split the heavy third-party code out of the entry chunk. The window
        // cannot paint until the entry chunk has parsed, so anything that is
        // not needed for the first frame is better off in its own file that
        // the browser can fetch and compile in parallel — or, for the provider
        // SDKs, never fetch at all.
        manualChunks(id) {
          if (!id.includes("node_modules")) return undefined;
          // Monaco is the single largest dependency and is bundled locally so
          // the editor works offline. Its own chunk keeps it from delaying the
          // shell.
          if (id.includes("monaco-editor")) return "monaco";
          // Provider SDKs are imported dynamically (see src/llm.ts); naming
          // them here keeps each in one file instead of scattered across the
          // dynamic-import graph.
          if (
            id.includes("/openai/") ||
            id.includes("@anthropic-ai") ||
            id.includes("@google/genai")
          ) {
            return "llm-providers";
          }
          if (id.includes("/react/") || id.includes("/react-dom/") || id.includes("/scheduler/")) {
            return "react";
          }
          return undefined;
        },
      },
    },
    // Monaco legitimately exceeds the default 500 kB warning threshold. It is
    // read from local disk rather than fetched over a network, and it loads
    // after the shell has painted (see CodeEditorLazy.tsx), so the size does
    // not sit on the critical path.
    chunkSizeWarningLimit: 4000,
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
