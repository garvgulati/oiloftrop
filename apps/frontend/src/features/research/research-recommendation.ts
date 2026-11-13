import type { CompanyReport } from "../../../../../../wealth-manager/lib/research";
import type { IndiaSummary } from "./india-summary";

interface ReviewPeriod {
  label: string;
  month: number;
  revenue: number | null;
  profit: number | null;
  cash: number | null;
  assets: number | null;
  liabilities: number | null;
}
export interface RecommendationEvidence {
  observation: string;
  explanation: string;
}
export interface ResearchRecommendation {
  outcome: "watchlist" | "review-risk" | "wait";
  reasons: RecommendationEvidence[];
  concerns: RecommendationEvidence[];
  gaps: string[];
  period: string | null;
  source: string;
}
const finite = (value: number | null | undefined) =>
  value != null && Number.isFinite(value) ? value : null;
const monthNumber = (date: Date) => date.getUTCFullYear() * 12 + date.getUTCMonth();
const monthLabel = (label: string) => {
  const month = [
    "Jan",
    "Feb",
    "Mar",
    "Apr",
    "May",
    "Jun",
    "Jul",
    "Aug",
    "Sep",
    "Oct",
    "Nov",
    "Dec",
  ].indexOf(label.slice(0, 3));
  return month < 0 ? NaN : Number(label.slice(4)) * 12 + month;
};

/** A transparent annual-data screen, not a valuation model or personalized trade instruction. */
function review(
  periods: ReviewPeriod[],
  source: string,
  unit: string,
  sectorSpecific: boolean,
  now: Date,
  comparisonAllowed = true,
): ResearchRecommendation {
  const latest = periods[0],
    previous = periods[1];
  const reasons: RecommendationEvidence[] = [],
    concerns: RecommendationEvidence[] = [];
  const gaps = [
    "No dated share price or market-value/fair-value comparison is used by this recommendation: a good business can still be too expensive. A separately opened chart does not perform that valuation check.",
    "Your holdings, entry cost, goals, cash needs and loss limits were not used. Personal buy, hold, sell and position-size decisions remain unassessed.",
    "Read recent results, filing notes, auditor findings, governance, debt maturities and business risks before acting. Annual figures do not establish the current outlook.",
  ];
  const money = (value: number) =>
    `${new Intl.NumberFormat("en", { maximumFractionDigits: 2 }).format(value)} ${unit}`;
  const add = (target: RecommendationEvidence[], observation: string, explanation: string) =>
    target.push({ observation, explanation });
  const validTime =
    latest &&
    Number.isFinite(latest.month) &&
    Number.isFinite(now.getTime()) &&
    latest.month <= monthNumber(now);
  const fresh = validTime && monthNumber(now) - latest.month <= 18;
  if (!fresh)
    gaps.push(
      "Latest annual period is missing, future-dated or over 18 calendar months old. This app's 18-month freshness screen is a conservative review rule, not a market standard.",
    );
  const comparable =
    latest && previous && latest.month - previous.month === 12 && comparisonAllowed;
  if (!comparable)
    gaps.push(
      "Two adjacent annual periods with the same reporting month and comparable source definitions are needed for a year-to-year comparison. Missing years or changed definitions are not bridged.",
    );
  const complete =
    comparable &&
    [latest, previous].every((p) => [p.revenue, p.profit, p.cash].every((v) => v !== null)) &&
    latest.revenue! > 0 &&
    previous.revenue! > 0;
  if (!complete)
    gaps.push(
      "The latest two annual periods need positive revenue and reported profit and operating cash figures. Missing values are not zero.",
    );
  if (sectorSpecific)
    gaps.push(
      "The disclosed industry needs specialist financial-sector or property analysis. Generic cash-flow screening cannot support a favorable conclusion here.",
    );
  if (latest) {
    if (latest.profit !== null && latest.profit <= 0)
      add(
        concerns,
        `${latest.label}: net profit ${money(latest.profit)}.`,
        "There is no positive annual profit to support an earnings-based purchase case. Check whether this reflects recurring operating weakness or exceptional items.",
      );
    if (latest.cash !== null && latest.cash <= 0)
      add(
        concerns,
        `${latest.label}: operating cash flow ${money(latest.cash)}.`,
        "Operations did not generate positive cash in this period. Review working capital, seasonality and financing needs before adding money.",
      );
    if (
      latest.cash !== null &&
      latest.profit !== null &&
      latest.profit > 0 &&
      latest.cash < latest.profit
    )
      add(
        concerns,
        `${latest.label}: operating cash ${money(latest.cash)} versus profit ${money(latest.profit)}.`,
        "Operating cash is below reported profit. Inspect receivables, inventory, taxes and non-cash items; this alone does not establish poor accounting or fraud.",
      );
    if (
      latest.assets !== null &&
      latest.assets > 0 &&
      latest.liabilities !== null &&
      latest.liabilities >= latest.assets
    )
      add(
        concerns,
        `${latest.label}: liabilities ${money(latest.liabilities)} versus assets ${money(latest.assets)}.`,
        "Reported liabilities meet or exceed assets. Review equity, obligations and sector accounting before taking more exposure. Total liabilities are not the same as debt.",
      );
    if (
      latest.assets === null ||
      latest.assets <= 0 ||
      latest.liabilities === null ||
      latest.liabilities < 0
    )
      gaps.push("A coherent latest-period asset/liability comparison is unavailable.");
    if (
      comparable &&
      latest.revenue !== null &&
      previous.revenue !== null &&
      previous.revenue > 0
    ) {
      const growth = (latest.revenue / previous.revenue - 1) * 100;
      if (Number.isFinite(growth))
        add(
          growth < 0 ? concerns : reasons,
          `Revenue: ${money(previous.revenue)} (${previous.label}) → ${money(latest.revenue)} (${latest.label}); ${growth.toFixed(1)}% change.`,
          growth < 0
            ? "Sales declined. Investigate demand, pricing, disposals and business-cycle effects before assuming a recovery."
            : "Sales held up or increased. Check whether growth came from recurring demand, acquisitions or temporary pricing; growth alone does not make the shares cheap.",
        );
    }
    if (comparable && latest.profit !== null && previous.profit !== null)
      add(
        latest.profit < previous.profit ? concerns : reasons,
        `Net profit: ${money(previous.profit)} (${previous.label}) → ${money(latest.profit)} (${latest.label}).`,
        latest.profit < previous.profit
          ? "Reported profit fell. Read operating results and exceptional items to understand whether earning power deteriorated."
          : "Reported profit held up or increased. Check recurring earnings and ownership attribution before extrapolating it.",
      );
    if (
      latest.cash !== null &&
      latest.profit !== null &&
      latest.cash > 0 &&
      latest.profit > 0 &&
      latest.cash >= latest.profit
    )
      add(
        reasons,
        `${latest.label}: operating cash ${money(latest.cash)} versus profit ${money(latest.profit)}.`,
        "Operating cash covers reported profit in this annual period. This is one cash-generation check; it does not deduct investment spending or prove sustainable earnings quality.",
      );
  }
  const favorable =
    complete &&
    latest.revenue! > previous.revenue! &&
    latest.profit! > previous.profit! &&
    previous.profit! > 0 &&
    previous.cash! > 0 &&
    latest.cash! >= latest.profit! &&
    latest.assets !== null &&
    latest.liabilities !== null &&
    latest.assets > latest.liabilities &&
    latest.liabilities >= 0;
  return {
    outcome:
      !fresh || !complete || sectorSpecific
        ? "wait"
        : concerns.length
          ? "review-risk"
          : favorable
            ? "watchlist"
            : "wait",
    reasons,
    concerns,
    gaps,
    period: latest?.label ?? null,
    source,
  };
}

