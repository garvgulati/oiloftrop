import { useTranslation } from "react-i18next";
import type { ResearchRecommendation } from "./research-recommendation";

export function RecommendationPanel({
  recommendation,
}: {
  recommendation: ResearchRecommendation;
}) {
  const { t } = useTranslation("common");
  const tr = (key: string) => t(`researchRecommendation.${key}`);
  return (
    <section className="space-y-3 rounded-xl border p-4" aria-label={tr("title")}>
      <h4 className="font-medium">{tr("title")}</h4>
      <p className="font-medium">{tr(recommendation.outcome)}</p>
      <p className="text-sm">{tr(`${recommendation.outcome}Action`)}</p>
      <p className="text-sm">{tr("scope")}</p>
      <p className="break-words text-sm">
        {recommendation.source} · {recommendation.period ?? tr("unknownPeriod")}
      </p>
      {(["reasons", "concerns"] as const).map((key) => (
        <div key={key} className="space-y-2 text-sm">
          <h5 className="font-medium">{tr(key)}</h5>
          {recommendation[key].length ? (
            <ul className="list-disc space-y-2 pl-5">
              {recommendation[key].map((item) => (
                <li key={item.observation}>
                  <p>{item.observation}</p>
                  <p className="text-muted-foreground">{item.explanation}</p>
                </li>
              ))}
            </ul>
          ) : (
            <p>{tr(`${key}Empty`)}</p>
          )}
        </div>
      ))}
      <details className="rounded-lg border p-3" open>
        <summary className="min-h-10 cursor-pointer text-sm font-medium">{tr("gaps")}</summary>
        <ul className="list-disc space-y-2 pl-5 text-sm">
          {recommendation.gaps.map((gap) => (
            <li key={gap}>{gap}</li>
          ))}
        </ul>
      </details>
      <p className="text-muted-foreground text-sm">{tr("method")}</p>
    </section>
  );
}
