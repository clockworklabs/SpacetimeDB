# @spacetimedb/retry

A typed retry factory for SpacetimeDB TypeScript modules. It creates a private
scheduled-task table, attempt history, admin controls, and exponential-backoff
dispatch around handlers defined by the host module.

## Install

```bash
npm install @spacetimedb/retry spacetimedb@^2.8.3
```

Requires SpacetimeDB 2.8.3 or later for submodule mounting.

`spacetimedb` is a peer dependency. Keep its version aligned with the SDK used
to build the host module.

For the install-to-publish workflow, see
[Getting started](https://spacetimedb.com/docs/).

## Usage

### Integrate into an application

Retry is a factory because task variants and handlers belong to the host. The
example below is a module-definition skeleton: replace `sendReceipt` with an
idempotent application handler. Keep the registration casts at this SDK/factory
boundary; application code stays typed through the handler map.

Create the factory before the schema so its tables can be mounted. Register the
scheduled reducer afterward to resolve the scheduled-table reference.

```ts
import { SenderError, schema, t, table } from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';
import {
  createRetrySubmodule,
  retryFailed,
  retryHandler,
  retryOk,
} from '@spacetimedb/retry';

const retry = createRetrySubmodule(
  { table, t, SenderError, ScheduleAt },
  {
    send_receipt: retryHandler(
      t.object('SendReceiptArgs', { orderId: t.u64() }),
      (ctx, { orderId }) => {
        const result = sendReceipt(ctx, orderId);
        return result.sent ? retryOk() : retryFailed(result.error);
      }
    ),
  }
);

const db = schema({ ...retry.tables });
const retryFire = db.reducer(
  { onSchedule: retry.tables.retryTask },
  { arg: retry.tables.retryTask.rowType },
  retry.reducers.retryFire
);

export const submitRetryTask = db.reducer(
  retry.reducers.submitRetryTask.params,
  retry.reducers.submitRetryTask.handler
);

export const init = db.init(ctx => retry.installRetry(ctx));
export default db;
```

Submit tagged arguments with an attempt cap and base backoff. The first attempt
is scheduled immediately; subsequent delays are `backoffSecs * 2^attempt`.

## API

- `retryHandler(args, run)` associates a SpacetimeDB type builder with a task
  handler. The handler returns `retryOk()` or `retryFailed(error)`.
- `makeRetryDispatch(handlers)` creates a typed tagged-union dispatcher.
- `createRetrySubmodule(deps, handlers, auth?)` returns tables, enum helpers,
  reducers, admin views, and installation.
- `installRetry(ctx)` seeds the publishing identity as the initial admin.

The generated client can submit a task when the host exports
`submitRetryTask`. The default factory authorization restricts this operation
to Retry administrators:

```ts
await conn.reducers.submitRetryTask({
  name: `receipt:${orderId}`,
  args: { tag: 'send_receipt', value: { orderId } },
  maxAttempts: 5,
  backoffSecs: 2,
});
```

Product-facing applications usually expose a narrower reducer with fixed retry
limits and arguments derived from authorized application state. Operational
screens can subscribe to the factory's admin task and history views.

Package entrypoints:

- `@spacetimedb/retry/submodule` exports `createRetrySubmodule`.
- `@spacetimedb/retry/kit` exports handler, dispatch, and result helpers.
- `@spacetimedb/retry` re-exports the supported public surface.

The helpers are available from the package root and `./kit`; the complete
factory is available from `./submodule`.

## Testing

```bash
pnpm test
pnpm run lint
pnpm run build
```

The repository build compiles the local fixture module that mounts the factory.

## License

BUSL-1.1. See [`LICENSE.txt`](./LICENSE.txt).
