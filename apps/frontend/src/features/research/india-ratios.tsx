import { loadIndiaResearchRatios } from "@/adapters";
import { useIsFetching, useMutation } from "@tanstack/react-query";
import { Button } from "@wealthfolio/ui";
import { useTranslation } from "react-i18next";
import { z } from "zod";

const names = ["P/E", "P/B", "ROA", "ROE", "ROCE", "EV/EBITDA"] as const;
const amount = z.number().finite().min(-1e9).max(1e9).nullable();
const schema = z
  .object({
    listing: z
      .object({
        key: z.string(),
        symbol: z.string(),
        isin: z.string(),
        exchange: z.string(),
        name: z.string(),
      })
      .strict(),
    retrievedAt: z.string().datetime({ offset: true }),
    ratios: z
      .array(z.object({ name: z.enum(names), company: amount, sector: amount }).strict())
      .max(6),
  })
  .strict();

export function IndiaRatios({ instrumentKey, paused }: { instrumentKey: string; paused: boolean }) {
  const { t, i18n } = useTranslation("common");
  const tr = (key: string) => t(`researchRatios.${key}`);
  const chartBusy = useIsFetching({ queryKey: ["india-research-chart"] }) > 0;
  const query = useMutation({
    mutationKey: ["india-research-ratios", instrumentKey],
    mutationFn: async () => {
      const report = schema.parse(await loadIndiaResearchRatios(instrumentKey));
      if (
        report.listing.key !== instrumentKey ||
        `${report.listing.exchange}_EQ|${report.listing.isin}` !== instrumentKey ||
        new Set(report.ratios.map((row) => row.name)).size !== report.ratios.length
      )
        throw new Error("INDIA_RATIOS_INVALID");
      return report;
    },
    retry: false,
    gcTime: 0,
  });
  const report = query.data;
  const value = (number: number | null, percentage: boolean) =>
    number === null
      ? tr("unknown")
      : `${new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 2 }).format(number)}${percentage ? "%" : "×"}`;
  return (
    <section className="space-y-3 rounded-xl border p-4" aria-label={tr("title")}>
      <h4 className="font-medium">{tr("title")}</h4>
      <p className="text-sm">{tr("scope")}</p>
      <div className="flex flex-wrap gap-3">
        <Button
          variant="outline"
          disabled={paused || chartBusy || query.isPending}
          onClick={() => query.mutate()}
        >
          {tr(query.isPending ? "loading" : "load")}
        </Button>
        <Button
          variant="outline"
          disabled={query.isPending || (!report && !query.isError)}
          onClick={() => query.reset()}
        >
          {tr("clear")}
        </Button>
      </div>
      {query.isError && (
        <p role="alert" className="text-sm">
          {tr("error")}
        </p>
      )}
      {report && (
        <div className="space-y-3" aria-live="polite">
          <p className="break-words text-sm">
            {report.listing.name} · {report.listing.exchange}:{report.listing.symbol} ·{" "}
            {report.listing.isin}
          </p>
          <p className="break-words text-sm">
            {tr("retrieved")}: {report.retrievedAt}
          </p>
          <p className="text-sm">{tr("gaps")}</p>
          <div className="overflow-x-auto">
            <table className="block w-full text-left text-sm sm:table">
              <caption className="block pb-2 text-left sm:table-caption">{tr("caption")}</caption>
              <thead className="hidden sm:table-header-group">
                <tr>
                  {["ratio", "company", "sector"].map((key) => (
                    <th key={key} className="p-2">
                      {tr(key)}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody className="block sm:table-row-group">
                {names.map((name, index) => {
                  const row = report.ratios.find((row) => row.name === name);
                  const percentage = ["ROA", "ROE", "ROCE"].includes(name);
                  const unusable = !percentage && row?.company != null && row.company <= 0;
                  return (
                    <tr key={name} className="block border-t py-3 align-top sm:table-row sm:py-0">
                      <th className="block p-2 text-left font-normal sm:table-cell">
                        {name}
                        <span className="text-muted-foreground block">
                          {tr(`definition${index}`)}
                        </span>
                      </th>
                      <td className="inline-block w-1/2 break-words p-2 align-top sm:table-cell sm:w-auto">
                        <span className="block font-medium sm:hidden">{tr("company")}</span>
                        {value(row?.company ?? null, percentage)}
                        {unusable && <span className="block">{tr("nonpositive")}</span>}
                      </td>
                      <td className="inline-block w-1/2 break-words p-2 align-top sm:table-cell sm:w-auto">
                        <span className="block font-medium sm:hidden">{tr("sector")}</span>
                        {value(row?.sector ?? null, percentage)}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
          <p className="text-sm">{tr("interpretation")}</p>
        </div>
      )}
      <a
        className="inline-block min-h-10 text-sm underline"
        href="https://upstox.com/developer/api-documentation/get-key-ratios/"
        target="_blank"
        rel="noopener noreferrer"
      >
        {tr("source")}
      </a>
    </section>
  );
}