export function recommendIndia(report: IndiaSummary, now = new Date()): ResearchRecommendation {
  const result = review(
    report.periods.map((p) => ({
      label: p.period,
      month: monthLabel(p.period),
      revenue: finite(p.revenue),
      profit: finite(p.profit),
      cash: finite(p.operatingCash),
      assets: finite(p.assets),
      liabilities: finite(p.liabilities),
    })),
    `Upstox · ${report.listing.exchange} · ${report.listing.isin} · ${report.basis}`,
    "₹ crore",
    false,
    now,
  );
  result.gaps.push(
    "Upstox supplies month-labelled annual summaries. Exact duration, audit status, sector classification and owners/noncontrolling profit attribution are not established; verify these in the company's filings. Standalone and consolidated results must not be mixed.",
  );
  return result;
}

export function recommendUS(report: CompanyReport, now = new Date()): ResearchRecommendation {
  const [latest, previous] = report.annual;
  const comparable = Boolean(
    latest &&
    previous &&
    Math.abs(latest.days - previous.days) <= 14 &&
    (["revenue", "netIncome", "operatingCash"] as const).every(
      (key) =>
        latest.metrics[key]?.tag !== undefined &&
        latest.metrics[key]?.tag === previous.metrics[key]?.tag,
    ),
  );
  const result = review(
    report.annual.map((p) => ({
      label: `${p.start} – ${p.end}`,
      month: monthNumber(new Date(p.end)),
      revenue: finite(p.metrics.revenue?.value),
      profit: finite(p.metrics.netIncome?.value),
      cash: finite(p.metrics.operatingCash?.value),
      assets: finite(p.metrics.assets?.value),
      liabilities: finite(p.metrics.liabilities?.value),
    })),
    `SEC · CIK ${report.cik} · annual US GAAP facts`,
    "USD",
    /bank|insurance|financial|broker|real estate|reit|investment trust/i.test(report.industry),
    now,
    comparable,
  );
  result.gaps.push(
    "This recommendation screen uses annual facts. Interim findings and source warnings below may change the picture; they do not silently become a new annual period.",
  );
  return result;
}
