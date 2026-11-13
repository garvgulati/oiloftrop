import { act, fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, expect, it, vi } from "vitest";
import common from "@/i18n/locales/en/common.json";
import type { SavedReviewedSnapshot } from "@/adapters/shared/reviewed-import";
import { PersonalHoldingsHome } from "./holdings-home";

const api = vi.hoisted(() => ({ latestReviewedImport: vi.fn() }));
vi.mock("@/adapters", () => api);
vi.mock("@/features/profiles/profile-context", () => ({
  useProfile: () => ({ profile: { id: "synthetic-profile" } }),
}));
vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) =>
      key.split(".").reduce<unknown>((v, k) => (v as Record<string, unknown>)[k], common),
  }),
}));
const accounts = [
  { id: "e2d7dd79-1c8c-4e25-8dd3-e05be059b62b", name: "Synthetic A", currency: "INR" as const },
  { id: "f7fd74c8-261b-47d8-a7a4-5304967b6dbf", name: "Synthetic B", currency: "INR" as const },
];
function record(id: string, symbol = "SYNTH"): SavedReviewedSnapshot {
  return {
    receipt: {
      schemaVersion: 1,
      reviewId: "synthetic-review",
      inputHash: "synthetic-hash",
      accountId: id,
      snapshotId: "synthetic-snapshot",
      asOf: "2026-10-28",
      importedAt: "2026-10-29T03:00:00Z",
    },
    snapshot: {
      schemaVersion: 1,
      accountId: id,
      currency: "INR",
      source: { label: "Synthetic broker", account: "Synthetic source" },
      asOf: "2026-10-28",
      settledScope: "partial",
      otherExposure: "unknown",
      cash: null,
      positions: [
        {
          isin: "INE213A01029",
          symbol,
          name: "Synthetic equity",
          currency: "INR",
          sourceVenue: "XNSE",
          valuationVenue: "XNSE",
          ordinaryEquityConfirmed: true,
          quantity: "1.23456789",
          averageCost: null,
          price: null,
        },
      ],
    },
    assetIds: {},
    historyAvailable: false,
  };
}
function open() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <PersonalHoldingsHome accounts={accounts} />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}
beforeEach(() => vi.clearAllMocks());
it("shows exact source values and unknowns without generating aggregate totals or P&L", async () => {
  api.latestReviewedImport.mockResolvedValue(record(accounts[0].id));
  open();
  await screen.findByText(/1.23456789/);
  expect(screen.getByText(/Average cost per share.*Unknown/)).toBeInTheDocument();
  expect(screen.getByText(common.personal.savedScope)).toBeInTheDocument();
  expect(screen.getByText(common.reviewedImport.historyUnavailable)).toBeInTheDocument();
  expect(api.latestReviewedImport).toHaveBeenCalledWith({ accountId: accounts[0].id });
});
it("rejects a record returned for another account", async () => {
  api.latestReviewedImport.mockResolvedValue(record(accounts[1].id));
  open();
  await screen.findByText(common.personal.recordError);
  expect(screen.queryByText(/1.23456789/)).not.toBeInTheDocument();
});
it("does not replace the selected account with a late response from another account", async () => {
  let resolve!: (value: SavedReviewedSnapshot) => void;
  api.latestReviewedImport.mockImplementation(({ accountId }: { accountId: string }) =>
    accountId === accounts[0].id
      ? new Promise<SavedReviewedSnapshot>((done) => {
          resolve = done;
        })
      : Promise.resolve(record(accountId, "SECOND")),
  );
  open();
  fireEvent.change(screen.getByLabelText(common.reviewedImport.targetAccount), {
    target: { value: accounts[1].id },
  });
  await screen.findByText(/SECOND/);
  await act(async () => {
    resolve(record(accounts[0].id, "FIRST"));
    await Promise.resolve();
  });
  expect(screen.getByText(/SECOND/)).toBeInTheDocument();
  expect(screen.queryByText(/FIRST/)).not.toBeInTheDocument();
});
