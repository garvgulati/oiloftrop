import { latestReviewedImport, type ReviewedImportAccount } from "@/adapters";
import { useProfile } from "@/features/profiles/profile-context";
import { ReviewedSnapshotDetails } from "@/features/research/reviewed-import";
import { useQuery } from "@tanstack/react-query";
import { Button } from "@wealthfolio/ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router-dom";

export function PersonalHoldingsHome({ accounts }: { accounts: ReviewedImportAccount[] }) {
  const profileId = useProfile()?.profile?.id;
  const { t } = useTranslation("common");
  const [selected, setSelected] = useState("");
  const account = accounts.find((a) => a.id === selected) ?? accounts[0];
  const id = account?.id;
  const record = useQuery({
    queryKey: ["personal-saved-holdings", profileId, id],
    queryFn: async () => {
      if (!id) return null;
      const saved = await latestReviewedImport({ accountId: id });
      if (saved && (saved.snapshot.accountId !== id || saved.receipt.accountId !== id))
        throw new Error("PERSONAL_IMPORT_RECORD_INVALID");
      return saved;
    },
    enabled: !!id && !!profileId,
    retry: false,
    staleTime: 0,
    gcTime: 0,
  });
  return (
    <section className="space-y-4" aria-labelledby="personal-home-title">
      <h1 id="personal-home-title" className="text-2xl font-semibold">
        {t("personal.home")}
      </h1>
      <p className="text-muted-foreground">{t("personal.savedScope")}</p>
      <Link className="inline-block min-h-11 underline" to="/research">
        {t("personal.manageHoldings")}
      </Link>
      {accounts.length === 0 ? (
        <p>{t("personal.noAccounts")}</p>
      ) : (
        <>
          <label className="block space-y-1 text-sm">
            <span>{t("reviewedImport.targetAccount")}</span>
            <select
              className="bg-background min-h-11 w-full rounded-md border p-2"
              value={id}
              onChange={(event) => setSelected(event.target.value)}
            >
              {accounts.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.name} · {a.currency}
                </option>
              ))}
            </select>
          </label>
          <Button
            variant="outline"
            disabled={record.isFetching}
            onClick={() => void record.refetch()}
          >
            {t("reviewedImport.reload")}
          </Button>
          {record.isPending && <p role="status">{t("loading")}</p>}
          {record.isError && <p role="alert">{t("personal.recordError")}</p>}
          {record.data === null && <p>{t("personal.noSnapshot")}</p>}
          {record.data && (
            <div className="space-y-3 rounded-xl border p-4">
              <h2 className="font-medium">{account?.name}</h2>
              <ReviewedSnapshotDetails snapshot={record.data.snapshot} />
              <p className="text-sm">
                {t("personal.recordedAt")}:{" "}
                {new Date(record.data.receipt.importedAt).toLocaleString()}
              </p>
              <p className="text-muted-foreground text-sm">
                {t("reviewedImport.historyUnavailable")}
              </p>
            </div>
          )}
        </>
      )}
    </section>
  );
}
