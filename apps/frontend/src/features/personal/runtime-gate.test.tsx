import { act, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, expect, it, vi } from "vitest";
import common from "@/i18n/locales/en/common.json";
import { RuntimeGate } from "./runtime-gate";

const api = vi.hoisted(() => ({ getReviewedImportStatus: vi.fn() }));
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
beforeEach(() => vi.clearAllMocks());
function open() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const normal = vi.fn(() => <p>Normal providers mounted</p>);
  const personal = vi.fn(() => <p>Personal providers mounted</p>);
  const Normal = normal;
  render(
    <QueryClientProvider client={client}>
      <RuntimeGate personal={personal}>
        <Normal />
      </RuntimeGate>
    </QueryClientProvider>,
  );
  return { client, normal, personal };
}
it("mounts neither application until host mode is verified", async () => {
  let resolve!: (status: unknown) => void;
  api.getReviewedImportStatus.mockImplementation(
    () =>
      new Promise((done) => {
        resolve = done;
      }),
  );
  const { normal, personal } = open();
  expect(normal).not.toHaveBeenCalled();
  expect(personal).not.toHaveBeenCalled();
  await act(async () => {
    resolve({ supported: true, accounts: [] });
    await Promise.resolve();
  });
  await screen.findByText("Personal providers mounted");
  expect(normal).not.toHaveBeenCalled();
});
it("preserves the normal application only on explicit false", async () => {
  api.getReviewedImportStatus.mockResolvedValue({ supported: false, accounts: [] });
  const { personal } = open();
  await screen.findByText("Normal providers mounted");
  expect(personal).not.toHaveBeenCalled();
});
it.each(["failure", "malformed", "missingAccounts"])(
  "does not admit providers on %s status",
  async (kind) => {
    if (kind === "failure")
      api.getReviewedImportStatus.mockRejectedValue(new Error("SYNTHETIC_PRIVATE_DIAGNOSTIC"));
    else
      api.getReviewedImportStatus.mockResolvedValue(
        kind === "missingAccounts" ? { supported: true } : { supported: "true" },
      );
    const { normal, personal } = open();
    await screen.findByText(common.personal.modeError);
    expect(normal).not.toHaveBeenCalled();
    expect(personal).not.toHaveBeenCalled();
    expect(screen.queryByText(/SYNTHETIC_PRIVATE_DIAGNOSTIC/)).not.toBeInTheDocument();
  },
);
it("requires reload rather than mounting normal providers over a personal in-memory session", async () => {
  api.getReviewedImportStatus.mockResolvedValue({ supported: true, accounts: [] });
  const { client, normal } = open();
  await screen.findByText("Personal providers mounted");
  act(() => {
    client.setQueryData(["reviewed-import-status", "synthetic-profile"], {
      supported: false,
      accounts: [],
    });
  });
  await screen.findByText(common.personal.modeChanged);
  expect(normal).not.toHaveBeenCalled();
  expect(screen.queryByText("Personal providers mounted")).not.toBeInTheDocument();
});
