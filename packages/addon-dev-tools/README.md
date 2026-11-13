# @wealthfolio/addon-dev-tools

Development tools for Wealthfolio addons including hot reload server and CLI.

## Installation

```bash
npm install -g @wealthfolio/addon-dev-tools
```

> **Deprecation note:** the CLI command is now `wealthfolio-addon`. The old
> `wealthfolio` alias still works but will be removed in a future release — the
> `wealthfolio` name is reserved for the upcoming native Wealthfolio CLI.

## CLI Commands

### Create New Addon

```bash
wealthfolio-addon create my-awesome-addon
```

### Start Development Server

```bash
# In your addon directory
wealthfolio-addon dev
```

### Build Addon

```bash
wealthfolio-addon build
```

### Package for Distribution

```bash
wealthfolio-addon package
```

### Test Setup

```bash
wealthfolio-addon test
```

## Development Server

> **Version compatibility:** Wealthfolio 3.7 requires
> `@wealthfolio/addon-dev-tools` 3.7 or newer. If the app reports that the
> server does not support v3.7 runtime packages, update this package and restart
> the development server.

The development server provides:

- Hot reload functionality
- File watching
- Auto-building
- Health check endpoints

### API Endpoints

- `GET /health` - Health check
- `GET /status` - Build state and published runtime-package generation
- `GET /manifest.json` - Addon manifest
- `GET /addon.js` - Built addon code
- `GET /runtime-package` - One coherent manifest, code, and asset-metadata
  snapshot
- `GET /runtime-files` - Built JavaScript and CSS modules
- `GET /runtime-assets` - Packaged asset metadata
- `GET /runtime-assets/:assetId?generation=<id>` - One asset from a published
  generation
- `GET /files` - List of built files
- `GET /test` - Test connectivity

The host loads `/runtime-package` first, then requests asset bytes from the same
generation. The server retains the four most recent immutable generations so a
reload cannot mix new metadata with old bytes. A generation older than that
window is intentionally unavailable and the host must load the current package
snapshot again. `/manifest.json` and `/addon.js` remain diagnostic/legacy
endpoints; Wealthfolio 3.7 live loading does not assemble a runtime from them.

Files below `assets/**` and non-code files below `dist/assets/**` are published
as private asset metadata and lazy byte responses. JavaScript and CSS remain in
`/runtime-files`. The same 256-entry, 5 MiB-per-file, and 25 MiB-package limits
used during installation are enforced during development.

Generated projects pin `build.target` to Chrome/Edge 107, Firefox 104, and
Safari 16, matching Wealthfolio 3.7. Keep that explicit target when customizing
Vite so a future Vite default cannot silently raise the addon's browser floor.
The sandbox supports packaged images, fonts, media, CSS, and WebAssembly, but
does not allow Worker/service-worker entry points, popups, direct network
requests, or remote CSS imports.

## Usage in Addon Projects

Add to your addon's `package.json`:

```json
{
  "scripts": {
    "dev:server": "wealthfolio-addon dev"
  },
  "devDependencies": {
    "@wealthfolio/addon-dev-tools": "^3.7.0"
  }
}
```

## Architecture

This package is separate from `@wealthfolio/addon-sdk` to:

- Keep the SDK lightweight for production
- Avoid unnecessary dependencies in addon bundles
- Provide optional development tooling

## License

MIT
