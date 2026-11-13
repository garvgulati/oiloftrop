import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e/addon-sandbox",
  fullyParallel: true,
  retries: process.env.CI ? 2 : 0,
  reporter: "line",
  use: {
    baseURL: process.env.WF_ADDON_SANDBOX_BASE_URL || "http://127.0.0.1:4174",
    trace: "on-first-retry",
  },
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] } },
    { name: "firefox", use: { ...devices["Desktop Firefox"] } },
    { name: "webkit", use: { ...devices["Desktop Safari"] } },
  ],
});
