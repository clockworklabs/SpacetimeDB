# Premium Store Stripe example

A small storefront with a cart and Stripe Checkout. Use it to try a purchase
with Stripe test payments.

## Run it locally

Requires Node.js 20+, pnpm 10, and the SpacetimeDB CLI and server built from
this checkout.

Start SpacetimeDB in a separate terminal:

```bash
spacetime start
```

From `modules/spacetime-stripe-ts/example`, copy [.env.example](./.env.example) to `.env`.

You also need a Stripe **test-mode** secret key. In `.env`, replace
`STRIPE_SECRET_KEY` with your `sk_test_...` key and set `STRIPE_SYNC_PRICES=1`.
The first start creates or reuses three test prices in your Stripe account.

Then publish the example and start its web server:

```bash
pnpm install
pnpm run build:module
pnpm run dev
```

Open <http://127.0.0.1:8787>.

## Try it

1. Add a product to the cart.
2. Start checkout. The app opens a Stripe Checkout page.
3. Complete checkout with a Stripe test payment method.
4. Return to the store. With webhooks configured below, the app confirms the
   purchase from stored payment state and clears the cart.
5. Open **Billing** in the header, then **Manage billing** to open Stripe's customer portal. Enable its test-mode
   configuration in the Stripe Dashboard first. Reopen **Billing** after a change
   to see the updated subscription state. **Refresh** checks again while it is open.

Use test mode throughout. Do not enter real card details.

## Receive payment updates

Forward Stripe test webhooks to this URL with the Stripe CLI or a tunnel:

```text
http://127.0.0.1:3000/v1/database/spacetime-stripe-example/route/stripe/webhook
```

Put the signing secret for that endpoint in `STRIPE_WEBHOOK_SECRET`, then
restart `pnpm run dev`. Incoming events update the stored Stripe records.

A successful browser redirect is not proof of payment. In your own app, fulfill
an order only after a verified webhook records its payment status as `paid`
or `no_payment_required` for a payment or subscription checkout. Setup checkout
only saves payment details. This example writes one purchase receipt per session
inside the webhook transaction. It keeps billing details private to each buyer.

The browser retains the checkout request ID for retries. Keep the same cart to
retry a failed request; changing it starts a new attempt. Check billing state
before retrying. After 24 hours, the example checks the old attempt with Stripe
and cancels any unpaid checkout before starting again. A completed checkout
blocks the restart so you can check its payment result in **Billing**.

## Configuration

- `STRIPE_SYNC_PRICES=1` creates or links missing test prices at startup.
  You can set it back to `0` once the catalog is ready.
- `STRIPE_RETURN_BASE_URL` is where Checkout returns the buyer. Change it if
  you change the store's address.
- Price-check and webhook-count debug controls are enabled during local
  development. For public deployments, set `NODE_ENV=production` and keep
  `STRIPE_ALLOW_BROWSER_PROVIDER_ACTIONS=0`.

See [.env.example](./.env.example) for the remaining settings.

## Before deploying

Add account-based access checks before exposing checkout to the public.
The demo accepts any browser identity and limits its Stripe calls to five
per ten minutes; a new browser identity gets a new allowance.
The server saves its admin credential in `.stdb-server-token`. Keep that file
and `.env` private.

## Troubleshooting

- **Sync price first:** set `STRIPE_SYNC_PRICES=1` and restart with a valid test key.
- **`stripe.not_authorized` or `store.not_authorized`:** use the CLI account
  that published the database, then restart.
- **`store.rate_limited`:** wait for the ten-minute limit to reset.
- **Payment records do not update:** check webhook forwarding and the signing secret.

## Change the example

- [spacetimedb/src/store/operations.ts](./spacetimedb/src/store/operations.ts): catalog and checkout logic.
- [server.ts](./server.ts): Stripe setup.
- [public/ui.js](./public/ui.js): cart and storefront controls.

After changing server code, run `pnpm run build:module`. Restart
`pnpm run dev` after changing browser code or `.env`.

To start over, run `pnpm run build:module:fresh`. **This deletes all data in
the local `spacetime-stripe-example` database.**

To use the submodule in your own app, see the
[package integration guide](../README.md#integrate-into-an-application).
