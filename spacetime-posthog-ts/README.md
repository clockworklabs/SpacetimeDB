# @spacetimedb/posthog

A SpacetimeDB submodule for server-side PostHog analytics: direct capture,
durable queued events, scheduled batch delivery, feature flag evaluation, and
admin-scoped delivery state. Procedures call PostHog through `ctx.http.fetch`.

---

## Install

```bash
npm install @spacetimedb/posthog spacetimedb
```

Requires SpacetimeDB 2.8.3 or later for submodule mounting.

For the install-to-publish workflow, see
[Getting started](https://spacetimedb.com/docs/).

This submodule can be published directly as its own SpacetimeDB module from the root entry point.

## Usage

### Integrate into an application

Register PostHog in the host schema, call `install` from the host `init`, and
enqueue events from reducers. `install` starts a scheduled procedure that
delivers queued events once credentials are configured:

```ts
import { schema, t } from 'spacetimedb/server';
import * as posthog from '@spacetimedb/posthog/submodule';

const spacetimedb = schema({ posthog });
export default spacetimedb;

export const init = spacetimedb.init(ctx => {
  posthog.install(ctx.as.posthog);
});

export const completeOrder = spacetimedb.reducer(
  { orderId: t.string(), totalCents: t.u64() },
  (ctx, args) => {
    // Apply the application's order mutation in this reducer transaction.
    posthog.enqueueEventInTx(ctx.as.posthog, {
      distinctId: ctx.sender.toHexString(),
      event: 'order_completed',
      propertiesJson: JSON.stringify({
        orderId: args.orderId,
        totalCents: args.totalCents.toString(),
      }),
      idempotencyKey: `order_completed:${args.orderId}`,
    });
  }
);
```

`enqueueEventInTx` does not throw on invalid event data, so analytics cannot roll
back the host reducer. It returns `{ outboxId, inserted, error }`; an invalid
event is stored with status `Rejected` and the error code in `lastError`, and is
never sent.

The client calls the business operation. Analytics remain a server-side
concern:

```ts
await conn.reducers.completeOrder({ orderId, totalCents });
```

The host must decide which events a caller may trigger. See the
[Context Cafe host module](./example/spacetimedb/) for reducer-safe queueing and
admin-scoped observability.

### Standalone configuration

PostHog credentials live in a private `posthog_config` singleton. During
`init`, a fresh database seeds the owner into the private
`posthog_admin_identity` table.

```bash
spacetime call --server http://127.0.0.1:3000 posthog-ts set_posthog_config \
  '"https://us.i.posthog.com"' \
  '"phc_..."'
```

Verify:

```bash
spacetime call --server http://127.0.0.1:3000 posthog-ts get_posthog_config_status '{}'
```

`get_posthog_config_status` is admin-only. The project token stays in private
module state.

## Public views

The submodule stores operational state in private tables and exposes admin-gated subscribable views:

| View                         | Notes                                                      |
| ---------------------------- | ---------------------------------------------------------- |
| `posthog_outbox_admin`       | up to 500 queued and in-flight events                      |
| `posthog_delivery_log_admin` | recent direct capture, flush, and flag evaluation attempts |

## API

**Setup**

- `set_posthog_config({ host, projectApiKey })`
- `get_posthog_config_status()` returns `{ isConfigured, host, projectApiKeyLength }`.
- `add_admin_identity(identity)` / `remove_admin_identity(identity)`

**Delivery**

- `install(ctx)` seeds the caller as an administrator and starts
  `scheduled_flush`, which runs every 5 seconds. Each run deletes up to 1,000
  delivered, failed, rejected, and delivery log rows older than 30 days, then,
  once PostHog is configured, sends up to 10 batches of 100 queued events.
- Each event is sent with its enqueue time as `timestamp` and a stable `uuid`,
  so PostHog deduplicates events sent more than once.
- Network errors, 408, 429, and 5xx responses retry with exponential backoff, up
  to 5 attempts. Other 4xx responses, such as an invalid project key, fail the
  events immediately. PostHog's `/batch` endpoint reports one status for the
  whole request, so every event in a batch gets the same result.
- `flush_outbox({ limit })` sends up to `limit` (1 to 100) queued events now and
  returns `{ attempted, delivered, failed }`.
- `requeue_failed_events({ limit })` moves up to `limit` failed events back to
  the queue with a fresh retry budget, for example after fixing the project key.
  Rejected events are not requeued.

**Analytics**

- `enqueue_event({ distinctId, event, propertiesJson, idempotencyKey })` queues
  an event and throws on invalid input. Host reducers should call
  `enqueueEventInTx` after their own authorization.
- `capture_now({ distinctId, event, propertiesJson })` sends one event
  immediately and returns `{ ok, statusCode, error }`.
- `get_feature_flag({ key, distinctId, personPropertiesJson, groupsJson })` calls
  PostHog `/flags?v=2` and returns `{ ok, statusCode, enabled, variant, error }`.
  `enabled` and `variant` are unset when the flag is missing from the response.

**Maintenance**

- `clear_analytics({ maxRows })` deletes up to `maxRows` outbox and delivery log
  rows. Events already received by PostHog remain there.
- The private `posthog_delivery_stats` singleton counts current outbox rows:
  `pending` (queued and in flight), `delivered`, and `failed` (failed and
  rejected). Host views can read it through `ctx.db.posthog.posthogDeliveryStats`.
- `errors`, exported from `@spacetimedb/posthog/submodule`, lists the error
  codes the submodule throws.

Every procedure and reducer above except `scheduled_flush` is admin-only because
it can spend provider quota or change delivery state. `scheduled_flush` only runs
when called by the database's own scheduler. Expose product-specific host operations that
derive the distinct ID and event or flag name from authorized application state.

Package entrypoints:

- `@spacetimedb/posthog` can run as a standalone analytics database.
- `@spacetimedb/posthog/submodule` supplies submodule state, configuration,
  scheduled delivery, `enqueueEventInTx`, and admin views.

## Architecture notes

- **Synchronous HTTP API.** Module procedures call PostHog's HTTP endpoints
  directly through `ctx.http.fetch`.
- **Direct plus outbox.** Immediate capture is useful for important events. The outbox is for reducer-safe transactional queueing and scheduled delivery.
- **Browser analytics.** Applications can add `posthog-js` in the frontend for
  autocapture and session replay.

## Testing

```bash
pnpm test
pnpm exec tsc --noEmit
pnpm run build
npm pack --dry-run --json
```

The example app in `example/` mounts the submodule under the `posthog` namespace and subscribes to the admin views.

## License

Apache-2.0. The published package includes the license text in `LICENSE`.
