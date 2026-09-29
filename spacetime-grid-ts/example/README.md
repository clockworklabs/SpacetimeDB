# Grid tactics example

This example is a turn-based hex-grid tactics game built with
[`@spacetimedb/grid`](../). The Grid submodule owns grids, cell
state, and entity positions; the host module owns matches, participants, unit
statistics, turns, and combat rules.

## What this demonstrates

- Mounting the Grid and Auth submodules in one host module.
- Authenticated match membership and caller-scoped subscriptions.
- Hex-grid pathfinding with `computePath`.
- Movement ranges with `cellsInRange`; attack range uses the hex `distance`
  helper.
- Layering application rules over submodule-owned spatial state.
- Human-versus-human matchmaking and a solo match against the built-in Xeno
  Garrison actor, whose turns run in a scheduled reducer.
- Calling Grid helpers from reducers (`ctx.as.grid`) and from procedures that
  return data to the client (`tx.as.grid`).

## Prerequisites

- Node.js 20 or later and pnpm 10.
- A SpacetimeDB CLI and server built from this checkout, with the CLI available as `spacetime`.
- A local SpacetimeDB server registered as `local`.
- A logged-in CLI identity. A fresh publish seeds it as the initial auth
  administrator.

This example uses the workspace SDK. Keep the matching local server running in
a separate terminal:

```powershell
spacetime start
```

```powershell
spacetime server ping local
spacetime login show
```

## Quick start

From `spacetime-grid-ts/example`:

```powershell
pnpm install
pnpm --dir spacetimedb install
node -e "require('node:fs').copyFileSync('.env.example', '.env')"
pnpm run build:module:fresh
pnpm run dev
```

Open <http://localhost:8793>, create an account, and deploy a solo match. For the
human-versus-human flow, open a private/incognito window, create a second account,
and join the open match.

`build:module:fresh` deletes and recreates only the local `spacetime-grid-example`
database. Use `pnpm run build:module` when existing matches must be preserved.

## Use in your project

This workspace tests the submodule source in this repository. Consumer applications install published releases:

```bash
npm install @spacetimedb/grid spacetimedb
```

