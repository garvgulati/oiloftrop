import { expect, test } from "@playwright/test";

test("protected personal intake survives desktop and phone reload without generic provider requests", async ({
  page,
}) => {
  const disallowed: string[] = [];
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("request", (request) => {
    const url = new URL(request.url());
    if (url.origin !== "https://127.0.0.1:18489") disallowed.push(url.origin);
    if (/\/api\/v1\/research\/(company|india\/(search|analyse|chart|ratios))/.test(url.pathname)) {
      disallowed.push(url.pathname);
    }
    if (
      /\/api\/v1\/(connect|addons|ai\/|market-data|holdings|research\/context|research\/example)/.test(
        url.pathname,
      )
    ) {
      disallowed.push(url.pathname);
    }
  });
  await page.goto("/");
  await page.locator('input[type="password"]').fill("Synthetic personal browser password 42");
  await page.getByTestId("login-submit-button").click();
  await page.getByRole("button", { name: "Personal", exact: true }).click();
  await page.getByLabel("Password", { exact: true }).fill("Synthetic personal browser password 42");
  await page.getByRole("button", { name: "Unlock", exact: true }).click();
  await expect(
    page.getByRole("navigation", { name: "Personal portfolio navigation" }),
  ).toBeVisible();
  await page.getByRole("link", { name: "Review / manage holdings", exact: true }).first().click();
  await page.getByLabel("Account nickname", { exact: true }).fill("Synthetic browser account");
  await page.getByRole("button", { name: "Create holdings account", exact: true }).click();
  await expect(
    page.getByRole("combobox", { name: "Save into account", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("combobox", { name: "Save into account", exact: true })
    .selectOption({ label: "Synthetic browser account · INR" });
  await page.getByLabel("Date of the broker record", { exact: true }).fill("2026-10-29");
  await page.getByLabel("Broker or record source", { exact: true }).fill("Synthetic broker");
  await page
    .getByLabel("Source account nickname (no account number needed)", { exact: true })
    .fill("Synthetic source");
  await page.getByLabel("ISIN (12 characters)", { exact: true }).fill("INE213A01029");
  await page.getByLabel("Trading symbol", { exact: true }).fill("SYNTHETIC");
  await page.getByLabel("Company / share name", { exact: true }).fill("Synthetic ordinary equity");
  await page.getByLabel("Settled share quantity", { exact: true }).fill("1.23456789");
  await page
    .getByRole("combobox", { name: "Exchange in the broker record", exact: true })
    .selectOption("XNSE");
  await page
    .getByRole("combobox", { name: "Exchange for valuation / observed price", exact: true })
    .selectOption("XNSE");
  await page.getByRole("checkbox", { name: /I checked the ISIN/ }).check();
  await page.getByLabel("Observed price per share (optional)", { exact: true }).fill("200");
  await page.getByLabel("Date of that price", { exact: true }).fill("2026-11-03");
  await page.getByLabel("Source of that price", { exact: true }).fill("Synthetic dated source");
  await page.getByRole("button", { name: "Preview before saving", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText(
    "The price date is later than the holdings-record date",
  );
  await page.getByRole("alert").scrollIntoViewIfNeeded();
  await page.screenshot({ path: test.info().outputPath("holdings-date-error.png") });
  await expect(page.getByLabel("Settled share quantity", { exact: true })).toHaveValue(
    "1.23456789",
  );
  await expect(
    page.getByRole("button", { name: "Confirm and save this snapshot", exact: true }),
  ).toHaveCount(0);
  await page.getByLabel("Observed price per share (optional)", { exact: true }).fill("");
  await page.getByLabel("Date of that price", { exact: true }).fill("");
  await page.getByLabel("Source of that price", { exact: true }).fill("");
  await page.getByRole("button", { name: "Preview before saving", exact: true }).click();
  const preview = page.getByLabel("Review this snapshot", { exact: true });
  await expect(preview).toBeVisible();
  await expect(preview).toBeFocused();
  await expect(preview).toContainText("1.23456789");
  await page.getByRole("button", { name: "Confirm and save this snapshot", exact: true }).click();
  await expect(page.getByLabel("Saved holdings record", { exact: true })).toContainText(
    "1.23456789",
  );
  await page.getByRole("link", { name: "Home", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Synthetic browser account", exact: true }),
  ).toBeVisible();
  await expect(page.locator("body")).toContainText(
    "Average cost per share (optional): Unknown INR",
  );
  await page.reload();
  await expect(
    page.getByText("Settled share quantity: 1.23456789", { exact: false }),
  ).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  await page.reload();
  await expect(
    page.getByRole("navigation", { name: "Personal portfolio navigation" }),
  ).toBeVisible();
  await expect(
    page.getByText("Settled share quantity: 1.23456789", { exact: false }),
  ).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(
    true,
  );
  const palette = await page
    .getByRole("heading", { name: "Home", exact: true })
    .evaluate((element) => ({
      text: getComputedStyle(element).color,
      bodyText: getComputedStyle(document.body).color,
      background: getComputedStyle(element.closest("section")?.parentElement ?? document.body)
        .backgroundColor,
      fill: getComputedStyle(element).webkitTextFillColor,
    }));
  await test
    .info()
    .attach("personal-theme-colors", {
      body: JSON.stringify(palette),
      contentType: "application/json",
    });
  expect(palette.text).not.toBe(palette.background);
  await page.evaluate(() => document.fonts.ready);
  await page.screenshot({
    path: test.info().outputPath("personal-phone.png"),
    fullPage: true,
  });
  await page.getByRole("button", { name: "Lock", exact: true }).click();
  await expect(
    page.getByRole("navigation", { name: "Personal portfolio navigation" }),
  ).not.toBeVisible();
  expect(disallowed).toEqual([]);
  expect(errors).toEqual([]);
});
