# @spacetimedb/stripe

A SpacetimeDB submodule that mirrors Stripe customers, subscriptions, Checkout
sessions, invoices, and payments. Stripe webhooks feed private base tables, and
host modules expose product-specific views and workflows. Procedures are
synchronous and webhook payloads use valibot validation.

---

## Install

```bash
npm install @spacetimedb/stripe spacetimedb@^2.8.3
```

Requires SpacetimeDB 2.8.3 or later for submodule mounting.

For the install-to-publish workflow, see
[Getting started](https://spacetimedb.com/docs/).

This submodule can be published directly as its own SpacetimeDB module from the root entry point.

## Usage

### Integrate into an application

Mount Stripe in the application schema and initialize its private tables. The
host must place authorization in front of customer, Checkout, portal, and
billing procedures and expose only caller-scoped billing views:

```ts
import { schema } from 'spacetimedb/server';
import * as stripe from '@spacetimedb/stripe/submodule';

const spacetimedb = schema({ stripe });
export default spacetimedb;

export const init = spacetimedb.init(ctx => {
  stripe.installStripe(ctx.as.stripe);
});
```

Configure credentials through an administrator-only startup path. See the
[Premium Store host module](./example/spacetimedb/)
for service-identity delegation, safe return URLs, catalog synchronization, and
webhook routing.

Expose a product-facing Checkout procedure that resolves the application user,
validates every price against a server-owned catalog, fixes the allowed return
URL origins, and delegates to Stripe:

```ts
export const create_store_checkout_session = spacetimedb.procedure(
  storeCheckoutParams,
  storeCheckoutResult,
  (ctx, args) => {
    const checkout = authorizeStoreCheckout(ctx, args);
    return stripe.create_checkout_session(ctx.as.stripe, checkout);
  }
);
```

`storeCheckoutParams`, `storeCheckoutResult`, and `authorizeStoreCheckout`
belong to the host application. The generated client calls that wrapper and
navigates only to the returned Stripe URL:

```ts
const checkout = await conn.procedures.createStoreCheckoutSession({
  items: [{ priceId, quantity: 1n }],
  customerId,
  mode: 'payment',
  successUrl: `${location.origin}/?checkout=success`,
  cancelUrl: `${location.origin}/?checkout=cancelled`,
  metadataJson: JSON.stringify({ cartId }),
  subscriptionMetadataJson: undefined,
  paymentIntentMetadataJson: undefined,
});

if (!checkout.url) throw new Error('stripe.checkout_url_missing');
location.assign(checkout.url);
```

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

The Stripe secret stays in private module state. Every provider-backed,
billing-state, configuration, and query procedure is admin-gated. A host module
can perform application-specific authorization and then call the helpers through
its mounted `ctx.as.stripe` context.

## Private tables

| Table                     | Key                          | Notes                                     |
| ------------------------- | ---------------------------- | ----------------------------------------- |
| `stripe_customer`         | `stripe_customer_id`         | indexed by email, app-userId              |
| `stripe_subscription`     | `stripe_subscription_id`     | indexed by customer, org, user            |
| `stripe_checkout_session` | `stripe_checkout_session_id` | indexed by customer, status               |
| `stripe_invoice`          | `stripe_invoice_id`          | indexed by customer, subscription, status |
| `stripe_payment`          | `stripe_payment_intent_id`   | indexed by customer, status               |

- `stripe_webhook_event`: idempotency log (`get_webhook_event_count` exposes the size)
- `stripe_config`: credentials singleton
- `stripe_admin_identity`: admin allowlist

Stripe base tables are private. Expose product-specific fields and rows through
caller-scoped host views. Resolve customer, user, and organization IDs from
trusted application context.

## API

**Setup**

- `set_stripe_config(secretKey, stripeVersion, webhookSigningSecret)`
- `set_stripe_webhook_signing_secret(webhookSigningSecret)`: rotates only the webhook secret
- `get_stripe_config_status()`: returns `{ isConfigured, hasWebhookSecret, secretKeyLength, ... }`
- `add_admin_identity(identity)` / `remove_admin_identity(identity)`

**Customer / billing flows**

- `create_customer({ email, name, metadataJson, idempotencyKey })`
- `create_or_update_customer({ stripeCustomerId, email, name, metadataJson })`
- `get_or_create_customer({ userId, email, name})`
- `create_checkout_session({ items, mode, successUrl, cancelUrl, customerId, ...metadata })`
- `validate_stripe_price({ priceId })`: confirms a price exists and is active
- `get_remote_checkout_session({ sessionId })`: fetch session state from Stripe
- `create_customer_portal_session({ customerId, returnUrl })`
- `cancel_subscription({ stripeSubscriptionId, cancelAtPeriodEnd })`
- `reactivate_subscription({ stripeSubscriptionId })`
- `update_subscription_quantity({ stripeSubscriptionId, quantity })`
- `update_subscription_metadata({ stripeSubscriptionId, metadataJson, orgId, userId })`
- `stripe_api_request({ method, path, formBody, idempotencyKey })`: admin-gated
  request to a relative `/v1/` path on `api.stripe.com`; accepted methods are
  `GET`, `POST`, and `DELETE`

**Webhook ingest / replay**

- `ingest_stripe_webhook(eventId, eventType, livemode, payloadJson, signatureHeader)`: idempotent
- `replay_webhook_event(eventId)`: re-applies a stored event
- `get_webhook_event_count()`: observability
- `stripe_webhook_handler` and `handle_stripe_webhook` support direct host HTTP
  routing.
- `upsert_customer`, `upsert_subscription`, `update_payment_customer`, and
  `update_subscription_quantity_internal` apply trusted synchronization data.

**Admin queries**

- `get_customer`, `get_customer_by_email`, `get_customer_by_user_id`
- `get_subscription`, `list_subscriptions`, `list_subscriptions_with_creation_time`, `get_subscription_by_org_id`, `list_subscriptions_by_org_id`, `list_subscriptions_by_user_id`
- `get_payment`, `list_payments`, `list_payments_by_org_id`, `list_payments_by_user_id`
- `list_invoices`, `list_invoices_by_org_id`, `list_invoices_by_user_id`
- `get_checkout_session`, `list_checkout_sessions`

List procedures return at most 1,000 rows. Build paginated, product-specific
views in the host module when a UI needs a larger history.

Package entrypoints:

- `@spacetimedb/stripe` can run as a standalone billing database.
- `@spacetimedb/stripe/submodule` supplies mounted billing, webhook,
  configuration, and query operations.

## Webhook events handled

```
customer.created                customer.updated
customer.subscription.created   customer.subscription.updated
customer.subscription.deleted   checkout.session.completed
invoice.created                 invoice.finalized
invoice.paid                    invoice.payment_succeeded
invoice.payment_failed          payment_intent.succeeded
```

Other event types are accepted but stored with `status = 'ignored'`.

## Webhook signature verification

Both entry points verify the Stripe signature in-module against the configured
`webhookSigningSecret` (HMAC-SHA256 over `${timestamp}.${rawBody}` via
`@spacetimedb/crypto`). Missing secrets produce a service-unavailable response:

- `stripe_webhook_handler` (HTTP) - for direct Stripe-to-SpacetimeDB delivery.
- `ingest_stripe_webhook` (reducer) - for a relay forwarding the raw body +
  `stripe-signature` header over the SDK; it verifies before mutating state.

`replay_webhook_event` re-applies an already-stored event and is admin-gated.
The relay reducer also verifies that its separately supplied event metadata
matches the signed payload before using the event ID as its idempotency key.
Webhook application is atomic. Invalid event data rolls back the event row and
business-table changes so Stripe can redeliver the event.

## Integration testing

```bash
# Build + publish + happy paths, idempotency, signed-metadata checks, and authorization checks
pnpm run test:smoke

# Real Stripe sandbox via Stripe CLI (requires `stripe login`)
pnpm run test:stripe:e2e
```

The smoke test publishes only to the dedicated `stripe-ts-smoke-test` database.
The Stripe CLI E2E suite likewise defaults to the dedicated `stripe-ts-e2e`
database, forwards the original signed body, and rotates only that database's
ephemeral listener secret.

## Architecture notes

- **valibot for runtime validation.** `vStripeEvent` is a `v.variant('type', [...])` over the 12 supported event types. `attemptToParse` returns a tagged result; `assertExhaustive` makes the typed `switch` compiler-checked.
- **SDK types, sync HTTP.** The `stripe` npm package supplies event types such as `Stripe.CustomerCreatedEvent`. Procedures use the synchronous `ctx.http.fetch` API through the request boundary in `submodule/http.ts`.
- **Compile-time SDK alignment.** `_align*` checks in `schema.ts` assert valibot output is structurally assignable to `Stripe.*Event`. If Stripe ships a breaking change, typecheck fails.
- **Idempotency.** Each webhook event is keyed by `event.id`; re-ingest is a no-op. `replay_webhook_event` applies the stored event state again.

## Testing

```bash
pnpm test
pnpm run lint
```

Credentialed sandbox coverage is described in **Integration testing** above.

## License

[BUSL-1.1](./LICENSE.txt) - same as SpacetimeDB.
