import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/browser",
  testMatch: "**/*.e2e.ts",
  fullyParallel: false,
  workers: 1,
  reporter: "line",
  use: {
    ...devices["Desktop Chrome"],
    baseURL: "http://127.0.0.1:5173",
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
  webServer: {
    command: "node ../node_modules/vite/bin/vite.js --host 127.0.0.1",
    reuseExistingServer: true,
    timeout: 120_000,
    url: "http://127.0.0.1:5173/component-examples.html",
  },
});
