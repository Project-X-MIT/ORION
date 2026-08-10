import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/browser",
  testMatch: "**/*.e2e.ts",
  fullyParallel: true,
  reporter: "line",
  use: {
    baseURL: "http://127.0.0.1:5173",
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
  webServer: {
    command: "npm run dev --workspaces=false -- --host 127.0.0.1",
    reuseExistingServer: true,
    timeout: 30_000,
    url: "http://127.0.0.1:5173/component-examples.html",
  },
});
