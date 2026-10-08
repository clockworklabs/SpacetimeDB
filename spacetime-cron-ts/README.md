# @spacetimedb/cron

Run recurring tasks in your SpacetimeDB application, such as daily reports,
scheduled cleanup, or periodic data updates. Choose a calendar schedule or a
fixed interval, pass arguments to each job, and inspect recent results.

Schedules are stored in your database. Your application can change or cancel
them at runtime. Calendar schedules support time zones and daylight-saving
changes.

## Requirements

- A SpacetimeDB CLI and server compatible with the host SDK
- The `spacetimedb` npm version required by this package's peer dependency
- Host support for the `spacetime:sys@2.0` volatile procedure used by failure
  recovery
- Node.js 20 or later for package tooling

## Install

```bash
npm install @spacetimedb/cron spacetimedb
```

## Quick start

This module records a report each weekday at 9 AM in New York. Replace the
handler's database write with the work your application needs.

```ts
import { schema, table, t } from 'spacetimedb/server';
import { client, cronTable } from '@spacetimedb/cron';

const dailyReport = cronTable({ name: 'daily_report' });
const cron = client({
  jobs: [dailyReport],
  reconcileEverySeconds: 300,
});

const report = table(
  { name: 'report', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    generatedAt: t.timestamp(),
  }
);

const spacetimedb = schema({ ...cron.tables, report });
export default spacetimedb;

export const generateReport = dailyReport.cronReducer(
  spacetimedb,
  (ctx, invocation) => {
    ctx.db.report.insert({
      id: 0n,
      generatedAt: invocation.scheduledFor,
    });
  }
);

export const cronReconcile = cron.reconcileReducer(spacetimedb);
export const { jobs: cronJobs } = cron.publicViews(spacetimedb);

export const init = spacetimedb.init(ctx => {
  cron.schedule(ctx, dailyReport, '0 9 * * 1-5', {
    timezone: 'America/New_York',
    maxFailures: 3,
  });
});
```

`init` creates the schedule for a new database. Later module publishes keep
schedule changes made at runtime. To run at a fixed interval instead, pass
`{ everySeconds: 30 }` in place of the cron expression.

