// eslint-disable-next-line @typescript-eslint/triple-slash-reference
/// <reference path="./sys-abi.d.ts" />
import { volatile_nonatomic_schedule_immediate } from 'spacetime:sys@2.0';
import { BinaryWriter, Timestamp } from 'spacetimedb';
import {
  ScheduleAt,
  SenderError,
  t,
  toCamelCase,
  type Infer,
  type VariantsObj,
} from 'spacetimedb/server';
import { errors, internalErrors } from './errors.js';
import {
  boundedScheduleTime,
  CronInputError,
  MAX_FAILURES,
  normalizeHistoryCap,
  normalizeJobArgs,
  normalizeJobName,
  normalizeMaxFailures,
  normalizeReconcileEverySeconds,
  normalizeSchedule,
  nextOccurrence,
  ONE_SECOND_MICROS,
  truncateError,
} from './schedule.js';
import {
  cronFireTable,
  cronJobTable,
  cronJobView,
  cronReconcileTickTable,
  cronRunTable,
  type CronFireTable,
  type CronFireTableDef,
  type CronProcedureCtx,
  type CronReconcileTickTableDef,
  type CronTx,
  type FireRow,
  type JobRow,
} from './tables.js';
import type {
  CronArgsBuilder,
  CronClient,
  CronConfig,
  CronInvocation,
  CronJobHandle,
  CronJobReference,
  CronPublicViews,
  CronSchema,
  CronTableOpts,
  CronTableWithArgsOpts,
  CronTables,
  ScheduleSpec,
} from './types.js';

const RECONCILE_REDUCER_NAME = 'cron_reconcile';
const RECONCILE_TICK_KEY = 'cron';

const PUBLIC_DISABLED_REASONS = {
  disabled: 'disabled',
  disabledByOperator: 'disabled_by_operator',
  failureThresholdReached: 'failure_threshold_reached',
  invalidScheduleState: 'invalid_schedule_state',
  lostFireThresholdReached: 'lost_fire_threshold_reached',
} as const;

type RunStatus = 'Ok' | 'Failed';

export function publicDisabledReason(
  reason: string | undefined
): string | undefined {
  if (reason === undefined) return undefined;
  if (reason === 'disabled_by_operator') {
    return PUBLIC_DISABLED_REASONS.disabledByOperator;
  }
  if (reason.startsWith(`${errors.invalidScheduleState}:`)) {
    return PUBLIC_DISABLED_REASONS.invalidScheduleState;
  }
  if (/^failed_[1-9][0-9]*_consecutive_times:lost_fire$/.test(reason)) {
    return PUBLIC_DISABLED_REASONS.lostFireThresholdReached;
  }
  if (/^failed_[1-9][0-9]*_consecutive_times:/.test(reason)) {
    return PUBLIC_DISABLED_REASONS.failureThresholdReached;
  }
  return PUBLIC_DISABLED_REASONS.disabled;
}

type InternalHandler = (...args: unknown[]) => unknown;

interface RuntimeScheduleOpts {
  timezone?: string;
  maxFailures?: number;
  args?: unknown;
}

interface JobMetadata {
  readonly handle: CronJobReference;
  readonly argsType: CronArgsBuilder;
  readonly hasArgs: boolean;
  readonly fireTableName: string;
  readonly fireAccessor: string;
  readonly reducerName: string;
  fire: CronFireTableDef | undefined;
  core: Core | undefined;
  registration: 'reducer' | 'procedure' | undefined;
}

interface Core {
  readonly jobs: Map<string, JobMetadata>;
  readonly historyCap: number;
  readonly reconcileEverySeconds: number | undefined;
  readonly reconcileTick: CronReconcileTickTableDef | undefined;
  reconcileRegistered: boolean;
  publicViewsRegistered: boolean;
}

interface PreparedFire {
  readonly invocation: CronInvocation;
  readonly args: unknown;
}

const metadata = new WeakMap<CronJobReference, JobMetadata>();

function isThenable(value: unknown): value is PromiseLike<unknown> {
  return (
    (typeof value === 'object' || typeof value === 'function') &&
    value !== null &&
    'then' in value &&
    typeof value.then === 'function'
  );
}

