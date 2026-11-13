import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import fixture from "../../../../../packages/research-engine/fixtures/conditional-example-v1.json";
import common from "@/i18n/locales/en/common.json";
import { ResearchPlanEditor } from "./plan-editor";
import type { ResearchPlan, StoredResearchPlan } from "@/adapters/shared/research";
import { planningProfileSchema } from "../../../../../../wealth-manager/lib/assessment-contracts";

const api = vi.hoisted(() => ({
  getResearchPlan: vi.fn<() => Promise<StoredResearchPlan | null>>(),
  saveResearchPlan: vi.fn<(revision: number, plan: ResearchPlan) => Promise<StoredResearchPlan>>(),
}));
vi.mock("@/adapters", () => api);
vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) =>
      key
        .split(".")
        .reduce<unknown>((value, field) => (value as Record<string, unknown>)[field], common),
  }),
}));

const initial: StoredResearchPlan = {
  schemaVersion: 1 as const,
  mode: "fictional" as const,
  revision: 1,
  createdAt: "2026-10-25T04:30:00Z",
  updatedAt: "2026-10-25T04:30:00Z",
  plan: { profile: planningProfileSchema.parse(fixture.portfolio.planning), cashLinks: [] },
};
function open() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <ResearchPlanEditor profileId="fictional" mode="fictional" />
    </QueryClientProvider>,
  );
  return client;
}
beforeEach(() => {
  vi.clearAllMocks();
  api.getResearchPlan.mockResolvedValue(structuredClone(initial));
});

it("leaves absent answers unknown and does not save on loading", async () => {
  api.getResearchPlan.mockResolvedValue(null);
  open();
  await screen.findByLabelText(common.researchPlan.goalLabel);
  expect(screen.getByLabelText(common.researchPlan.canAffordToLoseBase)).toHaveValue("");
  expect(api.saveResearchPlan).not.toHaveBeenCalled();
  expect(screen.getByText(common.researchPlan.fictional)).toBeInTheDocument();
});

it("preserves cash, debt and unknown answers when saving goal edits", async () => {
  api.saveResearchPlan.mockImplementation((_version, plan) =>
    Promise.resolve({
      ...initial,
      revision: 2,
      plan,
    }),
  );
  open();
  fireEvent.change(await screen.findByLabelText(common.researchPlan.goalLabel), {
    target: { value: "Fictional goal edit" },
  });
  fireEvent.change(screen.getByLabelText(common.researchPlan.newMoneyAvailableBase), {
    target: { value: "123.45678901" },
  });
  fireEvent.change(screen.getByLabelText(common.researchPlan.canAffordToLoseBase), {
    target: { value: "" },
  });
  fireEvent.click(screen.getByRole("button", { name: common.save }));
  await screen.findByText(common.researchPlan.saved);
  const [version, plan] = api.saveResearchPlan.mock.calls[0];
  expect(version).toBe(1);
  expect(plan.profile.cash).toEqual(initial.plan.profile.cash);
  expect(plan.profile.debts).toEqual(initial.plan.profile.debts);
  expect(plan.profile.newMoneyAvailableBase).toBe("123.45678901");
  expect(plan.profile.canAffordToLoseBase).toBeNull();
});

it("keeps a conflicting draft and explicitly reapplies only edited fields before a new save", async () => {
  const newer = structuredClone(initial);
  newer.revision = 2;
  newer.plan.profile.restrictions = "Newer device restriction";
  const client = open();
  fireEvent.change(await screen.findByLabelText(common.researchPlan.goalLabel), {
    target: { value: "My unsaved goal" },
  });
  api.saveResearchPlan.mockRejectedValueOnce(new Error("RESEARCH_PLAN_EDIT_CONFLICT"));
  fireEvent.click(screen.getByRole("button", { name: common.save }));
  await screen.findByText(common.researchPlan.conflict);
  expect(screen.getByLabelText(common.researchPlan.goalLabel)).toHaveValue("My unsaved goal");
  api.getResearchPlan.mockResolvedValue(newer);
  fireEvent.click(screen.getByRole("button", { name: common.researchPlan.reapply }));
  await waitFor(() =>
    expect(screen.getByLabelText(common.researchPlan.restrictions)).toHaveValue(
      "Newer device restriction",
    ),
  );
  expect(screen.getByLabelText(common.researchPlan.goalLabel)).toHaveValue("My unsaved goal");
  expect(api.saveResearchPlan).toHaveBeenCalledTimes(1);
  api.saveResearchPlan.mockImplementation((_version, plan) =>
    Promise.resolve({
      ...newer,
      revision: 3,
      plan,
    }),
  );
  fireEvent.click(screen.getByRole("button", { name: common.save }));
  await screen.findByText(common.researchPlan.saved);
  expect(api.saveResearchPlan.mock.calls[1][0]).toBe(2);
  expect(api.saveResearchPlan.mock.calls[1][1].profile.restrictions).toBe(
    "Newer device restriction",
  );
  expect(client.getQueryData(["research-plan", "fictional"])).toMatchObject({ revision: 3 });
});

