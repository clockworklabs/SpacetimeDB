# Powerhouse rate-limit example

Powerhouse is an arcade-style reactor game built with
[`@spacetimedb/rate-limit`](../). The browser requests actions; SpacetimeDB
owns energy, heat, upgrades, events, and the fixed-window limiter buckets registered
under the `rateLimit` namespace.

## What this demonstrates

- Mounting the Rate Limit submodule in an application module.
- Deriving server-owned actor keys and fixed gameplay scopes.
- Enforcing limits from procedures with typed allow/deny results.
- Using independent buckets for taps, overcharge, upgrades, and repair.
- Showing caller-specific cooldown status through scoped views.
- Bounded scheduled cleanup and administrator-only maintenance controls.

## Prerequisites

- Node.js 20 or later and pnpm 10.
- The released SpacetimeDB 2.8 CLI.
- A local SpacetimeDB server registered as `local`.
- A logged-in CLI identity for publishing and optional admin grants.

Select the supported CLI release, then keep the local server running in a
separate terminal:

```powershell
spacetime version install 2.8.3
spacetime version use 2.8.3
spacetime start
```

```powershell
spacetime server ping local
spacetime login show
```

## Quick start

From `spacetime-rate-limit-ts/example`:

```powershell
pnpm install
pnpm --dir spacetimedb install
node -e "require('node:fs').copyFileSync('.env.example', '.env')"
pnpm run build:module:fresh
pnpm run dev
```

Open <http://127.0.0.1:8792>, start the reactor, and tap rapidly enough to fill
the tap bucket and heat meter.

`build:module:fresh` deletes and recreates only the local
`spacetime-rate-limit-example` database. Use `pnpm run build:module` to preserve current
players, upgrades, and limiter state.

## Use in your project

This workspace tests the submodule source in this repository. Consumer
applications install the published release:

```bash
npm install @spacetimedb/rate-limit spacetimedb@^2.8.3
```

Follow the package's
[integration guide](../README.md#integrate-into-an-application). Copy the
per-action policy and caller-status patterns; the reactor game, upgrades, and
heat model are application code.

## Configuration

| Variable              | Default                        | Purpose                              |
| --------------------- | ------------------------------ | ------------------------------------ |
| `HOST`                | `127.0.0.1`                    | Development web-server bind address. |
| `PORT`                | `8792`                         | Development web-server port.         |
| `STDB_URI`            | `ws://127.0.0.1:3000`          | Browser WebSocket endpoint.          |
| `SPACETIMEDB_DB_NAME` | `spacetime-rate-limit-example` | Published database name.             |

The Node process serves static files, `GET /api/health`, and browser-safe
`GET /api/config`. Gameplay calls go directly from the browser to SpacetimeDB.

## Limiting model

Each protected action calls
`rateLimit.consumeRateLimit(ctx.as.rateLimit, ...)` with a server-selected scope,
an actor key derived from `ctx.sender`, a limit, a window, and an optional cost.
The returned result includes remaining capacity, reset time, and retry delay.
The submodule `consume` procedure is reserved for administrators; normal gameplay
uses the lower-level helper inside the host procedure's transaction.

The submodule implements fixed-window limiting. Application heat and cooldown
mechanics are separate game rules layered over the rate limit, so a request may
be rejected by either system.

The primary UI subscribes to application-owned views such as `reactor_state`,
`reactor_limit_status`, and `reactor_shop`. Raw bucket and limiter event data is
reserved for administrators.

## Gameplay

- Reactor taps generate energy while consuming the `reactor.tap` limit.
- Heat cools according to server time; an overheated reactor rejects more taps
  until it recovers.
- Overcharge, repair, and shop installation use separate scopes and windows.
- Installed upgrades change server-owned capabilities and expose matching buttons.
- Recent events explain successful and rejected actions.

The browser interpolates timers for presentation, but procedure responses and
subscribed server timestamps are authoritative.

## Administration

A fresh publish seeds the publisher as the initial Rate Limit submodule
administrator. The debug drawer remains empty and maintenance calls fail for an
ordinary browser identity. To exercise those controls locally, grant the browser
identity from the logged-in owner identity:

```powershell
spacetime call --server local spacetime-rate-limit-example rateLimit.add_rate_limit_admin 0x<BROWSER_IDENTITY_HEX>
```

Admin resets and sweeps are bounded. Do not turn an unbounded delete into an
operator convenience endpoint.

## Security and deployment boundaries

- Actor keys are derived from trusted module context, not accepted from the
  browser. A client-selected actor key would allow trivial limit evasion.
- A limit must guard the authoritative operation in the same server-side flow;
  disabling a browser button is only presentation.
- Combine rate limiting with authentication, authorization, quotas, billing
  controls, and network-level defenses.
- Stored browser tokens are development credentials and must not be logged or
  committed.
- The admin event view returns at most 1,000 raw limiter events.
- The included Express process is a local static server. Production needs TLS,
  explicit binding, origin policy, and supervision.

## Build and verification

```powershell
pnpm --dir spacetimedb run build
pnpm run build
pnpm exec tsc -p tsconfig.json
```

For a release smoke test:

1. Confirm normal taps consume capacity and expose decreasing remaining counts.
2. Exceed each action limit and verify the denied operation makes no game-state
   change.
3. Wait through a reset boundary and confirm the next action is accepted.
4. Verify two browser identities have independent player and bucket state.
5. Confirm a non-admin cannot view raw events, reset demo state, change config, or
   trigger privileged maintenance.
6. Grant an admin identity and verify bounded sweep/reset behavior.

## Troubleshooting

- **Actions fail immediately:** inspect both limiter status and reactor heat; they
  are independent rejection paths.
- **Debug data is empty:** grant the connected browser identity Rate Limit submodule
  administrator access.
- **State is stale:** confirm `STDB_URI` targets the database published by
  the `local` server registration.
- **An identity fails after reset:** reload once so the client can replace a
  rejected development token.

## Important files

- `spacetimedb/src/index.ts` - submodule registration, reactor rules, scoped views, and
  bounded maintenance operations.
- `src/app.ts` - connection, procedures, subscriptions, and UI bridge.
- `server.ts` - static development server and browser-safe configuration.
- `public/index.html` - Powerhouse interface.
- `public/ui.js` - reactor rendering and interaction handling.
- `public/styles.css` - Powerhouse presentation.
