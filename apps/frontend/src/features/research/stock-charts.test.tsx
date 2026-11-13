import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import common from "@/i18n/locales/en/common.json";
import { IndiaChart } from "./india-chart";
import { UsChart } from "./us-chart";
import { usChartUrl } from "./us-chart-url";
const api = vi.hoisted(() => ({ loadIndiaResearchChart: vi.fn() }));
vi.mock("@/adapters", () => api);
vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) =>
      key.split(".").reduce<unknown>((v, k) => (v as Record<string, unknown>)[k], common),
  }),
}));
const text = common.researchChart;
const data = {
  listing: {
    exchange: "NSE",
    symbol: "TEST",
    isin: "INE213A01029",
    key: "NSE_EQ|INE213A01029",
    name: "Fictional Test",
  },
  range: "month",
  interval: "1 day",
  quote: { price: 12, lastTradeAt: "2023-10-18T10:29:32.130Z" },
  retrievedAt: "2026-10-27T00:00:00Z",
  candles: [
    { time: "2026-10-23T00:00:00+05:30", open: 10, high: 13, low: 9, close: 12, volume: 100 },
  ],
};
beforeEach(() => {
  vi.clearAllMocks();
  api.loadIndiaResearchChart.mockResolvedValue(data);
});
it("loads India only after opening, labels old trades and clears its query after closing", async () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const writes = vi.spyOn(Storage.prototype, "setItem");
  render(
    <QueryClientProvider client={client}>
      <IndiaChart instrumentKey={data.listing.key} paused={false} />
    </QueryClientProvider>,
  );
  expect(api.loadIndiaResearchChart).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: text.open }));
  await screen.findByRole("group", { name: text.lineLabel });
  expect(api.loadIndiaResearchChart).toHaveBeenCalledExactlyOnceWith(data.listing.key, "month");
  expect(screen.getByText(text.stale)).toBeVisible();
  expect(screen.getByLabelText(text.automatic)).not.toBeChecked();
  fireEvent.click(screen.getByRole("button", { name: text.close }));
  expect(screen.queryByRole("group", { name: text.lineLabel })).not.toBeInTheDocument();
  await waitFor(() => expect(client.getQueryCache().getAll()).toHaveLength(0));
  expect(writes).not.toHaveBeenCalled();
  writes.mockRestore();
});
it("does not substitute an old chart after refresh failure or an empty intraday result", async () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <IndiaChart instrumentKey={data.listing.key} paused={false} />
    </QueryClientProvider>,
  );
  fireEvent.click(screen.getByRole("button", { name: text.open }));
  await screen.findByRole("group", {name:text.lineLabel});
  api.loadIndiaResearchChart.mockRejectedValueOnce(Error("INDIA_AUTH_REQUIRED"));
  fireEvent.click(screen.getByRole("button", { name: text.refresh }));
  await screen.findByRole("alert");
  expect(screen.queryByRole("group", { name: text.lineLabel })).not.toBeInTheDocument();
  api.loadIndiaResearchChart.mockResolvedValueOnce({ ...data, range: "day", candles: [] });
  fireEvent.change(screen.getByLabelText(text.range), { target: { value: "day" } });
  await screen.findByText(text.empty);
  expect(screen.queryByRole("group", { name: text.lineLabel })).not.toBeInTheDocument();
});
it("keeps US frames unloaded until exchange selection and explicit open, then unloads on a listing change", () => {
  render(<UsChart tickers={["MSFT"]} />);
  expect(document.querySelector("iframe")).toBeNull();
  expect(screen.getByRole("button", { name: text.open })).toBeDisabled();
  fireEvent.change(screen.getByLabelText(text.exchange), { target: { value: "NASDAQ" } });
  fireEvent.click(screen.getByRole("button", { name: text.open }));
  const frame = screen.getByTitle("NASDAQ:MSFT TradingView chart");
  expect(frame).toHaveAttribute("referrerpolicy", "no-referrer");
  expect(frame).toHaveAttribute("sandbox", "allow-scripts allow-same-origin allow-popups");
  expect(screen.getByText(/Delayed \/ provider-limited/)).toBeVisible();
  fireEvent.change(screen.getByLabelText(text.exchange), { target: { value: "NYSE" } });
  expect(document.querySelector("iframe")).toBeNull();
});
it("constructs only the fixed widget URL with a bounded public symbol and no page identity", () => {
  expect(usChartUrl("https://evil.example", "MSFT")).toBeNull();
  expect(usChartUrl("NASDAQ", "MSFT#private")).toBeNull();
  const url = new URL(usChartUrl("NASDAQ", "MSFT")!);
  expect(url.origin + url.pathname).toBe(
    "https://www.tradingview-widget.com/embed-widget/advanced-chart/",
  );
  const settings = JSON.parse(decodeURIComponent(url.hash.slice(1))) as Record<string, unknown>;
  expect(settings.symbol).toBe("NASDAQ:MSFT");
  expect(settings.allow_symbol_change).toBe(false);
  expect(settings).not.toHaveProperty("page-uri");
  expect(settings).not.toHaveProperty("utm_source");
});
