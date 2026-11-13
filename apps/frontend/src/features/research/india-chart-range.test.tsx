import { cloneElement, type ReactElement } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import common from "@/i18n/locales/en/common.json";
import type { IndiaChartData } from "@/adapters/shared/research";
import { IndiaChart } from "./india-chart";
const api = vi.hoisted(() => ({ loadIndiaResearchChart: vi.fn() }));
vi.mock("@/adapters", () => api);
vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) =>
      key.split(".").reduce<unknown>((v, k) => (v as Record<string, unknown>)[k], common),
  }),
}));
vi.mock("recharts", async (importOriginal) => {
  const actual = await importOriginal<typeof import("recharts")>();
  return {
    ...actual,
    ResponsiveContainer: ({ children }: { children: ReactElement }) =>
      cloneElement(children as ReactElement<{ width: number; height: number }>, {
        width: 800,
        height: 360,
      }),
  };
});
const key = "NSE_EQ|INE213A01029";
function report(range: "month" | "year", count: number): IndiaChartData {
  return {
    listing: { key, exchange: "NSE", symbol: "TEST", name: "Fictional Test", isin: "INE213A01029" },
    range,
    interval: "1 day",
    retrievedAt: "2026-10-27T00:00:00Z",
    quote: { price: 140, lastTradeAt: "2026-10-26T10:00:00Z" },
    candles: Array.from({ length: count }, (_, i) => ({
      time:
        range === "month"
          ? `2026-10-${String(i + 1).padStart(2, "0")}T00:00:00+05:30`
          : new Date(
              Date.UTC(2025, 9, 27) + Math.floor((i * 364) / (count - 1)) * 86400000,
            ).toISOString(),
      open: 90 + i / 10,
      high: 92 + i / 10,
      low: 89 + i / 10,
      close: 91 + i / 10,
      volume: 100 + i,
    })),
  };
}
it("opens the full year after a zoomed month even when year data is already cached, then refreshes the year", async () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  client.setQueryData(["india-research-chart", key, "year"], report("year", 247));
  let resolveYear: (value: IndiaChartData) => void = () => { throw Error("Year request not started"); };
  api.loadIndiaResearchChart.mockImplementation((_key: string, range: string) =>
    range === "year"
      ? new Promise<IndiaChartData>((resolve) => {
          resolveYear = resolve;
        })
      : Promise.resolve(report("month", 12)),
  );
  const { container } = render(
    <QueryClientProvider client={client}>
      <IndiaChart instrumentKey={key} paused={false} />
    </QueryClientProvider>,
  );
  const points = () =>
    container
      .querySelector(".research-candle-series .recharts-line-curve")
      ?.getAttribute("d")
      ?.match(/[ML]/g)?.length;
  fireEvent.click(screen.getByRole("button", { name: common.researchChart.open }));
  await waitFor(() => expect(points()).toBe(12));
  fireEvent.click(screen.getByRole("button", { name: common.researchChart.zoomin }));
  await waitFor(() => expect(points()).toBe(6));
  fireEvent.change(screen.getByLabelText(common.researchChart.range), {
    target: { value: "year" },
  });
  await waitFor(() => expect(points()).toBe(247));
  resolveYear(report("year", 248));
  await waitFor(() => expect(points()).toBe(248));
  await waitFor(() =>
    expect(screen.getByRole("button", { name: common.researchChart.refresh })).toBeEnabled(),
  );
  fireEvent.click(screen.getByRole("button", { name: common.researchChart.refresh }));
  await waitFor(() => expect(api.loadIndiaResearchChart).toHaveBeenCalledTimes(3));
  resolveYear({
    ...report("year", 248),
    quote: { price: 141, lastTradeAt: "2026-10-27T06:36:00Z" },
  });
  await screen.findByText(/Last trade snapshot: ₹141/);
  await waitFor(()=>expect(container.querySelector(".recharts-line-dots circle")).not.toBeNull());
  expect(screen.queryByRole("slider")).not.toBeInTheDocument();
  expect(api.loadIndiaResearchChart).toHaveBeenLastCalledWith(key, "year");
});
