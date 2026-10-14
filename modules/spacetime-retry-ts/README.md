# @spacetimedb/retry

Retry failed background tasks in your SpacetimeDB application. Pass arguments
to a task, set an attempt limit, and choose how long to wait between attempts.
Each retry waits longer than the last, and administrators can inspect pending
tasks and past attempts.

## Install

```bash
npm install @spacetimedb/retry spacetimedb
```

`spacetimedb` is a peer dependency. Keep its version aligned with the SDK used
to build the host module.

## Integrate into an application

Define the task's arguments, add Retry's tables to your module, then register
the function that does the work. In this example, replace `sendReceipt` with
your application's function. It must be safe to run more than once for the
same order.

```ts
import { schema, t } from 'spacetimedb/server';
import { client, retryFailed, retryOk } from '@spacetimedb/retry';

const retry = client({
  tasks: {
    sendReceipt: t.object('SendReceiptArgs', { orderId: t.u64() }),
  },
});

const db = schema({ ...retry.tables });
export default db;

export const init = db.init(ctx => retry.install(ctx));

export const retryFire = retry.retryReducer(db, {
  sendReceipt(ctx, { orderId }) {
    const result = sendReceipt(ctx, orderId);
    return result.sent ? retryOk() : retryFailed(result.error);
  },
});

export const submitRetryTask = db.reducer(
  retry.reducers.submitRetryTask.params,
  retry.reducers.submitRetryTask.handler
);
```

`retryReducer` requires a handler for every task. Registration fails to
compile if the schema does not include `retry.tables`.

Submit tagged arguments with an attempt cap and base backoff. The first attempt
is scheduled immediately; subsequent delays are `backoffSecs * 2^attempt`.

Handlers must be idempotent. A returned failure or a thrown exception records a
failed attempt and schedules the next attempt, up to `maxAttempts`. A thrown
`SenderError` marks the task's arguments as invalid: Retry records the attempt
as `GaveUp` with the error message and schedules nothing further. Writes made
by the handler before a failure are committed with that attempt; Retry does not
provide a separate transaction for the handler. A host crash or transaction
abort can still prevent the retry from being scheduled.

History retains the latest 1,000 attempts across all tasks. The admin history
view returns these attempts newest first.

## API

- `client({ tasks })` returns `tables`, `retryReducer`, `install`, `submit`,
  `requireAdmin`, `views`, and `reducers`.
- `retryReducer(schema, handlers)` registers the scheduled reducer that runs
  attempts. Export its result. Each handler returns `retryOk()` or
  `retryFailed(error)`.
- `install(ctx)` seeds the publishing identity as the initial admin. Call it
  from the host's `init` reducer.
- `submit(ctx, task)` validates a task and schedules its first attempt without
  an authorization check. Call it from host reducers that authorize the caller.
- `reducers.submitRetryTask`, `reducers.addRetryAdminIdentity`, and
  `reducers.removeRetryAdminIdentity` require a Retry admin.
- `views.retryTasksAdmin` and `views.retryHistoryAdmin` return up to 1,000
  pending tasks and attempts, newest first, to Retry admins.
- `errors` holds the `retry.*` codes these operations throw as `SenderError`s:
  `notAuthorized`, `invalidTaskName`, `invalidMaxAttempts`,
  `invalidBackoffSeconds`, `taskAlreadyExists` (followed by `:` and the task
  name), and `cannotRemoveLastAdmin`.

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

For user-facing operations, check permissions and set retry limits in a host
reducer:

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

Package entrypoints:

- `@spacetimedb/retry` exports `client`, `errors`, `retryOk`, `retryFailed`,
  and the related types.
- `@spacetimedb/retry/submodule` exports `client` and `errors`.

## Testing

```bash
pnpm test
pnpm run lint
pnpm --dir spacetimedb run build
```

## License

Apache-2.0. See [`LICENSE.txt`](./LICENSE.txt).