The repair sweep checks every five minutes for jobs that lost their next
scheduled run. Failure recovery is best effort; see [Execution model](#execution-model)
for crash behavior. `maxFailures: 3` disables the job after three consecutive
recorded failures. The `cronJobs` view lets clients subscribe to job status.

See the [browser example](./example/) to change schedules and try a cleanup
job with arguments.

## API

### Registering jobs

Create one handle for each statically known job:

```ts
const cleanup = cronTable({ name: 'cleanup' });
```

Declare a SpacetimeDB type when a job needs durable arguments:

```ts
const archiveWorkspace = cronTable({
  name: 'archive_workspace',
  args: t.object('ArchiveWorkspaceCronArgs', {
    workspaceId: t.u64(),
    retainDays: t.u32(),
  }),
});
```

The handle carries the inferred argument type through `cron.schedule()`,
`cronReducer()`, and `cronProcedure()`. The argument builder may be any
SpacetimeDB type, although a named `t.object()` gives most jobs the clearest
call site and database schema.

Job names use lowercase snake_case and may contain up to 48 characters. Pass
every handle to one `client()` call when the module loads, then register
exactly one reducer or procedure for each handle after `schema()`.
`cron.schedule()` rejects a configuration with a missing handler. When
`reconcileEverySeconds` is configured, export `cron.reconcileReducer()`.
Applications that expose cron status export the `jobs` view returned by
`cron.publicViews()`.

Use `cronReducer` for deterministic database work. The handler receives the
host module's reducer context, inferred from `spacetimedb`, and a
`CronInvocation`. Registration fails to compile if the schema does not include
`cron.tables`:

```ts
export const runCleanup = cleanup.cronReducer(
  spacetimedb,
  (ctx, invocation) => {
    console.log(invocation.id);
    // Database writes commit together when the handler succeeds.
  }
);
```

Use `cronProcedure` for HTTP requests and other procedure capabilities:

```ts
export const syncRemote = remoteSync.cronProcedure(
  spacetimedb,
  (ctx, invocation) => {
    sendRequest({ idempotencyKey: invocation.id });
  }
);
```

Handlers complete synchronously. SpacetimeDB procedure APIs, including `ctx.http.fetch` and `ctx.withTx`, expose synchronous module calls.

Argument-bearing handlers receive their typed payload before the invocation
metadata:

```ts
export const runArchive = archiveWorkspace.cronReducer(
  spacetimedb,
  (ctx, args, invocation) => {
    archiveRows(ctx, args.workspaceId, args.retainDays);
    console.log(invocation.id);
  }
);
```

### Scheduling and cancellation

`cron.schedule()` first repairs any enabled jobs with missing triggers. It then
creates or replaces the requested schedule, clears failure state, increments
the job generation, and enables the job.

```ts
cron.schedule(ctx, cleanup, '30 2 * * *', { timezone: 'UTC' });
cron.schedule(ctx, cleanup, { everySeconds: 300 });

cron.schedule(ctx, archiveWorkspace, '0 3 * * *', {
  timezone: 'UTC',
  args: { workspaceId: 42n, retainDays: 90 },
});
```

Arguments are required when scheduling an argument-bearing job. Rescheduling
replaces the schedule, arguments, generation, and pending fire atomically. A
handler receives the argument value read at the start of its fire.

Cron expressions accept five fields or six fields when seconds are included. Fixed intervals accept whole seconds from 1 through 31,536,000.

`cron.unschedule()` runs the same opportunistic repair before removing the
target fire, incrementing its generation, and leaving job and history rows
available for inspection.

```ts
cron.unschedule(ctx, cleanup);
```

Application reducers must authorize schedule changes.

Input errors from `cron.schedule()` are thrown as `SenderError`s. `errors`
holds the codes a caller can receive at runtime, for example
`errors.invalidExpression` (`cron.invalid_expression`). Codes may be followed
by `:` and detail text.

- `notAuthorized`: a client called a `<job_name>_cron` function directly.
- `invalidMaxFailures`, `invalidInterval`, `invalidExpression`,
  `invalidTimezone`, `unsatisfiableExpression`, `missingArgs`, and
  `unexpectedArgs`: `cron.schedule()` rejected its input.
- `invalidScheduleState`, `invalidArgsState`, `invalidTrigger`, and
  `noFutureOccurrence`: stored in the private `cron_job.disabledReason` when
  cron disables a job.

Programming errors, such as invalid job definitions or client configuration,
missing handler registration, a job handle from another client, or a handler
that returns a promise, fail module loading or the call with `cron.*` codes
that are not exported.

### Database state

The package adds these shared tables:

| Table      | Purpose                                                             |
| ---------- | ------------------------------------------------------------------- |
| `cron_job` | Private schedule, typed arguments, generation, health, and next run |
| `cron_run` | Completed invocation identity, outcome, and bounded history         |

When `reconcileEverySeconds` is set, the package also adds
`cron_reconcile_tick`. It contains one native interval row that periodically
repairs enabled jobs with missing triggers.

Each job receives one `<job_name>_fire` schedule table bound directly to its
`<job_name>_cron` reducer or procedure. An enabled job owns exactly one row in
that table. Calendar jobs replace one-shot rows after each fire. Fixed-rate
jobs retain one native interval row.

The package owns the shared names above, the public view name `cron_jobs`, every
`<job_name>_fire` table, and every `<job_name>_cron` scheduled function.
Enabling periodic reconciliation also reserves `cron_reconcile` and
`cron_reconcile_tick`. Consumer modules should keep those database function,
table, and view names available for cron.

`cron_job` is always private. Registering `cron.publicViews()` exposes
`cron_jobs`, a subscribable projection of job state that omits typed arguments
and detailed failure text. Its optional `disabledReason` is one of
`disabled_by_operator`, `failure_threshold_reached`,
`lost_fire_threshold_reached`, `invalid_schedule_state`, or `disabled`.
`publicTables` defaults to `false`; enabling it exposes each per-job fire table
and `cron_run`, including run error details. The fire-table `recovery` column is
internal. Stored schedule rows leave it empty.

For calendar jobs, `cron_jobs.nextRunAt` and the job's fire-table `targetAt`
identify the logical next occurrence. For native interval jobs, `nextRunAt` is
an estimate based on the most recent fire. The database owner and module
reducers can inspect the private `cron_job` table directly.

### Execution model

Each fire table is bound directly to one statically registered handler.

For reducer jobs, the middleware rearms a calendar schedule before calling the
handler. On success, the successor, application writes, job health, and run
record commit in one transaction. On failure, the middleware serializes the
fire row with an internal recovery payload, schedules the same
`<job_name>_cron` reducer through
`volatile_nonatomic_schedule_immediate`, and rethrows. The fire transaction
rolls back, including partial application writes. The recovery invocation runs
in a fresh transaction. It validates the database caller, generation, and
sequence before it records the failure. Calendar recovery replaces the pending
fire. Native interval rows persist, so interval recovery keeps that row and
updates job health.

Recovery calls do not run the application handler.

The volatile call is best effort and is not persisted. A process crash,
uncatchable trap, or lost message can temporarily leave an enabled calendar job
without a fire. The package detects that broken invariant during every
`cron.schedule()` and `cron.unschedule()` operation. Set
`reconcileEverySeconds` to add a low-frequency native interval sweep:

```ts
const cron = client({ jobs, reconcileEverySeconds: 300 });

// Export after registering the job handlers.
export const cronReconcile = cron.reconcileReducer(spacetimedb);
```

Repair removes any stale trigger, inserts a valid current-generation trigger,
and records one `Failed` run with error `lost_fire`. The normal failure counter,
history cap, and automatic disable policy apply. Without the optional sweep,
repair occurs on the next management operation. With it, detection is bounded
by the configured interval and scheduler availability.

Procedure jobs secure the next calendar fire in a committed transaction, run
the procedure work, then record the outcome in another transaction. A process
failure during external work can lose the run record, but it does not remove
the next calendar fire. `CronInvocation.id` is stable and should be used as an
external idempotency key.

`maxFailures` counts consecutive recorded failures. A positive threshold
disables the job and stores the reason. A successful invocation resets the
counter.

### Argument schema changes

Job names and argument builders are part of the module's database schema.
Adding a new job adds a new internal union variant. Changing the argument
builder for an existing job requires a SpacetimeDB schema migration. For an
incompatible payload change, a new job name provides a clean version boundary.

### Scheduling behavior

- The next calendar occurrence is computed strictly after the dispatch timestamp.
- An overdue one-shot trigger produces one catch-up invocation. Intermediate missed occurrences are skipped.
- Spring-forward and fall-back behavior follows `cron-parser` 5.x.
- Sparse expressions use internal checkpoint triggers so valid occurrences beyond the host timer horizon remain scheduled.
- Fixed intervals use SpacetimeDB native `ScheduleAt.interval` rows.

A long-running procedure delays other scheduled work in the same module.

### Run history

`historyCap` defaults to five completed records per job and accepts values from
0 through 1,000.

Run statuses are:

- `Ok`: the handler completed without throwing
- `Failed`: the handler threw, or reconciliation recorded a lost fire

### Parser exports

```ts
import {
  isValidTimezone,
  nextFireAfter,
  parseCronExpression,
} from '@spacetimedb/cron/parser';
```

## Testing

```bash
pnpm test
pnpm lint
pnpm typecheck
pnpm run test:recovery
pnpm run test:module:local
```

`test:module:local` requires a running local SpacetimeDB server.
`test:recovery` tests scheduled work across a host restart.

## License

Apache-2.0. See [`LICENSE.txt`](./LICENSE.txt).
