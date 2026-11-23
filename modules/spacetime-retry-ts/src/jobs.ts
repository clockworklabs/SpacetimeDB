import {
  Range,
  ScheduleAt,
  SenderError,
  t,
  table,
  type Infer,
  type SumBuilder,
  type VariantsObj,
} from 'spacetimedb/server';
import { Timestamp } from 'spacetimedb';
import { retryFailed, type RetryResult } from './handler.js';

const ONE_SECOND_MICROS = 1_000_000n;
const MAX_ATTEMPTS = 10;
const MAX_BACKOFF_SECONDS = 3600;
const MAX_NAME_LENGTH = 128;
const MAX_ERROR_LENGTH = 2048;
const MAX_ROWS = 1000;
const MAX_PARALLELISM = 1000;
const DEFAULT_LEASE_SECONDS = 300;

export const jobErrors = {
  invalidTaskName: 'retry.invalid_task_name',
  invalidQueueName: 'retry.invalid_queue_name',
  invalidMaxAttempts: 'retry.invalid_max_attempts',
  invalidBackoffSeconds: 'retry.invalid_backoff_seconds',
  invalidParallelism: 'retry.invalid_parallelism',
  taskAlreadyExists: 'retry.task_already_exists',
  notAuthorized: 'retry.not_authorized',
  leaseExpired: 'retry.lease_expired',
} as const;

type JobStatusBuilder = SumBuilder<
  Record<
    'Queued' | 'Parked' | 'Running' | 'Succeeded' | 'Failed' | 'Cancelled',
    ReturnType<typeof t.unit>
  >
>;
const jobStatus: JobStatusBuilder = t.enum('RetryJobStatus', {
  Queued: t.unit(),
  Parked: t.unit(),
  Running: t.unit(),
  Succeeded: t.unit(),
  Failed: t.unit(),
  Cancelled: t.unit(),
});
type JobStatusTag = Infer<typeof jobStatus>['tag'];
const FINISHED: JobStatusTag[] = ['Succeeded', 'Failed', 'Cancelled'];

function retryJobColumns(args: SumBuilder<VariantsObj>) {
  return {
    id: t.u64().primaryKey().autoInc(),
    name: t.string().index(),
    queue: t.string(),
    args,
    status: jobStatus,
    attempt: t.u8(),
    maxAttempts: t.u8(),
    backoffSecs: t.u32(),
    runAt: t.timestamp(),
    generation: t.u64(),
    error: t.option(t.string()),
    createdAt: t.timestamp(),
    updatedAt: t.timestamp(),
  };
}
const retryJobOptions = {
  name: 'retry_job',
  public: false,
  indexes: [
    {
      accessor: 'byQueueStatus',
      algorithm: 'btree',
      columns: ['queue', 'status', 'id'],
    } as const,
    {
      accessor: 'byStatusUpdatedAt',
      algorithm: 'btree',
      columns: ['status', 'updatedAt'],
    } as const,
  ],
};
export function retryJobTable(
  args: SumBuilder<VariantsObj>
): ReturnType<
  typeof table<ReturnType<typeof retryJobColumns>, typeof retryJobOptions>
> {
  return table(retryJobOptions, retryJobColumns(args));
}

const fireOptions = { name: 'retry_job_fire', public: false } as const;
const fireColumns = {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
  jobId: t.u64().index(),
  generation: t.u64(),
};
export const retryJobFire: ReturnType<
  typeof table<typeof fireColumns, typeof fireOptions>
> = table(fireOptions, fireColumns);

const queueOptions = { name: 'retry_queue', public: false } as const;
const queueColumns = {
  name: t.string().primaryKey(),
  maxParallelism: t.u32(),
  updatedAt: t.timestamp(),
};
export const retryQueue: ReturnType<
  typeof table<typeof queueColumns, typeof queueOptions>
> = table(queueOptions, queueColumns);

export type RetryJobRow = Infer<ReturnType<typeof retryJobTable>['rowType']>;
export type RetryJobFireRow = Infer<typeof retryJobFire.rowType>;
export type RetryQueueRow = Infer<typeof retryQueue.rowType>;
type HistoryRow = {
  id: bigint;
  taskName: string;
  attempt: number;
  status: { tag: 'Ok' | 'Failed' | 'GaveUp' };
  error: string | undefined;
  ranAt: Timestamp;
};

type Status<Tag extends JobStatusTag> = { tag: Tag };
type IndexScan<Row> = IteratorObject<Row, undefined>;

