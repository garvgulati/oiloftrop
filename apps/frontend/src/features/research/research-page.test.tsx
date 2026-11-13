import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import common from "@/i18n/locales/en/common.json";
import ResearchPage from "./research-page";

const api = vi.hoisted(() => ({
  getResearchContext: vi.fn(),
  getReviewedImportStatus: vi.fn(),
  runResearchExample: vi.fn(),
}));
vi.mock("@/adapters", () => api);
vi.mock("./plan-editor", () => ({ ResearchPlanEditor: () => null }));
vi.mock("./reviewed-import", () => ({
  ReviewedImportPanel: () => <p>Synthetic personal import panel</p>,
}));
vi.mock("@/features/profiles/profile-context", () => ({
  useProfile: () => ({ profile: { id: "fictional" } }),
}));
vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) =>
      key
        .split(".")
        .reduce<unknown>((value, field) => (value as Record<string, unknown>)[field], common),
    i18n: { language: "en" },
  }),
}));

const example = {
  mode: "fictional",
  caseId: "baseline",
  asOf: "2026-10-12T04:30:00.000Z",
  quantity: "0.99000000",
  horizons: [1, 3, 5, 6, 7, 8].map((years) => ({
    years,
    end: `${2026 + years}-10-12`,
    remainingA: 800,
    remainingB: 900,
    difference: -100,
  })),
};

function open() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <ResearchPage />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  api.getReviewedImportStatus.mockResolvedValue({ supported: false, accounts: [] });
  api.getResearchContext.mockResolvedValue({
    valuationDate: "2026-10-24",
    issues: [{ code: "FINANCIAL_IDENTITY_UNVERIFIED" }],
    positions: [
      {
        accountId: "fictional",
        assetId: "fictional-share",
        symbol: "TRIALIN",
        quantity: "999999999.12345678",
        quote: { price: "224.64", currency: "INR", observationDate: "2026-10-23" },
      },
    ],
  });
  api.runResearchExample.mockResolvedValue(example);
});

describe("research input and fictional assessment", () => {
  it("uses the personal import view without querying generic portfolio data or showing fictional examples", async () => {
    api.getReviewedImportStatus.mockResolvedValue({ supported: true, accounts: [] });
    open();
    await screen.findByText("Synthetic personal import panel");
    expect(api.getResearchContext).not.toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: common.research.run })).not.toBeInTheDocument();
  });

  it("does not fall back to generic holdings when personal status cannot be checked", async () => {
    api.getReviewedImportStatus.mockRejectedValue(new Error("SYNTHETIC_PRIVATE_DIAGNOSTIC"));
    open();
    await screen.findByText(common.reviewedImport.statusError);
    expect(api.getResearchContext).not.toHaveBeenCalled();
    expect(screen.queryByText(/SYNTHETIC_PRIVATE_DIAGNOSTIC/)).not.toBeInTheDocument();
  });
  it("renders exact quantities without converting through a JavaScript number", async () => {
    open();
    const row = await screen.findByRole("row", { name: /TRIALIN/ });
    expect(within(row).getByText("999999999.12345678")).toBeInTheDocument();
    expect(within(row).getByText("2026-10-23")).toBeInTheDocument();
    expect(screen.getByText(common.research.issues.identity)).toBeInTheDocument();
    expect(screen.getByText(common.research.notReady)).toBeInTheDocument();
  });

  it("runs only an explicit fictional case and shows all six dated outcomes", async () => {
    open();
    expect(api.runResearchExample).not.toHaveBeenCalled();
    fireEvent.click(await screen.findByRole("button", { name: common.research.run }));
    await screen.findByText("0.99000000");
    expect(api.runResearchExample.mock.calls[0][0]).toBe("baseline");
    expect(screen.getByText(common.research.exampleNote)).toBeInTheDocument();
    expect(screen.getByText(common.research.scoreUnavailable)).toBeInTheDocument();
    expect(screen.getAllByText("-$100.00")).toHaveLength(6);
    for (const years of [1, 3, 5, 6, 7, 8])
      expect(screen.getByText(`${years} · ${2026 + years}-10-12`)).toBeInTheDocument();
  });

  it("clears an old result when example inputs change", async () => {
    open();
    fireEvent.click(await screen.findByRole("button", { name: common.research.run }));
    await screen.findByText("0.99000000");
    fireEvent.change(screen.getByRole("combobox", { name: common.research.case }), {
      target: { value: "unknown-loss" },
    });
    await waitFor(() => expect(screen.queryByText("0.99000000")).not.toBeInTheDocument());
    api.runResearchExample.mockResolvedValue({ ...example, caseId: "unknown-loss" });
    fireEvent.click(await screen.findByRole("button", { name: common.research.run }));
    await screen.findByText(common.research.exampleNeedsLoss);
    expect(api.runResearchExample.mock.calls[1][0]).toBe("unknown-loss");
  });

  it("shows a safe calculator failure without rendering raw diagnostic content", async () => {
    api.runResearchExample.mockRejectedValue(new Error("SYNTHETIC_PRIVATE_DIAGNOSTIC"));
    open();
    fireEvent.click(await screen.findByRole("button", { name: common.research.run }));
    await screen.findByText(common.research.exampleError);
    expect(screen.queryByText(/SYNTHETIC_PRIVATE_DIAGNOSTIC/)).not.toBeInTheDocument();
  });
});
