import { afterEach, expect, it, vi } from "vitest";
import { installProfileSession } from "@/features/profiles/session";
import { invoke } from "./web/core";
afterEach(() => vi.unstubAllGlobals());
it("keeps private context and credentials out of public research requests", async () => {
  installProfileSession({ profileId: "profile-canary", scopeId: "scope-canary" });
  const fetchMock = vi
    .fn<typeof fetch>()
    .mockImplementation(() => Promise.resolve(new Response("{}")));
  vi.stubGlobal("fetch", fetchMock);
  await invoke("search_india_research", {
    request: { query: "ONGC", token: "secret-canary", plan: { cash: 5 } },
  });
  await invoke("analyse_india_research", {
    request: {
      instrumentKey: "NSE_EQ|INE213A01029",
      basis: "consolidated",
      token: "secret-canary",
      portfolio: { quantity: 1 },
    },
  });
  expect(JSON.parse(fetchMock.mock.calls[0][1]?.body as string)).toEqual({ query: "ONGC" });
  expect(JSON.parse(fetchMock.mock.calls[1][1]?.body as string)).toEqual({
    instrumentKey: "NSE_EQ|INE213A01029",
    basis: "consolidated",
  });
  await invoke("load_india_research_chart", {
    request: {
      instrumentKey: "NSE_EQ|INE213A01029",
      range: "day",
      token: "secret-canary",
      portfolio: { quantity: 1 },
    },
  });
  expect(JSON.parse(fetchMock.mock.calls[2][1]?.body as string)).toEqual({
    instrumentKey: "NSE_EQ|INE213A01029",
    range: "day",
  });
  await invoke("load_india_research_ratios", {
    request: {
      instrumentKey: "NSE_EQ|INE213A01029",
      token: "secret-canary",
      portfolio: { quantity: 1 },
    },
  });
  expect(JSON.parse(fetchMock.mock.calls[3][1]?.body as string)).toEqual({
    instrumentKey: "NSE_EQ|INE213A01029",
  });
  for (const [, init] of fetchMock.mock.calls)
    expect(new Headers(init?.headers).get("x-wf-profile-scope")).toBe("scope-canary");
});
