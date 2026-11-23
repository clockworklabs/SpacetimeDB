# @spacetimedb/retry

Retry failed background tasks in your SpacetimeDB application. Pass arguments
to a task, set an attempt limit, and choose how long to wait between attempts.
Each retry waits longer than the last, and administrators can inspect pending
tasks and past attempts.

## Install

Run this in your SpacetimeDB TypeScript module directory:

```bash
npm install @spacetimedb/retry
```

Requires SpacetimeDB SDK, CLI, and server 2.12 or later.

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

## Jobs that call outside services

A reducer can't make HTTP calls. For work like sending an email, declare
`jobs` and register `retryProcedure`. Each attempt runs in a procedure outside
a transaction, so the handler can call `ctx.http.fetch`.

```ts
const retry = client({
  jobs: {
    sendEmail: t.object('SendEmailArgs', { emailId: t.u64() }),
  },
});

export const retryRunJob = retry.retryProcedure(
  db,
  {
    sendEmail(ctx, { emailId }, job) {
      const response = ctx.http.fetch(url, {
        method: 'POST',
        headers: { 'Idempotency-Key': job.idempotencyKey },
        body: emailBody(ctx, emailId),
      });
      return response.ok ? retryOk() : retryFailed(`status ${response.status}`);
    },
  },
  {
    onComplete(tx, outcome) {
      // Runs in the transaction that finishes the job.
    },
  }
);

export const sendWelcome = db.reducer({ emailId: t.u64() }, (ctx, args) => {
  retry.enqueue(ctx, {
    name: `email:${args.emailId}`,
    args: { tag: 'sendEmail', value: args },
    maxAttempts: 5,
    backoffSecs: 2,
    queue: 'email',
  });
});
```

- `enqueue` writes the job in the caller's transaction, so the job exists only
  if that transaction commits. It returns the job's ID. Set `runAt` to start
  later.
- A failed attempt waits `backoffSecs * 2^(attempt - 1)` seconds, plus or minus
  25%, before the next one.
- An attempt that hasn't recorded a result after `leaseSecs` (set in `client`,
  default 300) counts as failed with `retry.lease_expired`, and the job is
  retried. This covers a host that stops mid-attempt. Set `leaseSecs` above
  your slowest handler. The late result of a timed-out attempt is dropped, so
  send `job.idempotencyKey` to services that accept one.
- `setQueue(ctx, name, { maxParallelism })` limits how many jobs in a queue run
  at once. 0 pauses the queue. Jobs that come due while the queue is full wait
  and start oldest first when there is room. Queues without a limit run up to
  1,000 jobs at once.
- `cancel(ctx, id)` stops a job. A running attempt keeps going, but its result
  is dropped.
- `onComplete` runs for jobs that succeed, fail, or are cancelled. A throw rolls
  back that transaction.
- `status(ctx, id)` returns the job's row. The latest 1,000 jobs of each final
  status are kept.

A submodule can mount its own copy of Retry by spreading `retry.tables` into
its schema. Each copy's tables and queues are separate.

## API

- `client({ tasks, jobs, leaseSecs })` returns `tables`, `retryReducer`,
  `retryProcedure`, `install`, `submit`, `enqueue`, `cancel`, `status`,
  `setQueue`, `requireAdmin`, `views`, and `reducers`. `tasks` and `jobs` are
  both optional.
- `retryReducer(schema, handlers)` registers the scheduled reducer that runs
  attempts. Export its result. Each handler returns `retryOk()` or
  `retryFailed(error)`.
- `install(ctx)` seeds the publishing identity as the initial admin. Call it
  from the host's `init` reducer.
- `submit(ctx, task)` validates a task and schedules its first attempt without
  an authorization check. Call it from host reducers that authorize the caller.
- `retryProcedure(schema, handlers, options)`, `enqueue`, `cancel`, `status`,
  and `setQueue` are described in Jobs that call outside services. `enqueue`,
  `cancel` and `setQueue` don't check authorization.
- `reducers.submitRetryTask`, `reducers.cancelRetryJob`,
  `reducers.setRetryQueue`, `reducers.addRetryAdminIdentity`, and
  `reducers.removeRetryAdminIdentity` require a Retry admin.
- `views.retryTasksAdmin`, `views.retryJobsAdmin` and `views.retryHistoryAdmin`
  return up to 1,000 pending tasks, jobs, and attempts, newest first, to Retry
  admins.
- `errors` holds the `retry.*` codes these operations throw as `SenderError`s:
  `notAuthorized`, `invalidTaskName`, `invalidQueueName`, `invalidMaxAttempts`,
  `invalidBackoffSeconds`, `invalidParallelism`, `taskAlreadyExists` (followed
  by `:` and the task or job name), and `cannotRemoveLastAdmin`.
  `leaseExpired` is recorded as an attempt's error, not thrown.

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

Apache-2.0. See the [license](https://github.com/clockworklabs/SpacetimeDB/blob/master/licenses/apache2.txt).
