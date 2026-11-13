import { Button, Input } from "@wealthfolio/ui";
import { useState, type ComponentProps, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type {
  Debt,
  LiabilityRow,
  PlanningProfile,
} from "../../../../../../wealth-manager/lib/assessment-contracts";
import type { ResearchContext, ResearchPlan } from "@/adapters/shared/research";

export type FinanceValues = Pick<PlanningProfile, "cash" | "payments" | "debts"> &
  Pick<ResearchPlan, "cashLinks">;
export type FinanceKey = keyof FinanceValues;
interface Props extends FinanceValues {
  accounts: ResearchContext["accounts"];
  change: <K extends FinanceKey>(key: K, value: FinanceValues[K]) => void;
}
const grid = "grid gap-3 sm:grid-cols-2";
const card = "space-y-3 rounded-lg border p-3";
const control = "w-full rounded-md border bg-transparent p-2 text-sm";
function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="block space-y-1 text-sm">
      <span>{label}</span>
      {children}
    </label>
  );
}
function Action({ children, ...props }: ComponentProps<typeof Button>) {
  return (
    <Button
      type="button"
      variant="outline"
      className="h-auto max-w-full whitespace-normal"
      {...props}
    >
      {children}
    </Button>
  );
}

export function FinanceEntries({ cash, payments, debts, cashLinks, accounts, change }: Props) {
  const { t } = useTranslation("common");
  const tr = (key: string) => t(`researchFinance.${key}`);
  return (
    <div className="space-y-6">
      <section className="space-y-3" aria-label={tr("cash")}>
        <h3 className="font-medium">{tr("cash")}</h3>
        <p className="text-muted-foreground text-sm">{tr("cashNote")}</p>
        {cash.map((value, i) => {
          const link = cashLinks.find((link) => link.cashId === value.id);
          const source = accounts.find((account) => account.id === link?.accountId);
          const update = <K extends keyof typeof value>(key: K, next: (typeof value)[K]) =>
            change(
              "cash",
              cash.map((row, j) =>
                j === i
                  ? {
                      ...row,
                      [key]: next,
                      ...(["amount", "currency", "date"].includes(key) && row[key] !== next
                        ? { reconciled: false }
                        : {}),
                    }
                  : row,
              ),
            );
          return (
            <fieldset className={card} key={value.id}>
              <legend>
                {tr("cash")} {i + 1}
              </legend>
              <div className={grid}>
                <Field label={t("name")}>
                  <Input
                    value={value.name}
                    maxLength={100}
                    onChange={(e) => update("name", e.target.value)}
                  />
                </Field>
                <Field label={t("currency")}>
                  <select
                    className={control}
                    value={value.currency}
                    onChange={(e) => update("currency", e.target.value as "INR" | "USD")}
                  >
                    <option>INR</option>
                    <option>USD</option>
                  </select>
                </Field>
                <Field label={tr("balance")}>
                  <Input
                    inputMode="decimal"
                    value={value.amount}
                    onChange={(e) => update("amount", e.target.value)}
                  />
                </Field>
                <Field label={tr("reserve")}>
                  <Input
                    inputMode="decimal"
                    value={value.reserve}
                    onChange={(e) => update("reserve", e.target.value)}
                  />
                </Field>
                <Field label={t("date")}>
                  <Input
                    type="date"
                    value={value.date}
                    onChange={(e) => update("date", e.target.value)}
                  />
                </Field>
                <Field label={tr("source")}>
                  <select
                    className={control}
                    value={link?.accountId ?? ""}
                    onChange={(e) => {
                      update("reconciled", false);
                      change("cashLinks", [
                        ...cashLinks.filter((row) => row.cashId !== value.id),
                        ...(e.target.value
                          ? [{ cashId: value.id, accountId: e.target.value }]
                          : []),
                      ]);
                    }}
                  >
                    <option value="">{tr("manual")}</option>
                    {link && !accounts.some((a) => a.id === link.accountId) && (
                      <option value={link.accountId}>{tr("unavailable")}</option>
                    )}
                    {accounts.map((account) => (
                      <option key={account.id} value={account.id}>
                        {account.name}
                      </option>
                    ))}
                  </select>
                </Field>
              </div>
              {link && (
                <p className="text-muted-foreground text-sm">
                  {tr("sourceSnapshot")}:{" "}
                  {source?.cash[value.currency] ?? t("researchPlan.unknown")} {value.currency} ·{" "}
                  {source?.snapshotDate ?? t("researchPlan.unknown")}
                </p>
              )}
              <label className="flex items-start gap-2 text-sm">
                <input
                  type="checkbox"
                  checked={value.reconciled}
                  onChange={(e) => update("reconciled", e.target.checked)}
                />
                {tr("reconciled")}
              </label>
              <Action
                onClick={() => {
                  change(
                    "cash",
                    cash.filter((row) => row.id !== value.id),
                  );
                  change(
                    "cashLinks",
                    cashLinks.filter((row) => row.cashId !== value.id),
                  );
                }}
              >
                {tr("removeCash")}
              </Action>
            </fieldset>
          );
        })}
        <Action
          disabled={cash.length >= 30}
          onClick={() =>
            change("cash", [
              ...cash,
              {
                id: crypto.randomUUID(),
                name: "",
                currency: "INR",
                amount: "",
                reserve: "",
                date: "",
                reconciled: false,
              },
            ])
          }
        >
          {tr("addCash")}
        </Action>
      </section>
      <section className="space-y-3" aria-label={tr("payments")}>
        <h3 className="font-medium">{tr("payments")}</h3>
        <p className="text-muted-foreground text-sm">{tr("paymentsNote")}</p>
        {payments.map((value, i) => (
          <fieldset className={card} key={value.id}>
            <legend>
              {tr("payments")} {i + 1}
            </legend>
            <div className={grid}>
              <Field label={t("name")}>
                <Input
                  maxLength={100}
                  value={value.label}
                  onChange={(e) =>
                    change(
                      "payments",
                      payments.map((p, j) => (j === i ? { ...p, label: e.target.value } : p)),
                    )
                  }
                />
              </Field>
              <Field label={t("currency")}>
                <select
                  className={control}
                  value={value.currency}
                  onChange={(e) =>
                    change(
                      "payments",
                      payments.map((p, j) =>
                        j === i ? { ...p, currency: e.target.value as "INR" | "USD" } : p,
                      ),
                    )
                  }
                >
                  <option>INR</option>
                  <option>USD</option>
                </select>
              </Field>
              <Field label={t("amount")}>
                <Input
                  inputMode="decimal"
                  value={value.amount}
                  onChange={(e) =>
                    change(
                      "payments",
                      payments.map((p, j) => (j === i ? { ...p, amount: e.target.value } : p)),
                    )
                  }
                />
              </Field>
              <Field label={t("date")}>
                <Input
                  type="date"
                  value={value.date}
                  onChange={(e) =>
                    change(
                      "payments",
                      payments.map((p, j) => (j === i ? { ...p, date: e.target.value } : p)),
                    )
                  }
                />
              </Field>
            </div>
            <Action
              onClick={() =>
                change(
                  "payments",
                  payments.filter((p) => p.id !== value.id),
                )
              }
            >
              {tr("removePayment")}
            </Action>
          </fieldset>
        ))}
        <Action
          disabled={payments.length >= 50}
          onClick={() =>
            change("payments", [
              ...payments,
              { id: crypto.randomUUID(), label: "", currency: "INR", amount: "", date: "" },
            ])
          }
        >
          {tr("addPayment")}
        </Action>
      </section>
      <section className="space-y-3" aria-label={tr("debts")}>
        <h3 className="font-medium">{tr("debts")}</h3>
        <p className="text-muted-foreground text-sm">{tr("debtNote")}</p>
        {debts.map((debt, i) => (
          <DebtEntry
            key={debt.id}
            debt={debt}
            number={i + 1}
            change={(next) =>
              change(
                "debts",
                debts.map((d) => (d.id === debt.id ? next : d)),
              )
            }
            remove={() =>
              change(
                "debts",
                debts.filter((d) => d.id !== debt.id),
              )
            }
          />
        ))}
        <Action
          disabled={debts.length >= 20}
          onClick={() =>
            change("debts", [
              ...debts,
              {
                id: crypto.randomUUID(),
                label: "",
                currency: "INR",
                balance: "",
                annualInterestPct: null,
                repayments: [],
              },
            ])
          }
        >
          {tr("addDebt")}
        </Action>
      </section>
    </div>
  );
}

