import { useState } from "react";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import common from "@/i18n/locales/en/common.json";
import { FinanceEntries, type FinanceValues, type FinanceKey } from "./finance-entries";
import type { ResearchContext } from "@/adapters/shared/research";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) =>
      key.split(".").reduce<unknown>((v, k) => (v as Record<string, unknown>)[k], common),
  }),
}));
const empty: FinanceValues = { cash: [], payments: [], debts: [], cashLinks: [] };
const accounts: ResearchContext["accounts"] = [
  {
    id: "fictional-source",
    name: "Fictional source",
    currency: "INR",
    archived: false,
    snapshotDate: "2026-10-25",
    snapshotSource: "MANUAL",
    cash: { INR: "123.45678901" },
  },
];
function open(initial: FinanceValues) {
  function Harness() {
    const [value, setValue] = useState(structuredClone(initial));
    function change<K extends FinanceKey>(key: K, next: FinanceValues[K]) {
      setValue((current) => ({ ...current, [key]: next }));
    }
    return (
      <>
        <FinanceEntries {...value} accounts={accounts} change={change} />
        <output data-testid="values">{JSON.stringify(value)}</output>
      </>
    );
  }
  render(<Harness />);
  return () => JSON.parse(screen.getByTestId("values").textContent ?? "{}") as FinanceValues;
}

it("new cash rows leave amounts, reserves and observation dates unanswered", () => {
  const value = open(empty);
  fireEvent.click(screen.getByRole("button", { name: common.researchFinance.addCash }));
  expect(value().cash[0]).toMatchObject({ amount: "", reserve: "", date: "", reconciled: false });
  expect(value().cashLinks).toEqual([]);
});

it("linking cash preserves exact answers and removing it removes only its own link", () => {
  const initial: FinanceValues = {
    ...empty,
    cash: [
      {
        id: "cash-one",
        name: "Fictional",
        currency: "INR",
        amount: "123.45678901",
        reserve: "0",
        date: "2026-10-25",
        reconciled: false,
      },
    ],
    cashLinks: [{ cashId: "other-cash", accountId: "other-source" }],
  };
  const value = open(initial);
  fireEvent.change(screen.getByLabelText(common.researchFinance.source), {
    target: { value: "fictional-source" },
  });
  expect(value().cash[0].amount).toBe("123.45678901");
  expect(value().cash[0].reconciled).toBe(false);
  expect(value().cashLinks).toHaveLength(2);
  fireEvent.click(screen.getByRole("button", { name: common.researchFinance.removeCash }));
  expect(value().cash).toEqual([]);
  expect(value().cashLinks).toEqual([{ cashId: "other-cash", accountId: "other-source" }]);
});

it("requires cash confirmation again after changing the balance, date, currency or source", () => {
  const value = open({
    ...empty,
    cash: [
      {
        id: "cash-one",
        name: "Fictional",
        currency: "INR",
        amount: "123.45678901",
        reserve: "0",
        date: "2026-10-25",
        reconciled: true,
      },
    ],
  });
  const confirmation = screen.getByLabelText(common.researchFinance.reconciled);
  for (const [label, next] of [
    [common.researchFinance.balance, "124"],
    [common.date, "2026-10-24"],
    [common.currency, "USD"],
    [common.researchFinance.source, "fictional-source"],
  ]) {
    fireEvent.change(screen.getByLabelText(label), { target: { value: next } });
    expect(value().cash[0].reconciled).toBe(false);
    expect(confirmation).not.toBeChecked();
    fireEvent.click(confirmation);
  }
});

it("essential payments and debt repayments stay in separate lists and currencies", () => {
  const value = open(empty);
  fireEvent.click(screen.getByRole("button", { name: common.researchFinance.addPayment }));
  fireEvent.click(screen.getByRole("button", { name: common.researchFinance.addDebt }));
  const debt = screen.getByRole("group", { name: `${common.researchFinance.debts} 1` });
  fireEvent.change(within(debt).getByLabelText(common.currency), { target: { value: "USD" } });
  fireEvent.click(within(debt).getByRole("button", { name: common.researchFinance.addRepayment }));
  expect(value().payments).toHaveLength(1);
  expect(value().debts[0].repayments[0]).toMatchObject({ currency: "USD", amount: "", date: "" });
  expect(value().payments[0].id).not.toBe(value().debts[0].repayments[0].id);
  expect(value().debts[0].annualInterestPct).toBeNull();
});

it("closing optional reconciliation controls preserves all previously entered movements", async () => {
  const path = {
    openingDate: "2026-10-25",
    principal: "9",
    interestDue: "1.00000001",
    feesDue: "0",
    confirmedThrough: "2026-11-25",
    rows: [
      {
        date: "2026-11-25",
        interestAccrued: "0",
        feesAccrued: "0",
        capitalizedInterest: "1.00000001",
        capitalizedFees: "0",
        principalPaid: "0",
        interestPaid: "0",
        feesPaid: "0",
      },
    ],
  };
  const value = open({
    ...empty,
    debts: [
      {
        id: "debt-one",
        label: "Fictional loan",
        currency: "USD",
        balance: "10.00000001",
        annualInterestPct: null,
        repayments: [],
        liabilityPath: path,
      },
    ],
  });
  const details = screen.getByText(common.researchFinance.path).closest("details")!;
  details.open = true;
  fireEvent(details, new Event("toggle"));
  await screen.findByLabelText(common.researchFinance.principal);
  details.open = false;
  fireEvent(details, new Event("toggle"));
  await waitFor(() =>
    expect(screen.queryByLabelText(common.researchFinance.principal)).not.toBeInTheDocument(),
  );
  expect(value().debts[0].liabilityPath).toEqual(path);
});
