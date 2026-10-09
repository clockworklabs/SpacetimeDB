# Context Cafe PostHog example

Run a robot café, change drink prices, and watch sales. Add a PostHog key to
send the café's activity to your analytics project.

## Run it locally

Requires Node.js 20+, pnpm 10, and the SpacetimeDB CLI and server built from
this checkout.

Start SpacetimeDB in a separate terminal:

```bash
spacetime start
```

From `spacetime-posthog-ts/example`, copy [.env.example](./.env.example) to `.env`.

A PostHog key is optional. To send events, set `POSTHOG_PROJECT_API_KEY` in
`.env` and set `POSTHOG_HOST` to your project's ingestion address. Without a
key, you can still use the café.

Then publish the example and start its web server:

```bash
pnpm install
pnpm run build:module
pnpm run dev
```

Open <http://127.0.0.1:8796>.

## Try it

1. Click **Run** to start the café.
2. Watch customers arrive and sales change.
3. Click a drink to change its price or availability.
4. If you supplied a PostHog key, watch the **→ PostHog** counter and open
   your project to see the events.

Each browser identity has its own café. The delivery counter combines events
from all users of this example database.

## Configuration

`POSTHOG_HOST` defaults to `https://us.i.posthog.com`. Use the ingestion
address for your PostHog region. Restart the example server after changing
the key or host.

See [.env.example](./.env.example) for the database and web-server settings.

## Troubleshooting

- **Events do not arrive:** check the key and ingestion host, then restart.
  Events that already failed can be retried with
  `posthog.requeue_failed_events` as an administrator.
- **Startup is not authorized:** use the CLI account that published the database.
- **The app cannot connect:** make sure `STDB_URI` and `STDB_HTTP` point to
  the same SpacetimeDB server used by the publish command.

## Change the example

- [catalog/catalog.ts](./catalog/catalog.ts): drinks, recipes, and scenarios.
- [spacetimedb/src/economy.ts](./spacetimedb/src/economy.ts): café rules.
- [spacetimedb/src/index.ts](./spacetimedb/src/index.ts): sales and analytics events.
- [src/app.ts](./src/app.ts): café controls.

After changing server code, run `pnpm run build:module`. Restart
`pnpm run dev` after changing browser code or `.env`.

To start over, run `pnpm run build:module:fresh`. **This deletes all data in
the local `spacetime-posthog-example` database.**

To use the submodule in your own app, see the
[package integration guide](../README.md#integrate-into-an-application).
