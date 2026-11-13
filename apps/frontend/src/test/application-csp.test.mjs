// @vitest-environment node
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { assertApplicationScriptsAllowed } from "../../scripts/verify-addon-sandbox-runtime.mjs";
import { assertChartFramePolicy } from "../../scripts/verify-addon-sandbox-runtime.mjs";

const hash = (body) => `'sha256-${createHash("sha256").update(body).digest("base64")}'`;
const script = '\n document.documentElement.classList.add("dark");\n';

describe("built application CSP contract", () => {
  it("keeps the application scripts and explicit chart usable under the personal policy", () => {
    const source = readFileSync(
      new URL("../../../server/src/runtime_policy.rs", import.meta.url),
      "utf8",
    );
    const policy = source.match(/const PERSONAL_CSP: &str = "([^"]+)"/u)?.[1];
    expect(policy).toBeTruthy();
    const html = readFileSync(new URL("../../index.html", import.meta.url), "utf8");
    expect(() => assertApplicationScriptsAllowed(html, policy)).not.toThrow();
    expect(() => assertChartFramePolicy(policy)).not.toThrow();
    expect(policy).toContain("connect-src 'self';");
    expect(policy).toContain("img-src 'self' data: blob:;");
  });
  it("permits only the chart's exact external frame path, without script permissions", () => {
    expect(() =>
      assertChartFramePolicy(
        "frame-src https://www.tradingview-widget.com/embed-widget/advanced-chart/",
      ),
    ).not.toThrow();
    for (const value of [
      "*",
      "https:",
      "'self'",
      "https://www.tradingview-widget.com",
      "https://evil.example/",
      "https://www.tradingview-widget.com/embed-widget/advanced-chart/ https://evil.example/",
    ]) {
      expect(() => assertChartFramePolicy(`frame-src ${value}`)).toThrow("exact");
    }
  });
  it("accepts exact inline hashes and ignores external scripts and data blocks", () => {
    const html = `<script>${script}</script><script src="/assets/main.js"></script>
      <script type="application/json">{"data":true}</script><!-- <script>ignored()</script> -->`;
    expect(() =>
      assertApplicationScriptsAllowed(html, `script-src 'self' ${hash(script)}`),
    ).not.toThrow();
  });

  it("rejects the pre-fix policy and reports the required hash", () => {
    expect(() =>
      assertApplicationScriptsAllowed(`<script>${script}</script>`, "script-src 'self'"),
    ).toThrow(hash(script));
  });

  it("detects whitespace changes and additional inline scripts", () => {
    for (const html of [
      `<script>${script} </script>`,
      `<script>${script}</script><script type="module">run()</script>`,
    ]) {
      expect(() => assertApplicationScriptsAllowed(html, `script-src ${hash(script)}`)).toThrow(
        "missing",
      );
    }
  });

  it("checks effective directives and additional HTML policies", () => {
    expect(() =>
      assertApplicationScriptsAllowed(`<script>${script}</script>`, `default-src ${hash(script)}`),
    ).not.toThrow();
    expect(() =>
      assertApplicationScriptsAllowed(
        `<script>${script}</script>`,
        `script-src ${hash(script)}; script-src-elem 'self'`,
      ),
    ).toThrow("missing");
    expect(() =>
      assertApplicationScriptsAllowed(
        `<meta http-equiv="Content-Security-Policy" content="script-src 'self'"><script>${script}</script>`,
        `script-src ${hash(script)}`,
      ),
    ).toThrow("missing");
    expect(() =>
      assertApplicationScriptsAllowed(
        `<meta http-equiv="Content-Security-Policy" content="frame-src 'none'"><script>${script}</script>`,
        `script-src ${hash(script)}`,
      ),
    ).not.toThrow();
  });

  it("requires a hash policy on the server", () => {
    expect(() => assertApplicationScriptsAllowed(`<script>${script}</script>`, "")).toThrow(
      "missing",
    );
  });

  it("does not accept unsafe-inline as a substitute for a hash", () => {
    expect(() =>
      assertApplicationScriptsAllowed(`<script>${script}</script>`, "script-src 'unsafe-inline'"),
    ).toThrow("missing");
  });
});
