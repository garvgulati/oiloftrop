import { describe, expect, it } from "vitest";
import { checkValuation, emptyValuation, type ValuationInputs } from "./valuation-check";

const valid: ValuationInputs = {
  ...emptyValuation,
  price: "200",
  priceDate: "2026-10-27",
  priceSource: "https://example.com/quote",
  eps: "10",
  periodStart: "2025-04-01",
  periodEnd: "2026-03-31",
  filedDate: "2026-05-12",
  earningsSource: "https://example.com/filing",
  confirmed: true,
};
const run = (change: Partial<ValuationInputs> = {}) =>
  checkValuation({ ...valid, ...change }, "2026-10-27");
describe("reviewed valuation arithmetic", () => {
  it("calculates price/annual EPS and earnings yield without inventing a multiple", () => {
    expect(run()).toMatchObject({
      error: null,
      pe: 20,
      yield: 5,
      impliedPrice: null,
      difference: null,
      priceAge: 0,
    });
    expect(run({ multiple: "15", rationale: "Reviewed peer assumption" })).toMatchObject({
      impliedPrice: 150,
      difference: -25,
    });
  });
  it("requires reviewed share identity and sources", () => {
    expect(run({ confirmed: false }).error).toBe("confirm");
    for (const url of [
      "",
      "javascript:alert(1)",
      "http://example.com",
      "https://user:secret@example.com",
    ])
      expect(run({ earningsSource: url }).error).toBe("sources");
  });
  it("rejects missing/ambiguous numbers and nonpositive prices", () => {
    for (const value of [
      "",
      "1e2",
      "NaN",
      "Infinity",
      "1,000",
      "-1",
      "0",
      "1.123456789",
      "1000000001",
    ])
      expect(run({ price: value }).error).toBe("numbers");
    expect(run({ eps: "" }).error).toBe("numbers");
    expect(run({ eps: "0" }).error).toBe("loss");
    expect(run({ eps: "-5" }).error).toBe("loss");
  });
  it("prevents future/invalid dates, lookahead and mixing quarters with annual EPS", () => {
    for (const change of [
      { priceDate: "2026-10-28" },
      { priceDate: "2026-02-30" },
      { filedDate: "2026-11-01" },
      { filedDate: "2026-10-27" },
      { filedDate: "2026-03-01" },
      { periodStart: "2026-01-01" },
      { periodEnd: "" },
    ])
      expect(run(change).error).toBe("dates");
  });
  it("requires a justified optional positive multiple and gives explicit ages", () => {
    for (const multiple of ["0", "-5", "NaN", "1001", "15"])
      expect(run({ multiple }).error).toBe("multipleError");
    expect(run({ multiple: "15", rationale: "Peer check" }).error).toBeNull();
    expect(run({ priceDate: "2026-10-01" })).toMatchObject({ priceAge: 26, earningsAge: 184 });
  });
});
