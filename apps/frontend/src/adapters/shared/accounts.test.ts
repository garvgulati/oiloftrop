import { beforeEach, describe, expect, it, vi } from "vitest";
import { updateAccount } from "./accounts";

const platform = vi.hoisted(() => ({ invoke: vi.fn(), desktop: false }));
vi.mock("./platform", () => ({
  invoke: platform.invoke,
  logger: { error: vi.fn() },
  get isTauri() {
    return platform.desktop;
  },
}));

const account = {
  id: "8f14e45f-ceea-4e7a-9b5e-3c1d2a6b7f90",
  name: "Fictional account",
  accountType: "SECURITIES" as const,
  currency: "INR",
  isActive: true,
  isDefault: false,
  isArchived: false,
  trackingMode: "HOLDINGS" as const,
  expectedUpdatedAt: "2026-10-22T10:00:00.123456",
};

describe("account update version contract", () => {
  beforeEach(() => {
    platform.invoke.mockReset();
    platform.desktop = false;
  });

  it("sends the original version with full timestamp precision to the web API", async () => {
    await updateAccount(account);
    expect(platform.invoke).toHaveBeenCalledWith("update_account", { accountUpdate: account });
  });

  it("carries the same version to Tauri while retaining its immutable-currency boundary", async () => {
    platform.desktop = true;
    await updateAccount(account);
    const payload = (platform.invoke.mock.calls[0][1] as { accountUpdate: Record<string, unknown> })
      .accountUpdate;
    expect(payload.expectedUpdatedAt).toBe(account.expectedUpdatedAt);
    expect(payload).not.toHaveProperty("currency");
  });

  it("rejects an unversioned update before making any request", async () => {
    await expect(updateAccount({ ...account, expectedUpdatedAt: undefined })).rejects.toThrow(
      "Reopen the account",
    );
    expect(platform.invoke).not.toHaveBeenCalled();
  });
});
