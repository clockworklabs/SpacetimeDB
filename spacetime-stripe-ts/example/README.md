# Premium Store

Premium Store demonstrates a host database that mounts
`@spacetimedb/stripe/submodule` and delegates customer, price, checkout, and
webhook operations through the `stripe` namespace. The browser can shop and create
Stripe Checkout sessions, but it cannot configure Stripe or mutate administrative
catalog state.

## What this demonstrates

- Mounting the Stripe submodule inside an application-owned store module.
- Keeping Stripe credentials in private module state.
- Using a narrow server API for customer lookup, price validation, and Checkout;
  the browser never receives the privileged service identity.
- Seeding an application catalog independently of Stripe provider records.
- Creating or linking idempotent Stripe test prices during explicit server setup.
- Receiving Stripe webhooks through the module's native HTTP route.

## Prerequisites

- Node.js 20 or later and pnpm 10.
- The released SpacetimeDB 2.8 CLI.
- A local SpacetimeDB server reachable as `local`.
- A logged-in CLI identity that publishes the database.
- A Stripe **test-mode** secret key (`sk_test_...`).
- Optional: the Stripe CLI or another tunnel for forwarding test webhooks.

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

From `spacetime-stripe-ts/example`:

```powershell
pnpm install
node -e "require('node:fs').copyFileSync('.env.example', '.env')"
```

Set `STRIPE_SECRET_KEY=sk_test_...` in `.env`. Set
`STRIPE_SYNC_PRICES=1` for the first full checkout run; this creates or reuses
three Stripe test prices and writes their IDs to `store_product`.

```powershell
pnpm run build:module:fresh
pnpm run dev
```

Open <http://127.0.0.1:8787>. Add a product to the cart and create a Stripe test
Checkout session. No charge occurs unless the Checkout page is completed with a
Stripe test payment method.

`build:module:fresh` deletes and recreates only the local `spacetime-stripe-example`
database. Use `pnpm run build:module` to preserve existing data.

## Use in your project

This workspace tests the submodule source in this repository. Consumer applications install published releases:

```bash
npm install @spacetimedb/stripe spacetimedb@^2.8.3
```