it("does not replace unsaved text after background updates or a refetch failure", async () => {
  const client = open();
  fireEvent.change(await screen.findByLabelText(common.researchPlan.goalLabel), {
    target: { value: "Keep this draft" },
  });
  act(() => {
    client.setQueryData(["research-plan", "fictional"], { ...initial, revision: 2 });
  });
  expect(screen.getByLabelText(common.researchPlan.goalLabel)).toHaveValue("Keep this draft");
  api.getResearchPlan.mockRejectedValue(new Error("offline"));
  await act(async () => {
    await client.invalidateQueries({ queryKey: ["research-plan", "fictional"] });
  });
  await screen.findByText(common.researchPlan.loadError);
  expect(screen.getByLabelText(common.researchPlan.goalLabel)).toHaveValue("Keep this draft");
});

it("rejects invalid goal pairs without making a save request", async () => {
  api.getResearchPlan.mockResolvedValue(null);
  open();
  fireEvent.change(await screen.findByLabelText(common.researchPlan.goalAmount), {
    target: { value: "100" },
  });
  fireEvent.click(screen.getByRole("button", { name: common.save }));
  await screen.findByText(common.researchPlan.invalid);
  expect(api.saveResearchPlan).not.toHaveBeenCalled();
});

it("saves exact cash values and source links without assuming complete records", async () => {
  api.getResearchPlan.mockResolvedValue(null);
  api.saveResearchPlan.mockImplementation((_revision, plan) =>
    Promise.resolve({ ...initial, revision: 1, plan }),
  );
  open();
  fireEvent.click(await screen.findByRole("button", { name: common.researchFinance.addCash }));
  const cash = screen.getByRole("group", { name: `${common.researchFinance.cash} 1` });
  fireEvent.change(within(cash).getByLabelText(common.name), {
    target: { value: "Fictional manual cash" },
  });
  fireEvent.change(within(cash).getByLabelText(common.researchFinance.balance), {
    target: { value: "123.45678901" },
  });
  fireEvent.change(within(cash).getByLabelText(common.researchFinance.reserve), {
    target: { value: "0" },
  });
  fireEvent.change(within(cash).getByLabelText(common.date), { target: { value: "2026-10-25" } });
  fireEvent.click(screen.getByRole("button", { name: common.save }));
  await screen.findByText(common.researchPlan.saved);
  const saved = api.saveResearchPlan.mock.calls[0][1];
  expect(saved.profile.cash[0]).toMatchObject({
    amount: "123.45678901",
    reserve: "0",
    reconciled: false,
  });
  expect(saved.cashLinks).toEqual([]);
  expect(saved.profile.cashComplete).toBe(false);
  expect(saved.profile.debtsComplete).toBe(false);
});

it("explicitly reapplies an edited list while retaining newer untouched lists", async () => {
  open();
  await screen.findByLabelText(common.researchPlan.goalLabel);
  const cash = screen.getByRole("group", { name: `${common.researchFinance.cash} 1` });
  fireEvent.change(within(cash).getByLabelText(common.researchFinance.balance), {
    target: { value: "1234.56789012" },
  });
  api.saveResearchPlan.mockRejectedValueOnce(new Error("RESEARCH_PLAN_EDIT_CONFLICT"));
  fireEvent.click(screen.getByRole("button", { name: common.save }));
  await screen.findByText(common.researchPlan.conflict);
  expect(screen.getByText(common.researchFinance.reapplyLists)).toBeInTheDocument();
  const newer = structuredClone(initial);
  newer.revision = 2;
  newer.plan.profile.payments.push({
    id: "90000000-0000-4000-8000-000000000099",
    label: "Fictional remote payment",
    currency: "USD",
    amount: "1",
    date: "2027-10-25",
  });
  api.getResearchPlan.mockResolvedValue(newer);
  fireEvent.click(screen.getByRole("button", { name: common.researchPlan.reapply }));
  await screen.findByDisplayValue("Fictional remote payment");
  expect(api.saveResearchPlan).toHaveBeenCalledTimes(1);
  api.saveResearchPlan.mockImplementation((_revision, plan) =>
    Promise.resolve({ ...newer, revision: 3, plan }),
  );
  fireEvent.click(screen.getByRole("button", { name: common.save }));
  await screen.findByText(common.researchPlan.saved);
  const [revision, saved] = api.saveResearchPlan.mock.calls[1];
  expect(revision).toBe(2);
  expect(saved.profile.cash[0].amount).toBe("1234.56789012");
  expect(saved.profile.payments.at(-1)?.label).toBe("Fictional remote payment");
});
