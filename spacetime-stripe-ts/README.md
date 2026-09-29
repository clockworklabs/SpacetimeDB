# @spacetimedb/stripe

A SpacetimeDB submodule that mirrors Stripe customers, subscriptions, Checkout
sessions, invoices, and payments. Stripe webhooks feed private base tables, and
host modules expose product-specific views and workflows. Procedures are
synchronous and webhook payloads use valibot validation.

---

## Install

```bash
npm install @spacetimedb/stripe spacetimedb
```

Requires SpacetimeDB 2.8.3 or later for submodule mounting.

For the install-to-publish workflow, see
[Getting started](https://spacetimedb.com/docs/).

This submodule can be published directly as its own SpacetimeDB module from the root entry point.

## Usage

### Integrate into an application

Register Stripe in the application schema, initialize it, and route Stripe
webhooks to it:

```ts
import { Router, SenderError, schema, t } from 'spacetimedb/server';
import * as stripe from '@spacetimedb/stripe/submodule';

const spacetimedb = schema({ stripe, storeProduct });
export default spacetimedb;

export const init = spacetimedb.init(ctx => {
  stripe.install(ctx.as.stripe);
});

export const stripeWebhook = spacetimedb.httpHandler((ctx, req) =>
  stripe.handleStripeWebhook(ctx.as.stripe, req)
);
export const router = spacetimedb.httpRouter(
  new Router().post('/stripe/webhook', stripeWebhook)
);
```

The submodule's procedures are admin-only. Signed-in users reach Stripe through
host procedures that call the host helpers with `ctx.as.stripe`. The helpers do
no authorization, so the host procedure resolves the user from `ctx.sender`,
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
`ctx.db.stripe.stripeAdminIdentity` instead of keeping a second list. The
[Premium Store example](./example/) shows a complete host module.

### Standalone configuration

Stripe credentials live in a private `stripe_config` singleton. During `init`, a
fresh database seeds the owner into the private `stripe_admin_identity` table.

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

The Stripe secret stays in private module state. Every procedure other than
`ingest_stripe_webhook` is admin-gated.

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

Stripe base tables are private. Expose product-specific fields and rows through
caller-scoped host views. Resolve customer, user, and organization IDs from
trusted application context.

## API

**Host helpers** (`@spacetimedb/stripe/submodule`, plain functions that do no
authorization; pass `ctx.as.stripe`)

- `install(ctx)`: call from the host `init` reducer
- `createUserCheckoutSession(ctx, { userId, email?, name?, items, mode, successUrl, cancelUrl, metadata? })`
- `getOrCreateUserCustomer(ctx, { userId, email?, name? })`: matches customers
  by `userId` only
- `stripeRequest(ctx, { method, path, formBody?, idempotencyKey? })`: request to
  a relative `/v1/` path on `api.stripe.com`; accepted methods are `GET`,
  `POST`, and `DELETE`
- `handleStripeWebhook(ctx, req)`: HTTP webhook handler for a host route
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
- `update_subscription_metadata({ stripeSubscriptionId, metadataJson, orgId, userId })`

**Webhooks**

- `ingest_stripe_webhook(eventId, eventType, livemode, payloadJson, signatureHeader)`:
  signature-verified relay ingest, idempotent
- `replay_webhook_event(eventId)` (admin): re-applies a stored event
- `prune_webhook_events`: scheduled retention sweep
- `get_webhook_event_count()` (admin): observability

**Queries** (admin)

- `get_customer`, `get_customer_by_user_id`
- `get_subscription`, `list_subscriptions`, `get_subscription_by_org_id`, `list_subscriptions_by_org_id`, `list_subscriptions_by_user_id`
- `get_payment`, `list_payments`, `list_payments_by_org_id`, `list_payments_by_user_id`
- `list_invoices`, `list_invoices_by_org_id`, `list_invoices_by_user_id`
- `get_checkout_session`, `list_checkout_sessions`

List procedures return at most 1,000 rows. Build paginated, product-specific
views in the host module when a UI needs a larger history.

Package entrypoints:

- `@spacetimedb/stripe` publishes the procedures and reducers above as a
  standalone billing database with a `/stripe/webhook` HTTP route.
- `@spacetimedb/stripe/submodule` registers the same procedures and reducers
  under the host's namespace and adds the host helpers.

## Webhook events handled

```
customer.created                           customer.updated
customer.subscription.created              customer.subscription.updated
customer.subscription.deleted              checkout.session.completed
checkout.session.async_payment_succeeded   checkout.session.async_payment_failed
invoice.created                            invoice.finalized
invoice.paid                               invoice.payment_succeeded
invoice.payment_failed                     invoice_payment.paid
payment_intent.succeeded
```

Other event types are accepted and stored with status `Ignored`.

Stripe does not deliver events in order. Each customer, subscription, Checkout
session, and invoice row stores the `created` time of the event it reflects in
`eventCreatedUnix`. An event older than that, or one that would move the status
backward (a canceled subscription to active, a paid invoice to open, a paid
Checkout session to unpaid), is stored as `Ignored` without changing the row.
`replay_webhook_event` follows the same rule.

### Checkout fulfillment

`checkout.session.completed` also fires for asynchronous payment methods before
the payment settles. Grant access only when `stripe_checkout_session.paymentStatus`
is `paid` or `no_payment_required`. Subscribe the webhook endpoint to
`checkout.session.async_payment_succeeded` and
`checkout.session.async_payment_failed` so delayed payments update that field.

### Stripe API versions

Webhook payloads use the API version configured on the webhook endpoint, not
the `stripeVersion` sent with API requests. The submodule reads both payload
shapes:

- Invoice subscriptions come from `invoice.subscription` before
  `2025-03-31.basil` and from `invoice.parent.subscription_details.subscription`
  from that version on.
- `stripe_payment` mirrors every succeeded PaymentIntent. `stripeInvoiceId` is
  set when the PaymentIntent paid an invoice, from `payment_intent.invoice`
  before `2025-03-31.basil` and from `invoice_payment.paid` from that version on.
  Endpoints on `2025-03-31.basil` or later must subscribe to
  `invoice_payment.paid`. Exclude rows with `stripeInvoiceId` when totaling
  payments alongside `stripe_invoice`.

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
redelivery retries the stored payload. The relay reducer throws on failure,
which rolls back its transaction. Unexpected transaction errors return `500`
from the HTTP handler without committing partial state.

## Integration testing

```bash
# Build + publish + happy paths, idempotency, signed-metadata checks, and authorization checks
pnpm run test:smoke

# Include native HTTP delivery and redelivery checks against a local server
pnpm run test:smoke --server http://127.0.0.1:3000 --http-url http://127.0.0.1:3000

# Real Stripe sandbox via Stripe CLI (requires `stripe login`)
pnpm run test:stripe:e2e
```

The smoke test publishes only to the dedicated `stripe-ts-smoke-test` database.
The Stripe CLI E2E suite likewise defaults to the dedicated `stripe-ts-e2e`
database, forwards the original signed body, and rotates only that database's
ephemeral listener secret.

## Architecture notes

- **valibot for runtime validation.** `vStripeEvent` is a `v.variant('type', [...])` over the supported event types, and `assertExhaustive` makes the typed `switch` compiler-checked.
- **Sync HTTP.** Procedures call Stripe with the synchronous `ctx.http.fetch` API through the request boundary in `submodule/http.ts`.
- **Compile-time SDK alignment.** `scripts/type-alignment.ts` asserts that the `stripe` package's `Stripe.*Event` types are assignable to the valibot output, so `pnpm run typecheck` fails if Stripe ships an incompatible payload change.
- **Idempotency.** Each webhook event is keyed by `event.id`. Processed and ignored events are acknowledged without re-applying them. Failed events are retried using the stored payload. The HTTP handler retains failures and returns `400`; the reducer rejects failed payloads and rolls back its transaction. `replay_webhook_event` applies the stored event state again.

## Testing

```bash
pnpm test
pnpm run lint
```

Credentialed sandbox coverage is described in **Integration testing** above.

## License

[Apache-2.0](./LICENSE).
