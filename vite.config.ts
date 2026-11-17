import { fileURLToPath } from "node:url";

import vue from "@vitejs/plugin-vue";
import { defineConfig } from "vite";

const root = (file: string) => fileURLToPath(new URL(file, import.meta.url));

// Tauri 开发模式固定使用 1420 端口，见 src-tauri/tauri.conf.json 的 devUrl
export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**", "**/tools/**"] },
  },
  build: {
    // WebView2 基于 Chromium，可以直接用较新的语法
    target: "chrome110",
    outDir: "dist",
    emptyOutDir: true,
    rollupOptions: {
      input: {
        main: root("./index.html"),
        overlay: root("./overlay.html"),
      },
    },
  },
});
