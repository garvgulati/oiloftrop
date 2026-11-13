import { afterEach, expect, it, vi } from "vitest";
import { installProfileSession } from "@/features/profiles/session";
import { invoke } from "./web/core";
afterEach(() => vi.unstubAllGlobals());
it("sends only the stock identifier and retains owner/profile authorization on the local host request", async () => {
  installProfileSession({ profileId: "private-profile-canary", scopeId: "private-scope-canary" });
  const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(new Response("{}"));
  vi.stubGlobal("fetch", fetchMock);
  await invoke("load_research_company", {
    request: {
      symbol: "MSFT",
      plan: { cash: "private-cash-canary" },
      apiKey: "private-key-canary",
    },
  });
  const [url, init] = fetchMock.mock.calls[0];
  expect(url).toBe("/api/v1/research/company");
  expect(init?.method).toBe("POST");
  expect(JSON.parse(init?.body as string)).toEqual({ symbol: "MSFT" });
  expect(new Headers(init?.headers).get("x-wf-profile-scope")).toBe("private-scope-canary");
});
