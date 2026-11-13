import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import common from "@/i18n/locales/en/common.json";
import { OnDemandResearch } from "./on-demand-research";

const api = vi.hoisted(() => ({ loadResearchCompany: vi.fn() }));
vi.mock("@/adapters", () => api);
vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) =>
      key.split(".").reduce<unknown>((v, k) => (v as Record<string, unknown>)[k], common),
    i18n: { language: "en" },
  }),
}));

function publicData() {
  const annual = (val: number, year: number) => ({
    val,
    start: `${year}-01-01`,
    end: `${year}-12-31`,
    filed: `${year + 1}-02-20`,
    accn: `0000001234-${String(year + 1).slice(-2)}-000001`,
    form: "10-K",
  });
  return {
    retrievedAt: "2026-10-25T12:00:00Z",
    submissions: {
      cik: "0000001234",
      name: "Fictional Source Test Inc.",
      tickers: ["TEST"],
      sic: "3571",
      filings: { recent: {} },
    },
    facts: {
      cik: 1234,
      facts: {
        "us-gaap": {
          RevenueFromContractWithCustomerExcludingAssessedTax: {
            units: { USD: [annual(1e6, 2023), annual(1.2e6, 2024)] },
          },
          NetIncomeLoss: { units: { USD: [annual(120000, 2024)] } },
          EarningsPerShareDiluted: { units: { "USD/shares": [annual(12.5, 2024)] } },
          NetCashProvidedByUsedInOperatingActivities: { units: { USD: [annual(200000, 2024)] } },
          PaymentsToAcquirePropertyPlantAndEquipment: { units: { USD: [annual(50000, 2024)] } },
        },
      },
    },
  };
}

function open() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return {
    client,
    ...render(
      <QueryClientProvider client={client}>
        <OnDemandResearch />
      </QueryClientProvider>,
    ),
  };
}
function run(symbol = "TEST") {
  fireEvent.change(screen.getByLabelText(common.researchDemand.symbol), {
    target: { value: symbol },
  });
  fireEvent.click(screen.getByRole("button", { name: common.researchDemand.analyse }));
}
beforeEach(() => {
  vi.clearAllMocks();
  api.loadResearchCompany.mockResolvedValue(publicData());
});

describe("temporary on-demand public analysis", () => {
  it("makes no automatic provider request and reuses the checked reader on explicit action", async () => {
    open();
    expect(api.loadResearchCompany).not.toHaveBeenCalled();
    run(" test ");
    expect(await screen.findByRole("heading", { name: /Fictional Source Test Inc/ })).toBeVisible();
    expect(api.loadResearchCompany).toHaveBeenCalledExactlyOnceWith("TEST");
    expect(
      screen.getByText("Revenue grew 20.0% versus the preceding comparable annual period."),
    ).toBeVisible();
    expect(screen.getByText(/Calculated findings — rule-based, not AI/)).toBeVisible();
    expect(screen.getByText(/2026-10-25T12:00:00Z/)).toBeVisible();
    expect(screen.getAllByText("Not reported").length).toBeGreaterThan(0);
    expect(screen.getByLabelText(`${common.researchValuation.eps} (USD/share)`)).toHaveValue("12.5");
    expect(screen.getByLabelText(common.researchValuation.periodStart)).toHaveValue("2024-01-01");
    expect(screen.getByLabelText(common.researchValuation.periodEnd)).toHaveValue("2024-12-31");
    expect(screen.getByLabelText(common.researchValuation.filedDate)).toHaveValue("2025-02-20");
    expect(screen.getByLabelText(`${common.researchValuation.price} (USD/share)`)).toHaveValue("");
    expect(screen.getByLabelText(common.researchValuation.confirmation)).not.toBeChecked();
    expect(
      screen.getByRole("region", { name: common.researchRecommendation.title }),
    ).toHaveTextContent(common.researchRecommendation.wait);
  });
  it("clears results without writing browser storage and removes its mutation cache on leaving", async () => {
    const writes = vi.spyOn(Storage.prototype, "setItem");
    const view = open();
    run();
    await screen.findByRole("heading", { name: /Fictional Source Test Inc/ });
    fireEvent.click(screen.getByRole("button", { name: common.researchDemand.clear }));
    expect(
      screen.queryByRole("heading", { name: /Fictional Source Test Inc/ }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("region", { name: common.researchRecommendation.title }),
    ).not.toBeInTheDocument();
    expect(screen.getByLabelText(common.researchDemand.symbol)).toHaveValue("");
    view.unmount();
    await waitFor(() => expect(view.client.getMutationCache().getAll()).toHaveLength(0));
    expect(writes).not.toHaveBeenCalled();
    writes.mockRestore();
  });
  it("does not substitute an earlier report when the next source request fails", async () => {
    open();
    run();
    await screen.findByRole("heading", { name: /Fictional Source Test Inc/ });
    api.loadResearchCompany.mockRejectedValueOnce(new Error("COMPANY_SOURCE_UNAVAILABLE"));
    run("BAD");
    expect(await screen.findByRole("alert")).toHaveTextContent(/No stored analysis is substituted/);
    expect(
      screen.queryByRole("heading", { name: /Fictional Source Test Inc/ }),
    ).not.toBeInTheDocument();
  });
  it("rejects mismatched issuer datasets instead of displaying findings", async () => {
    const data = publicData();
    data.facts.cik = 9999;
    api.loadResearchCompany.mockResolvedValueOnce(data);
    open();
    run();
    expect(await screen.findByRole("alert")).toBeVisible();
    expect(screen.queryByText(/Revenue grew/)).not.toBeInTheDocument();
  });
});
