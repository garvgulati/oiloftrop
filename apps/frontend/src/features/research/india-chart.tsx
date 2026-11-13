import { loadIndiaResearchChart } from "@/adapters";
import type { IndiaChartData } from "@/adapters/shared/research";
import { useQuery } from "@tanstack/react-query";
import { Button } from "@wealthfolio/ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { InteractivePriceChart } from "./interactive-price-chart";

export function IndiaChart({ instrumentKey, paused }: { instrumentKey: string; paused: boolean }) {
  const { t } = useTranslation("common");
  const tr = (key: string) => t(`researchChart.${key}`);
  const [open, setOpen] = useState(false);
  return (
    <section className="space-y-3 rounded-xl border p-4" aria-label={tr("indiaTitle")}>
      <h4 className="font-medium">{tr("indiaTitle")}</h4>
      <p className="text-sm">{tr("indiaScope")}</p>
      <Button variant="outline" disabled={paused} onClick={() => setOpen(!open)}>
        {tr(open ? "close" : "open")}
      </Button>
      {open && <IndiaChartContent instrumentKey={instrumentKey} paused={paused} />}
    </section>
  );
}

function IndiaChartContent({ instrumentKey, paused }: { instrumentKey: string; paused: boolean }) {
  const { t } = useTranslation("common");
  const tr = (key: string) => t(`researchChart.${key}`);
  const [range, setRange] = useState<IndiaChartData["range"]>("month");
  const [automatic, setAutomatic] = useState(false);
  const query = useQuery({
    queryKey: ["india-research-chart", instrumentKey, range],
    queryFn: () => loadIndiaResearchChart(instrumentKey, range),
    enabled: !paused,
    gcTime: 0,
    staleTime: 0,
    retry: false,
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
    refetchInterval: (query) =>
      automatic && !paused && query.state.status !== "error" ? 15000 : false,
    refetchIntervalInBackground: false,
  });
  const report =
    !query.isError && query.data?.range === range && query.data.listing.key === instrumentKey
      ? query.data
      : undefined;
  const date = (value: string) =>
    new Date(value).toLocaleString("en-IN", { timeZone: "Asia/Kolkata" });
  const stale = report && Date.now() - Date.parse(report.quote.lastTradeAt) > 120000;
  const values = report?.candles.map((p) => p.close) ?? [];
  const low = Math.min(...values),
    high = Math.max(...values);
  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-3">
        <label className="text-sm">
          {tr("range")}{" "}
          <select
            className="bg-background min-h-10 rounded border p-2"
            value={range}
            disabled={query.isFetching || paused}
            onChange={(e) => setRange(e.target.value as IndiaChartData["range"])}
          >
            <option value="day">{tr("day")}</option>
            <option value="month">{tr("month")}</option>
            <option value="year">{tr("year")}</option>
          </select>
        </label>
        <Button
          variant="outline"
          disabled={query.isFetching || paused}
          onClick={() => void query.refetch()}
        >
          {tr("refresh")}
        </Button>
        <label className="flex min-h-10 items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={automatic}
            onChange={(e) => setAutomatic(e.target.checked)}
          />
          {tr("automatic")}
        </label>
      </div>
      {query.isFetching && (
        <p role="status" className="text-sm">
          {tr("loading")}
        </p>
      )}
      {query.isError && (
        <p role="alert" className="text-sm">
          {tr("error")}
        </p>
      )}
      {report && (
        <>
          <p className="break-words text-sm">
            {report.listing.exchange}:{report.listing.symbol} · {report.listing.isin} · INR ·{" "}
            {report.interval}
          </p>
          <p className="text-sm">
            {tr("lastPrice")}: ₹
            {report.quote.price.toLocaleString("en-IN", { maximumFractionDigits: 2 })}.{" "}
            {tr("lastTrade")}: {date(report.quote.lastTradeAt)} IST
          </p>
          <p className="text-sm">
            {tr("retrieved")}: {date(report.retrievedAt)} IST
          </p>
          {stale && <p className="text-sm font-medium">{tr("stale")}</p>}
          {values.length > 0 ? (
            <>
              <p className="text-sm">
                {tr("closeRange")}: ₹{low.toFixed(2)} – ₹{high.toFixed(2)}
              </p>
              <InteractivePriceChart
                key={`${instrumentKey}:${range}`}
                candles={report.candles}
                intraday={report.range === "day"}
                quote={report.quote}
              />
              <p className="text-sm">
                {tr("loadedDates")}: {date(report.candles[0].time)} –{" "}
                {date(report.candles.at(-1)!.time)} IST
              </p>
              <details>
                <summary className="min-h-10 cursor-pointer text-sm">{tr("table")}</summary>
                <div className="overflow-x-auto">
                  <table className="w-full text-left text-sm">
                    <thead>
                      <tr>
                        {["time", "openPrice", "high", "low", "closePrice", "volume"].map((key) => (
                          <th className="p-2" key={key}>
                            {tr(key)}
                          </th>
                        ))}
                      </tr>
                    </thead>
                    <tbody>
                      {report.candles.map((p) => (
                        <tr key={p.time}>
                          <th className="p-2 font-normal">{date(p.time)} IST</th>
                          {[p.open, p.high, p.low, p.close, p.volume].map((v, i) => (
                            <td className="p-2" key={i}>
                              {v.toLocaleString("en-IN")}
                            </td>
                          ))}
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </details>
            </>
          ) : (
            <p className="text-sm">{tr("empty")}</p>
          )}
          <p className="text-muted-foreground text-sm">{tr("candleNote")}</p>
        </>
      )}
    </div>
  );
}
