import {
  getResearchPlan,
  saveResearchPlan,
  type StoredResearchPlan,
  type ResearchContext,
} from "@/adapters";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Button, Input } from "@wealthfolio/ui";
import { useState } from "react";
import { useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";
import {
  blankPlanningProfile,
  planningProfileSchema,
  type PlanningProfile,
} from "../../../../../../wealth-manager/lib/assessment-contracts";
import { FinanceEntries, type FinanceValues, type FinanceKey } from "./finance-entries";

type PlanForm = PlanningProfile & Pick<FinanceValues, "cashLinks">;

function today() {
  const parts = new Intl.DateTimeFormat("en-CA", {
    timeZone: "Asia/Kolkata",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).formatToParts(new Date());
  return ["year", "month", "day"]
    .map((key) => parts.find((part) => part.type === key)?.value)
    .join("-");
}

export function ResearchPlanEditor({
  profileId,
  mode,
  accounts = [],
}: {
  profileId: string;
  mode: "fictional" | "personal";
  accounts?: ResearchContext["accounts"];
}) {
  const { t } = useTranslation("common");
  const plan = useQuery({
    queryKey: ["research-plan", profileId],
    queryFn: getResearchPlan,
    retry: false,
    staleTime: 0,
  });
  return (
    <section className="space-y-4 rounded-xl border p-4" aria-labelledby="research-plan-title">
      <h2 id="research-plan-title" className="text-lg font-medium">
        {t("researchPlan.title")}
      </h2>
      <p className="text-sm">
        {t(mode === "fictional" ? "researchPlan.fictional" : "researchPlan.personal")}
      </p>
      <p className="text-muted-foreground text-sm">{t("researchPlan.scope")}</p>
      {plan.isPending && <p role="status">{t("loading")}</p>}
      {plan.isError && (
        <>
          <p role="alert">{t("researchPlan.loadError")}</p>
          <Button variant="outline" onClick={() => void plan.refetch()}>
            {t("retry")}
          </Button>
        </>
      )}
      {plan.data !== undefined && (
        <PlanDraft
          key={profileId}
          initial={plan.data}
          latest={plan.data}
          profileId={profileId}
          accounts={accounts}
        />
      )}
    </section>
  );
}

function PlanDraft({
  initial,
  latest,
  profileId,
  accounts,
}: {
  initial: StoredResearchPlan | null;
  latest: StoredResearchPlan | null;
  profileId: string;
  accounts: ResearchContext["accounts"];
}) {
  const { t } = useTranslation("common");
  const queryClient = useQueryClient();
  const [opened, setOpened] = useState(initial);
  const [error, setError] = useState<"invalid" | "conflict" | "saveError" | "cashLinkError" | null>(
    null,
  );
  const [validationDetails, setValidationDetails] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState(false);
  const {
    register,
    handleSubmit,
    reset,
    getValues,
    setValue,
    watch,
    formState: { isDirty, dirtyFields },
  } = useForm<PlanForm>({
    defaultValues: {
      ...(initial?.plan.profile ?? blankPlanningProfile(today())),
      cashLinks: initial?.plan.cashLinks ?? [],
    },
  });
  const revision = opened?.revision ?? 0;
  const changedElsewhere = (latest?.revision ?? 0) !== revision;
  const inputClass = "w-full rounded-md border bg-transparent p-2 text-sm";
  const amount = (
    key:
      | "goalAmount"
      | "newMoneyAvailableBase"
      | "canAffordToLoseBase"
      | "willingToLoseBase"
      | "maxDrawdownPct"
      | "issuerLimitPct"
      | "sectorLimitPct"
      | "currencyLimitPct"
      | "realHurdlePct",
  ) => (
    <label className="space-y-1 text-sm" key={key}>
      <span>{t(`researchPlan.${key}`)}</span>
      <Input
        inputMode={key === "realHurdlePct" ? "text" : "decimal"}
        placeholder={t("researchPlan.unknown")}
        {...register(key, { setValueAs: (value: unknown) => (value === "" ? null : value) })}
      />
    </label>
  );

  async function reload() {
    setBusy(true);
    setSaved(false);
    try {
      const current = await getResearchPlan();
      setOpened(current);
      reset({
        ...(current?.plan.profile ?? blankPlanningProfile(today())),
        cashLinks: current?.plan.cashLinks ?? [],
      });
      queryClient.setQueryData(["research-plan", profileId], current);
      setError(null);
    } catch {
      setError("saveError");
    } finally {
      setBusy(false);
    }
  }

  async function reapply() {
    const draft = getValues();
    const editable = [
      "goalLabel",
      "goalDate",
      "goalAmount",
      "goalCurrency",
      "baseCurrency",
      "goalInTodaysMoney",
      "newMoneyAvailableBase",
      "canAffordToLoseBase",
      "willingToLoseBase",
      "maxDrawdownPct",
      "issuerLimitPct",
      "sectorLimitPct",
      "currencyLimitPct",
      "realHurdlePct",
      "restrictions",
      "cash",
      "payments",
      "debts",
      "cashLinks",
      "cashComplete",
      "paymentsComplete",
      "debtsComplete",
      "holdingsComplete",
    ] as const;
    const base = {
      ...(opened?.plan.profile ?? blankPlanningProfile(today())),
      cashLinks: opened?.plan.cashLinks ?? [],
    };
    const edited = editable.filter(
      (key) => JSON.stringify(draft[key]) !== JSON.stringify(base[key]),
    );
    setBusy(true);
    setSaved(false);
    try {
      const current = await getResearchPlan();
      setOpened(current);
      reset({
        ...(current?.plan.profile ?? blankPlanningProfile(today())),
        cashLinks: current?.plan.cashLinks ?? [],
      });
      for (const key of edited) setValue(key, draft[key], { shouldDirty: true });
      queryClient.setQueryData(["research-plan", profileId], current);
      setError(null);
    } catch {
      setError("saveError");
    } finally {
      setBusy(false);
    }
  }

  return (
    <form
      className="space-y-4"
      onSubmit={handleSubmit(async (draft) => {
        const { cashLinks, ...answers } = draft;
        const parsed = planningProfileSchema.safeParse({ ...answers, confirmedDate: today() });
        setSaved(false);
        setValidationDetails([]);
        if (!parsed.success) {
          setError("invalid");
          setValidationDetails(
            [
              ...new Set(
                parsed.error.issues.map((issue) => {
                  const group = issue.path[0];
                  const index = issue.path[1];
                  return ["cash", "payments", "debts"].includes(String(group)) &&
                    typeof index === "number"
                    ? `${t(`researchFinance.${String(group)}`)} ${index + 1}: ${issue.message}`
                    : issue.message;
                }),
              ),
            ].slice(0, 5),
          );
          return;
        }
        const linkedSources = new Set<string>();
        for (const link of cashLinks) {
          const cash = parsed.data.cash.find((row) => row.id === link.cashId);
          const source = JSON.stringify([link.accountId, cash?.currency]);
          if (!cash?.reconciled || linkedSources.has(source)) {
            setError("cashLinkError");
            return;
          }
          linkedSources.add(source);
        }
        setBusy(true);
        setError(null);
        try {
          const result = await saveResearchPlan(revision, {
            profile: parsed.data,
            cashLinks,
          });
          setOpened(result);
          reset({ ...result.plan.profile, cashLinks: result.plan.cashLinks });
          setSaved(true);
          queryClient.setQueryData(["research-plan", profileId], result);
          void queryClient.invalidateQueries({ queryKey: ["research-context", profileId] });
        } catch (caught) {
          setError(
            caught instanceof Error && caught.message.includes("RESEARCH_PLAN_EDIT_CONFLICT")
              ? "conflict"
              : caught instanceof Error && caught.message.includes("RESEARCH_CASH_LINK")
                ? "cashLinkError"
                : "saveError",
          );
        } finally {
          setBusy(false);
        }
      })}
    >
      <fieldset disabled={busy} className="space-y-4">
        <div className="grid gap-4 sm:grid-cols-2">
          <label className="space-y-1 text-sm">
            <span>{t("researchPlan.goalLabel")}</span>
            <Input maxLength={1500} {...register("goalLabel")} />
          </label>
          <label className="space-y-1 text-sm">
            <span>{t("researchPlan.goalDate")}</span>
            <Input
              type="date"
              {...register("goalDate", { setValueAs: (v: unknown) => (v === "" ? null : v) })}
            />
          </label>
          {amount("goalAmount")}
          {(["baseCurrency", "goalCurrency"] as const).map((key) => (
            <label className="space-y-1 text-sm" key={key}>
              <span>{t(`researchPlan.${key}`)}</span>
              <select className={inputClass} {...register(key)}>
                <option>INR</option>
                <option>USD</option>
              </select>
            </label>
          ))}
        </div>
        <label className="flex items-center gap-2 text-sm">
          <input type="checkbox" {...register("goalInTodaysMoney")} />
          {t("researchPlan.todaysMoney")}
        </label>
        <p className="text-muted-foreground text-sm">{t("researchPlan.limitsNote")}</p>
        <div className="grid gap-4 sm:grid-cols-2">
          {(
            [
              "newMoneyAvailableBase",
              "canAffordToLoseBase",
              "willingToLoseBase",
              "maxDrawdownPct",
              "issuerLimitPct",
              "sectorLimitPct",
              "currencyLimitPct",
              "realHurdlePct",
            ] as const
          ).map(amount)}
        </div>
        <label className="block space-y-1 text-sm">
          <span>{t("researchPlan.restrictions")}</span>
          <textarea
            className={inputClass}
            rows={3}
            maxLength={1500}
            {...register("restrictions")}
          />
        </label>
        <FinanceEntries
          cash={watch("cash")}
          payments={watch("payments")}
          debts={watch("debts")}
          cashLinks={watch("cashLinks")}
          accounts={accounts}
          change={(key, value) => setValue<FinanceKey>(key, value, { shouldDirty: true })}
        />
        <section className="space-y-3">
          <h3 className="font-medium">{t("researchFinance.complete")}</h3>
          {(["cashComplete", "paymentsComplete", "debtsComplete", "holdingsComplete"] as const).map(
            (key) => (
              <label key={key} className="flex items-start gap-2 text-sm">
                <input type="checkbox" {...register(key)} />
                {t(`researchFinance.${key}`)}
              </label>
            ),
          )}
        </section>
        {(error === "conflict" || changedElsewhere) &&
          (dirtyFields.cash ||
            dirtyFields.payments ||
            dirtyFields.debts ||
            dirtyFields.cashLinks) && (
            <p className="text-sm">{t("researchFinance.reapplyLists")}</p>
          )}
        <div className="flex flex-wrap gap-3">
          <Button type="submit">{t("save")}</Button>
          <Button type="button" variant="outline" onClick={() => void reload()}>
            {t(isDirty ? "researchPlan.discardReload" : "researchPlan.reload")}
          </Button>
          {(error === "conflict" || changedElsewhere) && (
            <Button type="button" variant="outline" onClick={() => void reapply()}>
              {t("researchPlan.reapply")}
            </Button>
          )}
        </div>
      </fieldset>
      {(error || changedElsewhere) && (
        <p role="alert">
          {error === "cashLinkError"
            ? t("researchFinance.cashLinkError")
            : t(`researchPlan.${error ?? "conflict"}`)}
        </p>
      )}
      {error === "invalid" && validationDetails.length > 0 && (
        <ul className="list-disc pl-5 text-sm">
          {validationDetails.map((message) => (
            <li key={message}>{message}</li>
          ))}
        </ul>
      )}
      {saved && !isDirty && <p role="status">{t("researchPlan.saved")}</p>}
    </form>
  );
}
