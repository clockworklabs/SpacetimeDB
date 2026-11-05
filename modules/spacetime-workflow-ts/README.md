# @spacetimedb/workflow

Write long background jobs as one function in your SpacetimeDB application. A
run survives restarts, can sleep or wait for an event, and retries failed steps.
Its progress lives in a table your application can read.

## Install

Run this in your SpacetimeDB TypeScript module directory:

```bash
npm install @spacetimedb/workflow
```

Requires SpacetimeDB SDK, CLI, and server 2.12 or later.

## Integrate into an application

Declare each workflow's arguments, add the workflow tables to your module, then
register a function for each workflow. In this example, replace `reserveStock`,
`chargeCard`, `releaseStock`, `queueReceipt`, `queueReview`,
`requireOrderOwner`, and `requireWarehouse` with your application's functions.

```ts
import { schema, t } from 'spacetimedb/server';
import { client } from '@spacetimedb/workflow';

const workflows = client({
  workflows: {
    checkout: {
      args: t.object('CheckoutArgs', { orderId: t.u64() }),
      retry: { maxAttempts: 3, backoffSeconds: 2 },
    },
  },
});

const db = schema({ ...workflows.tables });
export default db;

export const init = db.init(ctx => workflows.install(ctx));

export const workflowRunAttempt = workflows.register(db, {
  checkout(step, { orderId }) {
    step.tx('reserve', ctx => reserveStock(ctx, orderId));

    let receiptId: string;
    try {
      receiptId = step.call('charge', (ctx, run) =>
        chargeCard(ctx, orderId, run.idempotencyKey)
      );
    } catch (error) {
      step.tx('release', ctx => releaseStock(ctx, orderId));
      throw error;
    }

    step.waitFor('shipped');
    step.tx('receipt', ctx => queueReceipt(ctx, receiptId));

    step.sleep(7 * 24 * 60 * 60);
    step.tx('review', ctx => queueReview(ctx, orderId));
  },
});

export const placeOrder = db.reducer({ orderId: t.u64() }, (ctx, args) => {
  requireOrderOwner(ctx, args.orderId);
  workflows.start(ctx, 'checkout', args);
});

export const markShipped = db.reducer({ runId: t.u64() }, (ctx, { runId }) => {
  requireWarehouse(ctx);
  workflows.signal(ctx, runId, 'shipped');
});
```

