// @vitest-environment node

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { installFakeAddonDom } from "@/test/fake-addon-dom";
import {
  clearAddonStyles,
  createCssModuleSource,
  installAddonCssFiles,
  installAddonStyle,
  isCssFile,
  resolveAddonCssAssetUrls,
} from "./addon-sandbox-styles";

function addonStyleElements() {
  return Array.from(
    document.head.querySelectorAll<HTMLStyleElement>("style[data-wealthfolio-addon-style]"),
  );
}

afterEach(() => {
  clearAddonStyles();
});

describe("addon sandbox styles", () => {
  beforeEach(() => {
    installFakeAddonDom();
  });

  it("detects css assets case-insensitively", () => {
    expect(isCssFile("dist/style.css")).toBe(true);
    expect(isCssFile("dist/theme.CSS")).toBe(true);
    expect(isCssFile("dist/addon.js")).toBe(false);
  });

  it("injects extracted addon css files and ignores non-css files", async () => {
    await installAddonCssFiles([
      { content: "export default {}", isMain: true, name: "dist/addon.js" },
      { content: ".addon-card { color: red; }", name: "dist/style.css" },
    ]);

    const styles = addonStyleElements();
    expect(styles).toHaveLength(1);
    expect(styles[0]?.getAttribute("data-wealthfolio-addon-style")).toBe("dist/style.css");
    expect(styles[0]?.textContent).toContain(".addon-card");
  });

  it("upserts css imports without duplicating style tags", () => {
    installAddonStyle("dist/style.css", ".addon-card { color: red; }");
    installAddonStyle("dist/style.css", ".addon-card { color: green; }");

    const styles = addonStyleElements();
    expect(styles).toHaveLength(1);
    expect(styles[0]?.textContent).toContain("green");
  });

  it("creates a synchronous module export for native css imports", () => {
    const source = createCssModuleSource(".addon-card { color: red; }");

    expect(source).not.toContain("await");
    expect(source).not.toContain("__wealthfolioInstallAddonStyle");
    expect(source).toContain("export default css");
  });

  it("resolves relative and root packaged URLs while preserving local URLs", async () => {
    const getAssetUrl = vi.fn((path: string) => Promise.resolve(`blob:${path}`));
    const css = await resolveAddonCssAssetUrls(
      "dist/styles/addon.css",
      [
        '.logo { background: url("../assets/logo.png#mark"); }',
        ".font { src: URL(/assets/font.woff2); }",
        '.inline { background: url("data:image/png;base64,AA=="); }',
        '.local { background: url("blob:existing-asset"); }',
        '/* url("../assets/ignored.png") */',
      ].join("\n"),
      getAssetUrl,
    );

    expect(getAssetUrl).toHaveBeenNthCalledWith(1, "dist/assets/logo.png");
    expect(getAssetUrl).toHaveBeenNthCalledWith(2, "assets/font.woff2");
    expect(css).toContain('url("blob:dist/assets/logo.png#mark")');
    expect(css).toContain('url("blob:assets/font.woff2")');
    expect(css).toContain('url("data:image/png;base64,AA==")');
    expect(css).toContain('url("blob:existing-asset")');
    expect(css).toContain('/* url("../assets/ignored.png") */');
  });

  it("rejects remote CSS URLs", async () => {
    await expect(
      resolveAddonCssAssetUrls(
        "dist/addon.css",
        '.remote { background: url("https://example.com/image.png"); }',
        vi.fn(),
      ),
    ).rejects.toThrow("is not a packaged addon asset");
  });

  it("drops cache-busting queries from Blob URLs while preserving fragments", async () => {
    const getAssetUrl = vi.fn().mockResolvedValue("blob:packaged-font");
    const css = await resolveAddonCssAssetUrls(
      "dist/addon.css",
      '.font { src: url("./assets/font.woff2?v=4.7.0#face"); }',
      getAssetUrl,
    );

    expect(getAssetUrl).toHaveBeenCalledWith("dist/assets/font.woff2");
    expect(css).toContain('url("blob:packaged-font#face")');
    expect(css).not.toContain("?v=4.7.0");
  });

  it("decodes escaped quoted and unquoted packaged URLs", async () => {
    const getAssetUrl = vi.fn((path: string) => Promise.resolve(`blob:${path}`));
    const css = await resolveAddonCssAssetUrls(
      "dist/styles/addon.css",
      [
        String.raw`.quoted { background: u\72l("../assets/company\ logo.png"); }`,
        String.raw`.unquoted { background: url(../assets/company\ logo.png); }`,
      ].join("\n"),
      getAssetUrl,
    );

    expect(getAssetUrl).toHaveBeenNthCalledWith(1, "dist/assets/company logo.png");
    expect(getAssetUrl).toHaveBeenNthCalledWith(2, "dist/assets/company logo.png");
    expect(css.match(/url\("blob:dist\/assets\/company logo\.png"\)/g)).toHaveLength(2);
  });

  it("rejects escaped remote schemes and import keywords", async () => {
    await expect(
      resolveAddonCssAssetUrls(
        "dist/addon.css",
        String.raw`.remote { background: url("https\3a //example.com/image.png"); }`,
        vi.fn(),
      ),
    ).rejects.toThrow("is not a packaged addon asset");

    await expect(
      resolveAddonCssAssetUrls("dist/addon.css", String.raw`@\69mport "./theme.css";`, vi.fn()),
    ).rejects.toThrow("@import rules are not supported");
  });

  it("leaves malformed CSS URL tokens unchanged", async () => {
    const css = '.broken { background: url("./assets/logo.png"; }';
    await expect(resolveAddonCssAssetUrls("dist/addon.css", css, vi.fn())).resolves.toBe(css);
  });

  it("rejects CSS imports that would create a child-frame request", async () => {
    await expect(
      resolveAddonCssAssetUrls(
        "dist/addon.css",
        '@import "./theme.css"; .card { color: red; }',
        vi.fn(),
      ),
    ).rejects.toThrow("@import rules are not supported");
  });
});
