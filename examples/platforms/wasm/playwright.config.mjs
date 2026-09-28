import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./browser",
  testMatch: "*.spec.mjs",
  outputDir: "./target/playwright",
  fullyParallel: true,
  projects: ["chromium", "firefox", "webkit"].map((browserName) => ({ name: browserName, use: { browserName } })),
  webServer: [
    {
      command: "python3 -m http.server 4175 --bind 127.0.0.1 --directory ../../..",
      url: "http://127.0.0.1:4175/examples/platforms/wasm/browser/raw.html",
    },
    {
      command: "npm run build:browser && vite preview",
      url: "http://127.0.0.1:4176",
    },
  ],
});