See the [Dossier example](https://github.com/clockworklabs/SpacetimeDB/tree/master/modules/spacetime-workflow-ts/example)
for a research agent that keeps its work across restarts.

## Steps

Loops, branches, local variables, and `try/catch` work as usual around these:

- `step.tx(name, (ctx, run) => result)` runs in one transaction. The step's
  writes and its saved result commit together, so they happen exactly once. If it
  throws, its writes roll back. Its function may run again if the commit is
  retried, so it must not call outside services.
- `step.call(name, (ctx, run) => result)` runs in a procedure, so it can make HTTP
  requests and call `ctx.withTx`. It can run more than once, so outside calls must
  be safe to repeat. Pass `run.idempotencyKey`, which stays the same across
  attempts of a step. Writes made through `ctx.withTx` are committed even if the
  step later fails.
- `step.sleep(seconds)` pauses the run. Nothing runs in the meantime.
- `step.waitFor(event)` pauses until `signal` delivers `event`. An event that
  arrives early is kept until the run waits for it.
- `step.parallel(name, items, (ctx, item, run) => result)` runs the function once
  per item, each in its own procedure, at the same time. It waits for all of them
  and returns `{ ok: true, value }` or `{ ok: false, error }` for each item, in
  item order, so one failure does not stop the others. Each item retries and
  recovers like a `call` step, with its own idempotency key. Up to 32 items.

```ts
const pages = step.parallel('fetch', urls, (ctx, url) => fetchPage(ctx, url));
const fetched = pages.flatMap(page => (page.ok ? [page.value] : []));
```

Parallel items share the module's procedure pool, which has one instance per CPU
core by default. Items beyond that wait for a free instance.

## How a run resumes

A workflow function can stop and run again from the top, after a sleep, an event,
a retry, or a restart. Finished steps do not run again. They return their saved
results, and the function carries on from the first step that has not finished.
While nothing pauses, the function simply keeps running.

That gives the function three rules:

- Do all work in steps. The function has no `ctx`, so it cannot reach the database
  or the network directly.
- Make the same step calls each time. Do not branch on `Date.now()` or
  `Math.random()` outside a step. Get those values from a step, so they are saved.
- Keep step results small and JSON-compatible, up to 64 KiB. `bigint` is
  supported. Other classes, such as `Timestamp`, come back as plain objects.

A run can have up to 10,000 steps. Each sleep and wait counts as one.

## Failures and restarts

- A step that throws is retried with `backoffSeconds * 2^(attempt - 1)` between
  attempts, up to `retry.maxAttempts`. Without `retry`, a step gets one attempt.
- A thrown `SenderError` means the input is invalid, so the step is not retried.
- When a step gives up, it throws a `WorkflowStepError` in the workflow function.
  Catch it to clean up or carry on. If nothing catches it, the run ends as
  `Failed`.
- If the host stops during an attempt, another attempt starts after
  `stepTimeoutSeconds` (default 300). A `tx` step never committed, so it repeats. A
  `call` step counts this against `maxAttempts`, because its outside calls may have
  happened.
- A `call` step that runs longer than the timeout also gets another attempt. Its
  late result is ignored, but its outside calls still happened, so set the timeout
  above your slowest step.
- A long-running procedure delays other scheduled work in the same module.

## Reading progress

The `workflowRun` table holds each run's `status`, `lastStep`, `stepCount`,
`attempt`, `waitingFor`, `error`, and `startedBy`. A run is `Waiting` while it
waits for an event, or for parallel items, when `waitingFor` is
`parallel:<name>`. Its tables are private, so expose what your users need through
a view:

```ts
export const myRuns = db.view(
  { name: 'my_runs', public: true },
  t.array(workflows.tables.workflowRun.rowType),
  ctx => [...ctx.db.workflowRun.startedBy.filter(ctx.sender)]
);
```

The latest 1,000 finished runs and the latest 1,000 step attempts are kept. Older
ones are removed.

## Changing a workflow

A new version of a function has to replay the steps that active runs already
finished:

- Adding steps after a point runs have passed is safe.
- Renaming, removing, or reordering finished steps fails those runs with
  `workflow.replay_mismatch`. For an incompatible change, add a workflow under a
  new name and keep the old one until its runs finish.
- Changing an argument type requires a SpacetimeDB schema migration.

## API

- `client({ workflows })` returns `tables`, `register`, `install`, `start`,
  `signal`, `cancel`, `requireAdmin`, `views`, and `reducers`.
- `register(schema, handlers)` registers the scheduled procedure that runs
  workflows. Export its result.
- `install(ctx)` seeds the publishing identity as the initial admin. Call it from
  the host's `init` reducer.
- `start(ctx, name, args)` starts a run and returns its id, without an
  authorization check.
- `signal(ctx, runId, event)` delivers an event, without an authorization check.
  It returns `false` if the run is unknown or finished.
- `cancel(ctx, runId)` stops an active run, without an authorization check. It
  returns `false` if the run already finished. A `call` step already running
  finishes its work, and its result is ignored.
- `reducers.cancelWorkflowRun`, `reducers.addWorkflowAdminIdentity`, and
  `reducers.removeWorkflowAdminIdentity` require a workflow admin.
- `views.workflowRunsAdmin` and `views.workflowHistoryAdmin` return up to 1,000
  runs and attempts, newest first, to workflow admins.
- `errors` holds the `workflow.*` codes these operations throw as `SenderError`s:
  `notAuthorized`, `invalidEvent`, `tooManyEvents`, and `cannotRemoveLastAdmin`.
- `failures` holds the `workflow.*` errors recorded on runs: `stepLost`,
  `tooManySteps`, `resultTooLarge`, `asyncStep`, `replayMismatch`, and `tooManyBranches`.

Package entrypoints:

- `@spacetimedb/workflow` exports `client`, `errors`, `failures`,
  `WorkflowStepError`, and the related types.
- `@spacetimedb/workflow/submodule` exports `client`, `errors`, `failures`, and
  `WorkflowStepError`.

## Testing

```bash
pnpm test
pnpm run lint
pnpm --dir spacetimedb run build
pnpm run test:recovery
```

`test:recovery` runs real workflows on its own local server, and kills that server
in the middle of a step.

## License

Apache-2.0. See the [license](https://github.com/clockworklabs/SpacetimeDB/blob/master/licenses/apache2.txt).
