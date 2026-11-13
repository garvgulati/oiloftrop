import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { installProfileSession } from "@/features/profiles/session";
import { invoke } from "./web/core";

beforeEach(() => {
  installProfileSession({ profileId: "synthetic-profile", scopeId: "synthetic-scope" });
});
afterEach(() => vi.unstubAllGlobals());
it.each([
  [
    "create_reviewed_import_account",
    "accounts",
    { id: "synthetic-id", name: "Synthetic account", currency: "USD" },
  ],
  ["prepare_reviewed_import", "prepare", { quantity: "1.23456789", averageCost: null }],
  [
    "confirm_reviewed_import",
    "confirm",
    { reviewId: "synthetic-review", inputHash: "synthetic-hash" },
  ],
  ["discard_reviewed_import", "discard", { reviewId: "synthetic-review" }],
  ["latest_reviewed_import", "latest", { accountId: "synthetic-account" }],
])(
  "transports %s without adding browser authority or rounding values",
  async (command, path, request) => {
    const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(new Response("null"));
    vi.stubGlobal("fetch", fetchMock);
    await invoke(command, { request });
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe(`/api/v1/personal/holdings/${path}`);
    expect(init?.method).toBe("POST");
    expect(JSON.parse(init?.body as string)).toEqual(request);
    expect(new Headers(init?.headers).get("x-wf-profile-scope")).toBe("synthetic-scope");
  },
);
it("checks status without submitting private fields", async () => {
  const fetchMock = vi
    .fn<typeof fetch>()
    .mockResolvedValue(new Response('{"supported":false,"accounts":[]}'));
  vi.stubGlobal("fetch", fetchMock);
  expect(await invoke("get_reviewed_import_status")).toEqual({ supported: false, accounts: [] });
  expect(fetchMock.mock.calls[0][1]?.body).toBeUndefined();
});
