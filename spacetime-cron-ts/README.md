# @spacetimedb/cron

Calendar and interval scheduling for SpacetimeDB TypeScript modules.

Each job has stable database state, one per-job schedule table, a statically
registered handler, typed arguments, and bounded run history. Calendar
schedules support IANA time zones and daylight-saving transitions. Reducer
failures use SpacetimeDB's temporary volatile recovery mechanism until nested
transactions are available.

## Requirements

- SpacetimeDB CLI 2.8.3
- `spacetimedb` npm package 2.8.3
- Host support for the `spacetime:sys@2.0` volatile procedure used by failure
  recovery
- Node.js 20 or later for package tooling

## Install

```bash
npm install @spacetimedb/cron spacetimedb@^2.8.3
```

`spacetimedb` is a peer dependency. Use the same SDK version for the application module and this package.

For the complete install, build, and publish workflow, see the repository's
[Getting started guide](https://spacetimedb.com/docs/).

## Usage

### Integrate into an application

```ts
import {
  SenderError,
  schema,
  table,
  t,
  toCamelCase,
  type InferSchema,
  type ProcedureCtx,
  type ReducerCtx,
} from 'spacetimedb/server';
import { ScheduleAt, Timestamp } from 'spacetimedb';
import { spacetimeCron } from '@spacetimedb/cron';

const { cronTable, createCron, schedule, unschedule } = spacetimeCron({
  table,
  t,
  toCamelCase,
  ScheduleAt,
  Timestamp,
  SenderError,
});

const dailyReport = cronTable({
  name: 'daily_report',
  args: t.object('DailyReportCronArgs', {
    workspaceId: t.u64(),
    format: t.string(),
  }),
});
const heartbeat = cronTable({ name: 'heartbeat' });
const refreshCatalog = cronTable({ name: 'refresh_catalog' });

const cron = createCron([dailyReport, heartbeat, refreshCatalog], {
  publicTables: true,
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

type Schema = InferSchema<typeof spacetimedb>;
type Tx = ReducerCtx<Schema>;
type Proc = ProcedureCtx<Schema>;

export const generateReport = dailyReport.cronReducer(
  spacetimedb,
  (ctx: Tx, args, invocation) => {
    ctx.db.report.insert({
      id: 0n,
      generatedAt: invocation.scheduledFor,
    });
    console.log(
      `Generating ${args.format} report for workspace ${args.workspaceId}`
    );
  }
);

export const beat = heartbeat.cronReducer(spacetimedb, (_ctx: Tx) => {
  // Perform deterministic database work here.
});

export const refresh = refreshCatalog.cronProcedure(
  spacetimedb,
  (ctx: Proc, invocation) => {
    // Use invocation.id as the idempotency key for an external request.
    ctx.http.fetch('https://example.com/catalog');
  }
);

export const cronReconcile = cron.reconcileReducer(spacetimedb);
export const { jobs: cronJobs } = cron.publicViews(spacetimedb);

export const init = spacetimedb.init(ctx => {
  schedule(ctx, dailyReport, '0 9 * * 1-5', {
    timezone: 'America/New_York',
    maxFailures: 3,
    args: { workspaceId: 42n, format: 'summary' },
  });
  schedule(ctx, heartbeat, { everySeconds: 30 });
  schedule(ctx, refreshCatalog, '0 */15 * * * *');
});
```

`init` seeds schedules for a fresh database. Runtime schedule changes remain database state across module publishes.

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

The handle carries the inferred argument type through `schedule()`,
`cronReducer()`, and `cronProcedure()`. The argument builder may be any
SpacetimeDB type, although a named `t.object()` gives most jobs the clearest
call site and database schema.

Job names use lowercase snake_case and may contain up to 48 characters. Pass
every handle to one `createCron()` call, register exactly one reducer or
procedure for each handle. `schedule()` rejects a configuration with a missing
handler. When `reconcileEverySeconds` is configured, export
`cron.reconcileReducer()`. Applications that expose cron status export the
`jobs` view returned by `cron.publicViews()`.

Each reducer job handles its normal fires and its internal recovery calls. A
handler failure schedules the same job reducer with a private recovery payload.
The second invocation records the failure and restores calendar scheduling in a
fresh transaction. It does not call the application handler. Procedure jobs use
their separate transaction flow and do not use volatile recovery.

Use `cronReducer` for deterministic database work. The handler receives the consumer module's typed reducer context and a `CronInvocation`:

```ts
export const runCleanup = cleanup.cronReducer(
  spacetimedb,
  (ctx: Tx, invocation) => {
    console.log(invocation.id);
    // Database writes commit together when the handler succeeds.
  }
);
```

Use `cronProcedure` for HTTP requests and other procedure capabilities:

```ts
export const syncRemote = remoteSync.cronProcedure(
  spacetimedb,
  (ctx: Proc, invocation) => {
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
  (ctx: Tx, args, invocation) => {
    archiveRows(ctx, args.workspaceId, args.retainDays);
    console.log(invocation.id);
  }
);
```

### Scheduling and cancellation

`schedule()` first repairs any enabled jobs with missing triggers. It then
creates or replaces the requested schedule, clears failure state, increments
the job generation, and enables the job.

```ts
schedule(ctx, cleanup, '30 2 * * *', { timezone: 'UTC' });
schedule(ctx, cleanup, { everySeconds: 300 });

schedule(ctx, archiveWorkspace, '0 3 * * *', {
  timezone: 'UTC',
  args: { workspaceId: 42n, retainDays: 90 },
});
```

Arguments are required when scheduling an argument-bearing job. Rescheduling
replaces the schedule, arguments, generation, and pending fire atomically. A
handler receives the argument value read at the start of its fire.

Cron expressions accept five fields or six fields when seconds are included. Fixed intervals accept whole seconds from 1 through 31,536,000.

`unschedule()` runs the same opportunistic repair before removing the target
fire, incrementing its generation, and leaving job and history rows available
for inspection.

```ts
unschedule(ctx, cleanup);
```

These helpers perform scheduling operations. Application reducers remain responsible for authorization.

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

The explicit recovery payload distinguishes a recovery call from a normal fire.
The implementation does not infer the call type from schedule-row presence.
This gives calendar and interval jobs the same failure path and prevents the
application handler from running again during recovery.

The volatile call is best effort and is not persisted. A process crash,
uncatchable trap, or lost message can temporarily leave an enabled calendar job
without a fire. The package detects that broken invariant during every
`schedule()` and `unschedule()` operation. Set `reconcileEverySeconds` to add a
low-frequency native interval sweep:

```ts
const cron = createCron(jobs, { reconcileEverySeconds: 300 });

// Export after registering the job handlers.
export const cronReconcile = cron.reconcileReducer(spacetimedb);
```

Repair removes any stale trigger, inserts a valid current-generation trigger,
and records one `Failed` run with error `lost_fire`. The normal failure counter,
history cap, and automatic disable policy apply. Without the optional sweep,
repair occurs on the next management operation. With it, detection is bounded
by the configured interval and scheduler availability.

This remains a temporary best-effort design until nested transactions are
available. Native interval job rows remain scheduled independently.

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
- Spring-forward and fall-back behavior follows `cron-parser` 5.x and is covered by tests.
- Sparse expressions use internal checkpoint triggers so valid occurrences beyond the host timer horizon remain scheduled.
- Fixed intervals use SpacetimeDB native `ScheduleAt.interval` rows.

Scheduled functions execute through SpacetimeDB's scheduler. A long-running procedure delays other scheduled work in the same module, so procedure handlers should finish promptly.

### Run history

`historyCap` defaults to five completed records per job and accepts values from
0 through 1,000.

Run statuses are:

- `Ok`: work completed successfully
- `Failed`: the handler returned an error

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

The local integration suite requires `spacetime start` and validates module
publication, calendar chains, native intervals, typed reducer and procedure
arguments, same-reducer volatile recovery, reducer rollback,
opportunistic and interval-sweep lost-fire repair, automatic disablement,
procedure outcomes, generations, cancellation, history bounds, authorization,
and the example module. The recovery suite verifies that a procedure commits
its next calendar fire before external work, survives a host stop, and performs
at most one catch-up invocation after downtime.

See the
[browser example](./example/)
for a complete integration and [`DESIGN.md`](./DESIGN.md) for the transaction model and invariants.

## License

BUSL-1.1. See [`LICENSE.txt`](./LICENSE.txt).