const movements = [
  "interestAccrued",
  "feesAccrued",
  "capitalizedInterest",
  "capitalizedFees",
  "principalPaid",
  "interestPaid",
  "feesPaid",
] as const;
function DebtEntry({
  debt,
  number,
  change,
  remove,
}: {
  debt: Debt;
  number: number;
  change: (debt: Debt) => void;
  remove: () => void;
}) {
  const { t } = useTranslation("common");
  const tr = (key: string) => t(`researchFinance.${key}`);
  const [expanded, setExpanded] = useState(false);
  const path = debt.liabilityPath;
  const update = <K extends keyof Debt>(key: K, value: Debt[K]) =>
    change({ ...debt, [key]: value });
  return (
    <fieldset className={card}>
      <legend>
        {tr("debts")} {number}
      </legend>
      <div className={grid}>
        <Field label={t("name")}>
          <Input
            maxLength={100}
            value={debt.label}
            onChange={(e) => update("label", e.target.value)}
          />
        </Field>
        <Field label={t("currency")}>
          <select
            className={control}
            value={debt.currency}
            onChange={(e) =>
              change({
                ...debt,
                currency: e.target.value as Debt["currency"],
                repayments: debt.repayments.map((p) => ({
                  ...p,
                  currency: e.target.value as Debt["currency"],
                })),
              })
            }
          >
            <option>INR</option>
            <option>USD</option>
          </select>
        </Field>
        <Field label={tr("balance")}>
          <Input
            inputMode="decimal"
            value={debt.balance}
            onChange={(e) => update("balance", e.target.value)}
          />
        </Field>
        <Field label={tr("balanceDate")}>
          <Input
            type="date"
            value={debt.balanceDate ?? ""}
            onChange={(e) => update("balanceDate", e.target.value || null)}
          />
        </Field>
        <Field label={tr("annualInterest")}>
          <Input
            inputMode="decimal"
            value={debt.annualInterestPct ?? ""}
            onChange={(e) => update("annualInterestPct", e.target.value || null)}
          />
        </Field>
        <Field label={tr("scheduledThrough")}>
          <Input
            type="date"
            value={debt.cashScheduleThrough ?? ""}
            onChange={(e) => update("cashScheduleThrough", e.target.value || null)}
          />
        </Field>
      </div>
      <Field label={tr("scheduleBasis")}>
        <textarea
          className={control}
          maxLength={1000}
          value={debt.scheduleBasis ?? ""}
          onChange={(e) => update("scheduleBasis", e.target.value)}
        />
      </Field>
      <label className="flex items-start gap-2 text-sm">
        <input
          type="checkbox"
          checked={debt.scheduleConfirmed ?? false}
          onChange={(e) => update("scheduleConfirmed", e.target.checked)}
        />
        {tr("scheduleConfirmed")}
      </label>
      {debt.repayments.map((payment, i) => (
        <fieldset className={card} key={payment.id}>
          <legend>
            {tr("repayment")} {i + 1} · {debt.currency}
          </legend>
          <div className={grid}>
            <Field label={t("name")}>
              <Input
                maxLength={100}
                value={payment.label}
                onChange={(e) =>
                  update(
                    "repayments",
                    debt.repayments.map((p, j) => (j === i ? { ...p, label: e.target.value } : p)),
                  )
                }
              />
            </Field>
            <Field label={t("amount")}>
              <Input
                inputMode="decimal"
                value={payment.amount}
                onChange={(e) =>
                  update(
                    "repayments",
                    debt.repayments.map((p, j) => (j === i ? { ...p, amount: e.target.value } : p)),
                  )
                }
              />
            </Field>
            <Field label={t("date")}>
              <Input
                type="date"
                value={payment.date}
                onChange={(e) =>
                  update(
                    "repayments",
                    debt.repayments.map((p, j) => (j === i ? { ...p, date: e.target.value } : p)),
                  )
                }
              />
            </Field>
          </div>
          <Action
            onClick={() =>
              update(
                "repayments",
                debt.repayments.filter((p) => p.id !== payment.id),
              )
            }
          >
            {tr("removeRepayment")}
          </Action>
        </fieldset>
      ))}
      <Action
        disabled={debt.repayments.length >= 30}
        onClick={() =>
          update("repayments", [
            ...debt.repayments,
            { id: crypto.randomUUID(), label: "", currency: debt.currency, amount: "", date: "" },
          ])
        }
      >
        {tr("addRepayment")}
      </Action>
      <details onToggle={(event) => setExpanded(event.currentTarget.open)}>
        <summary>{tr("path")}</summary>
        {expanded && (
          <div className="space-y-3 py-3">
            <p className="text-muted-foreground text-sm">{tr("pathNote")}</p>
            {!path ? (
              <Action
                onClick={() =>
                  update("liabilityPath", {
                    openingDate: debt.balanceDate ?? "",
                    principal: "",
                    interestDue: "",
                    feesDue: "",
                    confirmedThrough: "",
                    rows: [],
                  })
                }
              >
                {tr("addPath")}
              </Action>
            ) : (
              <>
                <div className={grid}>
                  <Field label={tr("openingDate")}>
                    <Input
                      type="date"
                      value={path.openingDate}
                      onChange={(e) =>
                        update("liabilityPath", { ...path, openingDate: e.target.value })
                      }
                    />
                  </Field>
                  {(["principal", "interestDue", "feesDue"] as const).map((key) => (
                    <Field key={key} label={tr(key)}>
                      <Input
                        inputMode="decimal"
                        value={path[key]}
                        onChange={(e) =>
                          update("liabilityPath", { ...path, [key]: e.target.value })
                        }
                      />
                    </Field>
                  ))}
                  <Field label={tr("confirmedThrough")}>
                    <Input
                      type="date"
                      value={path.confirmedThrough}
                      onChange={(e) =>
                        update("liabilityPath", { ...path, confirmedThrough: e.target.value })
                      }
                    />
                  </Field>
                </div>
                {path.rows.map((row, i) => (
                  <fieldset className={card} key={i}>
                    <legend>
                      {tr("movement")} {i + 1}
                    </legend>
                    <div className={grid}>
                      <Field label={t("date")}>
                        <Input
                          type="date"
                          value={row.date}
                          onChange={(e) =>
                            update("liabilityPath", {
                              ...path,
                              rows: path.rows.map((r, j) =>
                                j === i ? { ...r, date: e.target.value } : r,
                              ),
                            })
                          }
                        />
                      </Field>
                      {movements.map((key) => (
                        <Field key={key} label={tr(key)}>
                          <Input
                            inputMode="decimal"
                            value={row[key]}
                            onChange={(e) =>
                              update("liabilityPath", {
                                ...path,
                                rows: path.rows.map((r, j) =>
                                  j === i ? { ...r, [key]: e.target.value } : r,
                                ),
                              })
                            }
                          />
                        </Field>
                      ))}
                    </div>
                    <Action
                      onClick={() =>
                        update("liabilityPath", {
                          ...path,
                          rows: path.rows.filter((_, j) => j !== i),
                        })
                      }
                    >
                      {tr("removeMovement")}
                    </Action>
                  </fieldset>
                ))}
                <div className="flex flex-wrap gap-2">
                  <Action
                    disabled={path.rows.length >= 150}
                    onClick={() => {
                      const row: LiabilityRow = {
                        date: "",
                        interestAccrued: "",
                        feesAccrued: "",
                        capitalizedInterest: "",
                        capitalizedFees: "",
                        principalPaid: "",
                        interestPaid: "",
                        feesPaid: "",
                      };
                      update("liabilityPath", { ...path, rows: [...path.rows, row] });
                    }}
                  >
                    {tr("addMovement")}
                  </Action>
                  <Action onClick={() => update("liabilityPath", null)}>{tr("removePath")}</Action>
                </div>
              </>
            )}
          </div>
        )}
      </details>
      <Action onClick={remove}>{tr("removeDebt")}</Action>
    </fieldset>
  );
}
