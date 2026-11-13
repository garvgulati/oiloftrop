import { z } from "zod";
import type { IndiaResearchData } from "@/adapters/shared/research";
import { validFilingIsin } from "../../../../../../wealth-manager/lib/india-filing-checks";

const months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const amount = z.number().finite().min(-1e12).max(1e12).nullable();
const row = z.object({ period: z.string().regex(/^[A-Z][a-z]{2} \d{4}$/), value: amount }).strict();
const series = z.object({ category: z.string(), history: z.array(row).max(30) }).strict();
const base = z.object({
  type: z.enum(["consolidated", "standalone"]),
  time_period: z.literal("yearly"),
  units_in: z.literal("crore"),
});
const incomeSchema = base.extend({ income_statement: z.array(series).max(3) }).strict();
const cashSchema = base.extend({ cash_flow: z.array(series).max(3) }).strict();
const balanceSchema = base
  .extend({
    history: z
      .array(
        z
          .object({ period: row.shape.period, total_asset: amount, total_liability: amount })
          .strict(),
      )
      .max(30),
  })
  .strict();
export interface IndiaPeriodSummary {
  period: string;
  revenue: number | null;
  operatingProfit: number | null;
  profit: number | null;
  operatingCash: number | null;
  investingCash: number | null;
  financingCash: number | null;
  assets: number | null;
  liabilities: number | null;
}
export interface IndiaSummary {
  listing: IndiaResearchData["listing"];
  basis: IndiaResearchData["basis"];
  retrievedAt: string;
  periods: IndiaPeriodSummary[];
  profitMargin: number | null;
  operatingMargin: number | null;
}

export function buildIndiaSummary(data: IndiaResearchData): IndiaSummary {
  const income = incomeSchema.parse(data.income),
    cash = cashSchema.parse(data.cash),
    balance = balanceSchema.parse(data.balance);
  if ([income, cash, balance].some((value) => value.type !== data.basis))
    throw Error("INDIA_BASIS_OR_UNITS_INVALID");
  const { listing } = data;
  if (
    !validFilingIsin(listing.isin) ||
    !listing.isin.startsWith("INE") ||
    !["NSE", "BSE"].includes(listing.exchange) ||
    listing.key !== `${listing.exchange}_EQ|${listing.isin}`
  )
    throw Error("INDIA_IDENTITY_INVALID");
  const retrieved = new Date(data.retrievedAt);
  if (!Number.isFinite(retrieved.getTime())) throw Error("INDIA_SOURCE_INVALID");
  const ordinal = (period: string) => {
    const month = months.indexOf(period.slice(0, 3)),
      year = Number(period.slice(4));
    if (
      month < 0 ||
      year < 1900 ||
      year * 12 + month > retrieved.getUTCFullYear() * 12 + retrieved.getUTCMonth()
    )
      throw Error("INDIA_PERIOD_INVALID");
    return year * 12 + month;
  };
  const periods = new Map<string, IndiaPeriodSummary>();
  function period(label: string) {
    ordinal(label);
    if (!periods.has(label))
      periods.set(label, {
        period: label,
        revenue: null,
        operatingProfit: null,
        profit: null,
        operatingCash: null,
        investingCash: null,
        financingCash: null,
        assets: null,
        liabilities: null,
      });
    return periods.get(label)!;
  }
  function readSeries(
    values: z.infer<typeof series>[],
    mapping: Record<string, keyof Omit<IndiaPeriodSummary, "period">>,
  ) {
    const categories = new Set<string>();
    for (const value of values) {
      const key = mapping[value.category];
      if (!key || categories.has(value.category)) throw Error("INDIA_CATEGORY_INVALID");
      categories.add(value.category);
      const labels = new Set<string>();
      for (const row of value.history) {
        if (labels.has(row.period)) throw Error("INDIA_PERIOD_DUPLICATE");
        labels.add(row.period);
        period(row.period)[key] = row.value;
      }
    }
  }
  readSeries(income.income_statement, {
    revenue: "revenue",
    operating_profit: "operatingProfit",
    net_profit: "profit",
  });
  readSeries(cash.cash_flow, {
    operating: "operatingCash",
    investing: "investingCash",
    financing: "financingCash",
  });
  const seen = new Set<string>();
  for (const row of balance.history) {
    if (seen.has(row.period)) throw Error("INDIA_PERIOD_DUPLICATE");
    seen.add(row.period);
    const value = period(row.period);
    value.assets = row.total_asset;
    value.liabilities = row.total_liability;
  }
  if (periods.size > 30) throw Error("INDIA_SOURCE_TOO_LARGE");
  const sorted = [...periods.values()].sort((a, b) => ordinal(b.period) - ordinal(a.period));
  const latest = sorted.find((row) => row.revenue !== null);
  const margin = (value: number | null | undefined) => {
    const result =
      value != null && latest?.revenue != null && latest.revenue > 0
        ? (value / latest.revenue) * 100
        : null;
    return result !== null && Number.isFinite(result) ? result : null;
  };
  return {
    listing,
    basis: data.basis,
    retrievedAt: data.retrievedAt,
    periods: sorted,
    profitMargin: margin(latest?.profit),
    operatingMargin: margin(latest?.operatingProfit),
  };
}
