# @spacetimedb/stripe

Take payments through Stripe from your SpacetimeDB application. Create Checkout
sessions for purchases or subscriptions, open the customer billing portal, and
track payment and subscription status in your database.

Stripe webhooks keep those records up to date. Your application uses them to
decide when to grant access or fulfill an order, and which billing details each
user can see.

## Install

Run this in your SpacetimeDB TypeScript module directory:

```bash
npm install @spacetimedb/stripe
```

Requires SpacetimeDB SDK, CLI, and server 2.12 or later.

## Integrate into an application

Add Stripe to your module, initialize it, and register a webhook route. The
example below starts Checkout for a product in your application's `storeProduct`
table. `APP_ORIGIN` is your application's URL.

After publishing, configure your Stripe API key and webhook signing secret as
an administrator with `stripe.set_stripe_config`. See the arguments in
[Standalone configuration](#standalone-configuration). Register the webhook URL
with Stripe and select the [events listed below](#webhook-events-handled).

```ts
import { Router, SenderError, schema, t } from 'spacetimedb/server';
import * as stripe from '@spacetimedb/stripe/submodule';

const spacetimedb = schema({ stripe, storeProduct });
export default spacetimedb;

export const init = spacetimedb.init(ctx => {
  stripe.install(ctx.as.stripe);
});

export const stripeWebhook = spacetimedb.httpHandler((ctx, req) =>
  stripe.handleStripeWebhook(ctx, req, { stripe: tx => tx.as.stripe })
);
export const router = spacetimedb.httpRouter(
  new Router().post('/stripe/webhook', stripeWebhook)
);
```

The submodule's procedures are admin-only. Signed-in users reach Stripe through
host procedures that call the host helpers with `ctx.as.stripe`. The host
procedure resolves the user from `ctx.sender`,
takes prices from a server-owned catalog, and fixes the return URLs:

```ts
const APP_ORIGIN = 'https://store.example.com';

export const createStoreCheckoutSession = spacetimedb.procedure(
  { productId: t.string() },
  t.object('StoreCheckoutSession', { url: t.option(t.string()) }),
  (ctx, { productId }) => {
    const product = ctx.withTx(tx =>
      tx.db.storeProduct.productId.find(productId)
    );
    if (!product?.active || !product.stripePriceId) {
      throw new SenderError('store.product_unavailable');
    }
    const session = stripe.createUserCheckoutSession(ctx.as.stripe, {
      userId: ctx.sender.toHexString(),
      items: [{ priceId: product.stripePriceId, quantity: 1n }],
      mode: 'payment',
      successUrl: `${APP_ORIGIN}/?checkout=success`,
      cancelUrl: `${APP_ORIGIN}/?checkout=cancelled`,
    });
    return { url: session.url };
  }
);
```

`createUserCheckoutSession` reuses or creates the user's Stripe customer and
writes `userId` into the session, subscription, and PaymentIntent metadata, so
the mirrored rows can be filtered by user. The client calls the host procedure
and navigates to the returned URL:

```ts
const { url } = await conn.procedures.createStoreCheckoutSession({ productId });
if (url) location.assign(url);
```

Fulfill from webhook state, not from the redirect. See
[Checkout fulfillment](#checkout-fulfillment).

Administrators are the identities in `stripe_admin_identity`. `install` seeds
the publishing identity; `stripe.add_admin_identity` adds more. Host procedures
that need an administrator check read the same table through
`tx.as.stripe.db.stripeAdminIdentity` inside `ctx.withTx`. The
[Premium Store example](https://github.com/clockworklabs/SpacetimeDB/tree/master/modules/spacetime-stripe-ts/example) shows a complete host module.

### Standalone configuration

When running Stripe as its own database, configure it with the unprefixed
operation below. Credentials are private, and the publishing owner is the
first administrator.

```bash
spacetime call --server http://127.0.0.1:3000 stripe-ts set_stripe_config \
  '"sk_test_..."' \
  null \
  '{"some":"whsec_..."}'   # webhook signing secret, optional
```

For `t.option(...)` CLI arguments, use `null` for no value and
`{"some":"value"}` for a string value.

Verify:

```bash
spacetime call --server http://127.0.0.1:3000 stripe-ts get_stripe_config_status '{}'
```

## Private tables

| Table                     | Key                          | Indexed by                        |
| ------------------------- | ---------------------------- | --------------------------------- |
| `stripe_customer`         | `stripe_customer_id`         | user ID                           |
| `stripe_subscription`     | `stripe_subscription_id`     | customer, org, user               |
| `stripe_checkout_session` | `stripe_checkout_session_id` | customer                          |
| `stripe_invoice`          | `stripe_invoice_id`          | customer, subscription, org, user |
| `stripe_payment`          | `stripe_payment_intent_id`   | customer, org, user               |

- `stripe_webhook_event`: idempotency log holding each event's payload (not its
  signature header). The scheduled `prune_webhook_events` reducer deletes events
  received more than 30 days ago, 500 per hourly run. `get_webhook_event_count`
  exposes the size.
- `stripe_webhook_prune_tick`: schedule for `prune_webhook_events`, created by
  `install`
- `stripe_config`: credentials singleton
- `stripe_admin_identity`: admin allowlist

Stripe base tables are private. Expose product-specific fields through
authorized application procedures. Resolve customer, user, and organization IDs from
trusted application context.

## API

**Host helpers** (`@spacetimedb/stripe/submodule`; pass `ctx.as.stripe`)

The host supplies a trusted `userId`. User subscription helpers check the stored
subscription/customer association against that ID. They do not authenticate a
user-supplied ID. Network calls are synchronous and must run in procedures,
outside `withTx`.

- `install(ctx)`: call from the host `init` reducer
- `createUserCheckoutSession(ctx, args)`: hosted or embedded payment, subscription,
  or setup checkout. See [checkout options](#checkout-options-and-billing-management).
- `getOrCreateUserCustomer(ctx, { userId, email?, name?, metadata? })`: matches customers
  by `userId` only
- `createCustomer(ctx, { email?, name?, metadata?, idempotencyKey? })`: creates a
  customer and stores its metadata. Performs no authorization.
- `createUserCustomerPortalSession(ctx, { userId, returnUrl, idempotencyKey? })`
- `cancelUserSubscription(ctx, { userId, stripeSubscriptionId, cancelAtPeriodEnd?, idempotencyKey? })`
- `reactivateUserSubscription(ctx, { userId, stripeSubscriptionId, idempotencyKey? })`:
  removes scheduled cancellation; a fully canceled subscription cannot restart
- `updateUserSubscriptionQuantity(ctx, { userId, stripeSubscriptionId, quantity, subscriptionItemId?, idempotencyKey? })`:
  requires an item ID for a multi-item subscription
- `stripeRequest(ctx, { method, path, formBody?, idempotencyKey? })`: request to
  a relative `/v1/` path on `api.stripe.com`; accepted methods are `GET`,
  `POST`, and `DELETE`
- `handleStripeWebhook(ctx, req, { stripe: tx => tx.as.stripe, onEvent? })`:
  HTTP handler using the host transaction; see [fulfillment](#checkout-fulfillment)
- `errors`: error code constants thrown by the submodule

**Setup** (admin)

- `set_stripe_config(secretKey, stripeVersion, webhookSigningSecret)`
- `set_stripe_webhook_signing_secret(webhookSigningSecret)`: rotates only the webhook secret
- `get_stripe_config_status()`: returns `{ isConfigured, hasWebhookSecret, secretKeyLength, ... }`
- `add_admin_identity(identity)` / `remove_admin_identity(identity)`

**Billing** (admin)

- `get_or_create_customer({ userId, email, name })`
- `create_checkout_session({ items, mode, successUrl, cancelUrl, customerId, ...metadata })`
- `validate_stripe_price({ priceId })`: confirms a price exists and is active
- `get_remote_checkout_session({ sessionId })`: fetch session state from Stripe
- `create_customer_portal_session({ customerId, returnUrl })`
- `cancel_subscription({ stripeSubscriptionId, cancelAtPeriodEnd })`
- `reactivate_subscription({ stripeSubscriptionId })`
- `update_subscription_quantity({ stripeSubscriptionId, quantity })`
- `update_subscription_metadata({ stripeSubscriptionId, metadataJson, orgId, userId })`:
  updates stored metadata only; it does not update Stripe

**Webhooks**

- `ingest_stripe_webhook(eventId, eventType, livemode, payloadJson, signatureHeader)`:
  signature-verified relay ingest, idempotent
- `replay_webhook_event(eventId)` (admin): re-applies a stored event
- `prune_webhook_events`: scheduled retention sweep
- `get_webhook_event_count()` (admin): observability
- `get_webhook_event(eventId)` (admin): stored event with `status`
  (`Received`, `Processed`, `Ignored`, or `Failed`) and `errorMessage`

**Queries** (admin)

- `get_customer`, `get_customer_by_user_id`
- `get_subscription`, `list_subscriptions`, `get_subscription_by_org_id`, `list_subscriptions_by_org_id`, `list_subscriptions_by_user_id`
- `get_payment`, `list_payments`, `list_payments_by_org_id`, `list_payments_by_user_id`
- `list_invoices`, `list_invoices_by_org_id`, `list_invoices_by_user_id`
- `get_checkout_session`, `list_checkout_sessions`

List procedures return at most 1,000 rows. Build paginated, product-specific
queries in the host module when a UI needs a larger history.

Package entrypoints:

- `@spacetimedb/stripe` publishes the procedures and reducers above as a
  standalone billing database with a `/stripe/webhook` HTTP route.
- `@spacetimedb/stripe/submodule` registers the same procedures and reducers
  under the host's namespace and adds the host helpers.

## Webhook events handled

```
customer.created                           customer.updated
customer.deleted
customer.subscription.created              customer.subscription.updated
customer.subscription.deleted              checkout.session.completed
checkout.session.async_payment_succeeded   checkout.session.async_payment_failed
invoice.created                            invoice.finalized
invoice.updated
invoice.paid                               invoice.payment_succeeded
invoice.payment_failed                     invoice_payment.paid
payment_intent.succeeded
payment_intent.payment_failed
```

Other event types are accepted and stored with status `Ignored`.
Repeated processed or ignored event IDs are acknowledged without reapplying them.

Stripe does not deliver events in order. Each customer, subscription, Checkout
session, and invoice row stores the `created` time of the event it reflects in
`eventCreatedUnix`. An event older than that, or one that would move the status
backward (a canceled subscription to active, a paid invoice to open, a paid
Checkout session to unpaid or failed), is stored as `Ignored` without changing
the row. `replay_webhook_event` follows the same rule.

### Checkout fulfillment

`checkout.session.completed` also fires for asynchronous payment methods before
the payment settles. Grant access only when `stripe_checkout_session.paymentStatus`
is `paid` or `no_payment_required` and its mode is `payment` or `subscription`.
Setup mode only saves payment details. Subscribe the webhook endpoint to
`checkout.session.async_payment_succeeded` and
`checkout.session.async_payment_failed` so delayed payments update that field.
Stripe reports a failed delayed payment as `unpaid`, the same as a pending one,
so the submodule records `checkout.session.async_payment_failed` as
`paymentStatus` `failed`.

Pass `onEvent: (tx, event) => { ... }` to `handleStripeWebhook` to write application
records in the same transaction as Stripe state. Its `stripe` selector receives
that same host transaction. The callback receives a verified event envelope;
validate fields of `event.data.object` before using them. Unhandled event types
also reach this callback. Keep it synchronous and free of network calls.

A callback exception rolls back all writes and returns HTTP 500 so Stripe can
retry. Deduplicate fulfillment by Checkout session or another business-object
ID: different event IDs can describe the same purchase. The example uses a
private purchase table keyed by session ID. Repeated processed event IDs skip
the callback. Admin replay and the relay reducer update Stripe state only; they
do not invoke a host HTTP callback.

### Checkout options and billing management

Use `params` on `createUserCheckoutSession` for Stripe Checkout options such as
`allow_promotion_codes`, `automatic_tax`, or `subscription_data.trial_period_days`.
Customer, line items, mode, URLs, and top-level metadata come from helper
arguments. Helper metadata wins over nested metadata, including `userId`.

For setup, pass `mode: 'setup'` and `currency`, with no line items. For embedded
Checkout, pass `uiMode: 'embedded_page'` and `returnUrl` instead of success/cancel
URLs. Mount Stripe.js embedded Checkout with the returned `clientSecret`. Keep
that secret private to the buyer. Hosted Checkout returns `url`.

```ts
export const openBilling = spacetimedb.procedure(
  {},
  t.object('BillingPortal', { url: t.string() }),
  ctx =>
    stripe.createUserCustomerPortalSession(ctx.as.stripe, {
      userId: ctx.sender.toHexString(),
      returnUrl: APP_ORIGIN,
    })
);
```

Give each new purchase a request ID and pass it as `idempotencyKey`. Keep the
same ID and arguments when retrying after a lost response. The checkout helper
scopes it to the customer. Stripe can expire request keys after 24 hours; check
existing billing state before retrying an older attempt. A new purchase needs a
new key. Stripe and STDB writes are separate: a failed local write can follow a
successful Stripe request. Retry with the same key and let webhooks reconcile
state. Deleted customers remain in the history and are not reused.

### Stripe API versions

Requests default to `2026-08-26.dahlia`, matching the pinned Stripe parameter
types. Set `stripeVersion` only when the supplied options match that version.
The SDK supplies types; requests still use STDB's synchronous HTTP client.

Webhook payloads use the API version configured on the webhook endpoint, not
the `stripeVersion` sent with API requests. The submodule reads both payload
shapes:

- Invoice subscriptions come from `invoice.subscription` before
  `2025-03-31.basil` and from `invoice.parent.subscription_details.subscription`
  from that version on.
- `stripe_payment` mirrors succeeded and failed PaymentIntents. `stripeInvoiceId` is
  set when the PaymentIntent paid an invoice, from `payment_intent.invoice`
  before `2025-03-31.basil` and from `invoice_payment.paid` from that version on.
  Endpoints on `2025-03-31.basil` or later must subscribe to
  `invoice_payment.paid`. Exclude rows with `stripeInvoiceId` when totaling
  payments alongside `stripe_invoice`.
- An invoice row takes `userId` and `orgId` from its subscription row, or from
  `invoice.parent.subscription_details.metadata` when the invoice arrives
  before the subscription.
- A payment row linked to an invoice takes its `stripeCustomerId`, `userId`,
  and `orgId` from the invoice when the payment event carries none. When the
  payment event arrives first, the next invoice event fills the empty fields.

## Webhook signature verification

Both entry points verify the Stripe signature in-module against the configured
`webhookSigningSecret` (HMAC-SHA256 over `${timestamp}.${rawBody}` via
`@spacetimedb/crypto`). Missing secrets produce a service-unavailable response:

- `handleStripeWebhook` (HTTP) - for direct Stripe-to-SpacetimeDB delivery,
  mounted at `/stripe/webhook` by the standalone module and routed by the host
  when used as a submodule.
- `ingest_stripe_webhook` (reducer) - for a relay forwarding the raw body +
  `stripe-signature` header over the SDK; it verifies before mutating state.

`replay_webhook_event` re-applies an already-stored event and is admin-gated.
The relay reducer also verifies that its separately supplied event metadata
matches the signed payload before using the event ID as its idempotency key.
The HTTP handler records rejected payloads as Failed and returns `400`. A
redelivery retries the stored payload. Unexpected transaction errors return
`500` from the HTTP handler without committing partial state.

The relay reducer throws, writing nothing, when size, secret, signature, or
metadata checks reject a request. Once a signed event is stored, the reducer
commits: an invalid payload keeps its `Failed` event record with the error
message. Callers read the outcome with `get_webhook_event`.

## Integration testing

```bash
# Build + publish + happy paths, idempotency, signed-metadata checks, and authorization checks
pnpm run test:smoke

# Include native HTTP delivery and redelivery checks against a local server
pnpm run test:smoke --server http://127.0.0.1:3000 --http-url http://127.0.0.1:3000

# Real Stripe sandbox via Stripe CLI (requires `stripe login`)
pnpm run test:stripe:e2e

# Host ownership and atomic webhook fulfillment; creates and deletes a test database
pnpm run test:example http://127.0.0.1:3000

# Browser cart and retry tests; requires Google Chrome, no server or Stripe keys
pnpm run test:ui

# Live helper contracts; requires STRIPE_SECRET_KEY (sk_test_...) and spacetime login
pnpm run test:billing:live http://127.0.0.1:3000
```

The billing test creates test-mode customers, prices, sessions, and a trial
subscription. It removes its database, deletes customers, expires sessions,
cancels subscriptions, and archives prices and products afterward.

With `STRIPE_SECRET_KEY` set to a test key, `test:example` also checks checkout
recovery against Stripe and completes a free order in Google Chrome. It expires
open sessions, deletes its customer, and archives its test product.

The smoke test publishes only to the dedicated `stripe-ts-smoke-test` database.
The Stripe CLI E2E suite likewise defaults to the dedicated `stripe-ts-e2e`
database, forwards the original signed body, and rotates only that database's
ephemeral listener secret.

## Testing

```bash
pnpm test
pnpm run lint
```

## License

Apache-2.0. See the [license](https://github.com/clockworklabs/SpacetimeDB/blob/master/licenses/apache2.txt).
