# Cron architecture

This document defines the runtime invariants and transaction behavior for
`@spacetimedb/cron`.

## Design goals

The package provides:

1. Stable job identity in ordinary database state.
2. Calendar scheduling with time zones and daylight-saving transitions.
3. Native fixed intervals.
4. Statically registered reducer and procedure handlers.
5. Typed arguments stored with each configured job.
6. Rollback of partial reducer writes when a handler fails.
7. Generation-safe rescheduling and cancellation.
8. Detectable and repairable loss of volatile recovery work.
9. Bounded operational history.
10. A direct path to nested transactions when the platform supports them.

The current reducer failure path uses
`volatile_nonatomic_schedule_immediate`. That host API is an unstable,
best-effort bridge for code that needs rollback plus follow-up work before
SpacetimeDB supports nested transactions. The invariant reconciler bounds its
crash limitation without adding another application-work hop.

## Tables

### `cron_job`

One private row represents each configured job. The row contains the schedule,
typed arguments, enabled state, failure policy, generation, fire count, last
outcome time, next logical occurrence, and disable reason.

The job name is the stable primary key. Scheduling, rescheduling, disabling,
and re-enabling preserve the row.

The argument column is a tagged union generated from the static handles passed
to `createCron()`. Argumentless jobs use a unit payload. Rescheduling replaces
the schedule and argument value atomically.

### `cron_jobs` view

The optional public anonymous view projects operational fields from
`cron_job`. It omits the argument column so clients can subscribe to schedule
and health state without receiving private application payloads. Detailed
disablement errors also remain private. The view maps them to stable reason
codes for operator disablement, failure thresholds, lost-fire thresholds,
invalid schedule state, and otherwise unspecified disablement.

### `<job_name>_fire`

Each statically declared job owns one schedule table bound directly to its
`<job_name>_cron` reducer or procedure.

An enabled job owns exactly one row in its fire table:

- Calendar jobs use a chain of one-shot `ScheduleAt.time` rows.
- Fixed-rate jobs use one persistent native `ScheduleAt.interval` row.

The row carries the job generation. Calendar rows also carry `targetAt`, the
logical occurrence represented by the trigger. The physical `scheduledAt`
may be an earlier checkpoint when the target exceeds the host timer horizon.
The optional `recovery` field is empty in stored rows. A volatile invocation
sets it to the failed sequence, logical occurrence, and bounded error.

### `cron_run`

The run table contains completed `Ok` and `Failed` outcomes. Each row carries
the stable invocation ID, job name, generation, sequence, logical scheduled
time, completion time, and bounded error text.

History is pruned synchronously by per-job sequence. No sweeper or retention
schedule is required.

### `cron_reconcile_tick`

When `reconcileEverySeconds` is configured, this schedule table contains one
native interval row bound to `cron_reconcile`. The sweep scans the statically
registered jobs and repairs broken fire invariants. It is recovery machinery,
not run-history retention.

## Registration model

`cronTable()` creates a typed job handle. `createCron()` builds `cron_job`,
`cron_run`, and one fire table for each handle. `cronReducer()` or
`cronProcedure()` binds one application handler directly to that job's fire
table. The reducer wrapper also handles volatile recovery calls for that job.
`cron.reconcileReducer()` registers the optional interval reconciler.
`cron.publicViews()` registers the optional sanitized job-state view.

An argument-bearing `cronTable()` carries its SpacetimeDB type builder.
`createCron()` combines those builders into the private `CronJobArgsValue`
union. Variants are ordered by job name so changing the order of handles passed
to `createCron()` does not change the generated schema.

Every job must register exactly one handler. Scheduling rejects a core with any
missing handler. When `reconcileEverySeconds` is configured, the consumer also
exports `cron.reconcileReducer()`. Registration and construction reject missing
handlers, duplicate handlers, duplicate jobs, foreign handles, multiple cores,
invalid names, and table-key collisions.

The package reserves:

- `cron_job`
- `cron_run`
- `cron_jobs`
- `cron_reconcile`
- `cron_reconcile_tick`
- every `<job_name>_fire` table
- every `<job_name>_cron` scheduled function

The factory receives `table`, `t`, `ScheduleAt`, `Timestamp`, and
`SenderError` from the consumer. Table builders contain SDK-private
registration symbols, so using the consumer's SDK values keeps all generated
tables on the same SDK instance as the host schema.

