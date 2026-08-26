import {
  schema,
  table,
  t,
  SenderError,
  toCamelCase,
  type ReducerCtx,
  type InferSchema,
  type ProcedureCtx,
} from 'spacetimedb/server';
import { ScheduleAt, Timestamp } from 'spacetimedb';
import {
  spacetimeCron,
  type CronJobHandle,
  type CronJobReference,
} from '@spacetimedb/cron';

// One injection point: hand the library this module's own SDK objects so the
// bundle contains exactly one copy of the spacetimedb SDK.
const { cronTable, createCron, schedule, unschedule } = spacetimeCron({
  table,
  t,
  toCamelCase,
  ScheduleAt,
  Timestamp,
  SenderError,
});

// ── Jobs ─────────────────────────────────────────────────────────────────────

const heartbeat = cronTable({ name: 'heartbeat' });
const report = cronTable({
  name: 'report',
  args: t.object('ReportCronArgs', {
    label: t.string(),
    batchSize: t.u32(),
  }),
});
const flaky = cronTable({ name: 'flaky' });
const probe = cronTable({
  name: 'probe',
  args: t.object('ProbeCronArgs', { source: t.string() }),
});
const recoveryProbe = cronTable({ name: 'recovery_probe' });

const cron = createCron([heartbeat, report, flaky, probe, recoveryProbe], {
  publicTables: true,
  reconcileEverySeconds: 2,
});

// ── App tables ───────────────────────────────────────────────────────────────

const tickLog = table(
  { name: 'tick_log', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    jobName: t.string().index(),
    at: t.timestamp(),
  }
);

const flakyState = table(
  { name: 'flaky_state', public: true },
  {
    singleton: t.bool().primaryKey(),
    failing: t.bool(),
  }
);

const argumentLog = table(
  { name: 'argument_log', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    jobName: t.string().index(),
    value: t.string(),
    count: t.u32(),
    invocationId: t.string(),
  }
);

const recoveryProbeState = table(
  { name: 'recovery_probe_state' },
  {
    singleton: t.bool().primaryKey(),
    invocationId: t.string(),
  }
);

const spacetimedb = schema({
  ...cron.tables,
  tickLog,
  flakyState,
  argumentLog,
  recoveryProbeState,
});
export default spacetimedb;

type Schema = InferSchema<typeof spacetimedb>;
type Tx = ReducerCtx<Schema>;
type Proc = ProcedureCtx<Schema>;

// ── Cron wiring ──────────────────────────────────────────────────────────────

export const runHeartbeat = heartbeat.cronReducer(spacetimedb, (ctx: Tx) => {
  ctx.db.tickLog.insert({ id: 0n, jobName: 'heartbeat', at: ctx.timestamp });
});

export const runReport = report.cronReducer(
  spacetimedb,
  (ctx: Tx, args, invocation) => {
    ctx.db.tickLog.insert({ id: 0n, jobName: 'report', at: ctx.timestamp });
    ctx.db.argumentLog.insert({
      id: 0n,
      jobName: 'report',
      value: args.label,
      count: args.batchSize,
      invocationId: invocation.id,
    });
  }
);

// Integration-only authorization probe. Direct calls must not execute a
// reducer that is reserved for the scheduler.
export const forgeReportFireForTest = spacetimedb.reducer(ctx => {
  const fireTable = ctx.db.reportFire as unknown as {
    iter(): Iterable<unknown>;
  };
  const fire = [...fireTable.iter()][0];
  if (!fire) throw new SenderError('cron.test_missing_report_fire');
  const invoke = runReport as unknown as (
    ctx: Tx,
    args: { arg: unknown }
  ) => void;
  invoke(ctx, { arg: fire });
});

// Handler writes roll back on failure. Volatile recovery records the failure
// and restores calendar jobs after the scheduled transaction aborts.
export const runFlaky = flaky.cronReducer(spacetimedb, (ctx: Tx) => {
  ctx.db.tickLog.insert({ id: 0n, jobName: 'flaky', at: ctx.timestamp });
  const state = ctx.db.flakyState.singleton.find(true);
  if (state?.failing) throw new Error('flaky.failure');
});

// Procedure job: the natural home for side-effecting work such as outbound
// HTTP. Here it writes through withTx so the local suite can observe it.
export const runProbe = probe.cronProcedure(
  spacetimedb,
  (ctx: Proc, args, invocation) => {
    const failing = ctx.withTx(
      (tx: Tx) => tx.db.flakyState.singleton.find(true)?.failing ?? false
    );
    if (failing) throw new Error('probe.failure');
    ctx.withTx((tx: Tx) => {
      tx.db.tickLog.insert({ id: 0n, jobName: 'probe', at: tx.timestamp });
      tx.db.argumentLog.insert({
        id: 0n,
        jobName: 'probe',
        value: args.source,
        count: 0,
        invocationId: invocation.id,
      });
    });
  }
);

