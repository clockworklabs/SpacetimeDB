# @spacetimedb/posthog

A SpacetimeDB submodule for server-side PostHog analytics: direct capture,
durable queued events, explicit batch flush, feature flag evaluation, and
admin-scoped delivery state. Procedures call PostHog through `ctx.http.fetch`.

---

## Install

```bash
npm install @spacetimedb/posthog spacetimedb@^2.8.3
```

Requires SpacetimeDB 2.8.3 or later for submodule mounting.

For the install-to-publish workflow, see
[Getting started](https://spacetimedb.com/docs/).

This submodule can be published directly as its own STDB module from the root entrypoint.

## Usage

### Integrate into an application

Mount PostHog in the host schema. Configure its private credentials through an
administrator-only startup path, enqueue events from reducers, and perform
network delivery from procedures:

```ts
import { schema, t } from 'spacetimedb/server';
import * as posthog from '@spacetimedb/posthog/submodule';

const spacetimedb = schema({ posthog });
export default spacetimedb;

export const init = spacetimedb.init(ctx => {
  posthog.installPostHog(ctx.as.posthog);
});

export const complete_order = spacetimedb.reducer(
  { orderId: t.string(), totalCents: t.u64() },
  (ctx, args) => {
    // Apply the application's order mutation in this reducer transaction.
    posthog.enqueueEvent(ctx.as.posthog, {
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

The host must decide which events and delivery controls a caller may use. See
the
[Context Cafe host module](./example/spacetimedb/)
for reducer-safe queueing, procedure delivery, and admin-scoped observability.

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

The project token stays in private module state.

## Public views

The submodule stores operational state in private tables and exposes admin-gated subscribable views:

| View                         | Notes                                                      |
| ---------------------------- | ---------------------------------------------------------- |
| `posthog_outbox_admin`       | up to 500 queued events waiting for explicit delivery      |
| `posthog_delivery_log_admin` | recent direct capture, flush, and flag evaluation attempts |

## API

**Setup**

- `set_posthog_config({ host, projectApiKey })`
- `get_posthog_config_status()`
- `add_admin_identity(identity)` / `remove_admin_identity(identity)`

**Analytics**

- `capture_now({ distinctId, event, propertiesJson })` sends one event immediately
  through PostHog `/batch` and returns a JSON result string.
- `flush_outbox({ limit })` sends queued events in one `/batch` request, updates
  delivery state, and returns a JSON result string.
- `get_feature_flag({ key, distinctId, personPropertiesJson, groupsJson })` calls
  PostHog `/flags?v=2` and returns a JSON result string with the requested flag
  value when present.

**Maintenance**

- `clearAnalytics(ctx, maxRows)` removes a bounded set of outbox and delivery
  rows for operator-controlled resets.
- `posthog_outbox_admin` and `posthog_delivery_log_admin` expose bounded,
  administrator-scoped operational views.

Mounted state exports include `posthogOutbox`, `posthogDeliveryLog`,
`posthogDeliveryStats`, and `OutboxStatus` for host-defined views and operator
workflows.

These mounted operations are admin-only because they can spend provider quota.
Expose product-specific host operations that derive the distinct ID and event or
flag name from authorized application state.

**Reducer-safe queueing**

- `enqueue_event({ distinctId, event, propertiesJson, idempotencyKey })` writes a
  durable event intent inside a reducer transaction. The mounted reducer is
  admin-only; host reducers should call `enqueueEvent` after authorization.

For mounted modules, import `@spacetimedb/posthog/submodule` and call `enqueueEvent(ctx.as.posthog, ...)` from reducers or `captureNow(ctx.as.posthog, ...)` / `flushOutbox(ctx.as.posthog, ...)` from procedures.

The client calls the business operation. Analytics remain a server-side
concern:

```ts
await conn.reducers.completeOrder({ orderId, totalCents });
```

An operator-owned procedure or scheduled workflow should call
`posthog.flushOutbox(ctx.as.posthog, { limit })`. Keep provider credentials and
generic event names inside the module.

Package entrypoints:

- `@spacetimedb/posthog` can run as a standalone analytics database.
- `@spacetimedb/posthog/submodule` supplies mounted state, configuration,
  delivery helpers, and admin views.

## Architecture notes

- **Synchronous HTTP API.** Module procedures call PostHog's HTTP endpoints
  directly through `ctx.http.fetch`.
- **Direct plus outbox.** Immediate capture is useful for important events. The outbox is for reducer-safe transactional queueing and explicit flush.
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

[BUSL-1.1](./LICENSE.txt) - same as SpacetimeDB.
