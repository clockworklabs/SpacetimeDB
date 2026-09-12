// eslint-disable-next-line @typescript-eslint/triple-slash-reference
/// <reference path="./sys-abi.d.ts" />
import { volatile_nonatomic_schedule_immediate } from 'spacetime:sys@2.0';
import { BinaryWriter } from 'spacetimedb';
import type { Infer, ModuleExport } from 'spacetimedb/server';
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
} from './schedule';
import type {
  CronApi,
  CronArgsBuilder,
  CronCore,
  CronCorePublicViews,
  CronInvocation,
  CronJobHandle,
  CronJobReference,
  CronSchedule,
  CronSchema,
  CronSdk,
  CronTableDefinition,
  CronTableOpts,
  CronTableWithArgsOpts,
  CronTimestamp,
  CreateCronOpts,
  ScheduleSpec,
} from './types';

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

interface IdentityLike {
  toHexString(): string;
}

interface DynamicSdkValue {
  readonly [key: string]: DynamicSdkValue;
  (...args: unknown[]): DynamicSdkValue;
}

function asTableDefinition(value: DynamicSdkValue): CronTableDefinition {
  return value as unknown as CronTableDefinition;
}

interface RegistrationSchema {
  anonymousView(...args: unknown[]): ModuleExport;
  reducer(...args: unknown[]): ModuleExport;
  procedure(...args: unknown[]): ModuleExport;
}

type InternalHandler = (...args: unknown[]) => unknown;

interface RuntimeScheduleOpts {
  timezone?: string;
  maxFailures?: number;
  args?: unknown;
}

interface UniqueAccessor<Row, Key> {
  find(key: Key): Row | undefined;
  update(row: Row): unknown;
}

interface IndexAccessor<Row, Key> {
  filter(key: Key): Iterable<Row>;
}

interface TableView<Row> {
  insert(row: Row): Row;
  delete(row: Row): unknown;
  iter(): Iterable<Row>;
}

interface CronArgsValue {
  tag: string;
  value: unknown;
}

interface JobRow {
  name: string;
  schedule: CronSchedule;
  args: CronArgsValue;
  enabled: boolean;
  maxFailures: number;
  consecutiveFailures: number;
  fireCount: bigint;
  generation: bigint;
  lastRunAt: CronTimestamp | undefined;
  nextRunAt: CronTimestamp | undefined;
  disabledReason: string | undefined;
}

