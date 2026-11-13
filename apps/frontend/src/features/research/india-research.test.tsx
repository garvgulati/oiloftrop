import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import common from "@/i18n/locales/en/common.json";
import { IndiaResearch } from "./india-research";
import { sampleIndiaData } from "./india-summary.fixture";
const api = vi.hoisted(() => ({
  getIndiaResearchConnection: vi.fn(),
  setIndiaResearchConnection: vi.fn(),
  removeIndiaResearchConnection: vi.fn(),
  searchIndiaResearch: vi.fn(),
  analyseIndiaResearch: vi.fn(),
}));
vi.mock("@/adapters", () => api);
vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) =>
      key.split(".").reduce<unknown>((v, k) => (v as Record<string, unknown>)[k], common),
    i18n: { language: "en" },
  }),
}));
function open() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return {
    client,
    ...render(
      <QueryClientProvider client={client}>
        <IndiaResearch profileId="fictional-profile" />
      </QueryClientProvider>,
    ),
  };
}
beforeEach(() => {
  vi.clearAllMocks();
  api.getIndiaResearchConnection.mockResolvedValue({ configured: true, expiresOn: "2027-01-01" });
  api.searchIndiaResearch.mockResolvedValue({
    listings: [sampleIndiaData().listing],
    retrievedAt: "2026-10-26T00:00:00Z",
  });
  api.analyseIndiaResearch.mockResolvedValue(sampleIndiaData());
});
async function choose() {
  await screen.findByText(/Upstox connection configured/);
  fireEvent.change(screen.getByLabelText(common.researchIndia.query), {
    target: { value: "TEST" },
  });
  fireEvent.click(screen.getByRole("button", { name: common.researchIndia.search }));
  const select = await screen.findByRole("combobox", { name: common.researchIndia.company });
  fireEvent.change(select, { target: { value: sampleIndiaData().listing.key } });
}
it("searches and analyses only on explicit action, with selected identity and basis", async () => {
  open();
  await screen.findByText(/Upstox connection configured/);
  expect(api.searchIndiaResearch).not.toHaveBeenCalled();
  expect(api.analyseIndiaResearch).not.toHaveBeenCalled();
  await choose();
  expect(api.searchIndiaResearch).toHaveBeenCalledWith("TEST", expect.anything());
  fireEvent.click(screen.getByRole("button", { name: common.researchIndia.analyse }));
  await screen.findByRole("heading", { name: /Fictional Source Test/ });
  expect(api.analyseIndiaResearch).toHaveBeenCalledWith("NSE_EQ|INE213A01029", "consolidated");
  expect(screen.getByText(/not the same as borrowings/)).toBeVisible();
  expect(screen.getByText(common.researchIndia.incomeCaption)).toBeVisible();
  expect(
    screen.getByRole("region", { name: common.researchRecommendation.title }),
  ).toHaveTextContent(common.researchRecommendation.wait);
});
it("clears displayed results when the basis or selected company changes", async () => {
  open();
  await choose();
  fireEvent.click(screen.getByRole("button", { name: common.researchIndia.analyse }));
  await screen.findByRole("heading", { name: /Fictional Source Test/ });
  fireEvent.change(screen.getByRole("combobox", { name: common.researchIndia.basis }), {
    target: { value: "standalone" },
  });
  expect(screen.queryByRole("heading", { name: /Fictional Source Test/ })).not.toBeInTheDocument();
  expect(
    screen.queryByRole("region", { name: common.researchRecommendation.title }),
  ).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: common.researchIndia.clear }));
  expect(screen.getByLabelText(common.researchIndia.query)).toHaveValue("");
});
it("writes credentials without retaining their variables in the mutation cache or browser storage", async () => {
  api.getIndiaResearchConnection.mockResolvedValueOnce({ configured: false, expiresOn: null });
  api.setIndiaResearchConnection.mockResolvedValueOnce({
    configured: true,
    expiresOn: "2027-01-01",
  });
  const writes = vi.spyOn(Storage.prototype, "setItem");
  const view = open();
  await screen.findByText(common.researchIndia.notConnected);
  const summary = screen.getByText(common.researchIndia.connection, { exact: true });
  summary.closest("details")!.open = true;
  fireEvent.change(screen.getByLabelText(common.researchIndia.token), {
    target: { value: "synthetic-token-canary-123456" },
  });
  fireEvent.change(screen.getByLabelText(common.researchIndia.expiry), {
    target: { value: "2027-01-01" },
  });
  fireEvent.click(screen.getByLabelText(common.researchIndia.freeConfirm));
  fireEvent.click(screen.getByLabelText(common.researchIndia.privateConfirm));
  fireEvent.click(screen.getByRole("button", { name: common.researchIndia.saveConnection }));
  await waitFor(() => expect(api.setIndiaResearchConnection).toHaveBeenCalledTimes(1));
  await waitFor(() => expect(screen.getByLabelText(common.researchIndia.token)).toHaveValue(""));
  expect(
    JSON.stringify(
      view.client
        .getMutationCache()
        .getAll()
        .map((v) => v.state.variables),
    ),
  ).not.toContain("synthetic-token-canary");
  expect(writes).not.toHaveBeenCalled();
  writes.mockRestore();
});