function invocationId(
  jobName: string,
  generation: bigint,
  sequence: bigint
): string {
  return `${jobName}:${generation}:${sequence}`;
}

function requireMetadata(job: CronJobReference): JobMetadata {
  const value = metadata.get(job);
  if (!value) throw new Error(internalErrors.foreignJobHandle);
  return value;
}

function requireCore(job: CronJobReference): Core {
  const value = requireMetadata(job).core;
  if (!value) {
    throw new Error(
      `${internalErrors.notWired}:${job.jobName}:pass the job to client() first`
    );
  }
  return value;
}

function requireRegisteredHandlers(core: Core): void {
  const missing = [...core.jobs.values()]
    .filter(job => !job.registration)
    .map(job => job.handle.jobName);
  if (missing.length > 0) {
    throw new Error(`${internalErrors.missingHandlers}:${missing.join(',')}`);
  }
}

function requireFire(job: JobMetadata): CronFireTableDef {
  if (!job.fire) {
    throw new Error(
      `${internalErrors.notWired}:${job.handle.jobName}:pass the job to client() first`
    );
  }
  return job.fire;
}

function requireDatabaseCaller(
  ctx: Pick<CronTx, 'sender' | 'databaseIdentity'>
): void {
  if (!ctx.sender.isEqual(ctx.databaseIdentity)) {
    throw new SenderError(errors.notAuthorized);
  }
}

function fireTable(ctx: CronTx, job: JobMetadata): CronFireTable {
  const value = ctx.db[job.fireAccessor];
  if (!value) {
    throw new Error(
      `${internalErrors.missingTable}:${job.fireTableName}:spread cron.tables into schema()`
    );
  }
  // The accessor was derived from the fire table registered by client().
  return value as CronFireTable;
}

function jobArgs(row: JobRow): unknown {
  return 'value' in row.args ? row.args.value : undefined;
}

function pruneHistory(ctx: CronTx, core: Core, jobName: string): void {
  const rows = [...ctx.db.cronRun.jobName.filter(jobName)].sort(
    (left, right) =>
      left.sequence < right.sequence
        ? -1
        : left.sequence > right.sequence
          ? 1
          : 0
  );
  const removeCount = Math.max(0, rows.length - core.historyCap);
  for (const row of rows.slice(0, removeCount)) {
    ctx.db.cronRun.delete(row);
  }
}

function recordRun(
  ctx: CronTx,
  core: Core,
  invocation: CronInvocation,
  status: RunStatus,
  error: string | undefined
): void {
  if (core.historyCap === 0) return;
  if (ctx.db.cronRun.invocationId.find(invocation.id)) return;
  ctx.db.cronRun.insert({
    invocationId: invocation.id,
    jobName: invocation.jobName,
    generation: invocation.generation,
    sequence: invocation.sequence,
    scheduledFor: invocation.scheduledFor,
    completedAt: ctx.timestamp,
    status: { tag: status },
    error,
  });
  pruneHistory(ctx, core, invocation.jobName);
}

function disarmFire(ctx: CronTx, job: JobMetadata): void {
  const tableView = fireTable(ctx, job);
  const pending = tableView.jobName.find(job.handle.jobName);
  if (pending) tableView.delete(pending);
}

function insertFire(
  ctx: CronTx,
  job: JobMetadata,
  row: JobRow,
  targetMicros: bigint | undefined
): Timestamp | undefined {
  const tableView = fireTable(ctx, job);
  if (row.schedule.tag === 'every') {
    tableView.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.interval(
        BigInt(row.schedule.value.seconds) * ONE_SECOND_MICROS
      ),
      jobName: row.name,
      generation: row.generation,
      targetAt: undefined,
      recovery: undefined,
    });
    return new Timestamp(
      ctx.timestamp.microsSinceUnixEpoch +
        BigInt(row.schedule.value.seconds) * ONE_SECOND_MICROS
    );
  }
  if (targetMicros === undefined) return undefined;
  const targetAt = new Timestamp(targetMicros);
  tableView.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.time(
      boundedScheduleTime(ctx.timestamp.microsSinceUnixEpoch, targetMicros)
    ),
    jobName: row.name,
    generation: row.generation,
    targetAt,
    recovery: undefined,
  });
  return targetAt;
}

