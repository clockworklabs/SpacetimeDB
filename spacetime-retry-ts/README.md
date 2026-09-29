# @spacetimedb/retry

Typed retries for SpacetimeDB TypeScript modules. `client()` creates a private
scheduled-task table, attempt history, admin controls, and exponential-backoff
dispatch around handlers defined by the host module.

## Install

```bash
npm install @spacetimedb/retry spacetimedb
```

The tables and reducers are registered in the host schema. Retry does not
mount a separate submodule schema.

`spacetimedb` is a peer dependency. Keep its version aligned with the SDK used
to build the host module.

For the install-to-publish workflow, see
[Getting started](https://spacetimedb.com/docs/).

## Usage

### Integrate into an application

The task variants and handlers belong to the host, so `client()` runs when the
module loads, before `schema()`. The example below is a module-definition
skeleton: replace `sendReceipt` with an idempotent application handler.

```ts
import { schema, t } from 'spacetimedb/server';
import { client, retryFailed, retryHandler, retryOk } from '@spacetimedb/retry';

const retry = client({
  handlers: {
    sendReceipt: retryHandler(
      t.object('SendReceiptArgs', { orderId: t.u64() }),
      (ctx, { orderId }) => {
        const result = sendReceipt(ctx, orderId);
        return result.sent ? retryOk() : retryFailed(result.error);
      }
    ),
  },
});

const db = schema({ ...retry.tables });
export default db;

export const init = db.init(ctx => retry.install(ctx));

export const retryFire = db.reducer(
  { onSchedule: retry.tables.retryTask },
  { arg: retry.tables.retryTask.rowType },
  retry.reducers.retryFire
);

export const submitRetryTask = db.reducer(
  retry.reducers.submitRetryTask.params,
  retry.reducers.submitRetryTask.handler
);
```

Register the scheduled reducer after `schema()` so it can reference the
scheduled table. Handlers receive the host's reducer context as `unknown`
because it is defined after the handlers.

Submit tagged arguments with an attempt cap and base backoff. The first attempt
is scheduled immediately; subsequent delays are `backoffSecs * 2^attempt`.

Handlers must be idempotent. A returned failure or a thrown exception records a
failed attempt and schedules the next attempt, up to `maxAttempts`. Writes made
by the handler before a failure are committed with that attempt; Retry does not
provide a separate transaction for the handler. A host crash or transaction
abort can still prevent the retry from being scheduled.

History retains the latest 1,000 attempts across all tasks. The admin history
view returns these attempts newest first.

## API

- `client({ handlers })` returns `tables`, `install`, `submit`,
  `requireAdmin`, `views`, and `reducers`.
- `install(ctx)` seeds the publishing identity as the initial admin. Call it
  from the host's `init` reducer.
- `submit(ctx, task)` validates a task and schedules its first attempt without
  an authorization check. Call it from host reducers that authorize the caller.
- `reducers.submitRetryTask`, `reducers.addRetryAdminIdentity`, and
  `reducers.removeRetryAdminIdentity` require a Retry admin.
- `views.retryTasksAdmin` and `views.retryHistoryAdmin` return up to 1,000
  pending tasks and attempts, newest first, to Retry admins.
- `retryHandler(args, run)` pairs a SpacetimeDB type builder with a task
  handler. The handler returns `retryOk()` or `retryFailed(error)`.
- `errors` holds the `retry.*` codes thrown by these operations.

The generated client can submit a task when the host exports
`submitRetryTask`:

```ts
await conn.reducers.submitRetryTask({
  name: `receipt:${orderId}`,
  args: { tag: 'SendReceipt', value: { orderId } },
  maxAttempts: 5,
  backoffSecs: 2,
});
```

Product-facing applications usually expose a narrower reducer that authorizes
the caller, fixes the retry limits, and calls `retry.submit`:

```ts
export const requestReceipt = db.reducer(
  { orderId: t.u64() },
  (ctx, { orderId }) => {
    requireOrderOwner(ctx, orderId);
    retry.submit(ctx, {
      name: `receipt:${orderId}`,
      args: { tag: 'sendReceipt', value: { orderId } },
      maxAttempts: 5,
      backoffSecs: 2,
    });
  }
);
```

Operational screens can subscribe to the admin task and history views.

Package entrypoints:

- `@spacetimedb/retry` exports `client`, `errors`, and the handler and result
  helpers.
- `@spacetimedb/retry/submodule` exports `client` and `errors`.

## Testing

```bash
pnpm test
pnpm run lint
pnpm --dir spacetimedb run build
```

The build compiles the fixture module that registers Retry's tables and reducers.

## License

Apache-2.0. See [`LICENSE.txt`](./LICENSE.txt).
