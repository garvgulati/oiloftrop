import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import common from "@/i18n/locales/en/common.json";
import type { ImportReview, ReviewedSnapshotInput } from "@/adapters/shared/reviewed-import";
import { ReviewedImportPanel } from "./reviewed-import";

const api = vi.hoisted(() => ({
  createReviewedImportAccount: vi.fn(),
  prepareReviewedImport: vi.fn(),
  confirmReviewedImport: vi.fn(),
  discardReviewedImport: vi.fn(),
  latestReviewedImport: vi.fn(),
}));
vi.mock("@/adapters", () => api);
vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) =>
      key.split(".").reduce<unknown>((v, k) => (v as Record<string, unknown>)[k], common),
  }),
}));
const text = common.reviewedImport;
const accounts = [
  {
    id: "e2d7dd79-1c8c-4e25-8dd3-e05be059b62b",
    name: "Synthetic account",
    currency: "INR" as const,
  },
];
function review(input: ReviewedSnapshotInput): ImportReview {
  return {
    reviewId: "50d049b9-e115-452f-8d72-e358fc9fe379",
    inputHash: "a".repeat(64),
    expiresAt: new Date(Date.now() + 1800000).toISOString(),
    accountName: "Synthetic account",
    snapshot: input,
    changes: [{ isin: input.positions[0].isin, kind: "added" }],
    warnings: ["CASH_UNKNOWN"],
  };
}
function fill() {
  for (const [key, value] of Object.entries({
    targetAccount: accounts[0].id,
    sourceDate: "2026-10-28",
    sourceLabel: "Synthetic broker",
    sourceAccount: "Synthetic A",
    isin: "INE213A01029",
    symbol: "ONGC",
    name: "Synthetic ordinary share",
    quantity: "1.23456789",
    sourceVenue: "XNSE",
    valuationVenue: "XNSE",
  })) {
    fireEvent.change(screen.getByLabelText(text[key as keyof typeof text] as string), {
      target: { value },
    });
  }
  fireEvent.click(screen.getByLabelText(text.ordinaryEquity));
}
beforeEach(() => {
  vi.clearAllMocks();
  api.discardReviewedImport.mockResolvedValue(undefined);
  api.latestReviewedImport.mockResolvedValue(null);
  api.prepareReviewedImport.mockImplementation((input: ReviewedSnapshotInput) =>
    Promise.resolve({ status: "review", review: review(input) }),
  );
  api.confirmReviewedImport.mockResolvedValue({});
  api.createReviewedImportAccount.mockImplementation(
    (request: { id: string; name: string; currency: "INR" | "USD" }) => Promise.resolve(request),
  );
});

it("creates an account from an empty profile and reuses its request after a lost response", async () => {
  api.createReviewedImportAccount.mockRejectedValueOnce(new Error("SYNTHETIC_PRIVATE_DIAGNOSTIC"));
  render(<ReviewedImportPanel accounts={[]} />);
  fireEvent.change(screen.getByLabelText(text.newAccountName), {
    target: { value: "Synthetic account" },
  });
  expect(api.createReviewedImportAccount).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: text.createAccount }));
  await screen.findByText(text.accountFailed);
  fireEvent.click(screen.getByRole("button", { name: text.createAccount }));
  await waitFor(() =>
    expect(screen.getByLabelText(text.targetAccount)).toHaveValue(
      (api.createReviewedImportAccount.mock.calls[0][0] as { id: string }).id,
    ),
  );
  expect(api.createReviewedImportAccount.mock.calls[0]).toEqual(
    api.createReviewedImportAccount.mock.calls[1],
  );
  expect(screen.queryByText(/SYNTHETIC_PRIVATE_DIAGNOSTIC/)).not.toBeInTheDocument();
});

