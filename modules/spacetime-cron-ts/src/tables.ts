import type { Identity, Timestamp } from 'spacetimedb';
import { t, table, type Infer, type VariantsObj } from 'spacetimedb/server';

const cronSchedule = t.enum('CronSchedule', {
  cron: t.object('CronSpec', {
    expression: t.string(),
    timezone: t.string(),
  }),
  every: t.object('EverySpec', {
    seconds: t.u32(),
  }),
});

const cronRunStatus = t.enum('CronRunStatus', ['Ok', 'Failed']);

const cronFireRecovery = t.object('CronFireRecovery', {
  sequence: t.u64(),
  scheduledFor: t.timestamp(),
  error: t.string(),
});

/** Public projection of `cron_job` without typed arguments. */
export const cronJobView = t.row('CronJobView', {
  name: t.string().primaryKey(),
  schedule: cronSchedule,
  enabled: t.bool(),
  maxFailures: t.u32(),
  consecutiveFailures: t.u32(),
  fireCount: t.u64(),
  generation: t.u64(),
  lastRunAt: t.option(t.timestamp()),
  nextRunAt: t.option(t.timestamp()),
  disabledReason: t.option(t.string()),
});

export function cronJobTable(args: VariantsObj) {
  return table(
    { name: 'cron_job', public: false },
    {
      name: t.string().primaryKey(),
      schedule: cronSchedule,
      args: t.enum('CronJobArgsValue', args),
      enabled: t.bool(),
      maxFailures: t.u32(),
      consecutiveFailures: t.u32(),
      fireCount: t.u64(),
      generation: t.u64(),
      lastRunAt: t.option(t.timestamp()),
      nextRunAt: t.option(t.timestamp()),
      disabledReason: t.option(t.string()),
    }
  );
}

export function cronRunTable(isPublic: boolean) {
  return table(
    { name: 'cron_run', public: isPublic },
    {
      invocationId: t.string().primaryKey(),
      jobName: t.string().index(),
      generation: t.u64(),
      sequence: t.u64(),
      scheduledFor: t.timestamp(),
      completedAt: t.timestamp(),
      status: cronRunStatus,
      error: t.option(t.string()),
    }
  );
}

export function cronFireTable(name: string, isPublic: boolean) {
  return table(
    { name, public: isPublic },
    {
      scheduledId: t.u64().primaryKey().autoInc(),
      scheduledAt: t.scheduleAt(),
      jobName: t.string().unique(),
      generation: t.u64(),
      targetAt: t.option(t.timestamp()),
      recovery: t.option(cronFireRecovery),
    }
  );
}

export function cronReconcileTickTable(isPublic: boolean) {
  return table(
    { name: 'cron_reconcile_tick', public: isPublic },
    {
      scheduledId: t.u64().primaryKey().autoInc(),
      scheduledAt: t.scheduleAt(),
      key: t.string().unique(),
    }
  );
}

export type CronJobTableDef = ReturnType<typeof cronJobTable>;
export type CronRunTableDef = ReturnType<typeof cronRunTable>;
export type CronFireTableDef = ReturnType<typeof cronFireTable>;
export type CronReconcileTickTableDef = ReturnType<
  typeof cronReconcileTickTable
>;

export type JobRow = Infer<CronJobTableDef['rowType']>;
export type RunRow = Infer<CronRunTableDef['rowType']>;
export type FireRow = Infer<CronFireTableDef['rowType']>;
export type ReconcileTickRow = Infer<CronReconcileTickTableDef['rowType']>;

/** The cron tables of a host reducer or transaction context. */
export interface CronDb {
  readonly cronJob: {
    readonly name: {
      find(name: string): JobRow | null;
      update(row: JobRow): JobRow;
    };
    insert(row: JobRow): JobRow;
  };
  readonly cronRun: {
    readonly invocationId: { find(id: string): RunRow | null };
    readonly jobName: { filter(jobName: string): Iterable<RunRow> };
    insert(row: RunRow): RunRow;
    delete(row: RunRow): boolean;
  };
  readonly cronReconcileTick?: {
    readonly key: { find(key: string): ReconcileTickRow | null };
    insert(row: ReconcileTickRow): ReconcileTickRow;
  };
  /** Per-job fire tables, keyed by `<jobName>Fire`. */
  readonly [accessor: string]: unknown;
}

/** The parts of a host fire table that cron reads and writes. */
export interface CronFireTable {
  readonly jobName: { find(jobName: string): FireRow | null };
  readonly scheduledId: { find(scheduledId: bigint): FireRow | null };
  insert(row: FireRow): FireRow;
  delete(row: FireRow): boolean;
}

/** A host reducer context whose schema includes `cron.tables`. */
export interface CronTx {
  readonly sender: Identity;
  readonly databaseIdentity: Identity;
  readonly timestamp: Timestamp;
  readonly db: CronDb;
}

/** A host procedure context whose schema includes `cron.tables`. */
export interface CronProcedureCtx {
  readonly sender: Identity;
  readonly databaseIdentity: Identity;
  readonly timestamp: Timestamp;
  withTx<T>(body: (tx: CronTx) => T): T;
}
