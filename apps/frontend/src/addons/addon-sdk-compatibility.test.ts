import type { AddonFile, AddonManifest, ExtractedAddon } from "@wealthfolio/addon-sdk";
import { describe, expect, it } from "vitest";

describe("addon SDK compatibility", () => {
  it("accepts the v3.6 extracted addon shape", () => {
    const legacyExtractedAddon = {
      files: [] as AddonFile[],
      metadata: {} as AddonManifest,
    } satisfies ExtractedAddon;

    expect("assets" in legacyExtractedAddon).toBe(false);
  });
});
