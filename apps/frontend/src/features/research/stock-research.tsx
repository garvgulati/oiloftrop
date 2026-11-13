import { useState } from "react";
import { useTranslation } from "react-i18next";
import { OnDemandResearch } from "./on-demand-research";
import { IndiaResearch } from "./india-research";
export function StockResearch({
  profileId,
  fixedMarket,
}: {
  profileId: string;
  fixedMarket?: "us" | "india";
}) {
  const { t } = useTranslation("common");
  const [market, setMarket] = useState("us");
  return (
    <div className="space-y-4">
      {!fixedMarket && (
        <label className="block space-y-1 text-sm">
          <span>{t("researchIndia.market")}</span>
          <select
            className="bg-background min-h-10 w-full rounded-md border p-2 sm:max-w-sm"
            value={market}
            onChange={(event) => setMarket(event.target.value)}
          >
            <option value="us">{t("researchIndia.us")}</option>
            <option value="india">{t("researchIndia.india")}</option>
          </select>
        </label>
      )}
      {(fixedMarket ?? market) === "india" ? (
        <IndiaResearch profileId={profileId} />
      ) : (
        <OnDemandResearch />
      )}
    </div>
  );
}
