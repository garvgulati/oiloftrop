import { useTranslation } from "react-i18next";

const sources = {
  ongcFiling:
    "https://nsearchives.nseindia.com/corporate/ONGC_04082026200149_finalfinancials04082026.pdf",
  ongcRelease: "https://ongcindia.com/web/eng/detail?assetClassPK=85545290&assetEntry=85545295",
  microsoftRelease:
    "https://www.microsoft.com/en-us/investor/earnings/fy-2026-q4/press-release-webcast",
  microsoftAnnual: "https://www.microsoft.com/investor/reports/ar26/",
};

export function CompanyEvidence() {
  const { t } = useTranslation("common");
  return (
    <section className="space-y-4 rounded-xl border p-4" aria-labelledby="company-evidence-title">
      <h2 id="company-evidence-title" className="text-lg font-medium">
        {t("researchEvidence.title")}
      </h2>
      <p className="text-muted-foreground text-sm">{t("researchEvidence.scope")}</p>
      <p className="text-sm">{t("researchEvidence.date")}</p>
      <p className="text-sm">{t("researchEvidence.limit")}</p>
      <div className="space-y-3">
        <details className="rounded-lg border p-3">
          <summary className="min-h-10 cursor-pointer font-medium">
            {t("researchEvidence.ongc.title")}
          </summary>
          <div className="space-y-3 pt-3 text-sm">
            <p>{t("researchEvidence.ongc.period")}</p>
            <dl className="space-y-2">
              {(["standalone", "owners", "minority", "total"] as const).map((key) => (
                <div key={key} className="flex flex-wrap justify-between gap-x-4 gap-y-1">
                  <dt>{t(`researchEvidence.ongc.${key}`)}</dt>
                  <dd className="font-mono">
                    {
                      {
                        standalone: "17,033.81 / 8,024.23",
                        owners: "11,898.93 / 9,804.07",
                        minority: "−5,344.49 / 1,750.14",
                        total: "6,554.44 / 11,554.21",
                      }[key]
                    }
                  </dd>
                </div>
              ))}
            </dl>
            <p>{t("researchEvidence.ongc.interpretation")}</p>
            <SourceLink href={sources.ongcFiling}>{t("researchEvidence.ongc.filing")}</SourceLink>
            <p>{t("researchEvidence.ongc.management")}</p>
            <SourceLink href={sources.ongcRelease}>{t("researchEvidence.ongc.release")}</SourceLink>
            <p>{t("researchEvidence.ongc.conclusion")}</p>
            <p>{t("researchEvidence.ongc.gaps")}</p>
          </div>
        </details>
        <details className="rounded-lg border p-3">
          <summary className="min-h-10 cursor-pointer font-medium">
            {t("researchEvidence.microsoft.title")}
          </summary>
          <div className="space-y-3 pt-3 text-sm">
            <p>{t("researchEvidence.microsoft.period")}</p>
            <p>{t("researchEvidence.microsoft.earnings")}</p>
            <SourceLink href={sources.microsoftRelease}>
              {t("researchEvidence.microsoft.release")}
            </SourceLink>
            <p>{t("researchEvidence.microsoft.cash")}</p>
            <p>{t("researchEvidence.microsoft.proxy")}</p>
            <SourceLink href={sources.microsoftAnnual}>
              {t("researchEvidence.microsoft.annual")}
            </SourceLink>
            <p>{t("researchEvidence.microsoft.conclusion")}</p>
            <p>{t("researchEvidence.microsoft.gaps")}</p>
          </div>
        </details>
      </div>
      <p className="text-muted-foreground text-sm">{t("researchEvidence.personal")}</p>
    </section>
  );
}

function SourceLink({ href, children }: { href: string; children: React.ReactNode }) {
  return (
    <a
      className="text-primary block min-h-10 break-words underline"
      href={href}
      target="_blank"
      rel="noopener noreferrer"
    >
      {children}
    </a>
  );
}
