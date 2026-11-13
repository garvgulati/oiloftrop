import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { StepNavigation } from "./step-navigation";

describe("StepNavigation", () => {
  it("explains why the next action is disabled while loading", () => {
    render(
      <StepNavigation
        onNext={vi.fn()}
        onBack={vi.fn()}
        canGoBack
        canGoNext={false}
        isNextLoading
        nextLoadingLabel="Checking your holdings…"
      />,
    );

    const button = screen.getByRole("button", { name: "Checking your holdings…" });
    expect(button).toBeDisabled();
  });
});
