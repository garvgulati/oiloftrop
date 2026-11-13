import { test, expect } from "@playwright/test";
import { sampleIndiaData } from "../../apps/frontend/src/features/research/india-summary.fixture";

// Synthetic public-source payloads only. The application and profile server are
// real; no owner token, portfolio, paid provider or outside request is used.
test.use({ trace: "off" });
test("Indian recommendation explains financial evidence and clears with the report", async ({ context, page }, info) => {
  const state = await (await context.request.post("/api/v1/profiles/get_profile_state", { data: {} })).json();
  const grant = await (await context.request.post("/api/v1/profiles/unlock_profile", { data: { profileId: state.profiles[0].id } })).json();
  const settings = await context.request.put("/api/v1/settings", { headers: { "x-wf-profile-scope": grant.scopeId }, data: { onboardingCompleted: true, language: "en", baseCurrency: "INR", timezone: "Asia/Kolkata", syncEnabled: false, autoUpdateCheckEnabled: false } });
  expect(settings.ok()).toBe(true);
  const data = sampleIndiaData();
  data.income = { type: "consolidated", time_period: "yearly", units_in: "crore", income_statement: [
    { category: "revenue", history: [{ period: "Mar 2026", value: 120 }, { period: "Mar 2025", value: 100 }] },
    { category: "net_profit", history: [{ period: "Mar 2026", value: 12 }, { period: "Mar 2025", value: 10 }] },
  ] };
  data.cash = { type: "consolidated", time_period: "yearly", units_in: "crore", cash_flow: [{ category: "operating", history: [{ period: "Mar 2026", value: 15 }, { period: "Mar 2025", value: 11 }] }] };
  let analysisCalls = 0;
  await page.route("**/api/v1/research/india/connection", (route) => route.fulfill({ json: { configured: true, expiresOn: "2027-01-01" } }));
  await page.route("**/api/v1/research/india/search", (route) => route.fulfill({ json: { listings: [data.listing], retrievedAt: data.retrievedAt } }));
  await page.route("**/api/v1/research/india/analyse", (route) => { analysisCalls++; return route.fulfill({ json: data }); });
  await page.clock.setFixedTime(new Date("2026-10-26T00:00:00Z"));
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/research");
  await page.getByLabel("Market for stock research").selectOption("india");
  expect(analysisCalls).toBe(0);
  await page.getByLabel("Indian company name, symbol or ISIN").fill("TEST");
  await page.getByRole("button", { name: "Search India", exact: true }).click();
  await page.getByLabel("Verified company listing").selectOption(data.listing.key);
  await page.getByRole("button", { name: "Analyse India", exact: true }).click();
  const panel = page.getByRole("region", { name: "Research recommendation and explanation — English beta" });
  await expect(panel.getByText("Watchlist for valuation review", { exact: true })).toBeVisible();
  await expect(panel).toContainText("20.0% change");
  await expect(panel).toContainText("No dated share price");
  await expect(panel).toContainText("₹ crore");
  await panel.scrollIntoViewIfNeeded();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.screenshot({ path: info.outputPath("recommendation-india-mobile.png") });
  await page.setViewportSize({ width: 1280, height: 900 });
  // The desktop layout remounts the route, which deliberately discards the
  // temporary mobile report. Start another explicit lookup for this layout.
  await expect(panel).toHaveCount(0);
  await page.getByLabel("Market for stock research").selectOption("india");
  await page.getByLabel("Indian company name, symbol or ISIN").fill("TEST");
  await page.getByRole("button", { name: "Search India", exact: true }).click();
  await page.getByLabel("Verified company listing").selectOption(data.listing.key);
  await page.getByRole("button", { name: "Analyse India", exact: true }).click();
  await expect(panel.getByText("Watchlist for valuation review", { exact: true })).toBeVisible();
  await panel.scrollIntoViewIfNeeded();
  await page.screenshot({ path: info.outputPath("recommendation-india-desktop.png") });
  await page.getByRole("button", { name: "Clear India result", exact: true }).click();
  await expect(panel).toHaveCount(0);
  await page.reload();
  await expect(panel).toHaveCount(0);
  expect(analysisCalls).toBe(2);
});