function replaceFire(ctx: CronTx, job: JobMetadata, row: JobRow): JobRow {
  disarmFire(ctx, job);

  let targetMicros: bigint | undefined;
  if (row.schedule.tag === 'cron') {
    try {
      targetMicros = nextOccurrence(
        row.schedule,
        ctx.timestamp.microsSinceUnixEpoch
      );
    } catch (error) {
      return disableJob(
        ctx,
        job,
        row,
        `${errors.invalidScheduleState}:${truncateError(error)}`
      );
    }
    if (targetMicros === undefined) {
      return disableJob(ctx, job, row, errors.noFutureOccurrence);
    }
  }

  const nextRunAt = insertFire(ctx, job, row, targetMicros);
  const updated = { ...row, nextRunAt };
  ctx.db.cronJob.name.update(updated);
  return updated;
}

function ensureReconcileTick(ctx: CronTx, core: Core): void {
  const everySeconds = core.reconcileEverySeconds;
  if (everySeconds === undefined) return;
  if (!core.reconcileRegistered) {
    throw new Error(
      `${internalErrors.reconcileReducerNotRegistered}:export cron.reconcileReducer()`
    );
  }
  const tick = ctx.db.cronReconcileTick;
  if (!tick) {
    throw new Error(
      `${internalErrors.missingTable}:cron_reconcile_tick:spread cron.tables into schema()`
    );
  }
  if (tick.key.find(RECONCILE_TICK_KEY)) return;
  tick.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(BigInt(everySeconds) * ONE_SECOND_MICROS),
    key: RECONCILE_TICK_KEY,
  });
}

function reconcileLostFires(ctx: CronTx, core: Core): void {
  for (const job of core.jobs.values()) {
    const row = ctx.db.cronJob.name.find(job.handle.jobName);
    if (!row || !row.enabled) {
      disarmFire(ctx, job);
      continue;
    }
    if (row.args.tag !== job.handle.jobName) {
      disableJob(ctx, job, row, errors.invalidArgsState);
      continue;
    }

    const pending = fireTable(ctx, job).jobName.find(row.name);
    const validPending =
      pending?.generation === row.generation &&
      pending.recovery === undefined &&
      (row.schedule.tag === 'cron'
        ? pending.targetAt !== undefined
        : pending.targetAt === undefined);
    if (validPending) continue;

    const sequence = row.fireCount + 1n;
    const invocation: CronInvocation = {
      id: invocationId(row.name, row.generation, sequence),
      jobName: row.name,
      generation: row.generation,
      sequence,
      scheduledFor: row.nextRunAt ?? ctx.timestamp,
    };
    replaceFire(ctx, job, row);
    applyOutcome(ctx, core, job, invocation, 'Failed', 'lost_fire', true);
  }
}

function disableJob(
  ctx: CronTx,
  job: JobMetadata,
  row: JobRow,
  reason: string
): JobRow {
  disarmFire(ctx, job);
  const updated = {
    ...row,
    enabled: false,
    nextRunAt: undefined,
    disabledReason: truncateError(reason),
  };
  ctx.db.cronJob.name.update(updated);
  return updated;
}

function applyOutcome(
  ctx: CronTx,
  core: Core,
  job: JobMetadata,
  invocation: CronInvocation,
  status: RunStatus,
  error: string | undefined,
  advanceFireCount: boolean
): void {
  if (ctx.db.cronRun.invocationId.find(invocation.id)) return;

  let row = ctx.db.cronJob.name.find(invocation.jobName);
  if (row && row.generation === invocation.generation) {
    if (advanceFireCount && invocation.sequence !== row.fireCount + 1n) {
      return;
    }
    const consecutiveFailures =
      status === 'Failed'
        ? Math.min(row.consecutiveFailures + 1, MAX_FAILURES)
        : 0;
    row = {
      ...row,
      fireCount:
        invocation.sequence > row.fireCount
          ? invocation.sequence
          : row.fireCount,
      consecutiveFailures,
      lastRunAt: ctx.timestamp,
    };
    ctx.db.cronJob.name.update(row);
    if (
      status === 'Failed' &&
      row.enabled &&
      row.maxFailures > 0 &&
      consecutiveFailures >= row.maxFailures
    ) {
      disableJob(
        ctx,
        job,
        row,
        `failed_${consecutiveFailures}_consecutive_times:${error ?? status}`
      );
    }
  }

  recordRun(ctx, core, invocation, status, error);
}

