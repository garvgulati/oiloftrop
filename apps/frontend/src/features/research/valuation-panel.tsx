import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button, Input } from "@wealthfolio/ui";
import { checkValuation, emptyValuation, type ValuationInputs } from "./valuation-check";

interface ValuationPanelProps {
  company: string;
  currency: "INR" | "USD";
  seed?: Partial<ValuationInputs>;
}

export function ValuationPanel({ company, currency, seed }: ValuationPanelProps) {
  const { t, i18n } = useTranslation("common");
  const tr = (key: string) => t(`researchValuation.${key}`);
  const [input, setInput] = useState<ValuationInputs>(() => ({
    ...emptyValuation,
    ...seed,
    confirmed: false,
  }));
  const [submitted, setSubmitted] = useState(false);
  const today = new Intl.DateTimeFormat("sv-SE", { timeZone: "Asia/Kolkata" }).format(new Date());
  const result = submitted ? checkValuation(input, today) : null;
  const money = (value: number) =>
    new Intl.NumberFormat(i18n.language, {
      style: "currency",
      currency,
      maximumFractionDigits: 2,
    }).format(value);
  const number = (value: number) =>
    new Intl.NumberFormat(i18n.language, {
      maximumFractionDigits: 2,
    }).format(value);
  const fields = [
    "price",
    "priceDate",
    "priceSource",
    "eps",
    "periodStart",
    "periodEnd",
    "filedDate",
    "earningsSource",
    "multiple",
    "rationale",
  ] as const;
  return (
    <details className="rounded-xl border p-4">
      <summary className="min-h-10 cursor-pointer font-medium">{tr("title")}</summary>
      <section className="space-y-3 pt-3" aria-label={tr("title")}>
        <p className="break-words font-medium">
          {company} · {currency}
        </p>
        <p className="text-sm">{tr("scope")}</p>
        <p className="text-muted-foreground text-sm">
          {tr(currency === "INR" ? "indiaNote" : "usNote")}
        </p>
        <form
          className="space-y-3"
          onSubmit={(event) => {
            event.preventDefault();
            setSubmitted(true);
          }}
        >
          <div className="grid gap-3 sm:grid-cols-2">
            {fields.map((field) => (
              <label key={field} className="min-w-0 space-y-1 text-sm">
                <span>
                  {tr(field)}
                  {["price", "eps"].includes(field) ? ` (${currency}/share)` : ""}
                </span>
                <Input
                  value={input[field]}
                  type={
                    field.includes("Date") || field === "periodStart" || field === "periodEnd"
                      ? "date"
                      : "text"
                  }
                  inputMode={["price", "eps", "multiple"].includes(field) ? "decimal" : undefined}
                  maxLength={field.includes("Source") || field === "rationale" ? 500 : 32}
                  onChange={(event) => {
                    setInput({ ...input, [field]: event.target.value, confirmed: false });
                    setSubmitted(false);
                  }}
                />
              </label>
            ))}
          </div>
          <label className="flex items-start gap-2 text-sm">
            <input
              type="checkbox"
              className="mt-1"
              checked={input.confirmed}
              onChange={(event) => {
                setInput({ ...input, confirmed: event.target.checked });
                setSubmitted(false);
              }}
            />
            <span>{tr("confirmation")}</span>
          </label>
          <div className="flex flex-wrap gap-3">
            <Button type="submit">{tr("calculate")}</Button>
            <Button
              type="button"
              variant="outline"
              onClick={() => {
                setInput({ ...emptyValuation });
                setSubmitted(false);
              }}
            >
              {tr("clear")}
            </Button>
          </div>
        </form>
        {result?.error && (
          <p role="alert" className="text-sm">
            {tr(result.error)}
          </p>
        )}
        {result && !result.error && (
          <div role="status" className="space-y-2 rounded-lg border p-3 text-sm">
            <p>
              {tr("pe")}: {number(result.pe)}× · {tr("earningsYield")}: {number(result.yield)}%
            </p>
            <p>
              {tr("used")}: {money(Number(input.price))} ({input.priceDate}) ÷{" "}
              {money(Number(input.eps))} ({input.periodStart} – {input.periodEnd}); {tr("filed")}:{" "}
              {input.filedDate}.
            </p>
            <p>
              {tr("ages")} {result.priceAge} / {result.earningsAge}.
            </p>
            {(result.priceAge > 7 || result.earningsAge > 550) && <p>{tr("stale")}</p>}
            {result.impliedPrice !== null && (
              <p>
                {tr("scenario")}: {money(result.impliedPrice)} · {tr("difference")}:{" "}
                {number(result.difference!)}%. {tr("assumption")}: {input.multiple}× —{" "}
                {input.rationale}
              </p>
            )}
            <p>{tr("interpretation")}</p>
          </div>
        )}
        <p className="text-muted-foreground text-sm">{tr("limitations")}</p>
        <a
          className="inline-block min-h-10 text-sm underline"
          href="https://www.investor.gov/introduction-investing/investing-basics/glossary/price-earnings-pe-ratio"
          target="_blank"
          rel="noopener noreferrer"
        >
          {tr("method")}
        </a>
      </section>
    </details>
  );
}
