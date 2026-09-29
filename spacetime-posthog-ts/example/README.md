# Context Cafe

Context Cafe is a small robot café simulator that demonstrates the
`@spacetimedb/posthog/submodule`. SpacetimeDB owns the catalog, simulation,
per-browser café state, metrics, and analytics outbox. The submodule's scheduled
flush delivers queued events to PostHog from inside the database; the browser
never receives submodule administrator privileges or the PostHog project key.

## What this demonstrates

- Mounting the PostHog submodule under the `posthog` namespace.
- Enqueuing analytics in deterministic reducers for delivery outside
  transactions.
- Scheduled outbox delivery started by `posthog.install` in the host `init`.
- Caller-scoped café state and safe public aggregate delivery metrics.
- Editing prices and availability while watching simulated conversion change.
- Synchronizing a TypeScript-authored catalog from `catalog/catalog.ts`.

## Prerequisites

- Node.js 20 or later and pnpm 10.
- A SpacetimeDB CLI and server built from this checkout, with the CLI available as `spacetime`.
- A local SpacetimeDB server reachable as `local`.
- A logged-in CLI identity. The identity that publishes the fresh database becomes
  its initial submodule administrator.
- Optional: a PostHog project API key for real event delivery.

This example uses the workspace SDK. Keep the matching local server running in
a separate terminal:

```powershell
spacetime start
```

Confirm the local server before continuing:

```powershell
spacetime server ping local
spacetime login show
```

## Quick start

From `spacetime-posthog-ts/example`:

```powershell
pnpm install
pnpm --dir spacetimedb install
node -e "require('node:fs').copyFileSync('.env.example', '.env')"
pnpm run build:module:fresh
pnpm run dev
```

Set `POSTHOG_PROJECT_API_KEY` in `.env` before starting if you want events delivered
to PostHog. Open <http://127.0.0.1:8796>, press **Run**, and watch the café and the
`→ PostHog` counter update.

`build:module:fresh` deletes and recreates only the local `spacetime-posthog-example`
database. Use `pnpm run build:module` when existing data must be preserved.

## Use in your project

This workspace tests the submodule source in this repository. Consumer
applications install the published release:

```bash
npm install @spacetimedb/posthog spacetimedb
```

Follow the package's
[integration guide](../README.md#integrate-into-an-application). Copy the
enqueue, delivery, and admin-observability boundaries; the cafe simulator and
its event catalog are demonstration code.

## Configuration

| Variable                  | Default                     | Purpose                                                                  |
| ------------------------- | --------------------------- | ------------------------------------------------------------------------ |
| `POSTHOG_PROJECT_API_KEY` | empty                       | Enables real PostHog delivery. Kept outside the browser.                 |
| `POSTHOG_HOST`            | `https://us.i.posthog.com`  | PostHog ingestion host.                                                  |
| `STDB_URI`                | `ws://127.0.0.1:3000`       | Browser and server WebSocket endpoint.                                   |
| `STDB_HTTP`               | `http://127.0.0.1:3000`     | CLI administration endpoint. Must address the same server as `STDB_URI`. |
| `SPACETIMEDB_DB_NAME`     | `spacetime-posthog-example` | Published database name.                                                 |
| `HOST`                    | `127.0.0.1`                 | Static-server bind address.                                              |
| `PORT`                    | `8796`                      | Static-server port.                                                      |

On startup, the server uses the logged-in CLI identity, which published the
database and is its initial submodule administrator, to call `sync_catalog` and
`posthog.set_posthog_config`. The browser stays unprivileged.

## Architecture

```text
Browser
  -> caller-scoped café reducers and views
  -> analytics events queued in the posthog submodule namespace

posthog.scheduled_flush (every 5 seconds, inside the database)
  -> delivers queued events in bounded batches
  -> PostHog ingestion API

Example server
  -> serves the UI
  -> syncs the catalog and PostHog config through the CLI identity
```

The public `cafe_analytics_summary` view exposes counts only. The submodule's
`posthog_outbox_admin` and `posthog_delivery_log_admin` views return rows only to
registered PostHog administrators.

The Node server exposes only:

| Route             | Purpose                                                    |
| ----------------- | ---------------------------------------------------------- |
| `GET /api/health` | Local health probe.                                        |
| `GET /api/config` | Browser-safe database and PostHog dashboard configuration. |

Submodule administrator grants are available only through module operations.

## Security and deployment boundaries

- `POSTHOG_PROJECT_API_KEY` is loaded by the server and written to the submodule's
  private configuration table through the authenticated CLI owner.
- `.env` and logs are ignored and must not be committed.
- `sync_catalog` requires a PostHog submodule administrator.
- `simulate_tick` ignores calls that arrive faster than one simulated tick per
  250 ms per caller, which bounds the events each caller queues.
- `init_session` deletes up to 10 sessions whose configuration has not changed
  for 7 days.
- The development server binds to loopback by default. Setting `HOST` to another
  address expands its network exposure.
- The example server is scoped to local development. Production deployments
  should provision service identities and
  lifecycle supervision explicitly.

## Verification

```powershell
pnpm --dir spacetimedb run build
pnpm run build
pnpm exec tsc -p tsconfig.json
```

For a complete local smoke test, fresh-publish the database, start the server, load
the UI, press **Run**, and confirm that ticks, queued activity, and the PostHog count
advance.

## Troubleshooting

- **Connection targets disagree:** `STDB_URI`, `STDB_HTTP`, and the server selected
  by the publish script must refer to the same SpacetimeDB instance.
- **Catalog sync or configuration is not authorized:** publish with the currently
  logged-in CLI identity, then restart.
- **Events stay queued or fail:** verify `POSTHOG_PROJECT_API_KEY`, inspect the
  `posthog_delivery_log_admin` view, and confirm the PostHog host is reachable.
  After fixing the key, call `posthog.requeue_failed_events` to retry events that
  failed.
- **Stored browser identity is rejected after a reset:** reload once; the client
  discards the rejected browser token and obtains a fresh caller identity automatically.

## Important files

- `spacetimedb/src/index.ts`: host schema, scoped views, reducers, and PostHog
  delegation.
- `spacetimedb/src/economy.ts`: simulation tuning, capacity rules, pricing, and
  deterministic purchase behavior.
- `catalog/catalog.ts`: product, recipe, and scenario source data.
- `scripts/test-economy.ts`: focused tests for the simulator's economy rules.
- `server.ts`: static hosting, catalog sync, and PostHog configuration.
- `src/app.ts`: browser connection and café UI behavior.
- `public/index.html`: café interface structure.
- `public/styles.css`: café presentation.