function prepareFire(
  ctx: CronTx,
  job: JobMetadata,
  arg: FireRow,
  reserveSequence: boolean
): PreparedFire | undefined {
  let row = ctx.db.cronJob.name.find(job.handle.jobName);
  if (!row || !row.enabled || row.generation !== arg.generation) return;
  if (row.args.tag !== job.handle.jobName) {
    disableJob(ctx, job, row, errors.invalidArgsState);
    return;
  }

  const nowMicros = ctx.timestamp.microsSinceUnixEpoch;
  let scheduledFor = ctx.timestamp;
  if (row.schedule.tag === 'cron') {
    if (!arg.targetAt) {
      disableJob(ctx, job, row, `${errors.invalidTrigger}:missing_target`);
      return;
    }
    const tableView = fireTable(ctx, job);
    const fired = tableView.scheduledId.find(arg.scheduledId);
    if (fired) tableView.delete(fired);

    const targetMicros = arg.targetAt.microsSinceUnixEpoch;
    if (targetMicros > nowMicros) {
      const nextRunAt = insertFire(ctx, job, row, targetMicros);
      ctx.db.cronJob.name.update({ ...row, nextRunAt });
      return;
    }

    scheduledFor = arg.targetAt;
    const nextAt = nextOccurrence(row.schedule, nowMicros);
    if (nextAt === undefined) {
      row = disableJob(ctx, job, row, errors.noFutureOccurrence);
    } else {
      const nextRunAt = insertFire(ctx, job, row, nextAt);
      row = { ...row, nextRunAt };
      ctx.db.cronJob.name.update(row);
    }
  } else {
    const nextRunAt = new Timestamp(
      nowMicros + BigInt(row.schedule.value.seconds) * ONE_SECOND_MICROS
    );
    row = { ...row, nextRunAt };
    ctx.db.cronJob.name.update(row);
  }

  const sequence = row.fireCount + 1n;
  if (reserveSequence) {
    row = { ...row, fireCount: sequence };
    ctx.db.cronJob.name.update(row);
  }
  return {
    args: jobArgs(row),
    invocation: {
      id: invocationId(row.name, row.generation, sequence),
      jobName: row.name,
      generation: row.generation,
      sequence,
      scheduledFor,
    },
  };
}

function invokeHandler(
  handler: InternalHandler,
  ctx: unknown,
  job: JobMetadata,
  prepared: PreparedFire
): unknown {
  return job.hasArgs
    ? handler(ctx, prepared.args, prepared.invocation)
    : handler(ctx, prepared.invocation);
}

function encodeFireArgument(job: JobMetadata, arg: FireRow): Uint8Array {
  const writer = new BinaryWriter(256);
  // A reducer with one row parameter has the same BSATN field sequence as
  // the row itself. The SDK serializer keeps this encoding tied to the
  // generated fire-table schema.
  requireFire(job).rowType.serialize(writer, arg);
  return writer.getBuffer();
}

function scheduleRecovery(
  job: JobMetadata,
  arg: FireRow,
  invocation: CronInvocation,
  error: string
): void {
  const recoveryArg: FireRow = {
    ...arg,
    recovery: {
      sequence: invocation.sequence,
      scheduledFor: invocation.scheduledFor,
      error,
    },
  };
  volatile_nonatomic_schedule_immediate(
    job.reducerName,
    encodeFireArgument(job, recoveryArg)
  );
}

