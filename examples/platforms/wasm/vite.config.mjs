import { defineConfig } from "vite";

export default defineConfig({
  root: "browser",
  build: { outDir: "../target/browser", emptyOutDir: true, target: "es2022" },
  preview: { host: "127.0.0.1", port: 4176, strictPort: true },
});
