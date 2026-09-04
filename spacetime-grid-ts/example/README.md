# Grid tactics example

This example is a turn-based hex-grid tactics game built with
[`@spacetimedb/grid`](../). The Grid submodule owns grids, cell
state, and entity positions; the host module owns matches, participants, unit
statistics, turns, and combat rules.

## What this demonstrates

- Mounting the Grid and Auth submodules in one host module.
- Authenticated match membership and caller-scoped subscriptions.
- Hex-grid pathfinding with `computePath`.
- Movement and attack ranges with `cellsInRange`.
- Layering application rules over submodule-owned spatial state.
- Human-versus-human matchmaking and a solo match against the built-in Xeno
  Garrison actor.

## Prerequisites

- Node.js 20 or later and pnpm 10.
- The released SpacetimeDB 2.8 CLI.
- A local SpacetimeDB server registered as `local`.
- A logged-in CLI identity. A fresh publish seeds it as the initial auth
  administrator.

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
npm install @spacetimedb/grid spacetimedb@^2.8.3
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

The development server calls `set_auth_config` automatically on startup as the
logged-in CLI identity. Restart it after changing auth or OAuth values.

## Gameplay and authority

1. A signed-in user creates a human or solo match.
2. Participants and initial units are created by module reducers.
3. The active player selects a unit and requests a legal move or attack.
4. The host module checks membership, turn ownership, path/range, occupancy, and
   unit state before changing Grid-owned positions or combat state.
5. Subscriptions update each participant's UI.

The browser may calculate highlights for responsiveness, but reducer validation is
authoritative. A custom client must not be able to move an opponent's unit, cross
blocked cells, exceed movement range, attack outside range, or act out of turn.

## Architecture and visibility

```text
Browser -> /auth/* proxy -> Auth submodule HTTP handlers
Browser -> linked SpacetimeDB connection
        -> my_matches / my_match_participants
        -> match-scoped my_player_units / my_grid_entities / my_cell_states

Host match rules -> Grid submodule tables and helpers
```

The browser first subscribes to caller-scoped match views. It creates a second,
match-filtered subscription only for the selected match. Public catalogs and the
open-match lobby are shared; private match state is restricted by
the linked authenticated user and participation checks.

## Security and deployment boundaries

- Match reducers derive the acting user from the linked auth session and never
  accept a browser-provided owner as authority.
- A fresh publish seeds only the publisher as auth administrator.
- Passwords, OAuth secrets, signing keys, cookies, `.env`, and development tokens
  must not be committed or logged.
- Solo actors are server-owned game actors, not privileged browser identities.
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
3. Join a human match with a second account and verify realtime state in both
   browsers.
4. Attempt out-of-turn, out-of-range, blocked, occupied, and opponent-unit actions
   and confirm each rejection leaves state unchanged.
5. Confirm a third account cannot subscribe to or mutate a private match.

## Troubleshooting

- **The server exits at startup:** verify the database exists and the CLI identity
  is its owner or an auth administrator.
- **No matches appear after login:** check that `link_connection` succeeded before
  the caller-scoped subscriptions were created.
- **OAuth redirects incorrectly:** make `AUTH_ISSUER_URL` match the exact origin
  registered with the provider.
- **The app and publish target disagree:** ensure all STDB endpoints refer to the
  same server registered as `local`.

## Important files

- `spacetimedb/src/index.ts` - submodule mounts, auth integration, match schema,
  scoped views, and game rules.
- `src/app.ts` - auth/session linking, subscriptions, and interaction bridge.
- `server.ts` - auth bootstrap, static serving, and same-origin proxy.
- `public/index.html` - tactics interface.
- `public/ui.js` - game rendering and interaction handling.
- `public/styles.css` - tactics presentation.
