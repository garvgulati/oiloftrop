# Oiloftrop

Oiloftrop is a self-hosted portfolio tracker and stock research app based on
Wealthfolio. This first source release adds on-demand India and US stock
research, interactive charts, and a personal web mode for reviewing and saving
dated holdings snapshots.

The personal web trial runs on a Windows host, with a browser or iPhone Home
Screen shortcut connecting to the same instance over private HTTPS. Holdings
are entered manually. Research availability and quote freshness depend on the
market and provider. Portfolio-wide advice and a packaged public installer are
still in development.

## Run locally

Install Node.js 24, pnpm 10.33.4 and the Rust toolchain specified in
`rust-toolchain.toml`. Copy `.env.web.example` to `.env.web`, generate a local
`WF_SECRET_KEY` as described in the example, and keep the default loopback bind
address for development.

```sh
pnpm install --frozen-lockfile
pnpm dev:web
```

Open `http://localhost:1420` for the local development app. On-demand provider
requests can be enabled with `WF_RESEARCH_PUBLIC_REQUESTS_ENABLED=true`; some
providers require their own credentials. Keep those in the local configuration
or app credential store. See [upstream build instructions](README.upstream.md)
for platform prerequisites and [browser tests](e2e/README.md) for a disposable
encrypted personal instance with synthetic holdings.

## Source and license

Based on [Wealthfolio 3.10.0](https://github.com/wealthfolio/wealthfolio/tree/f9643209e7cafcaf133d5a8fbe1ef2fa4fd7fc04).
Upstream notices and the [GNU AGPL v3 license](LICENSE) are retained. The first
commit containing the app imports existing work. Test fixtures are synthetic.
Oiloftrop is an independent project. Wealthfolio is a trademark of Teymz Inc.

The 14 November 2026 source import includes Oiloftrop's branding, stock research
and reviewed holdings changes. The experimental Windows calculator and its
separate build dependencies are outside this release.
