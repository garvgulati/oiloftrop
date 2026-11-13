import { loadResearchCompany } from "@/adapters";
import { useMutation } from "@tanstack/react-query";
import { Button, Input } from "@wealthfolio/ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { RecommendationPanel } from "./recommendation-panel";
import { recommendUS } from "./research-recommendation";
import { UsChart } from "./us-chart";
import { ValuationPanel } from "./valuation-panel";
import {
  buildCompanyReport,
  type CompanyReport,
} from "../../../../../../wealth-manager/lib/research";

async function analysePublicCompany(symbol: string): Promise<CompanyReport> {
  const data = await loadResearchCompany(symbol);
  return buildCompanyReport(data.submissions, data.facts, data.retrievedAt);
}

export function OnDemandResearch() {
  const { t, i18n } = useTranslation("common");
  const tr = (key: string) => t(`researchDemand.${key}`);
  const [symbol, setSymbol] = useState("");
  const analysis = useMutation({ mutationFn: analysePublicCompany, retry: false, gcTime: 0 });
  const report = analysis.data;
  const number = (value: number | null | undefined, scale = 1) =>
    value == null
      ? tr("unknown")
      : new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 2 }).format(value / scale);
  const errorCode = analysis.error instanceof Error ? analysis.error.message : "";
  const errorText = errorCode.includes("IDENTIFIER_INVALID")
    ? "invalid"
    : errorCode.includes("TICKER_NOT_FOUND") || errorCode.includes("SOURCE_NOT_FOUND")
      ? "notFound"
      : errorCode.includes("RATE_LIMITED") || errorCode.includes("REQUEST_BUSY")
        ? "busy"
        : errorCode.includes("NETWORK_DISABLED_IN_TEST")
          ? "testDisabled"
          : errorCode.includes("SOURCE_TOO_LARGE")
            ? "tooLarge"
            : "error";

  return (
    <section className="space-y-4 rounded-xl border p-4" aria-labelledby="demand-research-title">
      <h2 id="demand-research-title" className="text-lg font-medium">
        {tr("title")}
      </h2>
      <p className="text-sm">{tr("scope")}</p>
      <p className="text-muted-foreground text-sm">{tr("privacy")}</p>
      <form
        className="flex flex-wrap items-end gap-3"
        onSubmit={(event) => {
          event.preventDefault();
          if (!analysis.isPending && /^[A-Za-z0-9.-]{1,20}$/.test(symbol.trim()))
            analysis.mutate(symbol.trim().toUpperCase());
        }}
      >
        <label className="min-w-full flex-1 space-y-1 text-sm sm:min-w-0">
          <span>{tr("symbol")}</span>
          <Input
            value={symbol}
            maxLength={20}
            placeholder="MSFT"
            autoCapitalize="characters"
            spellCheck={false}
            disabled={analysis.isPending}
            onChange={(event) => {
              setSymbol(event.target.value);
              analysis.reset();
            }}
          />
        </label>
        <Button
          type="submit"
          disabled={analysis.isPending || !/^[A-Za-z0-9.-]{1,20}$/.test(symbol.trim())}
        >
          {analysis.isPending ? tr("loading") : tr("analyse")}
        </Button>
        <Button
          type="button"
          variant="outline"
          disabled={analysis.isPending || (!report && !analysis.isError)}
          onClick={() => {
            analysis.reset();
            setSymbol("");
          }}
        >
          {tr("clear")}
        </Button>
      </form>
      {analysis.isPending && <p role="status">{tr("loading")}</p>}
      {analysis.isError && <p role="alert">{tr(errorText)}</p>}
      {report && (
        <div className="space-y-4" aria-live="polite">
          <h3 className="break-words text-lg font-medium">
            {report.name} · {report.tickers.join(", ") || report.cik}
          </h3>
          <p className="break-words text-sm">
            {tr("retrieved")}: {report.retrievedAt}
          </p>
          <p className="text-sm">{tr("basis")}</p>
          <UsChart key={report.cik} tickers={report.tickers} />
          <RecommendationPanel recommendation={recommendUS(report)} />
          <ValuationPanel
            key={`${report.cik}:${report.retrievedAt}`}
            company={`${report.name} · CIK ${report.cik}`}
            currency="USD"
            seed={
              report.annual[0]?.metrics.dilutedEps?.unit === "USD/shares"
                ? {
                    eps: String(report.annual[0].metrics.dilutedEps.value),
                    periodStart: report.annual[0].metrics.dilutedEps.start ?? "",
                    periodEnd: report.annual[0].metrics.dilutedEps.end,
                    filedDate: report.annual[0].metrics.dilutedEps.filed,
                    earningsSource: report.annual[0].metrics.dilutedEps.url,
                  }
                : undefined
            }
          />
          <h4 className="font-medium">{tr("findings")}</h4>
          {report.findings.length ? (
            <ul className="list-disc space-y-2 pl-5 text-sm">
              {report.findings.map((finding) => (
                <li key={finding}>{finding}</li>
              ))}
            </ul>
          ) : (
            <p>{tr("noFacts")}</p>
          )}
          <div className="overflow-x-auto">
            <table className="w-full text-left text-sm">
              <caption className="pb-2 text-left">{tr("annual")}</caption>
              <thead>
                <tr>
                  {["period", "revenue", "profit", "cash", "proxy", "eps"].map((key) => (
                    <th className="p-2" key={key}>
                      {tr(key)}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {report.annual.map((period) => (
                  <tr className="border-t" key={period.end}>
                    <th className="p-2 font-normal">
                      {period.start} – {period.end}
                    </th>
                    <td className="p-2">{number(period.metrics.revenue?.value, 1e6)}</td>
                    <td className="p-2">{number(period.metrics.netIncome?.value, 1e6)}</td>
                    <td className="p-2">{number(period.metrics.operatingCash?.value, 1e6)}</td>
                    <td className="p-2">{number(period.freeCashFlow, 1e6)}</td>
                    <td className="p-2">{number(period.metrics.dilutedEps?.value)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <details className="rounded-lg border p-3">
            <summary className="min-h-10 cursor-pointer font-medium">{tr("sources")}</summary>
            <ul className="space-y-2 pt-3 text-sm">
              {report.sourceUrls.map((url) => (
                <li key={url}>
                  <a
                    className="block min-h-10 break-all underline"
                    href={url}
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    {url}
                  </a>
                </li>
              ))}
              {report.filings.map((filing) => (
                <li key={filing.url}>
                  <a
                    className="block min-h-10 underline"
                    href={filing.url}
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    {filing.form} · {filing.filed} · {filing.reportDate}
                  </a>
                </li>
              ))}
              {report.annual.flatMap((period) =>
                Object.entries(period.metrics)
                  .filter(([, fact]) => fact !== null)
                  .map(([key, fact]) => (
                    <li key={`${period.end}:${key}`} className="break-words">
                      {key}: {fact!.value} {fact!.unit} · {fact!.start ?? tr("instant")} –{" "}
                      {fact!.end} · {tr("filed")} {fact!.filed} ·{" "}
                      <a
                        className="underline"
                        href={fact!.url}
                        target="_blank"
                        rel="noopener noreferrer"
                      >
                        {fact!.tag}
                      </a>
                    </li>
                  )),
              )}
            </ul>
          </details>
          <div className="space-y-2 text-sm">
            <h4 className="font-medium">{tr("gaps")}</h4>
            <ul className="list-disc space-y-2 pl-5">
              {[...new Set([...report.warnings, ...report.coverage])].map((warning) => (
                <li key={warning}>{warning}</li>
              ))}
            </ul>
          </div>
        </div>
      )}
      <p className="text-muted-foreground text-sm">{tr("limits")}</p>
    </section>
  );
}