function blockUntilHostStops(): never {
  for (;;) {
    // The recovery test terminates the isolated host after observing its
    // durable entry marker. V8 2.8 does not time out this execution path.
  }
}

// The first run remains in flight until the recovery test kills its isolated
// host. Its separate transactions prove what survives the interruption.
export const runRecoveryProbe = recoveryProbe.cronProcedure(
  spacetimedb,
  (ctx: Proc, run) => {
    if (run.sequence === 1n) {
      ctx.withTx((tx: Tx) => {
        tx.db.recoveryProbeState.insert({
          singleton: true,
          invocationId: run.id,
        });
      });
      blockUntilHostStops();
    }
    ctx.withTx((tx: Tx) => {
      tx.db.tickLog.insert({
        id: 0n,
        jobName: 'recovery_probe',
        at: tx.timestamp,
      });
    });
  }
);

export const cronReconcile = cron.reconcileReducer(spacetimedb);
export const { jobs: cronJobs } = cron.publicViews(spacetimedb);

export const init = spacetimedb.init(ctx => {
  ctx.db.flakyState.insert({ singleton: true, failing: false });
  // Code-declared default for a fresh database.
  schedule(ctx, heartbeat, { everySeconds: 30 });
});

// ── CLI-facing management (thin wrappers over the library helpers) ───────────

const argumentlessJobs: Record<string, CronJobHandle> = {
  heartbeat,
  flaky,
  recovery_probe: recoveryProbe,
};

const allJobs: Record<string, CronJobReference> = {
  ...argumentlessJobs,
  report,
  probe,
};

function argumentlessJobByName(name: string): CronJobHandle {
  const job = argumentlessJobs[name];
  if (!job) throw new SenderError(`cron.unknown_job:${name}`);
  return job;
}

function jobByName(name: string): CronJobReference {
  const job = allJobs[name];
  if (!job) throw new SenderError(`cron.unknown_job:${name}`);
  return job;
}

function scheduleSafely(operation: () => void): void {
  try {
    operation();
  } catch (err) {
    throw err instanceof SenderError
      ? err
      : new SenderError(err instanceof Error ? err.message : String(err));
  }
}

export const scheduleCron = spacetimedb.reducer(
  {
    name: t.string(),
    expression: t.string(),
    timezone: t.string(),
    maxFailures: t.u32(),
  },
  (ctx, args) => {
    scheduleSafely(() => {
      schedule(ctx, argumentlessJobByName(args.name), args.expression, {
        timezone: args.timezone,
        maxFailures: args.maxFailures,
      });
    });
  }
);

export const scheduleEvery = spacetimedb.reducer(
  { name: t.string(), seconds: t.u32(), maxFailures: t.u32() },
  (ctx, args) => {
    scheduleSafely(() => {
      schedule(
        ctx,
        argumentlessJobByName(args.name),
        { everySeconds: args.seconds },
        { maxFailures: args.maxFailures }
      );
    });
  }
);

export const scheduleReport = spacetimedb.reducer(
  {
    expression: t.string(),
    timezone: t.string(),
    maxFailures: t.u32(),
    label: t.string(),
    batchSize: t.u32(),
  },
  (ctx, args) => {
    scheduleSafely(() => {
      schedule(ctx, report, args.expression, {
        timezone: args.timezone,
        maxFailures: args.maxFailures,
        args: { label: args.label, batchSize: args.batchSize },
      });
    });
  }
);

export const scheduleProbe = spacetimedb.reducer(
  {
    expression: t.string(),
    timezone: t.string(),
    maxFailures: t.u32(),
    source: t.string(),
  },
  (ctx, args) => {
    scheduleSafely(() => {
      schedule(ctx, probe, args.expression, {
        timezone: args.timezone,
        maxFailures: args.maxFailures,
        args: { source: args.source },
      });
    });
  }
);

export const unscheduleJob = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    unschedule(ctx, jobByName(name));
  }
);

export const setFlakyFailing = spacetimedb.reducer(
  { failing: t.bool() },
  (ctx, { failing }) => {
    const state = ctx.db.flakyState.singleton.find(true);
    if (state) ctx.db.flakyState.singleton.update({ ...state, failing });
  }
);

// Integration-only fault injection for the lost-fire reconciler.
export const dropHeartbeatFireForTest = spacetimedb.reducer(ctx => {
  const fireTable = ctx.db.heartbeatFire as unknown as {
    iter(): Iterable<{ jobName: string }>;
    delete(row: { jobName: string }): void;
  };
  const pending = [...fireTable.iter()].find(
    row => row.jobName === 'heartbeat'
  );
  if (pending) fireTable.delete(pending);
});
