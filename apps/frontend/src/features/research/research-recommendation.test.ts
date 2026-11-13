import { describe, expect, it } from "vitest";
import { buildCompanyReport } from "../../../../../../wealth-manager/lib/research";
import { buildIndiaSummary, type IndiaSummary } from "./india-summary";
import { sampleIndiaData } from "./india-summary.fixture";
import { recommendIndia, recommendUS } from "./research-recommendation";

const now = new Date("2026-10-26T00:00:00Z");
function india(): IndiaSummary {
  const report = buildIndiaSummary(sampleIndiaData());
  report.periods = [
    {
      period: "Mar 2026",
      revenue: 120,
      profit: 12,
      operatingCash: 15,
      operatingProfit: 20,
      investingCash: null,
      financingCash: null,
      assets: 200,
      liabilities: 80,
    },
    {
      period: "Mar 2025",
      revenue: 100,
      profit: 10,
      operatingCash: 11,
      operatingProfit: 18,
      investingCash: null,
      financingCash: null,
      assets: 180,
      liabilities: 70,
    },
  ];
  return report;
}
function us() {
  const duration = (val: number, year: number) => ({
    val,
    start: `${year}-01-01`,
    end: `${year}-12-31`,
    filed: `${year + 1}-02-20`,
    accn: `0000001234-${String(year + 1).slice(-2)}-000001`,
    form: "10-K",
  });
  const instant = (val: number, year: number) => {
    const { start: _start, ...rest } = duration(val, year);
    return rest;
  };
  return buildCompanyReport(
    { cik: "0000001234", name: "Fictional Test", sicDescription: "Software" },
    {
      cik: 1234,
      facts: {
        "us-gaap": {
          Revenues: { units: { USD: [duration(100, 2024), duration(120, 2025)] } },
          NetIncomeLoss: { units: { USD: [duration(10, 2024), duration(12, 2025)] } },
          NetCashProvidedByUsedInOperatingActivities: {
            units: { USD: [duration(11, 2024), duration(15, 2025)] },
          },
          Assets: { units: { USD: [instant(200, 2025)] } },
          Liabilities: { units: { USD: [instant(80, 2025)] } },
        },
      },
    },
    "2026-10-26T00:00:00Z",
  );
}
describe("explained annual-data recommendations", () => {
  it("shortlists coherent growth and cash data for valuation review, without a buy verdict", () => {
    const report = india();
    const before = JSON.stringify(report);
    const result = recommendIndia(report, now);
    expect(result.outcome).toBe("watchlist");
    expect(result.reasons.map((p) => p.observation).join(" ")).toContain("20.0% change");
    expect(result.reasons.map((p) => p.observation).join(" ")).toContain("₹ crore");
    expect(result.gaps.join(" ")).toContain("No dated share price");
    expect(result.gaps.join(" ")).toContain("owners/noncontrolling");
    expect(JSON.stringify(report)).toBe(before);
  });
  it.each(["profit", "operatingCash", "revenue"] as const)(
    "waits when %s is missing rather than substituting another period or zero",
    (key) => {
      const report = india();
      report.periods[0][key] = null;
      expect(recommendIndia(report, now).outcome).toBe("wait");
    },
  );
  it("does not bridge a missing year or promote old data", () => {
    const report = india();
    report.periods[1].period = "Mar 2024";
    const result = recommendIndia(report, now);
    expect(result.outcome).toBe("wait");
    expect(result.reasons.some((v) => v.observation.includes("change"))).toBe(false);
    expect(recommendIndia(india(), new Date("2028-01-01Z")).outcome).toBe("wait");
    expect(recommendIndia(india(), new Date("2025-01-01Z")).outcome).toBe("wait");
  });
  it("raises a risk review for losses, negative cash and liabilities above assets", () => {
    const report = india();
    report.periods[0].profit = -2;
    report.periods[0].operatingCash = -5;
    report.periods[0].liabilities = 210;
    const result = recommendIndia(report, now);
    expect(result.outcome).toBe("review-risk");
    expect(result.concerns.map((v) => v.explanation).join(" ")).toContain("not the same as debt");
    expect(result.concerns.map((v) => v.observation).join(" ")).toContain("-5 ₹ crore");
  });
  it("treats weak cash conversion as a review concern, not proof of fraud", () => {
    const report = india();
    report.periods[0].operatingCash = 3;
    const result = recommendIndia(report, now);
    expect(result.outcome).toBe("review-risk");
    expect(result.concerns.map((v) => v.explanation).join(" ")).toContain("does not establish");
  });
  it("does not shortlist flat figures, a turnaround without prior profits, or missing balance data", () => {
    for (const change of [
      (r: IndiaSummary) => {
        r.periods[0].revenue = 100;
      },
      (r: IndiaSummary) => {
        r.periods[1].profit = -1;
      },
      (r: IndiaSummary) => {
        r.periods[0].assets = null;
      },
    ]) {
      const report = india();
      change(report);
      expect(recommendIndia(report, now).outcome).toBe("wait");
    }
  });
  it("uses real SEC reader values and refuses changed source tags and specialist sectors", () => {
    const report = us();
    expect(recommendUS(report, now).outcome).toBe("watchlist");
    expect(recommendUS(report, now).reasons[0].observation).toContain("USD");
    report.annual[1].metrics.revenue!.tag = "DifferentRevenueDefinition";
    expect(recommendUS(report, now).outcome).toBe("wait");
    expect(recommendUS(report, now).reasons.some((v) => v.observation.includes("change"))).toBe(
      false,
    );
    const specialist = us();
    specialist.industry = "Commercial banks";
    expect(recommendUS(specialist, now).outcome).toBe("wait");
  });
  it("reports absent data without claiming safety", () => {
    const report = india();
    report.periods = [];
    const result = recommendIndia(report, now);
    expect(result.outcome).toBe("wait");
    expect(result.period).toBeNull();
    expect(result.reasons).toHaveLength(0);
  });
});
