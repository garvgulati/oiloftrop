import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@wealthfolio/ui";
import {
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import type { IndiaChartData } from "@/adapters/shared/research";
import { moveChartWindow, windowBounds, type TimeWindow } from "./chart-window";

type CandlePoint = IndiaChartData["candles"][number] & {
  timestamp: number;
  kind: "candle";
  tradePrice?: number;
};
interface TradePoint {
  timestamp: number;
  time: string;
  kind: "trade";
  close: null;
  tradePrice: number;
}
type Point = CandlePoint | TradePoint;
const money = (value: number) => `₹${value.toLocaleString("en-IN", { maximumFractionDigits: 2 })}`;
const date = (value: number) =>
  new Date(value).toLocaleString("en-IN", { timeZone: "Asia/Kolkata", hour12: false });
function TimeTick({
  x = 0,
  y = 0,
  payload,
  intraday,
}: {
  x?: number;
  y?: number;
  payload?: { value: number };
  intraday: boolean;
}) {
  if (!payload) return null;
  const day = new Intl.DateTimeFormat("en-IN", {
    timeZone: "Asia/Kolkata",
    day: "2-digit",
    month: "short",
  }).format(payload.value);
  const second = new Intl.DateTimeFormat("en-IN", {
    timeZone: "Asia/Kolkata",
    ...(intraday
      ? ({ hour: "2-digit", minute: "2-digit", hour12: false } as const)
      : ({ year: "numeric" } as const)),
  }).format(payload.value);
  return (
    <text x={x} y={y + 12} textAnchor="middle" fill="currentColor" fontSize={11}>
      <tspan x={x}>{day}</tspan>
      <tspan x={x} dy={14}>
        {second}
      </tspan>
    </text>
  );
}
function CandleTooltip({
  active,
  payload,
}: {
  active?: boolean;
  payload?: readonly { payload?: Point }[];
}) {
  const { t } = useTranslation("common");
  const point = payload?.[0]?.payload;
  if (!active || !point) return null;
  return (
    <div
      className="bg-background max-w-[210px] space-y-1 rounded border p-3 text-xs shadow"
      role="status"
    >
      <p>{date(point.timestamp)} IST</p>
      {point.kind === "trade" ? (
        <>
          <p>
            {t("researchChart.lastPrice")}: {money(point.tradePrice)}
          </p>
          <p>{t("researchChart.tradePointNote")}</p>
        </>
      ) : (
        <>
          {(["open", "high", "low", "close"] as const).map((key) => (
            <p key={key}>
              {t(
                `researchChart.${key === "open" ? "openPrice" : key === "close" ? "closePrice" : key}`,
              )}
              : {money(point[key])}
            </p>
          ))}
          <p>
            {t("researchChart.volume")}: {point.volume.toLocaleString("en-IN")}
          </p>
          {point.tradePrice !== undefined && (
            <p>
              {t("researchChart.lastPrice")}: {money(point.tradePrice)}
            </p>
          )}
        </>
      )}
    </div>
  );
}

export function InteractivePriceChart({
  candles,
  intraday,
  quote,
}: {
  candles: IndiaChartData["candles"];
  intraday: boolean;
  quote?: IndiaChartData["quote"];
}) {
  const { t } = useTranslation("common");
  const tr = (key: string) => t(`researchChart.${key}`);
  const data = useMemo(
    () =>
      candles.map(
        (point): CandlePoint => ({ ...point, timestamp: Date.parse(point.time), kind: "candle" }),
      ),
    [candles],
  );
  const times = useMemo(() => data.map((p) => p.timestamp), [data]);
  const [window, setWindow] = useState<TimeWindow | null>(null);
  const bounds = windowBounds(times, window);
  if (!data.length) return null;
  const [start, end] = bounds;
  const visibleData = data.slice(start, end + 1);
  const quoteTime = quote ? Date.parse(quote.lastTradeAt) : NaN;
  const showTrade =
    quote &&
    Number.isFinite(quoteTime) &&
    Number.isFinite(quote.price) &&
    quote.price > 0 &&
    end === data.length - 1 &&
    quoteTime >= times[end];
  const plottedData: Point[] = [...visibleData];
  if (showTrade) {
    if (quoteTime === times[end])
      plottedData[plottedData.length - 1] = {
        ...visibleData[visibleData.length - 1],
        tradePrice: quote.price,
      };
    else
      plottedData.push({
        kind: "trade",
        timestamp: quoteTime,
        time: quote.lastTradeAt,
        close: null,
        tradePrice: quote.price,
      });
  }
  return (
    <div className="space-y-3" role="group" aria-label={tr("lineLabel")}>
      <p className="text-sm">{tr("interactHelp")}</p>
      <div className="flex flex-wrap gap-2">
        {(["in", "out", "left", "right"] as const).map((action) => (
          <Button
            key={action}
            variant="outline"
            size="sm"
            disabled={
              action === "in"
                ? end - start < 2
                : action === "left"
                  ? start === 0
                  : action === "right"
                    ? end === data.length - 1
                    : start === 0 && end === data.length - 1
            }
            onClick={() => setWindow(moveChartWindow(times, bounds, action))}
          >
            {tr(`zoom${action}`)}
          </Button>
        ))}
        <Button
          variant="outline"
          size="sm"
          disabled={window === null}
          onClick={() => setWindow(null)}
        >
          {tr("resetZoom")}
        </Button>
      </div>
      {showTrade && (
        <p className="text-sm">
          {tr("tradeLegend")}: {money(quote.price)} · {date(quoteTime)} IST
        </p>
      )}
      <p className="text-xs">{tr("priceAxis")}</p>
      <div className="h-[360px] w-full min-w-0">
        <ResponsiveContainer width="100%" height="100%" minWidth={0}>
          <LineChart
            data={plottedData}
            margin={{ left: 0, right: 28, top: 12, bottom: 8 }}
            accessibilityLayer
          >
            <CartesianGrid stroke="var(--border)" strokeDasharray="3 3" />
            <XAxis
              dataKey="timestamp"
              type="number"
              scale="time"
              domain={["dataMin", "dataMax"]}
              tick={<TimeTick intraday={intraday} />}
              tickCount={4}
              height={48}
              minTickGap={20}
              interval="preserveStartEnd"
            />
            <YAxis
              width={68}
              tick={{ fontSize: 11, fill: "currentColor" }}
              tickFormatter={(value: number) =>
                `₹${value.toLocaleString("en-IN", { notation: "compact", maximumFractionDigits: 2 })}`
              }
              domain={([min, max]: readonly [number, number]) => {
                const pad = Math.max((max - min) * 0.05, Math.abs(max) * 0.005, 0.01);
                return [Math.max(0, min - pad), max + pad];
              }}
            />
            <Tooltip
              isAnimationActive={false}
              cursor={{ stroke: "var(--foreground)", strokeDasharray: "3 3" }}
              content={(props) => (
                <CandleTooltip
                  {...(props as unknown as {
                    active?: boolean;
                    payload?: readonly { payload?: Point }[];
                  })}
                />
              )}
            />
            <Line
              className="research-candle-series"
              dataKey="close"
              type="linear"
              stroke="var(--primary)"
              strokeWidth={2}
              dot={end === start ? { r: 4 } : false}
              activeDot={{ r: 4 }}
              isAnimationActive={false}
            />
            <Line
              className="research-trade-series"
              dataKey="tradePrice"
              stroke="var(--primary)"
              dot={{ r: 5 }}
              activeDot={{ r: 6 }}
              isAnimationActive={false}
            />
          </LineChart>
        </ResponsiveContainer>
      </div>
      <p className="text-xs">{tr("timeAxis")}</p>
    </div>
  );
}
