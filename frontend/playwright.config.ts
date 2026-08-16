import { defineConfig, devices } from "@playwright/test";
import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";

const require = createRequire(import.meta.url);
const viteCli = resolve(dirname(require.resolve("vite")), "../../bin/vite.js");

const baseURL = process.env.RESEARCH_E2E_FRONTEND_URL ?? "http://127.0.0.1:5173";

export default defineConfig({
  testDir: "../tests/e2e",
  timeout: 45_000,
  expect: { timeout: 10_000 },
  fullyParallel: false,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 2 : 0,
  reporter: [["list"], ["html", { open: "never", outputFolder: "playwright-report" }]],
  use: {
    baseURL,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    video: "retain-on-failure",
  },
  webServer: process.env.RESEARCH_E2E_FRONTEND_URL
    ? undefined
    : {
        command: `${process.execPath} ${viteCli} --host 127.0.0.1`,
        url: baseURL,
        reuseExistingServer: !process.env.CI,
        timeout: 120_000,
      },
  projects: [
    {
      name: "chromium-desktop",
      use: { ...devices["Desktop Chrome"] },
    },
    {
      name: "firefox-desktop",
      use: { ...devices["Desktop Firefox"] },
    },
    {
      name: "webkit-desktop",
      use: { ...devices["Desktop Safari"] },
    },
    {
      name: "chromium-mobile",
      use: { ...devices["iPhone 13"] },
    },
  ],
});
