# Dispatch

Dispatch demonstrates a host module that mounts
`@spacetimedb/resend/submodule`: compose an email, send it through Resend, and
watch verified delivery events stream back through SpacetimeDB. Provider credentials
and component administration remain outside the browser.

## What this demonstrates

- Mounting the Resend component under the `resend` namespace.
- Calling a host `send_dispatch` procedure backed by a private API key.
- Showing caller-scoped email and delivery-event views in real time.
- Receiving Resend webhooks through a native SpacetimeDB HTTP route.
- Verifying Svix signatures inside the module with `@spacetimedb/crypto`.
- Persisting and explicitly authorizing a dedicated local server identity.

## Prerequisites

- Node.js 20 or later and pnpm 10.
- The released SpacetimeDB 2.8 CLI.
- A local SpacetimeDB server reachable as `local`.
- A logged-in CLI identity that publishes the database.
- A Resend API key.
- A Resend webhook signing secret for delivery-status updates.

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

From `spacetime-resend-ts/example`:

```powershell
pnpm install
pnpm --dir spacetimedb install
node -e "require('node:fs').copyFileSync('.env.example', '.env')"
```

Set `RESEND_API_KEY` and, for webhooks, `RESEND_WEBHOOK_SECRET` in `.env`.

```powershell
pnpm run build:module:fresh
pnpm run dev
```

Open <http://127.0.0.1:8790>. The **Delivered**, **Bounced**, and **Complaint**
buttons fill Resend's test addresses. Sending creates a real Resend test request;
delivery-state transitions require a reachable webhook.

`build:module:fresh` deletes and recreates only the local `spacetime-resend-example`
database. Use `pnpm run build:module` to preserve existing data.

## Use in your project

This workspace tests the component source in this repository. Consumer applications install published releases:

```bash
npm install @spacetimedb/resend @spacetimedb/rate-limit @spacetimedb/crypto spacetimedb@^2.8.3
```

Follow the package's
[integration guide](../README.md#integrate-into-an-application). Copy the
authorized send procedure, caller-scoped views, and signed webhook route;
replace the example's development identity bootstrap in production.

## Configuration

| Variable                    | Default                    | Purpose                                                                                         |
| --------------------------- | -------------------------- | ----------------------------------------------------------------------------------------------- |
| `RESEND_API_KEY`            | empty                      | Required to send mail.                                                                          |
| `RESEND_WEBHOOK_SECRET`     | empty                      | Required to accept webhook requests. Missing or invalid signatures are rejected.                |
| `RESEND_ALLOWED_RECIPIENTS` | empty                      | Optional comma-separated external recipients. Resend test addresses are included automatically. |
| `DEFAULT_FROM`              | `onboarding@resend.dev`    | Default sender; use a verified address outside Resend's test flow.                              |
| `STDB_URI`                  | `ws://127.0.0.1:3000`      | Browser and server WebSocket endpoint.                                                          |
| `STDB_HTTP`                 | `http://127.0.0.1:3000`    | CLI and native-route HTTP endpoint. Must match `STDB_URI`.                                      |
| `STDB_DATABASE`             | `spacetime-resend-example` | Published database name.                                                                        |
| `STDB_SERVER_TOKEN`         | generated locally          | Optional pre-provisioned server identity token.                                                 |
| `HOST`                      | `127.0.0.1`                | Static-server bind address.                                                                     |
| `PORT`                      | `8790`                     | Static-server port.                                                                             |

When `STDB_SERVER_TOKEN` is unset, the server stores its identity token in the
ignored `.stdb-server-token` file. The logged-in publishing identity registers it
with `resend.add_admin_identity` before the server writes private configuration.
Configuration failure is fatal when an API key was supplied; the server will not
pretend to be ready with an unauthorized identity.

## Webhooks

The native module endpoint is:

```text
POST http://127.0.0.1:3000/v1/database/spacetime-resend-example/route/webhook/resend
```

The Node server also exposes:

```text
POST http://127.0.0.1:8790/webhook/resend
```

That route is a raw-body passthrough for stable tunnel URLs. Signature verification
and ingestion still occur inside the SpacetimeDB module. The module rejects requests
when no signing secret is configured or when the signature is invalid; there is no
unverified development fallback.

Without a publicly reachable webhook, sending still creates a queued email row, but
Delivered, Opened, Clicked, Bounced, and Complaint transitions cannot arrive.

## Architecture

```text
Browser
  -> send_dispatch host procedure
  -> recipient allowlist and caller/global quotas
  -> mounted resend namespace
  -> Resend API

Resend webhook
  -> native SpacetimeDB HTTP route
  -> signature verification and ingest
  -> caller-scoped live views

Authorized example server
  -> private Resend configuration during startup
  -> static UI and optional raw webhook passthrough
```

## Security and deployment boundaries

- The browser never receives the Resend API key, signing secret, server token, or
  component administrator role.
- `send_dispatch` accepts only server-configured recipients. It allows five sends
  per caller every ten minutes and 25 sends globally per hour.
- Provider failures return a stable application error while detailed delivery
  state remains in private component tables.
- `.env` and `.stdb-server-token` are ignored and must not be committed.
- The development server binds to loopback by default.
- A production deployment should provision its service identity and secret store
  through deployment infrastructure.

## Verification

```powershell
pnpm --dir spacetimedb run build
pnpm run build
pnpm exec tsc -p tsconfig.json
```

For a provider smoke test, send to `delivered@resend.dev` and confirm a queued row
appears. With a reachable signed webhook, confirm it advances to Delivered. Invalid
or unsigned webhook fixtures must return a non-success status.

## Troubleshooting

- **`resend.not_authorized`:** publish with the logged-in CLI identity and restart
  so it can authorize the persistent server identity.
- **Send remains Queued:** confirm the webhook is publicly reachable and its signing
  secret matches `RESEND_WEBHOOK_SECRET`.
- **Connection targets disagree:** make `STDB_URI`, `STDB_HTTP`, and the publish
  target refer to the same SpacetimeDB instance.
- **Changing the server identity intentionally:** stop the server, remove
  `.stdb-server-token`, and restart while logged in as a database administrator.

## Important files

- `spacetimedb/src/index.ts`: host procedures, caller-scoped views, and native route.
- `server.ts`: safe startup configuration, server identity, and webhook passthrough.
- `src/app.ts`: browser connection and Dispatch UI behavior.
- `public/index.html`: Dispatch interface structure.
- `public/styles.css`: Dispatch presentation.
