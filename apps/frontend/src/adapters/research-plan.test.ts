import { afterEach, beforeEach, expect, it, vi } from "vitest";
import fixture from "../../../../packages/research-engine/fixtures/conditional-example-v1.json";
import { installProfileSession } from "@/features/profiles/session";
import { invoke } from "./web/core";

beforeEach(() => {
  installProfileSession({ profileId: "fictional-profile", scopeId: "fictional-scope" });
});
afterEach(() => vi.unstubAllGlobals());

it("reads an absent plan within the admitted profile without creating answers", async () => {
  const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(new Response("null"));
  vi.stubGlobal("fetch", fetchMock);
  expect(await invoke("get_research_plan")).toBeNull();
  const [url, init] = fetchMock.mock.calls[0];
  expect(url).toBe("/api/v1/research/plan");
  expect(new Headers(init?.headers).get("x-wf-profile-scope")).toBe("fictional-scope");
  expect(init?.body).toBeUndefined();
});

it("transports the original edit revision and exact/unknown answers unchanged", async () => {
  const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(new Response("null"));
  vi.stubGlobal("fetch", fetchMock);
  const profile = structuredClone(fixture.portfolio.planning);
  const request = {
    expectedRevision: 0,
    plan: {
      profile: { ...profile, canAffordToLoseBase: null, newMoneyAvailableBase: "123.45678901" },
      cashLinks: [],
    },
  };
  await invoke("save_research_plan", { request });
  const [url, init] = fetchMock.mock.calls[0];
  expect(url).toBe("/api/v1/research/plan");
  expect(init?.method).toBe("PUT");
  expect(JSON.parse(init?.body as string)).toEqual(request);
  expect(new Headers(init?.headers).get("x-wf-profile-scope")).toBe("fictional-scope");
});

it("surfaces a stale edit conflict without retrying or changing the draft", async () => {
  const fetchMock = vi
    .fn<typeof fetch>()
    .mockImplementation(() =>
      Promise.resolve(
        new Response(JSON.stringify({ message: "RESEARCH_PLAN_EDIT_CONFLICT" }), { status: 409 }),
      ),
    );
  vi.stubGlobal("fetch", fetchMock);
  const request = {
    expectedRevision: 1,
    plan: { profile: fixture.portfolio.planning, cashLinks: [] },
  };
  const before = JSON.stringify(request);
  await expect(invoke("save_research_plan", { request })).rejects.toThrow(
    "RESEARCH_PLAN_EDIT_CONFLICT",
  );
  expect(fetchMock).toHaveBeenCalledTimes(1);
  expect(JSON.stringify(request)).toBe(before);
});
