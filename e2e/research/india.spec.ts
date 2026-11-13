import { test, expect } from "@playwright/test";

test("India credentials are profile isolated and unavailable through generic reads", async ({
  context,
  page,
}, info) => {
  const initial = await (
    await context.request.post("/api/v1/profiles/get_profile_state", { data: {} })
  ).json();
  const a = initial.profiles[0];
  const unlock = async (profileId: string) =>
    (
      await (
        await context.request.post("/api/v1/profiles/unlock_profile", { data: { profileId } })
      ).json()
    ).scopeId as string;
  let scopeA = await unlock(a.id);
  const headers = (scope: string) => ({ "x-wf-profile-scope": scope });
  const get = async (scope: string) => {
    const response = await context.request.get("/api/v1/research/india/connection", {
      headers: headers(scope),
    });
    expect(response.ok()).toBe(true);
    expect(response.headers()["cache-control"]).toBe("no-store");
    const data = await response.json();
    expect(Object.keys(data).sort()).toEqual(["configured", "expiresOn"]);
    return data;
  };
  const credential = {
    token: "fictional-analytics-token-canary-123456",
    expiresOn: "2099-01-01",
    freePersonalConfirmed: true,
    privateDashboardConfirmed: true,
  };
  expect((await get(scopeA)).configured).toBe(false);
  expect(
    (
      await context.request.put("/api/v1/research/india/connection", {
        headers: headers(scopeA),
        data: credential,
      })
    ).ok(),
  ).toBe(true);
  expect((await get(scopeA)).configured).toBe(true);
  for (const key of [
    "profile:research:upstox_analytics_v1",
    "PROFILE:RESEARCH:UPSTOX_ANALYTICS_V1",
  ]) {
    const read = await context.request.get("/api/v1/secrets", {
      headers: headers(scopeA),
      params: { secretKey: key },
    });
    expect(read.status()).toBe(400);
  }
  const created = await context.request.post("/api/v1/profiles/create_profile", {
    data: { name: "Fictional India profile B", avatarId: "clay-bot-animated" },
  });
  expect(created.ok()).toBe(true);
  const b = await created.json();
  let scopeB = await unlock(b.id);
  expect(
    (
      await context.request.get("/api/v1/research/india/connection", { headers: headers(scopeA) })
    ).ok(),
  ).toBe(false);
  expect((await get(scopeB)).configured).toBe(false);
  expect(
    (
      await context.request.put("/api/v1/research/india/connection", {
        headers: headers(scopeB),
        data: credential,
      })
    ).ok(),
  ).toBe(true);
  scopeA = await unlock(a.id);
  expect(
    (
      await context.request.delete("/api/v1/research/india/connection", {
        headers: headers(scopeA),
      })
    ).ok(),
  ).toBe(true);
  expect((await get(scopeA)).configured).toBe(false);
  scopeB = await unlock(b.id);
  expect((await get(scopeB)).configured).toBe(true);
  expect(
    (
      await context.request.put("/api/v1/settings", {
        headers: headers(scopeB),
        data: {
          onboardingCompleted: true,
          language: "en",
          baseCurrency: "INR",
          timezone: "Asia/Kolkata",
          syncEnabled: false,
          autoUpdateCheckEnabled: false,
        },
      })
    ).ok(),
  ).toBe(true);
  let lookups = 0;
  const external: string[] = [];
  page.on("request", (r) => {
    const url = new URL(r.url());
    if (
      url.pathname.startsWith("/api/v1/research/india/search") ||
      url.pathname.startsWith("/api/v1/research/india/analyse")
    )
      lookups++;
    if (url.origin !== "http://127.0.0.1:18388") external.push(url.origin);
  });
  await page.goto("/research");
  await page.getByRole("combobox", { name: "Market for stock research" }).selectOption("india");
  await expect(page.getByText(/Upstox connection configured/)).toBeVisible();
  expect(lookups).toBe(0);
  await page.getByLabel("Indian company name, symbol or ISIN").fill("ONGC");
  await page.getByRole("button", { name: "Search India", exact: true }).click();
  await expect(page.getByRole("alert")).toHaveText(/Live Indian company requests are disabled/);
  expect(lookups).toBe(1);
  await page.getByRole("button", { name: "Clear India result", exact: true }).click();
  await page.screenshot({ path: info.outputPath("india-desktop.png") });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.reload();
  await page.getByRole("combobox", { name: "Market for stock research" }).selectOption("india");
  await expect(page.getByText(/Upstox connection configured/)).toBeVisible();
  expect(
    (await page.getByLabel("Indian company name, symbol or ISIN").boundingBox())?.width,
  ).toBeGreaterThan(240);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(
    true,
  );
  await page.screenshot({ path: info.outputPath("india-mobile.png") });
  expect(lookups).toBe(1);
  expect(external).toEqual([]);
});
