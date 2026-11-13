import type { IndiaResearchData } from "@/adapters/shared/research";
export function sampleIndiaData(): IndiaResearchData {
  const base = { type: "consolidated", time_period: "yearly", units_in: "crore" };
  return {
    listing: {
      name: "Fictional Source Test",
      symbol: "TEST",
      isin: "INE213A01029",
      key: "NSE_EQ|INE213A01029",
      exchange: "NSE",
    },
    basis: "consolidated",
    retrievedAt: "2026-10-26T00:00:00Z",
    income: {
      ...base,
      income_statement: [
        { category: "revenue", history: [{ period: "Mar 2026", value: 100 }] },
        { category: "net_profit", history: [{ period: "Mar 2026", value: 10 }] },
      ],
    },
    cash: {
      ...base,
      cash_flow: [{ category: "operating", history: [{ period: "Mar 2025", value: 12 }] }],
    },
    balance: { ...base, history: [{ period: "Mar 2026", total_asset: 200, total_liability: 80 }] },
  };
}