function executeReducer(
  ctx: CronTx,
  core: Core,
  job: JobMetadata,
  arg: FireRow,
  handler: InternalHandler
): void {
  requireDatabaseCaller(ctx);
  if (arg.recovery) {
    recoverFailure(ctx, core, job, arg);
    return;
  }
  const prepared = prepareFire(ctx, job, arg, false);
  if (!prepared) return;

  try {
    const result = invokeHandler(handler, ctx, job, prepared);
    if (isThenable(result)) {
      throw new Error(
        `${internalErrors.asyncReducerHandler}:reducers must complete synchronously`
      );
    }
  } catch (error) {
    const detail = truncateError(error);
    try {
      scheduleRecovery(job, arg, prepared.invocation, detail);
    } catch {
      // Volatile recovery is best effort. Reconciliation repairs a missing
      // calendar fire if the host loses this request.
    }
    throw error;
  }

  applyOutcome(ctx, core, job, prepared.invocation, 'Ok', undefined, true);
}

function executeProcedure(
  ctx: CronProcedureCtx,
  core: Core,
  job: JobMetadata,
  arg: FireRow,
  handler: InternalHandler
): void {
  requireDatabaseCaller(ctx);
  if (arg.recovery) {
    throw new SenderError(internalErrors.invalidProcedureRecovery);
  }
  const prepared = ctx.withTx(tx => prepareFire(tx, job, arg, true));
  if (!prepared) return;

  let error: string | undefined;
  try {
    const result = invokeHandler(handler, ctx, job, prepared);
    if (isThenable(result)) {
      throw new Error(
        `${internalErrors.asyncProcedureHandler}:procedures must complete synchronously`
      );
    }
  } catch (caught) {
    error = truncateError(caught);
  }

  ctx.withTx(tx => {
    applyOutcome(
      tx,
      core,
      job,
      prepared.invocation,
      error === undefined ? 'Ok' : 'Failed',
      error,
      false
    );
  });
}

function recoverFailure(
  ctx: CronTx,
  core: Core,
  job: JobMetadata,
  arg: FireRow
): void {
  const recovery = arg.recovery;
  if (!recovery || arg.jobName !== job.handle.jobName) return;
  const jobName = job.handle.jobName;
  let row = ctx.db.cronJob.name.find(jobName);
  if (
    !row ||
    !row.enabled ||
    row.generation !== arg.generation ||
    recovery.sequence !== row.fireCount + 1n
  ) {
    return;
  }

  if (row.schedule.tag === 'cron') {
    // Replace the fire unconditionally so stale visible state cannot block
    // recovery.
    row = replaceFire(ctx, job, row);
  } else {
    const nextRunAt = new Timestamp(
      ctx.timestamp.microsSinceUnixEpoch +
        BigInt(row.schedule.value.seconds) * ONE_SECOND_MICROS
    );
    row = { ...row, nextRunAt };
    ctx.db.cronJob.name.update(row);
  }

  const invocation: CronInvocation = {
    id: invocationId(jobName, arg.generation, recovery.sequence),
    jobName,
    generation: arg.generation,
    sequence: recovery.sequence,
    scheduledFor: recovery.scheduledFor,
  };
  applyOutcome(
    ctx,
    core,
    job,
    invocation,
    'Failed',
    truncateError(recovery.error),
    true
  );
}

function registerJob(job: JobMetadata, kind: 'reducer' | 'procedure'): Core {
  const core = requireCore(job.handle);
  if (job.registration) {
    throw new Error(
      `${internalErrors.handlerAlreadyRegistered}:${job.handle.jobName}:${job.registration}`
    );
  }
  job.registration = kind;
  return core;
}

/** Declare a job. Pass every job to `client()` before registering handlers. */
export function cronTable<
  const Name extends string,
  ArgsBuilder extends CronArgsBuilder,
