# Premium Store

Premium Store demonstrates a host database that mounts
`@spacetimedb/stripe/submodule` and delegates customer, price, checkout, and
webhook operations through the `stripe` namespace. The browser calls buyer
procedures with its own identity to create its Stripe customer and Checkout
sessions, but it cannot configure Stripe or mutate administrative catalog state.

## What this demonstrates

- Mounting the Stripe submodule inside an application-owned store module.
- Keeping Stripe credentials in private module state.
- Buyer procedures (`create_store_checkout_session`,
  `get_or_create_store_customer`) that the browser calls directly. Each buyer's
  identity owns its Stripe customer, and prices, modes, and return URLs come from
  server-owned state.
- One administrator list: the store checks the Stripe submodule's
  `stripe_admin_identity` table.
- Seeding an application catalog independently of Stripe provider records.
- Creating or linking idempotent Stripe test prices during explicit server setup.
- Receiving Stripe webhooks through the module's native HTTP route.

## Prerequisites

- Node.js 20 or later and pnpm 10.
- A SpacetimeDB CLI and server built from this checkout, with the CLI available as `spacetime`.
- A local SpacetimeDB server reachable as `local`.
- A logged-in CLI identity that publishes the database.
- A Stripe **test-mode** secret key (`sk_test_...`).
- Optional: the Stripe CLI or another tunnel for forwarding test webhooks.

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
npm install @spacetimedb/stripe spacetimedb
```

Follow the package's
[integration guide](../README.md#integrate-into-an-application). Copy the
buyer procedures, the shared administrator check, and the signed webhook
route. The product catalog and storefront are demonstration code.

## Configuration

| Variable                                | Default                              | Purpose                                                       |
| --------------------------------------- | ------------------------------------ | ------------------------------------------------------------- |
| `STRIPE_SECRET_KEY`                     | empty                                | Required for provider operations. Use a test-mode key.        |
| `STRIPE_WEBHOOK_SECRET`                 | empty                                | Verifies incoming Stripe webhook signatures.                  |
| `STRIPE_VERSION`                        | submodule default                    | Optional Stripe API-version override.                         |
| `STRIPE_SYNC_PRICES`                    | `0`                                  | Set to `1` to create/link missing test prices during startup. |
| `STRIPE_ALLOW_BROWSER_PROVIDER_ACTIONS` | automatic on non-production loopback | Enables the price-validation and webhook-count debug routes.  |
| `STRIPE_RETURN_BASE_URL`                | `http://127.0.0.1:8787`              | Checkout return origin, stored in the module at startup.      |
| `NODE_ENV`                              | empty                                | Set to `production` to disable development-only defaults.     |
| `STDB_URI`                              | `ws://127.0.0.1:3000`                | Browser and server WebSocket endpoint.                        |
| `STDB_HTTP`                             | `http://127.0.0.1:3000`              | CLI administration endpoint. Must match `STDB_URI`.           |
| `SPACETIMEDB_DB_NAME`                   | `spacetime-stripe-example`           | Published database name.                                      |
| `STDB_SERVER_TOKEN`                     | generated locally                    | Optional pre-provisioned server identity token.               |
| `HOST`                                  | `127.0.0.1`                          | Static-server bind address.                                   |
| `PORT`                                  | `8787`                               | Static-server port.                                           |

When no server token is supplied, the server persists one in the ignored
`.stdb-server-token` file. The logged-in publishing identity registers that server
identity with `stripe.add_admin_identity`, the administrator list shared by the
store and the Stripe submodule. The browser identity is never an administrator.
The browser keeps its own identity token in `localStorage`, so the buyer and its
Stripe customer survive the Checkout redirect.

## Startup behavior

The example server performs the following bounded setup before accepting HTTP:

1. Connect with the persistent server identity.
2. Authorize it through the logged-in CLI publishing identity.
3. Store the Checkout return origin from `STRIPE_RETURN_BASE_URL`.
4. Seed the default store catalog if it is empty.
5. Store Stripe configuration when `STRIPE_SECRET_KEY` is present.
6. Synchronize missing prices only when `STRIPE_SYNC_PRICES=1`.

Price synchronization is opt-in because it creates test-mode objects in the linked
Stripe account. Existing prices use stable lookup keys and are reused.

## Architecture

```text
Browser storefront (own identity)
  -> public store_product subscription
  -> create_store_checkout_session / get_or_create_store_customer
  -> stripe submodule host helpers
  -> Stripe API

Stripe
  -> POST /route/stripe/webhook on the SpacetimeDB database
  -> host router
  -> stripe submodule webhook handler

Authorized example server
  -> private configuration and catalog setup during startup
  -> stripe.validate_stripe_price and stripe.get_webhook_event_count for the
     debug routes
```

The Node server serves the storefront and these routes:

| Route                          | Purpose                                         |
| ------------------------------ | ----------------------------------------------- |
| `GET /api/health`              | Local health probe.                             |
| `GET /api/config`              | Browser-safe database/setup status.             |
| `POST /api/validate-price`     | Checks a catalog price with Stripe (debug).     |
| `GET /api/webhook-event-count` | Reports the stored webhook event count (debug). |

The debug routes are enabled only when browser provider actions are allowed.

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
- The debug routes are automatic only for a non-production loopback host.
  Production and externally bound development servers default to disabled. Set
  `STRIPE_ALLOW_BROWSER_PROVIDER_ACTIONS=1` only when required.
- The buyer procedures create Stripe customers and Checkout sessions for any
  connected identity. Add application authentication and rate limiting (for
  example with `@spacetimedb/rate-limit`) before exposing them publicly.
- The module owns Checkout return URLs. Set `STRIPE_RETURN_BASE_URL` to the public
  HTTPS origin in production; the browser cannot supply redirect URLs.
- Checkout success in the UI is a redirect result. Fulfill from verified
  webhook state: grant access when `stripe_checkout_session.paymentStatus` is
  `paid` or `no_payment_required`.

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
- **`stripe.not_authorized` or `store.not_authorized`:** publish with the
  logged-in CLI identity and restart so it can grant the server identity through
  `stripe.add_admin_identity`.
- **Connection targets disagree:** make `STDB_URI`, `STDB_HTTP`, and the publish
  target refer to the same server.
- **Webhook state is stale:** verify the forwarding URL and
  `STRIPE_WEBHOOK_SECRET`.

## Important files

- `spacetimedb/src/store/operations.ts`: application catalog, buyer procedures,
  and Stripe delegation.
- `server.ts`: safe startup configuration and server identity authorization.
- `src/app.ts`: typed browser-side SpacetimeDB adapter.
- `public/index.html`: storefront and buyer tools.
- `public/ui.js`: storefront state, rendering, and interaction handling.
- `public/styles.css`: storefront presentation.
