import {
  getResearchContext,
  getReviewedImportStatus,
  runResearchExample,
  type ResearchExampleCase,
} from "@/adapters";
import { useProfile } from "@/features/profiles/profile-context";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Button } from "@wealthfolio/ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ResearchPlanEditor } from "./plan-editor";
import { StockResearch } from "./stock-research";
import { ReviewedImportPanel } from "./reviewed-import";

function issueGroup(code: string) {
  if (code.includes("SNAPSHOT")) return "snapshot";
  if (code.includes("DECIMAL") || code.includes("PRECISION")) return "amount";
  if (code.includes("QUOTE")) return "quote";
  if (code.includes("IDENTITY")) return "identity";
  if (code.includes("PLAN")) return "plan";
  if (code.includes("FX")) return "fx";
  if (code.includes("PROJECTION") || code.includes("ACTIVITY")) return "records";
  return "unsupported";
}

export default function ResearchPage() {
  const { t, i18n } = useTranslation("common");
  const profileId = useProfile()?.profile?.id;
  const imports = useQuery({
    queryKey: ["reviewed-import-status", profileId],
    queryFn: getReviewedImportStatus,
    enabled: !!profileId,
    retry: false,
    gcTime: 0,
  });
  const context = useQuery({
    queryKey: ["research-context", profileId],
    queryFn: getResearchContext,
    enabled: !!profileId && imports.data?.supported === false,
    retry: false,
    staleTime: 0,
  });
  const [caseId, setCaseId] = useState<ResearchExampleCase>("baseline");
  const example = useMutation({ mutationFn: runResearchExample });
  const format = (value: number | null) =>
    value === null
      ? "—"
      : new Intl.NumberFormat(i18n.language, {
          style: "currency",
          currency: "USD",
          maximumFractionDigits: 2,
        }).format(value);
  const groups = [...new Set(context.data?.issues.map((value) => issueGroup(value.code)) ?? [])];

  return (
    <main className="mx-auto max-w-5xl space-y-6 p-4 pb-28 sm:p-6">
      <header className="space-y-2">
        <h1 className="text-2xl font-semibold">{t("research.title")}</h1>
        <p className="text-muted-foreground">{t("research.subtitle")}</p>
      </header>
      <StockResearch key={profileId ?? "none"} profileId={profileId ?? "none"} />
      {imports.isError && (
        <div role="alert">
          <p>{t("reviewedImport.statusError")}</p>
          <Button variant="outline" onClick={() => void imports.refetch()}>
            {t("refresh")}
          </Button>
        </div>
      )}
      {imports.data?.supported === true && (
        <ReviewedImportPanel
          key={profileId}
          accounts={imports.data.accounts}
          onAccountCreated={() => void imports.refetch()}
        />
      )}
      {imports.data?.supported === false && (
        <>
          {profileId && context.data && (
            <ResearchPlanEditor
              profileId={profileId}
              mode={context.data.mode}
              accounts={context.data.accounts}
            />
          )}
          <section
            className="space-y-4 rounded-xl border p-4"
            aria-labelledby="research-portfolio-title"
          >
            <div className="flex flex-wrap items-center justify-between gap-3">
              <h2 id="research-portfolio-title" className="text-lg font-medium">
                {t("holdings")}
              </h2>
              <Button
                variant="outline"
                disabled={context.isFetching}
                onClick={() => void context.refetch()}
              >
                {t("refresh")}
              </Button>
            </div>
            {context.isPending && <p role="status">{t("loading")}</p>}
            {context.isError && <p role="alert">{t("research.contextError")}</p>}
            {context.data && (
              <>
                <p className="text-muted-foreground text-sm">
                  {t("date")}: {context.data.valuationDate}
                </p>
                <p className="text-sm">{t("research.notReady")}</p>
                <ul className="list-disc space-y-1 pl-5 text-sm">
                  {groups.map((group) => (
                    <li key={group}>{t(`research.issues.${group}`)}</li>
                  ))}
                </ul>
                {context.data.positions.length === 0 ? (
                  <p>{t("research.noPositions")}</p>
                ) : (
                  <div className="overflow-x-auto">
                    <table className="w-full text-left text-sm">
                      <thead>
                        <tr>
                          <th className="p-2">{t("holdings")}</th>
                          <th className="p-2">{t("quantity")}</th>
                          <th className="p-2">{t("price")}</th>
                          <th className="p-2">{t("research.quoteDate")}</th>
                        </tr>
                      </thead>
                      <tbody>
                        {context.data.positions.map((position) => (
                          <tr
                            key={`${position.accountId}:${position.assetId}`}
                            className="border-t"
                          >
                            <td className="p-2">{position.symbol ?? "—"}</td>
                            <td className="p-2 font-mono">{position.quantity ?? "—"}</td>
                            <td className="p-2 font-mono">
                              {position.quote?.price ?? "—"} {position.quote?.currency}
                            </td>
                            <td className="p-2">{position.quote?.observationDate ?? "—"}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                )}
              </>
            )}
          </section>
          <section
            className="space-y-4 rounded-xl border p-4"
            aria-labelledby="research-example-title"
          >
            <h2 id="research-example-title" className="text-lg font-medium">
              {t("research.exampleTitle")}
            </h2>
            <p className="text-muted-foreground text-sm">{t("research.exampleNote")}</p>
            <div className="flex flex-wrap items-end gap-3">
              <label className="space-y-1 text-sm">
                <span className="block">{t("research.case")}</span>
                <select
                  className="bg-background min-h-10 max-w-full rounded-md border p-2"
                  value={caseId}
                  onChange={(event) => {
                    setCaseId(event.target.value as ResearchExampleCase);
                    example.reset();
                  }}
                  disabled={example.isPending}
                >
                  <option value="baseline">{t("research.baseline")}</option>
                  <option value="unknown-loss">{t("research.unknownLoss")}</option>
                  <option value="changed-fee">{t("research.changedFee")}</option>
                </select>
              </label>
              <Button disabled={example.isPending} onClick={() => example.mutate(caseId)}>
                {example.isPending ? t("loading") : t("research.run")}
              </Button>
            </div>
            {example.isError && <p role="alert">{t("research.exampleError")}</p>}
            {example.data && (
              <div aria-live="polite" className="space-y-4">
                <p>
                  {t("quantity")}: <span className="font-mono">{example.data.quantity}</span> ·{" "}
                  {t("date")}: {example.data.asOf.slice(0, 10)}
                </p>
                <p className="text-sm">{t("research.scoreUnavailable")}</p>
                {example.data.caseId === "unknown-loss" && (
                  <p className="text-sm">{t("research.exampleNeedsLoss")}</p>
                )}
                <div className="overflow-x-auto">
                  <table className="w-full text-left text-sm">
                    <thead>
                      <tr>
                        <th className="p-2">{t("research.horizon")}</th>
                        <th className="p-2">{t("research.buy")}</th>
                        <th className="p-2">{t("research.cash")}</th>
                        <th className="p-2">{t("research.difference")}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {example.data.horizons.map((row) => (
                        <tr key={row.years} className="border-t">
                          <td className="p-2">
                            {row.years} · {row.end}
                          </td>
                          <td className="p-2">{format(row.remainingA)}</td>
                          <td className="p-2">{format(row.remainingB)}</td>
                          <td className="p-2">{format(row.difference)}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </div>
            )}
          </section>
        </>
      )}
    </main>
  );
}