function publicDisabledReason(reason: string | undefined): string | undefined {
  if (reason === undefined) return undefined;
  if (reason === 'disabled_by_operator') {
    return PUBLIC_DISABLED_REASONS.disabledByOperator;
  }
  if (reason.startsWith('cron.invalid_schedule_state:')) {
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

interface FireRow {
  scheduledId: bigint;
  scheduledAt: unknown;
  jobName: string;
  generation: bigint;
  targetAt: CronTimestamp | undefined;
  recovery: FireRecovery | undefined;
}

interface FireRecovery {
  sequence: bigint;
  scheduledFor: CronTimestamp;
  error: string;
}

interface RunRow {
  invocationId: string;
  jobName: string;
  generation: bigint;
  sequence: bigint;
  scheduledFor: CronTimestamp;
  completedAt: CronTimestamp;
  status: { tag: RunStatus };
  error: string | undefined;
}

interface ReconcileTickRow {
  scheduledId: bigint;
  scheduledAt: unknown;
  key: string;
}

type JobTable = TableView<JobRow> & {
  name: UniqueAccessor<JobRow, string>;
};

type FireTable = TableView<FireRow> & {
  jobName: UniqueAccessor<FireRow, string>;
  scheduledId: UniqueAccessor<FireRow, bigint>;
};

type RunTable = TableView<RunRow> & {
  invocationId: UniqueAccessor<RunRow, string>;
  jobName: IndexAccessor<RunRow, string>;
};

type ReconcileTickTable = TableView<ReconcileTickRow> & {
  key: UniqueAccessor<ReconcileTickRow, string>;
};

interface CronDatabase extends Record<string, unknown> {
  cronJob: JobTable;
  cronRun: RunTable;
  cronReconcileTick?: ReconcileTickTable;
}

interface TransactionContext {
  readonly db: CronDatabase;
  readonly timestamp: CronTimestamp;
  readonly sender: IdentityLike;
  readonly databaseIdentity: IdentityLike;
}

interface ProcedureContext {
  readonly timestamp: CronTimestamp;
  readonly sender: IdentityLike;
  readonly databaseIdentity: IdentityLike;
  withTx<T>(body: (ctx: TransactionContext) => T): T;
}

interface JobMetadata {
  readonly handle: CronJobReference;
  readonly argsType: DynamicSdkValue;
  readonly hasArgs: boolean;
  readonly fireTableName: string;
  readonly fireAccessor: string;
  readonly reducerName: string;
  fire: DynamicSdkValue | undefined;
  core: InternalCore | undefined;
  registration: 'reducer' | 'procedure' | undefined;
}

interface InternalCore extends CronCore {
  readonly jobs: Map<string, JobMetadata>;
  readonly historyCap: number;
  readonly reconcileEverySeconds: number | undefined;
  readonly reconcileTick: DynamicSdkValue | undefined;
  reconcileRegistered: boolean;
  publicViewsRegistered: boolean;
}

interface PreparedFire {
  readonly invocation: CronInvocation;
  readonly args: unknown;
}

function isThenable(value: unknown): value is PromiseLike<unknown> {
  return (
    (typeof value === 'object' || typeof value === 'function') &&
    value !== null &&
    'then' in value &&
    typeof (value as { then?: unknown }).then === 'function'
  );
}

function invocationId(
  jobName: string,
  generation: bigint,
  sequence: bigint
): string {
  return `${jobName}:${generation}:${sequence}`;
}

/** Create a cron factory bound to the consumer module's SDK instance. */
export function spacetimeCron(sdk: CronSdk): CronApi {
  const { ScheduleAt, Timestamp, SenderError, toCamelCase } = sdk;
  const table = sdk.table as unknown as DynamicSdkValue;
  const t = sdk.t as unknown as DynamicSdkValue;
  const metadata = new WeakMap<CronJobReference, JobMetadata>();
  let coreCreated = false;

  const cronScheduleType = t.enum('CronSchedule', {
    cron: t.object('CronSpec', {
      expression: t.string(),
      timezone: t.string(),
    }),
    every: t.object('EverySpec', {
      seconds: t.u32(),
    }),
  });
  const runStatusType = t.enum('CronRunStatus', ['Ok', 'Failed']);
  const fireRecoveryType = t.object('CronFireRecovery', {
    sequence: t.u64(),
    scheduledFor: t.timestamp(),
    error: t.string(),
  });

  function requireMetadata(job: CronJobReference): JobMetadata {
    const value = metadata.get(job);
    if (!value) throw new Error('cron.foreign_job_handle');
    return value;
  }

  function requireCore(job: CronJobReference): InternalCore {
    const value = requireMetadata(job).core;
    if (!value) {
      throw new Error(
        `cron.not_wired:${job.jobName}:pass the job to createCron() first`
      );
    }
    return value;
  }

  function requireRegisteredHandlers(core: InternalCore): void {
    const missing = [...core.jobs.values()]
      .filter(job => !job.registration)
      .map(job => job.handle.jobName);
    if (missing.length > 0) {
      throw new Error(`cron.missing_handlers:${missing.join(',')}`);
    }
  }

  function requireFire(job: JobMetadata): DynamicSdkValue {
    if (!job.fire) {
      throw new Error(
        `cron.not_wired:${job.handle.jobName}:pass the job to createCron() first`
      );
    }
    return job.fire;
  }

  function asTransactionContext(ctx: unknown): TransactionContext {
    return ctx as TransactionContext;
  }

  function requireDatabaseCaller(
    ctx: Pick<TransactionContext, 'sender' | 'databaseIdentity'>
  ): void {
    if (ctx.sender.toHexString() !== ctx.databaseIdentity.toHexString()) {
      throw new SenderError('cron.not_authorized');
    }
  }

  function fireTable(ctx: TransactionContext, job: JobMetadata): FireTable {
    const value = ctx.db[job.fireAccessor];
    if (!value) {
      throw new Error(
        `cron.missing_table:${job.fireTableName}:spread cron.tables into schema()`
      );
    }
    return value as FireTable;
  }

  function throwInputError(error: unknown): never {
    if (error instanceof CronInputError) {
      throw new SenderError(error.message);
    }
    throw error;
  }

  function pruneHistory(
    ctx: TransactionContext,
    core: InternalCore,
    jobName: string
  ): void {
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
    ctx: TransactionContext,
    core: InternalCore,
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

  function disarmFire(ctx: TransactionContext, job: JobMetadata): void {
    const tableView = fireTable(ctx, job);
    const pending = tableView.jobName.find(job.handle.jobName);
    if (pending) tableView.delete(pending);
  }

  function insertFire(
    ctx: TransactionContext,
    job: JobMetadata,
    row: JobRow,
    targetMicros: bigint | undefined
  ): CronTimestamp | undefined {
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

  function replaceFire(
    ctx: TransactionContext,
    job: JobMetadata,
    row: JobRow
  ): JobRow {
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
          `cron.invalid_schedule_state:${truncateError(error)}`
        );
      }
      if (targetMicros === undefined) {
        return disableJob(ctx, job, row, 'cron.no_future_occurrence');
      }
    }

    const nextRunAt = insertFire(ctx, job, row, targetMicros);
    const updated = { ...row, nextRunAt };
    ctx.db.cronJob.name.update(updated);
    return updated;
  }

  function ensureReconcileTick(
    ctx: TransactionContext,
    core: InternalCore
  ): void {
    const everySeconds = core.reconcileEverySeconds;
    if (everySeconds === undefined) return;
    if (!core.reconcileRegistered) {
      throw new Error(
        'cron.reconcile_reducer_not_registered:export cron.reconcileReducer()'
      );
    }
    const tick = ctx.db.cronReconcileTick;
    if (!tick) {
      throw new Error(
        'cron.missing_table:cron_reconcile_tick:spread cron.tables into schema()'
      );
    }
    if (tick.key.find(RECONCILE_TICK_KEY)) return;
    tick.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.interval(
        BigInt(everySeconds) * ONE_SECOND_MICROS
      ),
      key: RECONCILE_TICK_KEY,
    });
  }

  function reconcileLostFires(
    ctx: TransactionContext,
    core: InternalCore
  ): void {
    for (const job of core.jobs.values()) {
      const row = ctx.db.cronJob.name.find(job.handle.jobName);
      if (!row || !row.enabled) {
        disarmFire(ctx, job);
        continue;
      }
      if (row.args.tag !== job.handle.jobName) {
        disableJob(ctx, job, row, 'cron.invalid_args_state');
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
    ctx: TransactionContext,
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
    ctx: TransactionContext,
    core: InternalCore,
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
    ctx: TransactionContext,
    job: JobMetadata,
    arg: FireRow,
    reserveSequence: boolean
  ): PreparedFire | undefined {
    let row = ctx.db.cronJob.name.find(job.handle.jobName);
    if (!row || !row.enabled || row.generation !== arg.generation) return;
    if (row.args.tag !== job.handle.jobName) {
      disableJob(ctx, job, row, 'cron.invalid_args_state');
      return;
    }

    const nowMicros = ctx.timestamp.microsSinceUnixEpoch;
    let scheduledFor = ctx.timestamp;
    if (row.schedule.tag === 'cron') {
      if (!arg.targetAt) {
        disableJob(ctx, job, row, 'cron.invalid_trigger:missing_target');
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
        row = disableJob(ctx, job, row, 'cron.no_future_occurrence');
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
      args: row.args.value,
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
    const rowType = requireFire(job).rowType as unknown as {
      serialize(writer: BinaryWriter, value: FireRow): void;
    };
    const writer = new BinaryWriter(256);
    // A reducer with one row parameter has the same BSATN field sequence as
    // the row itself. The SDK serializer keeps this encoding tied to the
    // generated fire-table schema.
    rowType.serialize(writer, arg);
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

  function executeReducer<Ctx>(
    rawCtx: unknown,
    core: InternalCore,
    job: JobMetadata,
    arg: FireRow,
    handler: InternalHandler
  ): void {
    const ctx = asTransactionContext(rawCtx);
    requireDatabaseCaller(ctx);
    if (arg.recovery) {
      recoverFailure(ctx, core, job, arg);
      return;
    }
    const prepared = prepareFire(ctx, job, arg, false);
    if (!prepared) return;

    try {
      const result = invokeHandler(handler, rawCtx as Ctx, job, prepared);
      if (isThenable(result)) {
        throw new Error(
          'cron.async_reducer_handler:reducers must complete synchronously'
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

  function executeProcedure<Ctx>(
    rawCtx: unknown,
    core: InternalCore,
    job: JobMetadata,
    arg: FireRow,
    handler: InternalHandler
  ): void {
    const ctx = rawCtx as ProcedureContext;
    requireDatabaseCaller(ctx);
    if (arg.recovery) {
      throw new SenderError('cron.invalid_procedure_recovery');
    }
    const prepared = ctx.withTx(tx => prepareFire(tx, job, arg, true));
    if (!prepared) return;

    let error: string | undefined;
    try {
      const result = invokeHandler(handler, rawCtx as Ctx, job, prepared);
      if (isThenable(result)) {
        throw new Error(
          'cron.async_procedure_handler:procedures must complete synchronously'
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
    ctx: TransactionContext,
    core: InternalCore,
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

  function registerJob(
    job: JobMetadata,
    kind: 'reducer' | 'procedure'
  ): InternalCore {
    const core = requireCore(job.handle);
    if (job.registration) {
      throw new Error(
        `cron.handler_already_registered:${job.handle.jobName}:${job.registration}`
      );
    }
    job.registration = kind;
    return core;
  }

  function cronTable<
    const Name extends string,
    ArgsBuilder extends CronArgsBuilder,
  >(
    opts: CronTableWithArgsOpts<Name, ArgsBuilder>
  ): CronJobHandle<Name, Infer<ArgsBuilder>>;
  function cronTable<const Name extends string>(
    opts: CronTableOpts<Name>
  ): CronJobHandle<Name>;
  function cronTable(
    opts: CronTableOpts | CronTableWithArgsOpts
  ): CronJobReference {
    const jobName = normalizeJobName(opts.name);
    const hasArgs = 'args' in opts;
    const argsType = hasArgs
      ? (opts.args as unknown as DynamicSdkValue)
      : t.unit();
    const handle = {
      jobName,
      cronReducer(spacetimedb: CronSchema, handler: InternalHandler) {
        const job = requireMetadata(handle);
        const core = registerJob(job, 'reducer');
        const fire = requireFire(job);
        return (spacetimedb as RegistrationSchema).reducer(
          { name: job.reducerName, onSchedule: fire },
          { arg: fire.rowType },
          (ctx: unknown, { arg }: { arg: FireRow }) => {
            executeReducer(ctx, core, job, arg, handler);
          }
        );
      },
      cronProcedure(spacetimedb: CronSchema, handler: InternalHandler) {
        const job = requireMetadata(handle);
        const core = registerJob(job, 'procedure');
        const fire = requireFire(job);
        return (spacetimedb as RegistrationSchema).procedure(
          { name: job.reducerName, onSchedule: fire },
          { arg: fire.rowType },
          t.unit(),
          (ctx: unknown, { arg }: { arg: FireRow }) => {
            executeProcedure(ctx, core, job, arg, handler);
            return {};
          }
        );
      },
    };
    metadata.set(handle, {
      handle,
      argsType,
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

  function createCron(
    jobs: readonly CronJobReference[],
    opts?: CreateCronOpts
  ): CronCore {
    if (coreCreated) throw new Error('cron.multiple_cores_not_supported');
    if (jobs.length === 0) throw new Error('cron.no_jobs');
    coreCreated = true;
    const historyCap = normalizeHistoryCap(opts?.historyCap);
    const isPublic = opts?.publicTables ?? false;
    const reconcileEverySeconds = normalizeReconcileEverySeconds(
      opts?.reconcileEverySeconds
    );

    const jobsByName = new Map<string, JobMetadata>();
    const argumentTypes: Record<string, DynamicSdkValue> = {};
    const usedAccessors = new Set(['cronJob', 'cronRun', 'cronReconcileTick']);
    const sortedJobs = [...jobs].sort((left, right) =>
      left.jobName < right.jobName ? -1 : left.jobName > right.jobName ? 1 : 0
    );
    for (const handle of sortedJobs) {
      const job = requireMetadata(handle);
      if (jobsByName.has(handle.jobName)) {
        throw new Error(`cron.duplicate_job:${handle.jobName}`);
      }
      if (job.core) {
        throw new Error(`cron.job_already_wired:${handle.jobName}`);
      }
      if (usedAccessors.has(job.fireAccessor)) {
        throw new Error(`cron.table_key_collision:${job.fireAccessor}`);
      }
      usedAccessors.add(job.fireAccessor);
      jobsByName.set(handle.jobName, job);
      argumentTypes[handle.jobName] = job.argsType;
    }

    const cronJobArgsType = t.enum('CronJobArgsValue', argumentTypes);
    const cronJob = table(
      { name: 'cron_job', public: false },
      {
        name: t.string().primaryKey(),
        schedule: cronScheduleType,
        args: cronJobArgsType,
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
    const cronJobViewRow = t.row('CronJobView', {
      name: t.string().primaryKey(),
      schedule: cronScheduleType,
      enabled: t.bool(),
      maxFailures: t.u32(),
      consecutiveFailures: t.u32(),
      fireCount: t.u64(),
      generation: t.u64(),
      lastRunAt: t.option(t.timestamp()),
      nextRunAt: t.option(t.timestamp()),
      disabledReason: t.option(t.string()),
    });
    const cronRun = table(
      { name: 'cron_run', public: isPublic },
      {
        invocationId: t.string().primaryKey(),
        jobName: t.string().index(),
        generation: t.u64(),
        sequence: t.u64(),
        scheduledFor: t.timestamp(),
        completedAt: t.timestamp(),
        status: runStatusType,
        error: t.option(t.string()),
      }
    );

    const tables: Record<string, CronTableDefinition> = {
      cronJob: asTableDefinition(cronJob),
      cronRun: asTableDefinition(cronRun),
    };
    let reconcileTick: DynamicSdkValue | undefined;
    if (reconcileEverySeconds !== undefined) {
      reconcileTick = table(
        { name: 'cron_reconcile_tick', public: isPublic },
        {
          scheduledId: t.u64().primaryKey().autoInc(),
          scheduledAt: t.scheduleAt(),
          key: t.string().unique(),
        }
      );
      tables.cronReconcileTick = asTableDefinition(reconcileTick);
    }
    for (const job of jobsByName.values()) {
      const fire = table(
        { name: job.fireTableName, public: isPublic },
        {
          scheduledId: t.u64().primaryKey().autoInc(),
          scheduledAt: t.scheduleAt(),
          jobName: t.string().unique(),
          generation: t.u64(),
          targetAt: t.option(t.timestamp()),
          recovery: t.option(fireRecoveryType),
        }
      );
      job.fire = fire;
      tables[job.fireAccessor] = asTableDefinition(fire);
    }

    const core: InternalCore = {
      tables,
      jobs: jobsByName,
      historyCap,
      reconcileEverySeconds,
      reconcileTick,
      reconcileRegistered: false,
      publicViewsRegistered: false,
      reconcileReducer(spacetimedb: CronSchema) {
        if (core.reconcileRegistered) {
          throw new Error('cron.reconcile_reducer_already_registered');
        }
        if (
          core.reconcileEverySeconds === undefined ||
          core.reconcileTick === undefined
        ) {
          throw new Error(
            'cron.reconcile_not_configured:set createCron({ reconcileEverySeconds })'
          );
        }
        requireRegisteredHandlers(core);
        core.reconcileRegistered = true;
        const tick = core.reconcileTick;
        return (spacetimedb as RegistrationSchema).reducer(
          { name: RECONCILE_REDUCER_NAME, onSchedule: tick },
          { arg: tick.rowType },
          (rawCtx: unknown) => {
            const ctx = asTransactionContext(rawCtx);
            requireDatabaseCaller(ctx);
            reconcileLostFires(ctx, core);
          }
        );
      },
      publicViews(spacetimedb: CronSchema): CronCorePublicViews {
        if (core.publicViewsRegistered) {
          throw new Error('cron.public_views_already_registered');
        }
        core.publicViewsRegistered = true;
        const jobsView = (spacetimedb as RegistrationSchema).anonymousView(
          { name: 'cron_jobs', public: true },
          t.array(cronJobViewRow),
          (rawCtx: unknown) => {
            const ctx = rawCtx as Pick<TransactionContext, 'db'>;
            return [...ctx.db.cronJob.iter()].map(row => ({
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
            }));
          }
        );
        return { jobs: jobsView };
      },
    };
    for (const job of jobsByName.values()) job.core = core;
    return core;
  }

  function schedule<Ctx>(
    rawCtx: Ctx,
    handle: CronJobReference,
    spec: ScheduleSpec,
    opts?: RuntimeScheduleOpts
  ): void {
    const ctx = asTransactionContext(rawCtx);
    const job = requireMetadata(handle);
    const core = requireCore(handle);
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
      throwInputError(error);
    }

    ensureReconcileTick(ctx, core);
    reconcileLostFires(ctx, core);

    const existing = ctx.db.cronJob.name.find(handle.jobName);
    const generation = (existing?.generation ?? 0n) + 1n;
    disarmFire(ctx, job);

    let row: JobRow = {
      name: handle.jobName,
      schedule: normalized.schedule,
      args: { tag: handle.jobName, value: args },
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
      throw new SenderError('cron.unsatisfiable_expression');
    }
    row = { ...row, nextRunAt };
    ctx.db.cronJob.name.update(row);
  }

  function unschedule<Ctx>(rawCtx: Ctx, handle: CronJobReference): void {
    const ctx = asTransactionContext(rawCtx);
    const job = requireMetadata(handle);
    const core = requireCore(handle);
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

  return { cronTable, createCron, schedule, unschedule };
}
