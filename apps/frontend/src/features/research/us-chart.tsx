import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@wealthfolio/ui";

import { usChartUrl } from "./us-chart-url";

export function UsChart({ tickers }: { tickers: string[] }) {
  const { t } = useTranslation("common");
  const tr = (key: string) => t(`researchChart.${key}`);
  const supported = tickers.filter((value) => /^[A-Z][A-Z0-9.-]{0,19}$/.test(value));
  const [ticker, setTicker] = useState(supported.length === 1 ? supported[0] : "");
  const [exchange, setExchange] = useState("");
  const [open, setOpen] = useState(false);
  const src = usChartUrl(exchange, ticker);
  return (
    <section className="space-y-3 rounded-xl border p-4" aria-label={tr("usTitle")}>
      <h4 className="font-medium">{tr("usTitle")}</h4>
      <p className="text-sm">{tr("usScope")}</p>
      <p className="text-sm">{tr("externalPrivacy")}</p>
      {!supported.length ? (
        <p className="text-sm">{tr("noTicker")}</p>
      ) : (
        <>
          <div className="flex flex-wrap items-end gap-3">
            <label className="space-y-1 text-sm">
              {tr("ticker")}
              <select
                className="bg-background block min-h-10 rounded border p-2"
                value={ticker}
                onChange={(e) => {
                  setOpen(false);
                  setTicker(e.target.value);
                }}
              >
                <option value="">{tr("chooseTicker")}</option>
                {supported.map((v) => (
                  <option key={v} value={v}>
                    {v}
                  </option>
                ))}
              </select>
            </label>
            <label className="space-y-1 text-sm">
              {tr("exchange")}
              <select
                className="bg-background block min-h-10 rounded border p-2"
                value={exchange}
                onChange={(e) => {
                  setOpen(false);
                  setExchange(e.target.value);
                }}
              >
                <option value="">{tr("chooseExchange")}</option>
                {["NASDAQ", "NYSE", "AMEX", "NYSEARCA"].map((v) => (
                  <option key={v} value={v}>
                    {v}
                  </option>
                ))}
              </select>
            </label>
            <Button variant="outline" disabled={!src} onClick={() => setOpen(!open)}>
              {tr(open ? "close" : "open")}
            </Button>
          </div>
          <p className="text-sm">{tr("listingNote")}</p>
          {open && src && (
            <>
              <p className="text-sm font-medium">
                {tr("delayed")} · {exchange}:{ticker}
              </p>
              <iframe
                title={`${exchange}:${ticker} TradingView chart`}
                src={src}
                sandbox="allow-scripts allow-same-origin allow-popups"
                referrerPolicy="no-referrer"
                loading="lazy"
                className="h-[480px] w-full rounded border"
              />
              <p className="text-sm">
                <a
                  href={`https://www.tradingview.com/chart/?symbol=${encodeURIComponent(`${exchange}:${ticker}`)}`}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="underline"
                >
                  {ticker} chart by TradingView
                </a>
              </p>
              <p className="text-sm">{tr("providerUnavailable")}</p>
            </>
          )}
        </>
      )}
    </section>
  );
}
