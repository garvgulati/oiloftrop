import { defineConfig } from "@playwright/test";
import { resolve } from "node:path";
import profiles from "./playwright.profiles.config";

// Follow the repository's disposable-profile production-build workflow. No
// developer .env, installed trial data, owner keys or paid connections are used.
const server = profiles.webServer;
if (!server || Array.isArray(server)) throw new Error("Expected one isolated profile server");
export default defineConfig({
  ...profiles,
  testDir: "./e2e/research",
  timeout: 120000,
  expect: { timeout: 15000 },
  use: { ...profiles.use, actionTimeout: 15000 },
  outputDir: "test-results/research",
  webServer: {
    ...server,
    command: '"' + resolve("target/debug", process.platform === "win32" ? "wealthfolio-server.exe" : "wealthfolio-server") + '"',
    env: {
      ...server.env,
      WEALTHFOLIO_E2E: "1",
      WF_RESEARCH_PUBLIC_REQUESTS_ENABLED:
        process.env.WF_RESEARCH_LIVE_TEST === "1" ? "true" : "false",
      WEALTHFOLIO_FIXTURE_DIR: resolve("e2e/fixtures/quotes"),
    },
  },
});
