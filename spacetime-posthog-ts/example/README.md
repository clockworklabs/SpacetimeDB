# Context Cafe

Context Cafe is a small robot café simulator that demonstrates the
`@spacetimedb/posthog/submodule`. SpacetimeDB owns the catalog, simulation,
per-browser café state, metrics, and analytics outbox. A dedicated local server
identity delivers queued events to PostHog; the browser never receives submodule
administrator privileges or the PostHog project key.

## What this demonstrates

- Mounting the PostHog submodule under the `posthog` namespace.
- Enqueuing analytics in deterministic reducers for delivery outside
  transactions.
- Delivering the submodule outbox from an authorized server connection.
- Caller-scoped café state and safe public aggregate delivery metrics.
- Editing prices and availability while watching simulated conversion change.
- Synchronizing a TypeScript-authored catalog from `catalog/catalog.ts`.

## Prerequisites

- Node.js 20 or later and pnpm 10.
- The released SpacetimeDB 2.8 CLI.
- A local SpacetimeDB server reachable as `local`.
- A logged-in CLI identity. The identity that publishes the fresh database becomes
  its initial submodule administrator.
- Optional: a PostHog project API key for real event delivery.

Select the supported CLI release, then keep the local server running in a
separate terminal:

```powershell
spacetime version install 2.8.3
spacetime version use 2.8.3
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
npm install @spacetimedb/posthog spacetimedb@^2.8.3
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
| `STDB_SERVER_TOKEN`       | generated locally           | Optional pre-provisioned server identity token.                          |
| `HOST`                    | `127.0.0.1`                 | Static-server bind address.                                              |
| `PORT`                    | `8796`                      | Static-server port.                                                      |

When `STDB_SERVER_TOKEN` is unset, the server stores its generated identity token in
the ignored `.stdb-server-token` file. On startup, the logged-in CLI publisher calls
`posthog.add_admin_identity` for that identity. This keeps the browser unprivileged
and preserves the delivery identity across restarts.

## Architecture

```text
Browser
  -> caller-scoped café reducers and views
  -> analytics events queued in the posthog submodule namespace

Authorized example server
  -> subscribes to the admin-scoped outbox view
  -> calls flush_analytics in bounded batches
  -> PostHog ingestion API
```

The public `cafe_analytics_summary` view exposes counts only. The detailed outbox
and delivery-log views return rows only to registered PostHog administrators.

The Node server exposes only:

| Route             | Purpose                                                    |
| ----------------- | ---------------------------------------------------------- |
| `GET /api/health` | Local health probe.                                        |
| `GET /api/config` | Browser-safe database and PostHog dashboard configuration. |

Submodule administrator grants are available only through module operations.

## Security and deployment boundaries

- `POSTHOG_PROJECT_API_KEY` is loaded by the server and written to the submodule's
  private configuration table through the authenticated CLI owner.
- `.stdb-server-token`, `.env`, and logs are ignored and must not be committed.
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
advance through the authorized server identity.

## Troubleshooting

- **Connection targets disagree:** `STDB_URI`, `STDB_HTTP`, and the server selected
  by the publish script must refer to the same SpacetimeDB instance.
- **Server identity cannot be authorized:** publish with the currently logged-in
  CLI identity, then restart. Remove `.stdb-server-token` only when
  replacing the local server identity.
- **Events stay queued:** verify `POSTHOG_PROJECT_API_KEY`, inspect server output,
  and confirm the PostHog host is reachable.
- **Stored browser identity is rejected after a reset:** reload once; the client
  discards the rejected browser token and obtains a fresh caller identity automatically.

## Important files

- `spacetimedb/src/index.ts`: host schema, scoped views, reducers, and PostHog
  delegation.
- `spacetimedb/src/economy.ts`: simulation tuning, capacity rules, pricing, and
  deterministic purchase behavior.
- `catalog/catalog.ts`: product, recipe, and scenario source data.
- `scripts/test-economy.ts`: focused tests for the simulator's economy rules.
- `server.ts`: safe startup configuration, server identity, and outbox delivery.
- `src/app.ts`: browser connection and café UI behavior.
- `public/index.html`: café interface structure.
- `public/styles.css`: café presentation.
