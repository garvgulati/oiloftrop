import {
  analyseIndiaResearch,
  getIndiaResearchConnection,
  removeIndiaResearchConnection,
  searchIndiaResearch,
  setIndiaResearchConnection,
} from "@/adapters";
import type { IndiaResearchListing } from "@/adapters/shared/research";
import { useIsFetching, useIsMutating, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Button, Input } from "@wealthfolio/ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { buildIndiaSummary } from "./india-summary";
import { RecommendationPanel } from "./recommendation-panel";
import { recommendIndia } from "./research-recommendation";
import { IndiaChart } from "./india-chart";
import { ValuationPanel } from "./valuation-panel";
import { IndiaRatios } from "./india-ratios";

export function IndiaResearch({ profileId }: { profileId: string }) {
  const { t, i18n } = useTranslation("common");
  const tr = (key: string) => t(`researchIndia.${key}`);
  const connection = useQuery({
    queryKey: ["india-research-connection", profileId],
    queryFn: getIndiaResearchConnection,
    enabled: profileId !== "none",
    retry: false,
  });
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState("");
  const [basis, setBasis] = useState<"consolidated" | "standalone">("consolidated");
  const search = useMutation({ mutationFn: searchIndiaResearch, retry: false, gcTime: 0 });
  const analysis = useMutation({
    mutationFn: async () => buildIndiaSummary(await analyseIndiaResearch(selected, basis)),
    retry: false,
    gcTime: 0,
  });
  const ratiosPending = useIsMutating({ mutationKey: ["india-research-ratios"] }) > 0;
  const pending = search.isPending || analysis.isPending || ratiosPending;
  const chartFetching = useIsFetching({ queryKey: ["india-research-chart"] }) > 0;
  const report = analysis.data;
  const format = (value: number | null) =>
    value === null
      ? tr("unknown")
      : new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 2 }).format(value);
  const reset = () => {
    analysis.reset();
    search.reset();
    setSelected("");
    setQuery("");
  };
  function errorText(error: unknown) {
    const value = error instanceof Error ? error.message : "";
    return tr(
      value.includes("AUTH_REQUIRED") || value.includes("CONNECTION_REQUIRED")
        ? "authError"
        : value.includes("DISABLED_IN_TEST")
          ? "testDisabled"
          : value.includes("BUSY") || value.includes("RATE_LIMITED")
            ? "busy"
            : "sourceError",
    );
  }
  const marginPeriod = report?.periods.find((row) => row.revenue !== null)?.period;
  return (
    <section className="space-y-4 rounded-xl border p-4" aria-labelledby="india-research-title">
      <h2 id="india-research-title" className="text-lg font-medium">
        {tr("title")}
      </h2>
      <p className="text-sm">{tr("scope")}</p>
      <p className="text-muted-foreground text-sm">{tr("privacy")}</p>
      {connection.isError ? (
        <p role="alert">{tr("connectionError")}</p>
      ) : connection.isPending ? (
        <p role="status">{tr("loading")}</p>
      ) : (
        <>
          <p className="text-sm">
            {connection.data?.configured
              ? `${tr("connected")} · ${tr("expires")}: ${connection.data.expiresOn}`
              : tr("notConnected")}
          </p>
          <IndiaConnectionForm
            disabled={pending}
            profileId={profileId}
            configured={connection.data?.configured ?? false}
            onChanged={reset}
          />
          <form
            className="flex flex-wrap items-end gap-3"
            onSubmit={(event) => {
              event.preventDefault();
              if (!pending && connection.data?.configured && query.trim()) {
                analysis.reset();
                setSelected("");
                search.mutate(query.trim());
              }
            }}
          >
            <label className="min-w-full flex-1 space-y-1 text-sm sm:min-w-0">
              <span>{tr("query")}</span>
              <Input
                maxLength={40}
                value={query}
                disabled={pending || !connection.data?.configured}
                placeholder="ONGC"
                onChange={(event) => {
                  setQuery(event.target.value);
                  search.reset();
                  analysis.reset();
                  setSelected("");
                }}
              />
            </label>
            <Button
              type="submit"
              disabled={pending || !connection.data?.configured || !query.trim()}
            >
              {search.isPending ? tr("loading") : tr("search")}
            </Button>
          </form>
          {search.isError && <p role="alert">{errorText(search.error)}</p>}
          {search.data && (
            <>
              {search.data.listings.length === 0 ? (
                <p>{tr("noMatches")}</p>
              ) : (
                <label className="block space-y-1 text-sm">
                  <span>{tr("company")}</span>
                  <select
                    className="bg-background min-h-10 w-full rounded-md border p-2"
                    value={selected}
                    disabled={pending}
                    onChange={(event) => {
                      setSelected(event.target.value);
                      analysis.reset();
                    }}
                  >
                    <option value="">{tr("choose")}</option>
                    {search.data.listings.map((listing: IndiaResearchListing) => (
                      <option key={listing.key} value={listing.key}>
                        {listing.name} · {listing.symbol} · {listing.exchange} · {listing.isin}
                      </option>
                    ))}
                  </select>
                </label>
              )}
            </>
          )}
          <label className="block space-y-1 text-sm">
            <span>{tr("basis")}</span>
            <select
              className="bg-background min-h-10 w-full rounded-md border p-2"
              value={basis}
              disabled={pending}
              onChange={(event) => {
                setBasis(event.target.value as "consolidated" | "standalone");
                analysis.reset();
              }}
            >
              <option value="consolidated">{tr("consolidated")}</option>
              <option value="standalone">{tr("standalone")}</option>
            </select>
          </label>
          <div className="flex flex-wrap gap-3">
            <Button
              disabled={pending || chartFetching || !selected || !connection.data?.configured}
              onClick={() => analysis.mutate()}
            >
              {analysis.isPending ? tr("loading") : tr("analyse")}
            </Button>
            <Button
              variant="outline"
              disabled={
                pending || (!search.data && !analysis.data && !search.isError && !analysis.isError)
              }
              onClick={reset}
            >
              {tr("clear")}
            </Button>
          </div>
          {selected && <IndiaChart key={selected} instrumentKey={selected} paused={pending} />}
          {analysis.isPending && <p role="status">{tr("loading")}</p>}
          {analysis.isError && <p role="alert">{errorText(analysis.error)}</p>}
          {report && (
            <div className="space-y-4" aria-live="polite">
              <h3 className="break-words text-lg font-medium">
                {report.listing.name} · {report.listing.symbol}
              </h3>
              <p className="break-words text-sm">
                {report.listing.exchange} · {report.listing.isin} · {tr(report.basis)}
              </p>
              <p className="break-words text-sm">
                {tr("retrieved")}: {report.retrievedAt}
              </p>
              <p className="text-sm">{tr("periodNote")}</p>
              <RecommendationPanel recommendation={recommendIndia(report)} />
              <IndiaRatios
                key={`${report.listing.key}:${report.retrievedAt}`}
                instrumentKey={report.listing.key}
                paused={pending}
              />
              <ValuationPanel
                key={`${report.listing.key}:${report.basis}:${report.retrievedAt}`}
                company={`${report.listing.name} · ${report.listing.exchange}:${report.listing.symbol} · ${report.listing.isin} · ${report.basis}`}
                currency="INR"
              />
              <h4 className="font-medium">{tr("findings")}</h4>
              {report.periods.length === 0 ? (
                <p>{tr("noData")}</p>
              ) : (
                <>
                  {marginPeriod && (
                    <p className="text-sm">
                      {tr("marginPeriod")}: {marginPeriod}. {tr("profitMargin")}:{" "}
                      {format(report.profitMargin)}
                      {report.profitMargin === null ? "" : "%"}. {tr("operatingMargin")}:{" "}
                      {format(report.operatingMargin)}
                      {report.operatingMargin === null ? "" : "%"}.
                    </p>
                  )}
                  <div className="overflow-x-auto">
                    <table className="w-full text-left text-sm">
                      <caption className="pb-2 text-left">{tr("incomeCaption")}</caption>
                      <thead>
                        <tr>
                          {["period", "revenue", "operatingProfit", "profit"].map((key) => (
                            <th className="p-2" key={key}>
                              {tr(key)}
                            </th>
                          ))}
                        </tr>
                      </thead>
                      <tbody>
                        {report.periods.map((row) => (
                          <tr key={row.period} className="border-t">
                            <th className="p-2 font-normal">{row.period}</th>
                            {[row.revenue, row.operatingProfit, row.profit].map((value, index) => (
                              <td className="p-2" key={index}>
                                {format(value)}
                              </td>
                            ))}
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                  <div className="overflow-x-auto">
                    <table className="w-full text-left text-sm">
                      <caption className="pb-2 text-left">{tr("cashCaption")}</caption>
                      <thead>
                        <tr>
                          {[
                            "period",
                            "operatingCash",
                            "investingCash",
                            "financingCash",
                            "assets",
                            "liabilities",
                          ].map((key) => (
                            <th className="p-2" key={key}>
                              {tr(key)}
                            </th>
                          ))}
                        </tr>
                      </thead>
                      <tbody>
                        {report.periods.map((row) => (
                          <tr key={row.period} className="border-t">
                            <th className="p-2 font-normal">{row.period}</th>
                            {[
                              row.operatingCash,
                              row.investingCash,
                              row.financingCash,
                              row.assets,
                              row.liabilities,
                            ].map((value, index) => (
                              <td className="p-2" key={index}>
                                {format(value)}
                              </td>
                            ))}
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </>
              )}
              <p className="text-sm">{tr("attribution")}</p>
              <p className="text-sm">{tr("liabilityNote")}</p>
              <details className="rounded-lg border p-3">
                <summary className="min-h-10 cursor-pointer font-medium">{tr("sources")}</summary>
                <div className="space-y-2 pt-3 text-sm">
                  {["get-income-statement", "get-cash-flow", "get-balance-sheet"].map((path) => (
                    <a
                      className="block min-h-10 break-all underline"
                      key={path}
                      href={`https://upstox.com/developer/api-documentation/${path}/`}
                      target="_blank"
                      rel="noopener noreferrer"
                    >
                      Upstox · {path}
                    </a>
                  ))}
                  <a
                    className="block min-h-10 underline"
                    href="https://www.nseindia.com/companies-listing/corporate-integrated-filing"
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    {tr("exchange")}
                  </a>
                </div>
              </details>
            </div>
          )}
        </>
      )}
      <p className="text-muted-foreground text-sm">{tr("limits")}</p>
    </section>
  );
}

function IndiaConnectionForm({
  profileId,
  configured,
  disabled,
  onChanged,
}: {
  profileId: string;
  configured: boolean;
  disabled: boolean;
  onChanged: () => void;
}) {
  const { t } = useTranslation("common");
  const tr = (key: string) => t(`researchIndia.${key}`);
  const cache = useQueryClient();
  const [token, setToken] = useState("");
  const [expiresOn, setExpiresOn] = useState("");
  const [free, setFree] = useState(false);
  const [privateUse, setPrivateUse] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(false);
  return (
    <details className="rounded-lg border p-3">
      <summary className="min-h-10 cursor-pointer font-medium">{tr("connection")}</summary>
      <form
        className="space-y-3 pt-3"
        onSubmit={async (event) => {
          event.preventDefault();
          setBusy(true);
          setError(false);
          // Direct write-only call: credential variables never enter a mutation cache.
          try {
            const status = await setIndiaResearchConnection({
              token: token.trim(),
              expiresOn,
              freePersonalConfirmed: free,
              privateDashboardConfirmed: privateUse,
            });
            cache.setQueryData(["india-research-connection", profileId], status);
            onChanged();
          } catch {
            setError(true);
          } finally {
            setToken("");
            setBusy(false);
          }
        }}
      >
        <p className="text-sm">{tr("connectionNote")}</p>
        <a
          className="block min-h-10 text-sm underline"
          href="https://upstox.com/developer/api-documentation/analytics-token/"
          target="_blank"
          rel="noopener noreferrer"
        >
          {tr("tokenHelp")}
        </a>
        <label className="block space-y-1 text-sm">
          <span>{tr("token")}</span>
          <Input
            type="password"
            autoComplete="off"
            value={token}
            maxLength={4096}
            onChange={(e) => setToken(e.target.value)}
            disabled={disabled || busy}
          />
        </label>
        <label className="block space-y-1 text-sm">
          <span>{tr("expiry")}</span>
          <Input
            type="date"
            value={expiresOn}
            onChange={(e) => setExpiresOn(e.target.value)}
            disabled={disabled || busy}
          />
        </label>
        <label className="flex items-start gap-2 text-sm">
          <input
            type="checkbox"
            checked={free}
            onChange={(e) => setFree(e.target.checked)}
            disabled={disabled || busy}
          />
          {tr("freeConfirm")}
        </label>
        <label className="flex items-start gap-2 text-sm">
          <input
            type="checkbox"
            checked={privateUse}
            onChange={(e) => setPrivateUse(e.target.checked)}
            disabled={disabled || busy}
          />
          {tr("privateConfirm")}
        </label>
        <div className="flex flex-wrap gap-3">
          <Button
            type="submit"
            disabled={disabled || busy || !token.trim() || !expiresOn || !free || !privateUse}
          >
            {tr("saveConnection")}
          </Button>
          <Button
            type="button"
            variant="outline"
            disabled={disabled || busy || !configured}
            onClick={async () => {
              setBusy(true);
              setError(false);
              try {
                const status = await removeIndiaResearchConnection();
                cache.setQueryData(["india-research-connection", profileId], status);
                onChanged();
              } catch {
                setError(true);
              } finally {
                setBusy(false);
                setToken("");
              }
            }}
          >
            {tr("disconnect")}
          </Button>
        </div>
        {error && <p role="alert">{tr("connectionError")}</p>}
      </form>
    </details>
  );
}