Follow the package's
[integration guide](../README.md#integrate-into-an-application). Copy the
service-identity, narrow Checkout API, caller-scoped billing views, and signed
webhook route. The product catalog and storefront are demonstration code.

## Configuration

| Variable                                | Default                              | Purpose                                                       |
| --------------------------------------- | ------------------------------------ | ------------------------------------------------------------- |
| `STRIPE_SECRET_KEY`                     | empty                                | Required for provider operations. Use a test-mode key.        |
| `STRIPE_WEBHOOK_SECRET`                 | empty                                | Verifies incoming Stripe webhook signatures.                  |
| `STRIPE_VERSION`                        | submodule default                    | Optional Stripe API-version override.                         |
| `STRIPE_SYNC_PRICES`                    | `0`                                  | Set to `1` to create/link missing test prices during startup. |
| `STRIPE_ALLOW_BROWSER_PROVIDER_ACTIONS` | automatic on non-production loopback | Explicit provider-action opt-in for other environments.       |
| `STRIPE_RETURN_BASE_URL`                | `http://127.0.0.1:8787`              | Server-owned Checkout return origin.                          |
| `NODE_ENV`                              | empty                                | Set to `production` to disable development-only defaults.     |
| `STDB_URI`                              | `ws://127.0.0.1:3000`                | Browser and server WebSocket endpoint.                        |
| `STDB_HTTP`                             | `http://127.0.0.1:3000`              | CLI administration endpoint. Must match `STDB_URI`.           |
| `SPACETIMEDB_DB_NAME`                   | `spacetime-stripe-example`           | Published database name.                                      |
| `STDB_SERVER_TOKEN`                     | generated locally                    | Optional pre-provisioned server identity token.               |
| `HOST`                                  | `127.0.0.1`                          | Static-server bind address.                                   |
| `PORT`                                  | `8787`                               | Static-server port.                                           |

When no server token is supplied, the server persists one in the ignored
`.stdb-server-token` file. The logged-in publishing identity registers that server
identity in both the host store and mounted Stripe administrator registries. The
browser identity is never granted either role.

## Startup behavior

The example server performs the following bounded setup before accepting HTTP:

1. Connect with the persistent server identity.
2. Authorize it through the logged-in CLI publishing identity.
3. Seed the default store catalog if it is empty.
4. Store Stripe configuration when `STRIPE_SECRET_KEY` is present.
5. Synchronize missing prices only when `STRIPE_SYNC_PRICES=1`.

Price synchronization is opt-in because it creates test-mode objects in the linked
Stripe account. Existing prices use stable lookup keys and are reused.

## Architecture

```text
Browser storefront
  -> public store_product subscription
  -> same-origin /api checkout/customer/validation routes
  -> authorized server identity
  -> host checkout/customer/validation procedures
  -> mounted stripe namespace
  -> Stripe API

Stripe
  -> POST /route/stripe/webhook on the SpacetimeDB database
  -> host router
  -> mounted stripe webhook handler

Authorized example server
  -> private configuration and catalog setup during startup
```

The Node server exposes only browser-safe health and configuration routes:

| Route             | Purpose                             |
| ----------------- | ----------------------------------- |
| `GET /api/health` | Local health probe.                 |
| `GET /api/config` | Browser-safe database/setup status. |

There are no HTTP administration endpoints. The settings panel contains buyer and
debugging controls only.

## Webhooks

Forward Stripe test events to the database's native route:

```text
http://127.0.0.1:3000/v1/database/spacetime-stripe-example/route/stripe/webhook
```

Use the signing secret produced by the forwarding tool as
`STRIPE_WEBHOOK_SECRET`, then restart so the private submodule configuration is
updated.

## Security and deployment boundaries

- Never use a live-mode Stripe key for casual example testing.
- Stripe secrets and the persistent server token must never be committed or sent to
  the browser.
- The server binds to loopback by default.
- Production deployments should provision an authenticated service identity
  through deployment infrastructure.
- Browser provider actions are automatic only for a non-production loopback host.
  Production and externally bound development servers default to disabled. Add
  application authentication and rate limiting, then set
  `STRIPE_ALLOW_BROWSER_PROVIDER_ACTIONS=1` only when required.
- The server owns Checkout return URLs. Set `STRIPE_RETURN_BASE_URL` to the public
  HTTPS origin in production; browser-supplied redirect URLs are ignored.
- Checkout success in the UI is a redirect result; authoritative fulfillment must
  come from verified webhooks.

## Verification

```powershell
pnpm --dir spacetimedb run build
pnpm run build
pnpm exec tsc -p tsconfig.json
```

For the provider-backed smoke test, set `STRIPE_SYNC_PRICES=1`, fresh-publish,
start the server, confirm three prices synchronize, add a product to the cart, and
create a test Checkout session.

## Troubleshooting

- **Products say “Sync price first”:** set `STRIPE_SYNC_PRICES=1` and restart with
  a valid test key.
- **`stripe.not_authorized`:** publish with the logged-in CLI identity and restart;
  both the store and Stripe namespaces must authorize the server identity.
- **Connection targets disagree:** make `STDB_URI`, `STDB_HTTP`, and the publish
  target refer to the same server.
- **Webhook state is stale:** verify the forwarding URL and
  `STRIPE_WEBHOOK_SECRET`.

## Important files

- `spacetimedb/src/store/operations.ts`: application catalog and Stripe
  delegation.
- `server.ts`: safe startup configuration and server identity authorization.
- `src/app.ts`: typed browser-side SpacetimeDB adapter.
- `public/index.html`: storefront and buyer tools.
- `public/ui.js`: storefront state, rendering, and interaction handling.
- `public/styles.css`: storefront presentation.
