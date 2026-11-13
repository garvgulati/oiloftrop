import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import common from "@/i18n/locales/en/common.json";
import { IndiaRatios } from "./india-ratios";
const api = vi.hoisted(() => ({ loadIndiaResearchRatios: vi.fn() }));
vi.mock("@/adapters", () => api);
vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) =>
      key.split(".").reduce<unknown>((v, k) => (v as Record<string, unknown>)[k], common),
    i18n: { language: "en" },
  }),
}));
const text = common.researchRatios;
const key = "NSE_EQ|INE213A01029";
const data = {
  listing: { name: "Fictional issuer", symbol: "TEST", isin: "INE213A01029", exchange: "NSE", key },
  ratios: [
    { name: "P/E", company: 20, sector: null },
    { name: "ROE", company: -5, sector: 10 },
  ],
  retrievedAt: "2026-10-27T00:00:00Z",
};
it("loads explicitly, keeps missing values unknown and clears data/cache without storage writes", async () => {
  const client = new QueryClient();
  const writes = vi.spyOn(Storage.prototype, "setItem");
  api.loadIndiaResearchRatios.mockResolvedValue(data);
  const view = render(
    <QueryClientProvider client={client}>
      <IndiaRatios instrumentKey={key} paused={false} />
    </QueryClientProvider>,
  );
  expect(api.loadIndiaResearchRatios).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: text.load }));
  await screen.findByText("20×");
  expect(screen.getByText("-5%")).toBeVisible();
  expect(screen.getAllByText(text.unknown).length).toBeGreaterThan(1);
  expect(screen.getByText(text.gaps)).toBeVisible();
  api.loadIndiaResearchRatios.mockRejectedValueOnce(new Error("SOURCE_UNAVAILABLE"));
  fireEvent.click(screen.getByRole("button", { name: text.load }));
  await screen.findByRole("alert");
  expect(screen.queryByText("20×")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: text.clear }));
  await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  view.unmount();
  await waitFor(() => expect(client.getMutationCache().getAll()).toHaveLength(0));
  expect(writes).not.toHaveBeenCalled();
  writes.mockRestore();
});
it("rejects mismatched instruments and duplicate ratio names", async () => {
  const client = new QueryClient();
  api.loadIndiaResearchRatios
    .mockResolvedValueOnce({ ...data, listing: { ...data.listing, key: "wrong" } })
    .mockResolvedValueOnce({ ...data, ratios: [data.ratios[0], data.ratios[0]] });
  render(
    <QueryClientProvider client={client}>
      <IndiaRatios instrumentKey={key} paused={false} />
    </QueryClientProvider>,
  );
  for (let i = 0; i < 2; i++) {
    fireEvent.click(screen.getByRole("button", { name: text.load }));
    await screen.findByRole("alert");
    expect(screen.queryByText("20×")).toBeNull();
  }
});