## Scheduling and generations

`schedule()` validates the schedule, time zone, interval, failure policy, and
argument presence before changing state. It ensures the optional reconciliation
interval exists, repairs broken fire invariants for configured jobs, and then
performs the requested state change in the same transaction:

1. Delete the current fire row, if one exists.
2. Increment the job generation.
3. Upsert `cron_job` with the new schedule and typed arguments.
4. Insert one new fire row.
5. Store the logical next occurrence.

`unschedule()` runs the same opportunistic reconciliation before deleting the
target fire row, incrementing its generation, disabling it, and retaining its
job state and history.

Every fire row carries the generation that created it. Normal and recovery
invocations compare that generation with the current job row. Delayed work from
an earlier configuration cannot restore or modify a replacement schedule.

## Reducer execution

A reducer job executes in one scheduled transaction on the success path:

1. Verify the job exists, is enabled, and matches the fire generation.
2. Verify the tagged argument variant matches the job.
3. For a calendar job, delete the consumed row and insert its successor.
4. Build the invocation metadata and read the typed argument value.
5. Execute the application handler.
6. Record `Ok`, update health, and prune history.

The successor, application writes, job health, and run record commit together.
A successful calendar fire therefore advances atomically with its application
work. Native interval rows persist without rearming.

Reducers must complete synchronously. Returning a thenable is treated as a
handler failure.

## Reducer failure and volatile recovery

SpacetimeDB reducers do not currently support nested transactions or
savepoints. If application work fails after making writes, those writes must be
rolled back. Rethrowing the error accomplishes that, but also rolls back the
calendar successor inserted earlier in the same transaction.

The middleware uses this temporary recovery sequence:

1. Catch the handler error.
2. Copy the fire argument and set its private `recovery` field to the sequence,
   scheduled time, and bounded error.
3. Serialize that row with the fire table's SDK row serializer and submit it to
   the same `<job_name>_cron` reducer through
   `volatile_nonatomic_schedule_immediate`.
4. Rethrow the original error.
5. Let the fire transaction roll back.
6. Run the same reducer in a fresh transaction. Its recovery branch does not
   call the application handler.
7. Restore the calendar chain, record `Failed`, update failure state, and apply
   automatic disablement.

The recovery branch validates the sender, job name, generation, and expected
sequence. It ignores stale or duplicate work. The explicit payload also works
for native interval jobs, whose schedule row remains present after failure.

The direct host ABI remains isolated behind the recovery adapter. The package
uses the SDK fire-row builder and `BinaryWriter` for BSATN serialization. A
future SDK wrapper can replace `sys-abi.d.ts` and the direct host import without
changing the public cron API.

### Volatile crash gap

The volatile call is best effort and is not stored in the commit log. A process
crash, uncatchable trap, or lost volatile message can prevent the recovery
invocation from executing. For a failed calendar reducer, that can temporarily
leave an enabled job without a pending fire. A native interval row persists
independently, although its failure outcome can still be lost.

The stable `cron_job` row makes this state machine-detectable. Every scheduling
or cancellation operation opportunistically scans configured jobs. If
`reconcileEverySeconds` is configured, one native interval sweep performs the
same scan at a bounded cadence. An enabled job with no valid current-generation
fire is disarmed, rearmed from the current transaction time, and assigned one
`Failed` run with error `lost_fire`. Normal failure policy, history pruning, and
automatic disablement apply to that outcome.

The reconciler also treats a current-generation fire with the wrong trigger
shape as lost. A calendar job requires `targetAt`; an interval job must not have
it. This prevents a malformed row from satisfying the invariant while being
unable to execute correctly.

The interval sweep is optional. Without it, repair occurs on the next
`schedule()` or `unschedule()` call. With it, the crash gap is bounded by the
configured interval and scheduler availability. This remains a temporary
best-effort design rather than crash-proof execution.

### Recovery dispatch

The private `recovery` field determines which branch runs. Recovery does not
depend on schedule-row presence, sender heuristics, or cleanup timing. This is
required for native interval rows because they persist after a failed fire.

Calendar recovery replaces the visible fire state before it inserts the next
occurrence. Generation and sequence guards make stale or duplicate recovery
messages no-ops.

