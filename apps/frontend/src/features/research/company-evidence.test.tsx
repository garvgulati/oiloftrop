import { render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import common from "@/i18n/locales/en/common.json";
import { CompanyEvidence } from "./company-evidence";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) =>
      key
        .split(".")
        .reduce<unknown>((value, field) => (value as Record<string, unknown>)[field], common),
  }),
}));

describe("public company research pilot", () => {
  it("keeps the preparation date, reporting basis and incomplete coverage visible", () => {
    render(<CompanyEvidence />);
    expect(screen.getByText(/Prepared 25 October 2026/)).toBeVisible();
    expect(screen.getByText(/visual PDF review is incomplete/)).toBeVisible();
    expect(screen.getByText(/do not use your plan or portfolio/)).toBeVisible();
    const ongc = screen.getByText("ONGC — distinguish parent and group profit").closest("details")!;
    ongc.open = true;
    expect(within(ongc).getByText(/Unaudited results with limited review/)).toBeVisible();
    expect(within(ongc).getByText("Consolidated profit: owners")).toBeVisible();
    expect(within(ongc).getByText("Consolidated profit: noncontrolling interests")).toBeVisible();
    expect(within(ongc).getByText(/No valuation target has been established/)).toBeVisible();
  });

  it("uses primary sources in separate tabs without implying a funded US purchase", () => {
    render(<CompanyEvidence />);
    const links = screen.getAllByRole("link", { hidden: true });
    expect(links).toHaveLength(4);
    for (const link of links) {
      expect(new URL(link.getAttribute("href")!).hostname).toMatch(
        /^(nsearchives\.nseindia\.com|ongcindia\.com|www\.microsoft\.com)$/,
      );
      expect(link).toHaveAttribute("target", "_blank");
      expect(link).toHaveAttribute("rel", "noopener noreferrer");
    }
    expect(screen.getByText(/Microsoft is an unfunded watchlist example/)).toBeVisible();
  });
});
