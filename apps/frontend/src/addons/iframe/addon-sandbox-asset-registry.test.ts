// @vitest-environment node

import { afterEach, describe, expect, it, vi } from "vitest";
import {
  normalizeAddonAssetPath,
  resolveAddonAssetPath,
  SandboxAddonAssetRegistry,
} from "./addon-sandbox-asset-registry";

afterEach(() => {
  vi.restoreAllMocks();
});

describe("sandbox addon asset registry", () => {
  it("normalizes package paths without permitting traversal", () => {
    expect(normalizeAddonAssetPath("/dist/./assets/../assets/logo.png")).toBe(
      "dist/assets/logo.png",
    );
    expect(resolveAddonAssetPath("../assets/logo.png", "dist/styles/addon.css")).toBe(
      "dist/assets/logo.png",
    );
    expect(() => normalizeAddonAssetPath("../../outside.png")).toThrow("escapes the package root");
  });

  it("loads once, returns a typed Blob, and reuses one object URL", async () => {
    const bytes = new Uint8Array([1, 2, 3]);
    const requestBlob = vi.fn().mockResolvedValue(new Blob([bytes]));
    const createObjectURL = vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:addon-logo");
    const registry = new SandboxAddonAssetRegistry(
      [
        {
          id: "asset-id",
          mimeType: "image/png",
          path: "assets/logo.png",
          size: bytes.byteLength,
        },
      ],
      requestBlob,
    );

    const [firstBlob, secondBlob, firstUrl, secondUrl] = await Promise.all([
      registry.getBlob("/assets/logo.png"),
      registry.getBlob("assets/logo.png"),
      registry.getUrl("assets/logo.png"),
      registry.getUrl("assets/logo.png"),
    ]);

    expect(firstBlob).toBe(secondBlob);
    expect(firstBlob.type).toBe("image/png");
    expect(firstUrl).toBe("blob:addon-logo");
    expect(secondUrl).toBe(firstUrl);
    expect(requestBlob).toHaveBeenCalledTimes(1);
    expect(requestBlob).toHaveBeenCalledWith("asset-id");
    expect(createObjectURL).toHaveBeenCalledTimes(1);
    expect(registry.has("assets/logo.png")).toBe(true);
    expect(registry.list()).toEqual([expect.objectContaining({ path: "assets/logo.png" })]);
    expect(registry.list()[0]).not.toHaveProperty("id");
  });

  it("revokes object URLs on clear", async () => {
    vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:addon-logo");
    const revokeObjectURL = vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => undefined);
    const registry = new SandboxAddonAssetRegistry(
      [{ id: "asset-id", mimeType: "image/png", path: "assets/logo.png", size: 1 }],
      vi.fn().mockResolvedValue(new Blob([new Uint8Array([1])], { type: "image/png" })),
    );

    await registry.getUrl("assets/logo.png");
    registry.clear();

    expect(revokeObjectURL).toHaveBeenCalledWith("blob:addon-logo");
  });

  it("rejects duplicate normalized paths", () => {
    expect(
      () =>
        new SandboxAddonAssetRegistry(
          [
            { id: "first", mimeType: "image/png", path: "/assets/logo.png", size: 1 },
            { id: "second", mimeType: "image/png", path: "assets/logo.png", size: 1 },
          ],
          vi.fn(),
        ),
    ).toThrow("Duplicate packaged addon asset path 'assets/logo.png'");
  });

  it("does not create an object URL when cleared during a pending load", async () => {
    let resolveBlob!: (blob: Blob) => void;
    const requestBlob = vi.fn(
      () =>
        new Promise<Blob>((resolve) => {
          resolveBlob = resolve;
        }),
    );
    const createObjectURL = vi.spyOn(URL, "createObjectURL");
    const registry = new SandboxAddonAssetRegistry(
      [{ id: "asset-id", mimeType: "image/png", path: "assets/logo.png", size: 1 }],
      requestBlob,
    );

    const loading = registry.getUrl("assets/logo.png");
    registry.clear();
    resolveBlob(new Blob([new Uint8Array([1])], { type: "image/png" }));

    await expect(loading).rejects.toThrow("registry was cleared while loading");
    expect(createObjectURL).not.toHaveBeenCalled();
  });

  it("retries after a failed asset load", async () => {
    const requestBlob = vi
      .fn()
      .mockResolvedValueOnce(new Blob([new Uint8Array([1, 2])], { type: "image/png" }))
      .mockResolvedValueOnce(new Blob([new Uint8Array([1, 2, 3])], { type: "image/png" }));
    const registry = new SandboxAddonAssetRegistry(
      [{ id: "asset-id", mimeType: "image/png", path: "assets/logo.png", size: 3 }],
      requestBlob,
    );

    await expect(registry.getBlob("assets/logo.png")).rejects.toThrow("changed while loading");
    await expect(registry.getBlob("assets/logo.png")).resolves.toMatchObject({ size: 3 });
    expect(requestBlob).toHaveBeenCalledTimes(2);
  });
});
