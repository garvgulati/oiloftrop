import { expect, it } from "vitest";
import { blankReviewedDraft, draftInput } from "./reviewed-import-draft";

const accounts = [
  { id: "e2d7dd79-1c8c-4e25-8dd3-e05be059b62b", name: "Synthetic", currency: "INR" as const },
];
function draft() {
  const value = blankReviewedDraft();
  Object.assign(value, {
    accountId: accounts[0].id,
    label: "Synthetic broker",
    sourceAccount: "Synthetic A",
    asOf: "2026-10-28",
  });
  Object.assign(value.positions[0], {
    isin: "INE213A01029",
    symbol: "ONGC",
    name: "Synthetic equity",
    quantity: "1.23456789",
    sourceVenue: "XNSE",
    valuationVenue: "XNSE",
    ordinaryEquityConfirmed: true,
  });
  return value;
}
it("preserves exact input and keeps unknown cost/cash/price distinct from zero", () => {
  const value = draft();
  const unknown = draftInput(value, accounts);
  expect(unknown.positions[0].quantity).toBe("1.23456789");
  expect(unknown.positions[0].averageCost).toBeNull();
  expect(unknown.positions[0].price).toBeNull();
  expect(unknown.cash).toBeNull();
  value.cash = "0";
  value.positions[0].averageCost = "0.00000000";
  const zero = draftInput(value, accounts);
  expect(zero.cash).toEqual({ INR: "0" });
  expect(zero.positions[0].averageCost).toBe("0.00000000");
});
it("requires full independent price evidence and the selected account’s venue/currency", () => {
  const value = draft();
  value.positions[0].averageCost = "123.4567";
  expect(draftInput(value, accounts).positions[0].price).toBeNull();
  value.positions[0].priceDate = "2026-10-28";
  expect(() => draftInput(value, accounts)).toThrow();
  value.positions[0].price = "150.25";
  value.positions[0].priceSource = "Synthetic dated observation";
  expect(draftInput(value, accounts).positions[0].price).toEqual({
    amount: "150.25",
    observedOn: "2026-10-28",
    source: "Synthetic dated observation",
    venue: "XNSE",
    currency: "INR",
  });
  value.positions[0].valuationVenue = "XNAS";
  expect(() => draftInput(value, accounts)).toThrow();
  expect(() => draftInput(draft(), [])).toThrow();
});
it.each(["0", "-1", "1.000000001", "NaN", "1e3"])("rejects unsupported quantity %s", (quantity) => {
  const value = draft();
  value.positions[0].quantity = quantity;
  expect(() => draftInput(value, accounts)).toThrow();
});
it("rejects a later price date before sending a historical snapshot and accepts older dated prices", () => {
  const value = draft();
  value.asOf = "2026-10-13";
  Object.assign(value.positions[0], {
    price: "200",
    priceDate: "2026-11-03",
    priceSource: "Synthetic broker",
  });
  expect(() => draftInput(value, accounts)).toThrow("PRICE_AFTER_RECORD");
  value.positions[0].priceDate = "2026-10-13";
  expect(draftInput(value, accounts).positions[0].price?.observedOn).toBe("2026-10-13");
  value.positions[0].priceDate = "2026-10-12";
  expect(draftInput(value, accounts).positions[0].price?.observedOn).toBe("2026-10-12");
});