it("requires a preview, confirms identifiers only, and reloads exact saved unknowns", async () => {
  render(<ReviewedImportPanel accounts={accounts} />);
  fill();
  expect(api.prepareReviewedImport).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: text.prepare }));
  await screen.findByRole("button", { name: text.confirm });
  expect(screen.getByLabelText(text.preview)).toHaveFocus();
  const input = api.prepareReviewedImport.mock.calls[0][0] as ReviewedSnapshotInput;
  expect(input.cash).toBeNull();
  expect(input.positions[0].averageCost).toBeNull();
  expect(input.positions[0].price).toBeNull();
  expect(input.positions[0].quantity).toBe("1.23456789");
  expect(api.confirmReviewedImport).not.toHaveBeenCalled();
  api.latestReviewedImport.mockResolvedValue({
    snapshot: input,
    receipt: {},
    assetIds: {},
    historyAvailable: false,
  });
  fireEvent.click(screen.getByRole("button", { name: text.confirm }));
  await screen.findByText(text.savedRecord);
  expect(api.confirmReviewedImport).toHaveBeenCalledWith({
    reviewId: "50d049b9-e115-452f-8d72-e358fc9fe379",
    inputHash: "a".repeat(64),
  });
  expect(api.latestReviewedImport).toHaveBeenCalledWith({ accountId: accounts[0].id });
  expect(screen.getByText(text.historyUnavailable)).toBeInTheDocument();
});
it("explains a later quote date beside the submit action, preserves inputs and allows correction", async () => {
  render(<ReviewedImportPanel accounts={accounts} />);
  fill();
  fireEvent.change(screen.getByLabelText(text.price), { target: { value: "200" } });
  fireEvent.change(screen.getByLabelText(text.priceDate), { target: { value: "2026-11-03" } });
  fireEvent.change(screen.getByLabelText(text.priceSource), {
    target: { value: "Synthetic broker" },
  });
  fireEvent.click(screen.getByRole("button", { name: text.prepare }));
  const alert = await screen.findByRole("alert");
  expect(alert).toHaveTextContent(text.priceAfterRecord);
  expect(alert.closest("form")).toContainElement(
    screen.getByRole("button", { name: text.prepare }),
  );
  expect(api.prepareReviewedImport).not.toHaveBeenCalled();
  expect(screen.getByLabelText(text.quantity)).toHaveValue("1.23456789");
  expect(screen.getByLabelText(text.sourceDate)).toHaveValue("2026-10-28");
  fireEvent.change(screen.getByLabelText(text.priceDate), { target: { value: "2026-10-28" } });
  fireEvent.click(screen.getByRole("button", { name: text.prepare }));
  await screen.findByRole("button", { name: text.confirm });
  expect(api.prepareReviewedImport).toHaveBeenCalledOnce();
  expect(screen.queryByText(text.priceAfterRecord)).not.toBeInTheDocument();
});
it("editing invalidates the previous preview without saving", async () => {
  render(<ReviewedImportPanel accounts={accounts} />);
  fill();
  fireEvent.click(screen.getByRole("button", { name: text.prepare }));
  await screen.findByRole("button", { name: text.confirm });
  fireEvent.change(screen.getByLabelText(text.quantity), { target: { value: "2" } });
  expect(screen.queryByRole("button", { name: text.confirm })).not.toBeInTheDocument();
  expect(api.discardReviewedImport).toHaveBeenCalledWith({
    reviewId: "50d049b9-e115-452f-8d72-e358fc9fe379",
  });
  expect(api.confirmReviewedImport).not.toHaveBeenCalled();
  expect(screen.getByLabelText(text.quantity)).toHaveValue("2");
});
it("a response arriving after an edit cannot restore a stale confirmation", async () => {
  let resolve!: (result: unknown) => void;
  api.prepareReviewedImport.mockImplementation(
    () =>
      new Promise((done) => {
        resolve = done;
      }),
  );
  render(<ReviewedImportPanel accounts={accounts} />);
  fill();
  fireEvent.click(screen.getByRole("button", { name: text.prepare }));
  await waitFor(() => expect(api.prepareReviewedImport).toHaveBeenCalledOnce());
  fireEvent.change(screen.getByLabelText(text.quantity), { target: { value: "2" } });
  await act(async () => {
    resolve({
      status: "review",
      review: review(api.prepareReviewedImport.mock.calls[0][0] as ReviewedSnapshotInput),
    });
    await Promise.resolve();
  });
  expect(screen.queryByRole("button", { name: text.confirm })).not.toBeInTheDocument();
  expect(api.discardReviewedImport).toHaveBeenCalledOnce();
});
it("expired reviews cannot confirm", async () => {
  api.prepareReviewedImport.mockImplementation((input: ReviewedSnapshotInput) =>
    Promise.resolve({
      status: "review",
      review: { ...review(input), expiresAt: "2000-01-01T00:00:00Z" },
    }),
  );
  render(<ReviewedImportPanel accounts={accounts} />);
  fill();
  fireEvent.click(screen.getByRole("button", { name: text.prepare }));
  expect(await screen.findByRole("button", { name: text.confirm })).toBeDisabled();
  expect(api.confirmReviewedImport).not.toHaveBeenCalled();
});
it("keeps safe retry after a lost save response and separates confirmed save from reload failure", async () => {
  api.confirmReviewedImport
    .mockRejectedValueOnce(new Error("SYNTHETIC_PRIVATE_DIAGNOSTIC"))
    .mockResolvedValueOnce({});
  render(<ReviewedImportPanel accounts={accounts} />);
  fill();
  fireEvent.click(screen.getByRole("button", { name: text.prepare }));
  fireEvent.click(await screen.findByRole("button", { name: text.confirm }));
  await screen.findByText(text.failed);
  expect(screen.queryByText(/SYNTHETIC_PRIVATE_DIAGNOSTIC/)).not.toBeInTheDocument();
  api.latestReviewedImport.mockRejectedValue(new Error("SYNTHETIC_PRIVATE_DIAGNOSTIC"));
  fireEvent.click(screen.getByRole("button", { name: text.confirm }));
  await screen.findByText(text.savedReloadFailed);
  expect(api.confirmReviewedImport.mock.calls[0]).toEqual(api.confirmReviewedImport.mock.calls[1]);
  expect(screen.getByText(text.saved)).toBeInTheDocument();
});
it("does not retain a previous profile’s draft or pending review on remount", async () => {
  const opened = render(<ReviewedImportPanel key="first" accounts={accounts} />);
  fill();
  fireEvent.click(screen.getByRole("button", { name: text.prepare }));
  await screen.findByRole("button", { name: text.confirm });
  opened.rerender(<ReviewedImportPanel key="second" accounts={accounts} />);
  expect(screen.getByLabelText(text.quantity)).toHaveValue("");
  expect(screen.queryByRole("button", { name: text.confirm })).not.toBeInTheDocument();
  expect(api.discardReviewedImport).toHaveBeenCalledOnce();
});
