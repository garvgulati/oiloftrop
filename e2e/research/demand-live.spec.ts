import { test, expect } from "@playwright/test";

// Explicit opt-in, disposable installation, no owner credentials and no traces
// containing source responses. Regular tests keep all live lookups disabled.
test.use({ trace: "off" });
test("live SEC analysis is displayed then discarded", async ({ context, page }, info) => {
  test.skip(process.env.WF_RESEARCH_LIVE_TEST !== "1", "Live public-source check is opt-in");
  const stateResponse = await context.request.post("/api/v1/profiles/get_profile_state", {
    data: {},
  });
  const state = await stateResponse.json();
  const grantResponse = await context.request.post("/api/v1/profiles/unlock_profile", {
    data: { profileId: state.profiles[0].id },
  });
  const grant = await grantResponse.json();
  const settings = await context.request.put("/api/v1/settings", {
    headers: { "x-wf-profile-scope": grant.scopeId },
    data: {
      onboardingCompleted: true,
      language: "en",
      baseCurrency: "USD",
      timezone: "Asia/Kolkata",
      syncEnabled: false,
      autoUpdateCheckEnabled: false,
    },
  });
  expect(settings.ok()).toBe(true);
  let companyRequests = 0;
  const externalRequests: string[] = [];
  page.on("request", (request) => {
    const url = new URL(request.url());
    if (url.origin !== "http://127.0.0.1:18388") externalRequests.push(url.origin);
    if (url.pathname === "/api/v1/research/company") {
      companyRequests++;
      expect(request.postDataJSON()).toEqual({ symbol: "MSFT" });
    }
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/research");
  await expect(
    page.getByRole("heading", { name: "Stock analysis on demand — English beta" }),
  ).toBeVisible();
  expect(companyRequests).toBe(0);
  const inputBox = await page.getByLabel("US ticker or SEC company ID").boundingBox();
  expect(inputBox?.width).toBeGreaterThan(240);
  await page.getByLabel("US ticker or SEC company ID").fill("MSFT");
  await page.getByRole("button", { name: "Analyse", exact: true }).click();
  await expect(page.getByRole("heading", { name: /Microsoft|MICROSOFT/ })).toBeVisible({
    timeout: 60000,
  });
  await expect(
    page.getByRole("heading", { name: "Calculated findings — rule-based, not AI" }),
  ).toBeVisible();
  await expect(page.getByRole("table").first().getByRole("row")).not.toHaveCount(1);
  const recommendation = page.getByRole("region", { name: "Research recommendation and explanation — English beta" });
  await expect(recommendation).toContainText("No dated share price");
  await expect(recommendation).toContainText("SEC · CIK");
  await recommendation.scrollIntoViewIfNeeded();
  await page.screenshot({path:info.outputPath("recommendation-us-mobile.png")});
  await expect(page.getByRole("heading", { name: /Microsoft|MICROSOFT/ })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(
    true,
  );
  await page.getByRole("button", { name: "Clear result", exact: true }).click();
  await expect(page.getByRole("heading", { name: /Microsoft|MICROSOFT/ })).toHaveCount(0);
  await page.getByRole("heading", { name: "Stock analysis on demand — English beta" }).scrollIntoViewIfNeeded();
  await page.screenshot({path:info.outputPath("on-demand-mobile-cleared.png")});
  await page.setViewportSize({width:1280,height:900});
  await page.reload();
  await expect(page.getByLabel("US ticker or SEC company ID")).toHaveValue("");
  await page.getByRole("heading", { name: "Stock analysis on demand — English beta" }).scrollIntoViewIfNeeded();
  await page.screenshot({path:info.outputPath("on-demand-desktop-cleared.png")});
  await page.reload();
  await expect(page.getByLabel("US ticker or SEC company ID")).toHaveValue("");
  await expect(page.getByRole("heading", { name: /Microsoft|MICROSOFT/ })).toHaveCount(0);
  expect(companyRequests).toBe(1);
  expect(externalRequests).toEqual([]);
});
