import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import common from "@/i18n/locales/en/common.json";
import { ValuationPanel } from "./valuation-panel";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) =>
      key
        .split(".")
        .reduce<unknown>((value, part) => (value as Record<string, unknown>)[part], common),
    i18n: { language: "en" },
  }),
}));
const text = common.researchValuation;
it("keeps valuation temporary, invalidates confirmation on edits and clears inputs/results", () => {
  const writes = vi.spyOn(Storage.prototype, "setItem");
  const seed = {
    price: "200",
    priceDate: "2026-01-01",
    priceSource: "https://example.com/quote",
    eps: "10",
    periodStart: "2024-04-01",
    periodEnd: "2025-03-31",
    filedDate: "2025-05-01",
    earningsSource: "https://example.com/filing",
  };
  const view = render(<ValuationPanel company="Fictional Company" currency="INR" seed={seed} />);
  fireEvent.click(screen.getByText(text.title, { selector: "summary" }));
  fireEvent.click(screen.getByRole("button", { name: text.calculate }));
  expect(screen.getByRole("alert")).toHaveTextContent(text.confirm);
  fireEvent.click(screen.getByRole("checkbox"));
  fireEvent.click(screen.getByRole("button", { name: text.calculate }));
  expect(screen.getByRole("status")).toHaveTextContent("20×");
  expect(screen.getByRole("status")).not.toHaveTextContent(text.scenario);
  fireEvent.change(screen.getByLabelText(`${text.eps} (INR/share)`), { target: { value: "5" } });
  expect(screen.queryByRole("status")).toBeNull();
  expect(screen.getByRole("checkbox")).not.toBeChecked();
  fireEvent.click(screen.getByRole("checkbox"));
  fireEvent.click(screen.getByRole("button", { name: text.calculate }));
  expect(screen.getByRole("status")).toHaveTextContent("40×");
  fireEvent.click(screen.getByRole("button", { name: text.clear }));
  expect(screen.getByLabelText(`${text.eps} (INR/share)`)).toHaveValue("");
  expect(screen.queryByRole("status")).toBeNull();
  view.unmount();
  render(<ValuationPanel company="Different Company" currency="USD" />);
  expect(screen.getByLabelText(`${text.price} (USD/share)`)).toHaveValue("");
  expect(writes).not.toHaveBeenCalled();
  writes.mockRestore();
});
