import { expect, it } from "vitest";
import { buildIndiaSummary } from "./india-summary";
import type { IndiaResearchData } from "@/adapters/shared/research";
import { sampleIndiaData } from "./india-summary.fixture";
it("keeps INR crore, basis, missing values and unmatched reporting months distinct", () => {
  const report = buildIndiaSummary(sampleIndiaData());
  expect(report.profitMargin).toBe(10);
  expect(report.operatingMargin).toBeNull();
  expect(report.periods[0]).toMatchObject({
    period: "Mar 2026",
    operatingCash: null,
    liabilities: 80,
  });
  expect(report.periods[1]).toMatchObject({ period: "Mar 2025", operatingCash: 12, profit: null });
});
it("rejects mixed bases, wrong units, invalid identity and future periods", () => {
  for (const mutate of [
    (v: IndiaResearchData) => {
      (v.income as Record<string, unknown>).type = "standalone";
    },
    (v: IndiaResearchData) => {
      (v.cash as Record<string, unknown>).units_in = "lakhs";
    },
    (v: IndiaResearchData) => {
      v.listing.key = "NSE_EQ|INE002A01018";
    },
    (v: IndiaResearchData) => {
      v.retrievedAt = "2025-01-01T00:00:00Z";
    },
  ]) {
    const value = sampleIndiaData();
    mutate(value);
    expect(() => buildIndiaSummary(value)).toThrow();
  }
});
it("rejects duplicate categories and month rows instead of selecting one silently", () => {
  const value = sampleIndiaData();
  const income = value.income as { income_statement: unknown[] };
  income.income_statement.push(income.income_statement[0]);
  expect(() => buildIndiaSummary(value)).toThrow();
});
