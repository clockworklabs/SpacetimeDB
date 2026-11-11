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

This example runs an order checkout. Replace `reserveStock`, `chargeCard`,
`releaseStock`, `queueReview`, `markOrder`, `requireOrderOwner`, and
`requireWarehouse` with your application's functions.

### Setting up the module

Add Workflow's tables to your module, make the publisher the first Workflow
admin, and register the procedure that runs workflows:

```ts
import { schema, t } from 'spacetimedb/server';
import { client } from '@spacetimedb/workflow';

const workflows = client();

const db = schema({ order, ...workflows.tables });
export default db;

export const init = db.init(ctx => workflows.install(ctx));
export const workflowRunAttempt = workflows.workflowProcedure(db);

export const cancelWorkflowRun = db.reducer(
  workflows.reducers.cancelWorkflowRun.params,
  workflows.reducers.cancelWorkflowRun.handler
);
```

`workflowProcedure` fails to compile if the schema does not include
`workflows.tables`. Export the admin [reducers and views](#api) you want, like
`cancelWorkflowRun` above.

### Defining a workflow

```ts
const checkout = workflows.define(
  db,
  'checkout',
  {
    args: t.object('CheckoutArgs', { orderId: t.u64() }),
    retry: { maxAttempts: 3, backoffSeconds: 2 },
  },
  (step, { orderId }) => {
    step.tx('reserve', ctx => reserveStock(ctx, orderId));

    let receiptId: string;
    try {
      receiptId = step.run('charge', (ctx, run) =>
        chargeCard(ctx, orderId, run.idempotencyKey)
      );
    } catch (error) {
      step.tx('release', ctx => releaseStock(ctx, orderId));
      throw error;
    }

    const tracking = step.waitForEvent<string>('shipped');
    step.sleep(7 * 24 * 60 * 60);
    step.tx('review', ctx => queueReview(ctx, orderId, tracking));
    return { receiptId };
  },
  (ctx, { args, status, result, error }) => {
    markOrder(ctx, args.orderId, status, result?.receiptId, error);
  }
);
```

`define` takes:

- `db`, so steps receive the host's typed contexts.
- A name. It is saved on every run, and finds the function when the run resumes.
- Options: the argument type, and optionally `retry` and `stepTimeoutSeconds`.
  They apply to every step, unless a step sets its own `retry`.
- The workflow function. It receives `step` and the run's arguments, and does all
  its work in [steps](#steps). Its return value is saved as the run's result.
- Optionally, `onComplete`, which runs once when the run completes, fails, or is
  cancelled. See [When a run ends](#when-a-run-ends).

It returns a handle to pass to `start`. Define workflows when the module loads,
in any file. Do not export a handle from the module's entry file, because
SpacetimeDB rejects exports it does not recognize.

### Starting runs

A run starts only when a host reducer calls `start`. Workflow has no public
reducer for it, because only your application knows who may start a workflow.
Check that in the host reducer, then start the run:

```ts
export const placeOrder = db.reducer(
  { orderId: t.u64() },
  (ctx, { orderId }) => {
    const order = requireOrderOwner(ctx, orderId);
    const runId = workflows.start(ctx, checkout, { orderId });
    ctx.db.order.id.update({ ...order, runId });
  }
);
```

- `start` saves the run and schedules it, and the workflow runs in the
  background. TypeScript checks the arguments against the handle.
- The run is saved in the reducer's transaction. If the reducer fails, no run
  starts.
- Keep the returned id, for example on the row the run is about. You need it to
  send the run events, cancel it, or restart it.

### Sending events and cancelling runs

`waitForEvent` pauses a run until a host reducer delivers its event. Check who may do
that, then call `sendEvent` with the event's value:

```ts
export const markShipped = db.reducer(
  { orderId: t.u64(), tracking: t.string() },
  (ctx, { orderId, tracking }) => {
    const order = requireWarehouse(ctx, orderId);
    workflows.sendEvent(ctx, order.runId, 'shipped', tracking);
  }
);
```

`workflows.cancel(ctx, runId)` stops a run the same way.

See the [Dossier example](https://github.com/clockworklabs/SpacetimeDB/tree/master/modules/spacetime-workflow-ts/example)
for a research agent that keeps its work across restarts.

## Steps

Loops, branches, local variables, and `try/catch` work as usual around these:

- `step.tx(name, (ctx, run) => result)` runs in one transaction. The step's
  writes and its saved result commit together, so they happen exactly once. If it
  throws, its writes roll back. Its function may run again if the commit is
  retried, so it must not call outside services.
- `step.run(name, (ctx, run) => result)` runs in a procedure, so it can make HTTP
  requests and call `ctx.withTx`. It can run more than once, so outside calls must
  be safe to repeat. Pass `run.idempotencyKey`, which stays the same across
  attempts of a step. Writes made through `ctx.withTx` are committed even if the
  step later fails.
- `step.sleep(seconds)` pauses the run. Nothing runs in the meantime.
- `step.waitForEvent(event)` pauses until `sendEvent` delivers `event`, and returns the
  value sent with it. Events that arrive early are kept, oldest first, until the
  run waits for them. Each wait takes one. Pass the expected type, as in
  `step.waitForEvent<string[]>('approved')`. It is not checked.
- `step.parallel(name, items, (ctx, item, run) => result)` runs the function once
  per item, each in its own procedure, at the same time. It waits for all of them
  and returns `{ ok: true, value }` or `{ ok: false, error }` for each item, in
  item order, so one failure does not stop the others. Each item retries and
  recovers like a `run` step, with its own idempotency key. Up to 32 items.

```ts
const pages = step.parallel('fetch', urls, (ctx, url) => fetchPage(ctx, url));
const fetched = pages.flatMap(page => (page.ok ? [page.value] : []));
```

Parallel items share the module's procedure pool, which has one instance per CPU
core by default. Items beyond that wait for a free instance.

`tx`, `run`, and `parallel` take an optional last argument. Its `retry` replaces
the workflow's for that step, so one flaky call can retry without the rest:

```ts
const page = step.run('fetch', ctx => fetchPage(ctx, url), {
  retry: { maxAttempts: 5, backoffSeconds: 2 },
});
```

## When a run ends

`onComplete` receives the run's `runId`, `args`, `status` (`Completed`,
`Failed`, or `Cancelled`), `result`, and `error`. It runs in its own transaction,
with your module's context.

Its writes commit together with the record that it ran, so it takes effect once.
If it throws, its writes roll back and it is retried like a step, using the
workflow's `retry` settings. A run is only pruned from the history after its
`onComplete` has run or given up.

## How a run resumes

A workflow function can stop and run again from the top, after a sleep, an event,
a retry, or a restart. Finished steps do not run again. They return their saved
results, and the function carries on from the first step that has not finished.

That gives the function three rules:

- Do all work in steps. The function has no `ctx`, so it cannot reach the database
  or the network directly.
- Make the same step calls each time. Do not branch on `Date.now()` or
  `Math.random()` outside a step. Get those values from a step, so they are saved.
- Keep step results, arguments, event values, and the returned value small, up
  to 64 KiB. They can hold plain objects, arrays, strings, finite numbers,
  booleans, `null`, `bigint`, `Uint8Array`, and the SpacetimeDB types
  `Timestamp`, `TimeDuration`, `Identity`, `ConnectionId`, and `Uuid`. Each
  comes back as the same type. Any other value, such as a `Map` or a `Date`,
  fails the step with `workflow.unsupported_value`.

A run can have up to 10,000 steps. Each sleep and wait counts as one.

## Failures and restarts

- A step that throws is retried with `backoffSeconds * 2^(attempt - 1)` between
  attempts, up to `retry.maxAttempts`. The step's own `retry` is used if it has
  one, then the workflow's. Without either, a step gets one attempt.
- A thrown `SenderError` means the input is invalid, so the step is not retried.
- When a step gives up, it throws a `WorkflowStepError` in the workflow function.
  Catch it to clean up or carry on. If nothing catches it, the run ends as
  `Failed`.
- If the server stops during an attempt, another attempt starts after
  `stepTimeoutSeconds` (default 300). A `tx` step never committed, so it repeats. A
  `run` step counts this against `maxAttempts`, because its outside calls may have
  happened.
- A `run` step that runs longer than the timeout also gets another attempt. Its
  late result is ignored, but its outside calls still happened, so set the timeout
  above your slowest step.
- A long-running procedure delays other scheduled work in the same module.

## Restarting a run

After you fix what failed a run, `restart` runs it again from a step. Steps
before it keep their saved results, so they do not run again:

```ts
workflows.restart(ctx, runId); // from the step that failed the run
workflows.restart(ctx, runId, 'charge'); // from the first step named 'charge'
workflows.restart(ctx, runId, 0); // from the first step
```

- Only `Failed` and `Cancelled` runs restart, after their `onComplete` has run.
- Without a step, it starts from the step that gave up and failed the run. If the
  function failed outside a step, or the run was cancelled, it continues after the
  last saved step.
- Steps from that point run again, including sleeps, and waits need their events
  again. A sleep is named `sleep`, and a wait is `wait:<event>`.
- `onComplete` runs again when the restarted run ends.

## Reading progress

The `workflowRun` table holds each run's `workflow` name, `status`, `lastStep`,
`stepCount`, `attempt`, `waitingFor`, `error`, `result`, and `startedBy`.
`completing` is true until its `onComplete` has run. A run is `Waiting` while it
waits for an event, or for parallel items, when `waitingFor` is
`parallel:<name>`. Its tables are private, so expose what your users need
through a view:

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
- Changing a workflow's arguments does not change the schema. Active runs keep the
  arguments they started with, so add fields as optional.
- Removing a workflow fails its active runs with `workflow.unknown_workflow`.

## API

- `client()` returns `tables`, `workflowProcedure`, `define`, `install`,
  `start`, `sendEvent`, `cancel`, `restart`, `requireAdmin`, `views`, and
  `reducers`.
- `workflowProcedure(schema)` registers the scheduled procedure that runs
  workflows. Export its result.
- `define(schema, name, { args, retry?, stepTimeoutSeconds? }, fn, onComplete?)`
  adds a workflow and returns its handle. It throws if the name is taken or an
  option is out of range.
- `install(ctx)` seeds the publishing identity as the initial admin. Call it
  from the host's `init` reducer.
- `start(ctx, handle, args)` starts a run and returns its id without an
  authorization check. Call it from host reducers that authorize the caller.
- `sendEvent(ctx, runId, event, value?)` delivers an event and its value without
  an authorization check. It returns `false` if the run is unknown or finished.
- `restart(ctx, runId, from?)` runs a failed or cancelled run again from a step
  without an authorization check. It returns `false` if the run cannot restart.
- `cancel(ctx, runId)` stops an active run without an authorization check. It
  returns `false` if the run already finished. A `run` step already running
  finishes its work, and its result is ignored.
- `reducers.cancelWorkflowRun`, `reducers.restartWorkflowRun`,
  `reducers.addWorkflowAdminIdentity`, and
  `reducers.removeWorkflowAdminIdentity` require a Workflow admin.
- `views.workflowRunsAdmin` and `views.workflowHistoryAdmin` return up to 1,000
  runs and attempts, newest first, to Workflow admins.
- `errors` holds the `workflow.*` codes these operations throw as `SenderError`s:
  `notAuthorized`, `invalidEvent`, `tooManyEvents`, `cannotRemoveLastAdmin`, and
  `unknownStep`.
- `failures` holds the `workflow.*` errors recorded on runs: `stepLost`,
  `tooManySteps`, `resultTooLarge`, `asyncStep`, `replayMismatch`, `tooManyBranches`,
  `unsupportedValue`, and `unknownWorkflow`.

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
