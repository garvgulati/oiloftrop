# Addon migration guide: v3.6 to v3.7

Wealthfolio 3.7 keeps the documented v3.6 sandbox API compatible and adds a
private packaged-asset registry. Existing v3.6 addons that only contain their
JavaScript/CSS bundle can continue to install, load, disable, and uninstall.

## Compatibility matrix

| Addon or tool                      | Wealthfolio 3.7                | Earlier Wealthfolio versions                          |
| ---------------------------------- | ------------------------------ | ----------------------------------------------------- |
| Existing v3.6 addon                | Supported                      | Supported according to its existing minimum version   |
| v3.7 addon using `ctx.assets`      | Supported                      | Not supported; set `minWealthfolioVersion` to `3.7.0` |
| `@wealthfolio/addon-dev-tools` 3.6 | Not supported for live loading | Supported by matching v3.6 hosts                      |
| `@wealthfolio/addon-dev-tools` 3.7 | Supported                      | Do not assume compatibility with older hosts          |

The manifest's `sdkVersion` describes the SDK used to build the addon. Runtime
compatibility is enforced with `minWealthfolioVersion`, so asset-using addons
must set both fields to `3.7.0` or newer.

## Runtime compatibility

Wealthfolio 3.7 pins both the host and addon build target instead of inheriting
Vite's changing default:

| Runtime                                 | Minimum supported version |
| --------------------------------------- | ------------------------- |
| Chrome, Edge, WebView2, Android WebView | 107                       |
| Firefox                                 | 104                       |
| Safari and WKWebView                    | 16                        |
| Wealthfolio for macOS                   | macOS 12                  |
| Wealthfolio for iPhone and iPad         | iOS/iPadOS 16             |

On macOS 12, apply current macOS and Safari updates so the system WKWebView
meets the Safari 16 floor. Linux and Android use the WebKitGTK and Android
System WebView installed on the device. Keep those components updated. Addon
Vite configurations should use the same explicit target:

```typescript
build: {
  target: ["chrome107", "edge107", "firefox104", "safari16"],
}
```

## Upgrade dependencies

```json
{
  "dependencies": {
    "@wealthfolio/addon-sdk": "^3.7.0",
    "@wealthfolio/ui": "^3.7.0"
  },
  "devDependencies": {
    "@wealthfolio/addon-dev-tools": "^3.7.0"
  }
}
```

Update the manifest when the addon adopts a v3.7 feature:

```json
{
  "sdkVersion": "3.7.0",
  "minWealthfolioVersion": "3.7.0"
}
```

The v3.7 host loads development addons from `/runtime-package`. A 404 or 405 for
that endpoint means the development server is too old; upgrade
`@wealthfolio/addon-dev-tools` and restart it.

## Package private assets

No `assets` manifest field or permission is required. Files are indexed
automatically from these package roots:

- `assets/**`
- `dist/assets/**`

JavaScript and CSS files in `dist/assets/` remain executable runtime files.
Source maps, `.gitkeep`, and `.DS_Store` are not exposed as assets. Asset roots
must be directories and symlinks are rejected.

```typescript
export async function enable(ctx: AddonContext) {
  const logoUrl = await ctx.assets.getUrl("assets/logo.png");
  const config = await ctx.assets.getBlob("assets/config.json");
  const knownAssets = ctx.assets.list();

  if (ctx.assets.has("assets/logo.png")) {
    ctx.ui.root.innerHTML = `<img alt="Addon logo" src="${logoUrl}">`;
  }

  console.log(knownAssets, await config.text());
}
```

`ctx.assets` is the addon's private package registry. It is unrelated to
`ctx.api.assets`, which reads and updates financial instruments in Wealthfolio.
The registry exposes logical paths and metadata, never host filesystem paths or
opaque asset identifiers.

### CSS and JavaScript behavior

Local `url(...)` references in packaged CSS are rewritten to sandbox-local Blob
URLs. A stylesheet at `dist/addon.css` can therefore reference
`url("./assets/background.png")`. Root-relative paths resolve from the package
root. `data:` and `blob:` URLs are preserved.

Remote CSS URLs and `@import` are rejected when the addon loads. Bundle imported
CSS into the addon and package remote images, fonts, media, or Wasm instead.
JavaScript string URLs such as `<img src="assets/logo.png">` are not rewritten;
use `await ctx.assets.getUrl(...)` explicitly.

### Limits and lifecycle

- Maximum 256 package entries across runtime files and assets.
- Maximum 5 MiB per file and 25 MiB for the package.
- Asset bytes are loaded lazily and checked against their indexed size and
  content identity.
- `getUrl()` values are cached for the sandbox lifetime and automatically
  revoked when the addon reloads or stops. Do not persist them.
- Missing, changed, or invalid assets reject the returned promise. Handle errors
  where a missing optional asset has a useful fallback.

## Network and browser limitations

The addon iframe has an opaque origin. Direct `fetch`, browser storage,
top-level navigation, and filesystem paths are not addon APIs. Use
`ctx.api.network.request()` for manifest-approved HTTPS hosts. Its response body
is text, so binary remote resources should be packaged as private assets rather
than fetched at runtime.

The sandbox also blocks Web Workers and service workers, popups/new windows, and
remote CSS imports. A packaged JavaScript asset therefore cannot be used as a
Worker entry point. These restrictions are intentional and consistent across the
supported browser engines.

## Suggested verification

1. Build and inspect the ZIP to confirm the asset paths are below an indexed
   root.
2. Run the addon with `@wealthfolio/addon-dev-tools` 3.7 and exercise a reload.
3. Test CSS backgrounds, fonts, images, and Wasm in the sandbox, not only in a
   regular browser page.
4. Confirm disable/reload releases UI roots, listeners, and references to Blob
   URLs.
5. Before publishing a Wealthfolio release, smoke-test one addon on Windows
   WebView2, macOS WKWebView, iOS WKWebView (including an input with the
   keyboard open), Linux WebKitGTK, and Android WebView when addons are
   available there.
