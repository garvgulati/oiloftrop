import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e/personal",
  testMatch: "**/*.spec.ts",
  workers: 1,
  timeout: 120000,
  expect: { timeout: 15000 },
  reporter: "list",
  outputDir: "test-results/personal",
  use: {
    ...devices["Desktop Chrome"],
    channel: "chrome",
    baseURL: "https://127.0.0.1:18489",
    ignoreHTTPSErrors: true, // This disposable context only; no OS trust change.
    locale: "en-US",
    trace: "retain-on-failure",
  },
  webServer: {
    command: '"' + process.execPath + '" e2e/personal/start-personal-host.mjs',
    url: "https://127.0.0.1:18489/api/v1/healthz",
    ignoreHTTPSErrors: true,
    reuseExistingServer: false,
    timeout: 90000,
  },
});