/** A transaction whose schema includes `retry.tables`. */
export interface RetryJobCtx {
  readonly timestamp: Timestamp;
  readonly senderAuth: { readonly isInternal: boolean };
  readonly random: () => number;
  readonly db: {
    readonly retryJob: {
      readonly id: {
        find(id: bigint): RetryJobRow | null;
        update(row: RetryJobRow): RetryJobRow;
      };
      readonly name: { filter(name: string): IndexScan<RetryJobRow> };
      readonly byQueueStatus: {
        filter(prefix: [string, Status<JobStatusTag>]): IndexScan<RetryJobRow>;
      };
      readonly byStatusUpdatedAt: {
        filter(
          prefix: [Status<JobStatusTag>, Range<Timestamp>]
        ): IndexScan<RetryJobRow>;
      };
      insert(row: RetryJobRow): RetryJobRow;
      delete(row: RetryJobRow): boolean;
    };
    readonly retryJobFire: {
      readonly jobId: { filter(jobId: bigint): IndexScan<RetryJobFireRow> };
      insert(row: RetryJobFireRow): RetryJobFireRow;
      delete(row: RetryJobFireRow): boolean;
    };
    readonly retryQueue: {
      readonly name: {
        find(name: string): RetryQueueRow | null;
        update(row: RetryQueueRow): RetryQueueRow;
      };
      insert(row: RetryQueueRow): RetryQueueRow;
    };
    readonly retryHistory: {
      readonly ranAt: {
        filter(range: Range<Timestamp>): IndexScan<HistoryRow>;
      };
      insert(row: HistoryRow): HistoryRow;
      delete(row: HistoryRow): boolean;
      count(): bigint;
    };
  };
}

/** A host procedure context whose schema includes `retry.tables`. */
export interface RetryProcedureCtx<Tx extends RetryJobCtx = RetryJobCtx> {
  withTx<T>(body: (tx: Tx) => T): T;
}

/** Identifies the current attempt. Passed to procedure handlers. */
export interface RetryJobInfo {
  readonly id: bigint;
  readonly name: string;
  /** 1 for the first attempt. */
  readonly attempt: number;
  /** The same for every attempt of this job. Send it as an idempotency key. */
  readonly idempotencyKey: string;
}

/** How a job ended. Passed to `onComplete`. */
export interface RetryOutcome<Args> {
  readonly id: bigint;
  readonly name: string;
  readonly queue: string;
  readonly args: Args;
  readonly status: 'Succeeded' | 'Failed' | 'Cancelled';
  readonly attempts: number;
  readonly error: string | undefined;
}

export interface EnqueueInput<Args> {
  name: string;
  args: Args;
  maxAttempts: number;
  backoffSecs: number;
  /** Defaults to `default`. */
  queue?: string;
  /** Defaults to now. */
  runAt?: Timestamp;
}

type InternalHandler = (
  ctx: unknown,
  args: unknown,
  job: RetryJobInfo
) => RetryResult;

function validName(name: string): boolean {
  return name.length > 0 && name.length <= MAX_NAME_LENGTH;
}