>(
  opts: CronTableWithArgsOpts<Name, ArgsBuilder>
): CronJobHandle<Name, Infer<ArgsBuilder>>;
export function cronTable<const Name extends string>(
  opts: CronTableOpts<Name>
): CronJobHandle<Name>;
export function cronTable(
  opts: CronTableOpts | CronTableWithArgsOpts
): CronJobReference {
  const jobName = normalizeJobName(opts.name);
  const hasArgs = 'args' in opts;
  const handle = {
    jobName,
    cronReducer(spacetimedb: CronSchema, handler: InternalHandler) {
      const job = requireMetadata(handle);
      const core = registerJob(job, 'reducer');
      const fire = requireFire(job);
      return spacetimedb.reducer(
        { name: job.reducerName, onSchedule: fire },
        { arg: fire.rowType },
        (ctx: CronTx, { arg }: { arg: FireRow }) => {
          executeReducer(ctx, core, job, arg, handler);
        }
      );
    },
    cronProcedure(spacetimedb: CronSchema, handler: InternalHandler) {
      const job = requireMetadata(handle);
      const core = registerJob(job, 'procedure');
      const fire = requireFire(job);
      return spacetimedb.procedure(
        { name: job.reducerName, onSchedule: fire },
        { arg: fire.rowType },
        t.unit(),
        (ctx: CronProcedureCtx, { arg }: { arg: FireRow }) => {
          executeProcedure(ctx, core, job, arg, handler);
          return {};
        }
      );
    },
  };
  metadata.set(handle, {
    handle,
    argsType: 'args' in opts ? opts.args : t.unit(),
    hasArgs,
    fireTableName: `${jobName}_fire`,
    fireAccessor: toCamelCase(`${jobName}_fire`),
    reducerName: `${jobName}_cron`,
    fire: undefined,
    core: undefined,
    registration: undefined,
  });
  return handle;
}

