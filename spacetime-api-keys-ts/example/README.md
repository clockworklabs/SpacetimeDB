# Colony API-key example

Colony is a shared map editor built with
[`@spacetimedb/api-keys`](../). Each SpacetimeDB identity owns a colony and
can issue scoped bearer links that allow another browser to view or modify it.

## What this demonstrates

- Mounting the API Keys component under the `apiKeys` namespace.
- Creating, rotating, validating, and revoking scoped bearer keys.
- Validating keys in native SpacetimeDB HTTP handlers.
- Composing API Keys with the Grid and Presence components.
- Giving owners native reducer access while routing key holders through HTTP.
- Recording allowed and rejected holder actions in an audit-style world event log.
- Returning a raw key only at creation or rotation time.

## Prerequisites

- Node.js 20 or later and pnpm 10.
- The released SpacetimeDB 2.8 CLI.
- A local SpacetimeDB server registered as `local`.
- A logged-in CLI identity for publishing the example.

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

From `spacetime-api-keys-ts/example`:

```powershell
pnpm install
pnpm --dir spacetimedb install
node -e "require('node:fs').copyFileSync('.env.example', '.env')"
pnpm run build:module:fresh
pnpm run dev
```

Open <http://127.0.0.1:8798>. Create a share link, copy it while it is visible,
and open it in a private/incognito window to test holder permissions.

`build:module:fresh` deletes and recreates only the local
`spacetime-api-keys-example` database. Use `pnpm run build:module` to preserve existing
local rows.

## Use in your project

This workspace tests the component source in this repository. Consumer applications install published releases:

```bash
npm install @spacetimedb/api-keys @spacetimedb/crypto spacetimedb@^2.8.3
```

Follow the package's
[integration guide](../README.md#integrate-into-an-application), then wrap key
verification in the host routes or procedures that define your scopes. The
colony, grid, and presence features are application-specific demonstration code.

## Configuration

| Variable        | Default                      | Purpose                              |
| --------------- | ---------------------------- | ------------------------------------ |
| `HOST`          | `127.0.0.1`                  | Development web-server bind address. |
| `PORT`          | `8798`                       | Development web-server port.         |
| `STDB_URI`      | `ws://127.0.0.1:3000`        | Browser WebSocket endpoint.          |
| `STDB_HTTP`     | `http://127.0.0.1:3000`      | Upstream module HTTP endpoint.       |
| `STDB_DATABASE` | `spacetime-api-keys-example` | Published database name.             |

The publish scripts target the SpacetimeDB server registered as `local`. If that
registration resolves to a different endpoint than `STDB_URI` and `STDB_HTTP`,
the browser and proxy may use a different database from the published module.

## Roles and scopes

The example combines four scopes into a small set of roles:

| Scope              | Allows                       |
| ------------------ | ---------------------------- |
| `colony:view`      | Load the colony snapshot.    |
| `colony:terraform` | Change cell terrain.         |
| `colony:build`     | Place and remove structures. |
| `colony:plant`     | Place natural objects.       |

The owner may reset the colony, clear its event log, and manage access keys. A key
holder can perform only the actions represented by the key's current scopes. Key
validation also checks status and expiry. Rotation and revocation preserve the
share URL format.

## HTTP routes

The module exposes these native routes:

```text
GET  /api/colony/snapshot
POST /api/colony/terraform
POST /api/colony/build
POST /api/colony/unbuild
POST /api/colony/plant
POST /api/colony/clear
```

Holder requests send the raw key as `Authorization: Bearer <key>`. The Node server
forwards `/api/colony/*` to the module's HTTP router. Owner actions use native
reducers authenticated by the owner's SpacetimeDB identity.

## Key handling

- The module stores a hash and safe key metadata. Raw keys are recoverable only
  from creation and rotation responses.
- Creation and rotation return the raw key once. The owner UI displays a copyable
  link for that response and keeps the raw secret out of persistent browser storage.
- Existing keys can be rotated or revoked, but their original link cannot be
  reconstructed. This is intentional.
- Share links place the key in the URL fragment (`#key=...`), which browsers do
  not include in the initial HTTP request. The browser reads the fragment and sends
  the key only in the authorization header for colony API calls.
- Anyone with a share link has its permissions until the key expires, is rotated,
  or is revoked. Treat the link as a secret.

## Architecture and visibility

```text
Owner browser -> SpacetimeDB reducers/procedures -> colony state + key management

Holder browser -> Node /api/colony proxy -> module HTTP handler
                                      -> API-key validation + scope check
                                      -> colony mutation

Both browsers -> SpacetimeDB subscriptions -> realtime colony and presence state
```

Colony world data is readable by colony identifier in this demonstration; write
authority is the behavior under test. The `my_access_keys` view is owner-scoped
and contains metadata only. Do not copy this public-read model into an application
where the resource itself must be confidential.

## Security and deployment boundaries

- Never log the `Authorization` header, raw creation response, pasted key, or full
  share URL.
- Prefer short expirations and narrow scopes for real integrations.
- Treat XSS prevention as part of key security because holder credentials exist in
  browser memory and the URL fragment while the page is open.
- The world-event rows provide UI feedback. Compliance auditing requires a
  tamper-resistant external trail.
- The included Express proxy is for local development. Production needs TLS,
  explicit network binding, request-size limits, origin policy, structured secret
  redaction, and process supervision.

## Build and verification

```powershell
pnpm --dir spacetimedb run build
pnpm run build
pnpm exec tsc -p tsconfig.json
```

For a release smoke test:

1. Create one key for each role and capture each link when shown.
2. Confirm each holder can load the snapshot and perform only its allowed actions.
3. Confirm a disallowed action returns an authorization failure, records a
   rejected world event, and leaves the grid unchanged.
4. Rotate a key and verify the previous link is rejected and the replacement is accepted.
5. Revoke the new key and verify subsequent snapshot and mutation requests fail.
6. Reload the owner and confirm no raw key can be recovered or copied from the key
   list.

## Troubleshooting

- **The holder opens the owner's colony instead:** use the complete `#key=...`
  link and verify no extension or redirect strips the URL fragment.
- **Every holder action is rejected:** inspect the key's scopes and status, then
  confirm the proxy and browser target the same published database.
- **Realtime state is stale:** confirm the WebSocket endpoint in
  `STDB_URI` is reachable independently of the HTTP proxy.
- **A browser identity fails after a fresh publish:** reload once so the
  client can discard the rejected development token.

## Important files

- `spacetimedb/src/index.ts` - colony schema, component mounts, views, reducers,
  and HTTP handlers.
- `server.ts` - static server and colony-route proxy.
- `src/app.ts` - owner/holder modes, key handling, subscriptions, and UI logic.
- `public/index.html` - colony interface.
- `public/styles.css` - colony presentation.