## Procedure execution

Procedures are not single transactions, so they do not need volatile recovery
to roll back application database work. A scheduled procedure executes in
three phases:

1. A `withTx` callback verifies the generation, advances a calendar chain,
   snapshots the typed arguments, and reserves the invocation sequence.
2. The handler performs procedure work.
3. A second `withTx` callback records `Ok` or `Failed`, updates health, and
   prunes history.

The first phase commits before external work begins, so a process failure during
a procedure does not remove the next calendar fire. It can lose the current run
record or leave an external operation with an unknown outcome. Handlers should
use `CronInvocation.id` as an idempotency key when the external service
supports one.

No mutable value captured outside a `withTx` callback influences that
transaction's database decisions. The callback returns the prepared invocation
and argument snapshot directly.

## Calendar chains and checkpoints

Cron expressions represent calendar occurrences and cannot be reduced to fixed
durations. After an actual calendar fire, the middleware computes the first
occurrence strictly after the transaction timestamp.

SpacetimeDB 2.8 has a finite timer horizon. A valid expression such as February
29 can produce a gap beyond that horizon. The package schedules a checkpoint at
most 365 days away while retaining the logical target in `targetAt`. A
checkpoint that fires before the target inserts another bounded trigger without
executing application work.

This repeats until the logical occurrence is within range.

## Downtime and time zones

An overdue calendar row fires once when the host resumes. The successor is
computed after the recovery timestamp, so intermediate missed occurrences are
skipped.

Occurrence calculation uses `cron-parser` 5.x with an IANA time zone. Tests
cover spring-forward gaps, fall-back repetition, strictly-after behavior,
impossible dates, and date bounds.

Native interval rows follow SpacetimeDB interval behavior.
`cron_job.nextRunAt` is an estimate updated after each interval fire or
failure recovery.

## Failure policy

`consecutiveFailures` counts recorded `Failed` outcomes for the active
generation. `Ok` resets the counter. When a positive `maxFailures` threshold
is reached, the package removes the fire row, disables the job, and stores a
bounded reason.

If volatile recovery work is lost, the later reconciler records `lost_fire` as a
normal `Failed` outcome. It advances the failure counter and can trigger the
same automatic disable policy.

## Storage bounds

The default history cap is five completed runs per job. The configured range is
0 through 1,000.

Errors and disable reasons are capped at 1,024 characters. Job names are
lowercase snake_case with a 48-character limit. Interval values are whole
seconds from 1 through 31,536,000.

## Security

Scheduling helpers perform state transitions and validation. Host reducers
remain responsible for application authorization.

Every `<job_name>_cron` function and the scheduled `cron_reconcile` reducer
accept calls only when the sender is the database identity. Generation and
sequence checks prevent stale or duplicate recovery messages from changing
current job state.

`cron_job` is always private. Applications can register the public
`cron_jobs` view to expose operational state without arguments.
`publicTables: true` exposes `cron_run`, the per-job fire tables, and the
optional reconciliation tick, including run error strings.

## Nested-transaction migration

Nested transactions are the intended long-term replacement for the volatile
recovery path. With platform support, reducer execution can become:

1. Advance the calendar chain in the parent transaction.
2. Run application work in a child transaction.
3. Commit the child on success or roll it back on failure.
4. Record the outcome in the parent transaction.
5. Commit the parent with the successor and outcome.

That model removes the volatile ABI and its crash gap without reintroducing a
second schedule table or scheduler hop. The public `cronTable()`,
`schedule()`, and handler APIs do not need to change.

## Verification

The package test suite covers:

- parser boundaries and time zones
- daylight-saving transitions
- one-catch-up behavior
- sparse-expression checkpointing
- input limits
- module schema builds
- typed reducer and procedure arguments
- argument replacement
- reducer rollback
- same-reducer volatile recovery and failure accounting
- calendar and native interval recovery
- opportunistic `lost_fire` repair
- interval-sweep `lost_fire` repair
- reducer and procedure automatic disablement
- generation changes
- rescheduling and cancellation
- bounded history
- scheduled-function authorization
- procedure calendar-chain continuity and at-most-one catch-up across host restart
- example module publication

Release verification also runs repository lint, formatting, typechecking,
module builds, client generation, consumer installation, and package tarball
inspection.