function take<T>(rows: Iterable<T>, limit: number): T[] {
  const out: T[] = [];
  for (const row of rows) {
    if (out.length >= limit) break;
    out.push(row);
  }
  return out;
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** The procedure mode of a retry client. */
export function jobs<Args extends { tag: string }>(
  retryJob: ReturnType<typeof retryJobTable>,
  leaseSecs = DEFAULT_LEASE_SECONDS
) {
  const leaseMicros = BigInt(leaseSecs) * ONE_SECOND_MICROS;
  let onComplete:
    | ((tx: unknown, outcome: RetryOutcome<Args>) => void)
    | undefined;

  function complete(
    ctx: RetryJobCtx,
    job: RetryJobRow,
    status: RetryOutcome<Args>['status'],
    error: string | undefined
  ): void {
    const done = update(ctx, job, { status: { tag: status }, error });
    onComplete?.(ctx, {
      id: done.id,
      name: done.name,
      queue: done.queue,
      args: done.args as Args,
      status,
      attempts: done.attempt,
      error,
    });
    if (job.status.tag === 'Running') wake(ctx, done.queue);
    prune(ctx, status);
  }

  function fireAt(ctx: RetryJobCtx, job: RetryJobRow, at: bigint): void {
    ctx.db.retryJobFire.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(at),
      jobId: job.id,
      generation: job.generation,
    });
  }

  function clearFires(ctx: RetryJobCtx, jobId: bigint): void {
    for (const fire of [...ctx.db.retryJobFire.jobId.filter(jobId)]) {
      ctx.db.retryJobFire.delete(fire);
    }
  }

  /**
   * The host deletes a scheduled procedure's row before calling it, so a
   * second row keeps the job alive if the host stops before claiming it.
   */
  function schedule(ctx: RetryJobCtx, job: RetryJobRow): void {
    const at = job.runAt.microsSinceUnixEpoch;
    fireAt(ctx, job, at);
    fireAt(ctx, job, at + leaseMicros);
  }

  function update(
    ctx: RetryJobCtx,
    job: RetryJobRow,
    changes: Partial<RetryJobRow>
  ): RetryJobRow {
    return ctx.db.retryJob.id.update({
      ...job,
      ...changes,
      generation: job.generation + 1n,
      updatedAt: ctx.timestamp,
    });
  }

  function running(ctx: RetryJobCtx, queue: string): number {
    let count = 0;
    for (const _ of ctx.db.retryJob.byQueueStatus.filter([
      queue,
      { tag: 'Running' },
    ]))
      count++;
    return count;
  }

  function capacity(ctx: RetryJobCtx, queue: string): number {
    const limit =
      ctx.db.retryQueue.name.find(queue)?.maxParallelism ?? MAX_PARALLELISM;
    return Math.max(0, limit - running(ctx, queue));
  }

  /** Start parked jobs while the queue has room, oldest first. */
  function wake(ctx: RetryJobCtx, queue: string): void {
    const room = capacity(ctx, queue);
    if (room === 0) return;
    const parked = take(
      ctx.db.retryJob.byQueueStatus.filter([queue, { tag: 'Parked' }]),
      room
    );
    for (const job of parked) {
      schedule(
        ctx,
        update(ctx, job, { status: { tag: 'Queued' }, runAt: ctx.timestamp })
      );
    }
  }

  function recordHistory(
    ctx: RetryJobCtx,
    job: RetryJobRow,
    status: HistoryRow['status']['tag'],
    error: string | undefined
  ): void {
    const history = ctx.db.retryHistory;
    if (history.count() >= MAX_ROWS) {
      const oldest = history.ranAt.filter(new Range<Timestamp>()).next().value;
      if (oldest) history.delete(oldest);
    }
    history.insert({
      id: 0n,
      taskName: job.name,
      attempt: job.attempt - 1,
      status: { tag: status },
      error,
      ranAt: ctx.timestamp,
    });
  }

  /** Keep at most `MAX_ROWS` finished jobs of each status. */
  function prune(ctx: RetryJobCtx, status: JobStatusTag): void {
    const scan = () =>
      ctx.db.retryJob.byStatusUpdatedAt.filter([
        { tag: status },
        new Range<Timestamp>(),
      ]);
    const excess = [...scan()].length - MAX_ROWS;
    for (const row of take(scan(), excess)) ctx.db.retryJob.delete(row);
  }

  return {
    enqueue(ctx: RetryJobCtx, input: EnqueueInput<Args>): bigint {
      const queue = input.queue ?? 'default';
      if (!validName(input.name)) {
        throw new SenderError(jobErrors.invalidTaskName);
      }
      if (!validName(queue)) throw new SenderError(jobErrors.invalidQueueName);
      if (
        !Number.isInteger(input.maxAttempts) ||
        input.maxAttempts < 1 ||
        input.maxAttempts > MAX_ATTEMPTS
      ) {
        throw new SenderError(jobErrors.invalidMaxAttempts);
      }
      if (
        !Number.isInteger(input.backoffSecs) ||
        input.backoffSecs < 1 ||
        input.backoffSecs > MAX_BACKOFF_SECONDS
      ) {
        throw new SenderError(jobErrors.invalidBackoffSeconds);
      }
      for (const job of ctx.db.retryJob.name.filter(input.name)) {
        if (!FINISHED.includes(job.status.tag)) {
          throw new SenderError(`${jobErrors.taskAlreadyExists}:${input.name}`);
        }
      }
      const job = ctx.db.retryJob.insert({
        id: 0n,
        name: input.name,
        queue,
        args: input.args as RetryJobRow['args'],
        status: { tag: 'Queued' },
        attempt: 0,
        maxAttempts: input.maxAttempts,
        backoffSecs: input.backoffSecs,
        runAt: input.runAt ?? ctx.timestamp,
        generation: 0n,
        error: undefined,
        createdAt: ctx.timestamp,
        updatedAt: ctx.timestamp,
      });
      schedule(ctx, job);
      return job.id;
    },

    /**
     * Stop a job that hasn't finished. A running attempt keeps going, but its
     * result is dropped. Returns false if the job had already finished.
     */
    cancel(ctx: RetryJobCtx, id: bigint): boolean {
      const job = ctx.db.retryJob.id.find(id);
      if (!job || FINISHED.includes(job.status.tag)) return false;
      clearFires(ctx, id);
      complete(ctx, job, 'Cancelled', undefined);
      return true;
    },

    status(ctx: RetryJobCtx, id: bigint): RetryJobRow | undefined {
      return ctx.db.retryJob.id.find(id) ?? undefined;
    },

    /** Limit how many jobs in `queue` run at once. 0 pauses the queue. */
    setQueue(
      ctx: RetryJobCtx,
      name: string,
      { maxParallelism }: { maxParallelism: number }
    ): void {
      if (!validName(name)) throw new SenderError(jobErrors.invalidQueueName);
      if (
        !Number.isInteger(maxParallelism) ||
        maxParallelism < 0 ||
        maxParallelism > MAX_PARALLELISM
      ) {
        throw new SenderError(jobErrors.invalidParallelism);
      }
      const row = { name, maxParallelism, updatedAt: ctx.timestamp };
      if (ctx.db.retryQueue.name.find(name)) ctx.db.retryQueue.name.update(row);
      else ctx.db.retryQueue.insert(row);
      wake(ctx, name);
    },

    /** Set the callback for finished jobs. Called once, at module load. */
    onComplete(
      callback: ((tx: unknown, outcome: RetryOutcome<Args>) => void) | undefined
    ): void {
      onComplete = callback;
    },

    /** Run one attempt for a fired row. */
    runAttempt(
      ctx: RetryProcedureCtx,
      fire: RetryJobFireRow,
      handlers: Record<string, InternalHandler>
    ): void {
      // Record a failed attempt, then retry or give up.
      const failed = (
        tx: RetryJobCtx,
        job: RetryJobRow,
        error: string,
        permanent: boolean
      ) => {
        const message = error.slice(0, MAX_ERROR_LENGTH);
        clearFires(tx, job.id);
        if (permanent || job.attempt >= job.maxAttempts) {
          recordHistory(tx, job, 'GaveUp', message);
          complete(tx, job, 'Failed', message);
          return;
        }
        recordHistory(tx, job, 'Failed', message);
        const base =
          BigInt(job.backoffSecs) *
          (1n << BigInt(job.attempt - 1)) *
          ONE_SECOND_MICROS;
        const jitter = BigInt(Math.floor(Number(base) * 0.5 * tx.random()));
        const retried = update(tx, job, {
          status: { tag: 'Queued' },
          runAt: new Timestamp(
            tx.timestamp.microsSinceUnixEpoch + base - base / 4n + jitter
          ),
          error: message,
        });
        schedule(tx, retried);
        wake(tx, job.queue);
      };

      const claimed = ctx.withTx(tx => {
        if (!tx.senderAuth.isInternal) {
          throw new SenderError(jobErrors.notAuthorized);
        }
        const job = tx.db.retryJob.id.find(fire.jobId);
        if (!job || job.generation !== fire.generation) return undefined;
        if (job.status.tag === 'Running') {
          // The guard row fired: the attempt never recorded a result.
          failed(tx, job, jobErrors.leaseExpired, false);
          return undefined;
        }
        if (job.status.tag !== 'Queued') return undefined;
        clearFires(tx, job.id);
        if (capacity(tx, job.queue) === 0) {
          update(tx, job, { status: { tag: 'Parked' } });
          return undefined;
        }
        const started = update(tx, job, {
          status: { tag: 'Running' },
          attempt: job.attempt + 1,
        });
        fireAt(tx, started, tx.timestamp.microsSinceUnixEpoch + leaseMicros);
        return started;
      });
      if (!claimed) return;

      let result: RetryResult;
      let permanent = false;
      try {
        const { tag } = claimed.args;
        if (!Object.prototype.hasOwnProperty.call(handlers, tag)) {
          throw new Error(`retry.unknown_handler:${tag}`);
        }
        result = handlers[tag](
          ctx,
          'value' in claimed.args ? claimed.args.value : {},
          {
            id: claimed.id,
            name: claimed.name,
            attempt: claimed.attempt,
            idempotencyKey: `retry-job-${claimed.id}`,
          }
        );
      } catch (error) {
        permanent = error instanceof SenderError;
        result = retryFailed(errorText(error));
      }

      ctx.withTx(tx => {
        const job = tx.db.retryJob.id.find(claimed.id);
        if (!job || job.generation !== claimed.generation) return;
        if (!result.ok) return failed(tx, job, result.error, permanent);
        clearFires(tx, job.id);
        recordHistory(tx, job, 'Ok', undefined);
        complete(tx, job, 'Succeeded', undefined);
      });
    },
  };
}