Follow the package's
[integration guide](../README.md#integrate-into-an-application). Add Auth or
Rate Limit only if your application needs them. The match, account, and tactics
rules are host-owned example code.

## Configuration

| Variable                            | Default                  | Purpose                                                                  |
| ----------------------------------- | ------------------------ | ------------------------------------------------------------------------ |
| `HOST`                              | `127.0.0.1`              | Development web-server bind address.                                     |
| `PORT`                              | `8793`                   | Development web-server port.                                             |
| `STDB_URI`                          | `ws://127.0.0.1:3000`    | Browser WebSocket endpoint.                                              |
| `STDB_HTTP`                         | `http://127.0.0.1:3000`  | HTTP endpoint used by the auth proxy.                                    |
| `STDB_SERVER`                       | `STDB_HTTP`              | CLI target used during startup auth configuration.                       |
| `SPACETIMEDB_DB_NAME`               | `spacetime-grid-example` | Published database name.                                                 |
| `AUTH_ISSUER_URL` / `AUTH_BASE_URL` | `http://localhost:8793`  | JWT issuer and browser-visible auth origin.                              |
| `AUTH_COOKIE_NAME`                  | `stdb_auth`              | Session-cookie name.                                                     |
| `AUTH_SESSION_TTL_SECONDS`          | `604800`                 | Session lifetime in seconds.                                             |
| OAuth client variables              | empty                    | Enables Google or GitHub when both values for that provider are present. |

The development server calls `auth.set_auth_config` automatically on startup as
the logged-in CLI identity. Without `AUTH_ES256_PRIVATE_KEY_PEM` the database
keeps its stored signing key, and a fresh database gets a newly generated one.
Restart it after changing auth or OAuth values.

## Gameplay and authority

1. A signed-in user creates a human or solo match with the `createMatch`
   procedure, which returns the match id. A second user joins a human match
   with the `joinMatch` reducer.
2. The active player selects a unit. The `getCellsInRange` procedure returns
   its movement range and only answers participants of the grid's match.
3. The `moveUnit` procedure validates and applies a move in one transaction and
   returns the A\* path for animation. `attackUnit` and `endTurn` are reducers.
4. The host module checks membership, turn ownership, path/range, occupancy, and
   unit state before changing Grid-owned positions or combat state.
5. When the solo opponent's seat becomes active, `endTurn` schedules
   `aiTakeTurn`. It plays the turn, records the moves and attacks in
   `ai_turn_log` for the client to animate, and passes the turn back, so the
   match continues even if the browser reloads.
6. Subscriptions update each participant's UI.

Match lifecycle:

- A user can be seated in at most 5 Waiting or Active matches.
- `leaveMatch` cancels (deletes) a Waiting match or forfeits an Active one,
  which ends it with the other seat as the winner.
- A Waiting match that nobody joins within 30 minutes is deleted by the
  scheduled `expireWaitingMatch` reducer.

The browser may calculate highlights for responsiveness, but server validation
is authoritative. A custom client must not be able to move an opponent's unit,
cross blocked cells, exceed movement range, attack outside range, act out of
turn, or trigger the solo opponent's turn.

## Architecture and visibility

```text
Browser -> /auth/* proxy -> Auth submodule HTTP handlers
Browser -> linked SpacetimeDB connection
        -> my_matches / my_match_participants
        -> match-scoped my_player_units / my_grid_entities / my_cell_states /
           my_ai_turns

Host match rules -> Grid submodule tables and helpers
```

The browser first subscribes to caller-scoped match views. It creates a second,
match-filtered subscription only for the selected match. Public catalogs and the
open-match lobby are shared; private match state is restricted by
the linked authenticated user and participation checks.

## Security and deployment boundaries

- Match reducers and procedures derive the acting user from the linked auth
  session and never accept a browser-provided owner as authority.
- A fresh publish seeds only the publisher as auth administrator.
- Passwords, OAuth secrets, signing keys, cookies, `.env`, and development tokens
  must not be committed or logged.
- Solo actors are server-owned game actors, not privileged browser identities.
  Scheduled reducers are private to the module, so clients cannot call
  `aiTakeTurn` or `expireWaitingMatch`.
- The included Express process is for local development. Production needs TLS,
  explicit binding, origin/host policy, durable signing keys, and supervision.

## Build and verification

```powershell
pnpm --dir spacetimedb run build
pnpm run build
pnpm exec tsc -p tsconfig.json
```

For a release smoke test:

1. Complete signup, reload-based session refresh, and logout.
2. Play a solo match through movement, attack, end-turn, and terminal match state.
   Reload during the opponent's turn and confirm the match continues.
3. Join a human match with a second account and verify realtime state in both
   browsers.
4. Attempt out-of-turn, out-of-range, blocked, occupied, and opponent-unit actions
   and confirm each rejection leaves state unchanged.
5. Confirm a third account cannot subscribe to, query, or mutate a private match.
6. Cancel a Waiting match and forfeit an Active one.

## Troubleshooting

- **The server exits at startup:** verify the database exists and the CLI identity
  is its owner or an auth administrator.
- **Sign-in fails with "Could not link this connection to your session":** the
  `auth.linkConnection` reducer rejected the session token. Sign in again, and
  check that the server's auth configuration matches the running database.
- **OAuth redirects incorrectly:** make `AUTH_ISSUER_URL` match the exact origin
  registered with the provider.
- **The app and publish target disagree:** ensure all STDB endpoints refer to the
  same server registered as `local`.

## Important files

- `spacetimedb/src/schema.ts` - submodule mounts, match tables, and schedule
  tables.
- `spacetimedb/src/index.ts` - auth integration, match lifecycle, game rules,
  and the solo opponent.
- `spacetimedb/src/views.ts` - caller-scoped match views and the open-match
  lobby.
- `src/app.ts` - auth/session linking, subscriptions, and interaction bridge.
- `src/module_bindings/` - generated client bindings.
- `server.ts` - auth bootstrap, static serving, and same-origin proxy.
- `public/index.html` - tactics interface.
- `public/ui.js` - game rendering and interaction handling.
- `public/hex-geometry.js` - hex layout math shared by the UI and
  `scripts/test-hex-geometry.mjs`.
- `public/styles.css` - tactics presentation.