/** Create the cron tables for `config.jobs` and the functions that manage them. */
export function client<const Config extends CronConfig>(
  config: Config
): CronClient<Config> {
  const { jobs } = config;
  if (jobs.length === 0) throw new Error(internalErrors.noJobs);
  const historyCap = normalizeHistoryCap(config.historyCap);
  const isPublic = config.publicTables ?? false;
  const reconcileEverySeconds = normalizeReconcileEverySeconds(
    config.reconcileEverySeconds
  );

  const jobsByName = new Map<string, JobMetadata>();
  const argumentTypes: VariantsObj = {};
  const usedAccessors = new Set(['cronJob', 'cronRun', 'cronReconcileTick']);
  const sortedJobs = [...jobs].sort((left, right) =>
    left.jobName < right.jobName ? -1 : left.jobName > right.jobName ? 1 : 0
  );
  for (const handle of sortedJobs) {
    const job = requireMetadata(handle);
    if (jobsByName.has(handle.jobName)) {
      throw new Error(`${internalErrors.duplicateJob}:${handle.jobName}`);
    }
    if (job.core) {
      throw new Error(`${internalErrors.jobAlreadyWired}:${handle.jobName}`);
    }
    if (usedAccessors.has(job.fireAccessor)) {
      throw new Error(
        `${internalErrors.tableKeyCollision}:${job.fireAccessor}`
      );
    }
    usedAccessors.add(job.fireAccessor);
    jobsByName.set(handle.jobName, job);
    argumentTypes[handle.jobName] = job.argsType;
  }

  const tables: Record<string, unknown> = {
    cronJob: cronJobTable(argumentTypes),
    cronRun: cronRunTable(isPublic),
  };
  const reconcileTick =
    reconcileEverySeconds === undefined
      ? undefined
      : cronReconcileTickTable(isPublic);
  if (reconcileTick) tables.cronReconcileTick = reconcileTick;
  for (const job of jobsByName.values()) {
    job.fire = cronFireTable(job.fireTableName, isPublic);
    tables[job.fireAccessor] = job.fire;
  }

  const core: Core = {
    jobs: jobsByName,
    historyCap,
    reconcileEverySeconds,
    reconcileTick,
    reconcileRegistered: false,
    publicViewsRegistered: false,
  };
  for (const job of jobsByName.values()) job.core = core;

  function jobInThisClient(handle: CronJobReference): JobMetadata {
    const job = requireMetadata(handle);
    if (job.core !== core) throw new Error(internalErrors.foreignJobHandle);
    return job;
  }

  function reconcileReducer(spacetimedb: CronSchema) {
    if (core.reconcileRegistered) {
      throw new Error(internalErrors.reconcileReducerAlreadyRegistered);
    }
    const tick = core.reconcileTick;
    if (tick === undefined) {
      throw new Error(
        `${internalErrors.reconcileNotConfigured}:set client({ reconcileEverySeconds })`
      );
    }
    requireRegisteredHandlers(core);
    core.reconcileRegistered = true;
    return spacetimedb.reducer(
      { name: RECONCILE_REDUCER_NAME, onSchedule: tick },
      { arg: tick.rowType },
      (ctx: CronTx) => {
        requireDatabaseCaller(ctx);
        reconcileLostFires(ctx, core);
      }
    );
  }

  function publicViews(spacetimedb: CronSchema): CronPublicViews {
    if (core.publicViewsRegistered) {
      throw new Error(internalErrors.publicViewsAlreadyRegistered);
    }
    core.publicViewsRegistered = true;
    const jobsView = spacetimedb.anonymousView(
      { name: 'cron_jobs', public: true },
      t.array(cronJobView),
      (ctx: { db: { cronJob: { iter(): Iterable<JobRow> } } }) =>
        [...ctx.db.cronJob.iter()].map(row => ({
          name: row.name,
          schedule: row.schedule,
          enabled: row.enabled,
          maxFailures: row.maxFailures,
          consecutiveFailures: row.consecutiveFailures,
          fireCount: row.fireCount,
          generation: row.generation,
          lastRunAt: row.lastRunAt,
          nextRunAt: row.nextRunAt,
          disabledReason: publicDisabledReason(row.disabledReason),
        }))
    );
    return { jobs: jobsView };
  }

  function schedule(
    ctx: CronTx,
    handle: CronJobReference,
    spec: ScheduleSpec,
    opts?: RuntimeScheduleOpts
  ): void {
    const job = jobInThisClient(handle);
    requireRegisteredHandlers(core);
    let normalized;
    let maxFailures: number;
    let args: unknown;
    try {
      normalized = normalizeSchedule(
        spec,
        opts,
        ctx.timestamp.microsSinceUnixEpoch
      );
      maxFailures = normalizeMaxFailures(opts?.maxFailures);
      args = normalizeJobArgs(handle.jobName, job.hasArgs, opts);
    } catch (error) {
      throw error instanceof CronInputError
        ? new SenderError(error.message)
        : error;
    }

    ensureReconcileTick(ctx, core);
    reconcileLostFires(ctx, core);

    const existing = ctx.db.cronJob.name.find(handle.jobName);
    const generation = (existing?.generation ?? 0n) + 1n;
    disarmFire(ctx, job);

    // The stored enum type is built from every job, so its static type keeps
    // only the tag.
    const taggedArgs = { tag: handle.jobName, value: args };
    let row: JobRow = {
      name: handle.jobName,
      schedule: normalized.schedule,
      args: taggedArgs,
      enabled: true,
      maxFailures,
      consecutiveFailures: 0,
      fireCount: existing?.fireCount ?? 0n,
      generation,
      lastRunAt: existing?.lastRunAt,
      nextRunAt: undefined,
      disabledReason: undefined,
    };
    if (existing) ctx.db.cronJob.name.update(row);
    else ctx.db.cronJob.insert(row);

    const nextRunAt = insertFire(ctx, job, row, normalized.firstAt);
    if (!nextRunAt) {
      throw new SenderError(errors.unsatisfiableExpression);
    }
    row = { ...row, nextRunAt };
    ctx.db.cronJob.name.update(row);
  }

  function unschedule(ctx: CronTx, handle: CronJobReference): void {
    const job = jobInThisClient(handle);
    requireRegisteredHandlers(core);
    ensureReconcileTick(ctx, core);
    reconcileLostFires(ctx, core);
    const row = ctx.db.cronJob.name.find(handle.jobName);
    if (!row) return;
    disarmFire(ctx, job);
    ctx.db.cronJob.name.update({
      ...row,
      enabled: false,
      generation: row.generation + 1n,
      consecutiveFailures: 0,
      nextRunAt: undefined,
      disabledReason: 'disabled_by_operator',
    });
  }

  return {
    // The keys were derived from config.jobs exactly as CronTables derives them.
    tables: tables as CronTables<Config>,
    reconcileReducer,
    publicViews,
    schedule,
    unschedule,
  };
}
