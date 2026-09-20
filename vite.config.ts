import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Vite 配置：Tauri 需要固定端口 1420，并忽略 src-tauri 目录的热更新
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // 忽略 Rust 后端目录，避免 Tauri 编译产物触发前端热更新
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    target: "chrome110",
    minify: "esbuild",
    sourcemap: false,
  },
});
