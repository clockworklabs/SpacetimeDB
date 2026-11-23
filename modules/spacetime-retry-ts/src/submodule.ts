import {
  Range,
  ScheduleAt,
  SenderError,
  t,
  table,
  type IdentityBuilder,
  type IdentityColumnBuilder,
  type Infer,
  type ModuleExport,
  type TimestampBuilder,
  type SumBuilder,
  type VariantsObj,
} from 'spacetimedb/server';
import type { Identity, Timestamp } from 'spacetimedb';
import { retryFailed, type RetryResult } from './handler.js';
import {
  jobs,
  retryJobFire,
  retryJobTable,
  retryQueue,
  type EnqueueInput,
  type RetryJobFireRow,
  type RetryJobCtx,
  type RetryJobInfo,
  type RetryJobRow,
  type RetryOutcome,
  type RetryProcedureCtx,
} from './jobs.js';

const ONE_SECOND_MICROS = 1_000_000n;
const MAX_ATTEMPTS = 10;
const MAX_BACKOFF_SECONDS = 3600;
const MAX_TASK_NAME_LENGTH = 128;
const MAX_ERROR_LENGTH = 2048;
const MAX_ROWS = 1000;

/** Codes a caller can receive as a `SenderError`. */
export const errors = {
  notAuthorized: 'retry.not_authorized',
  invalidTaskName: 'retry.invalid_task_name',
  invalidMaxAttempts: 'retry.invalid_max_attempts',
  invalidBackoffSeconds: 'retry.invalid_backoff_seconds',
  taskAlreadyExists: 'retry.task_already_exists',
  cannotRemoveLastAdmin: 'retry.cannot_remove_last_admin',
  invalidQueueName: 'retry.invalid_queue_name',
  invalidParallelism: 'retry.invalid_parallelism',
  leaseExpired: 'retry.lease_expired',
} as const;

function retryTaskColumns(args: SumBuilder<VariantsObj>) {
  return {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    name: t.string().index(),
    args,
    attempt: t.u8(),
    maxAttempts: t.u8(),
    backoffSecs: t.u32(),
  };
}

const retryTaskOptions = { name: 'retry_task', public: false } as const;
function retryTaskTable(
  args: SumBuilder<VariantsObj>
): ReturnType<
  typeof table<ReturnType<typeof retryTaskColumns>, typeof retryTaskOptions>
> {
  return table(retryTaskOptions, retryTaskColumns(args));
}

type RetryHistoryStatusBuilder = SumBuilder<
  Record<'Ok' | 'Failed' | 'GaveUp', ReturnType<typeof t.unit>>
>;
const historyStatus: RetryHistoryStatusBuilder = t.enum('RetryHistoryStatus', {
  Ok: t.unit(),
  Failed: t.unit(),
  GaveUp: t.unit(),
});
const historyOptions = { name: 'retry_history', public: false } as const;
const historyColumns = {
  id: t.u64().primaryKey().autoInc(),
  taskName: t.string(),
  attempt: t.u8(),
  status: historyStatus,
  error: t.option(t.string()),
  ranAt: t.timestamp().index(),
};
const retryHistory: ReturnType<
  typeof table<typeof historyColumns, typeof historyOptions>
> = table(historyOptions, historyColumns);

const adminOptions = { name: 'retry_admin_identity', public: false } as const;
const adminColumns: {
  identity: IdentityColumnBuilder<{ isPrimaryKey: true }>;
  addedAt: TimestampBuilder;
} = {
  identity: t.identity().primaryKey(),
  addedAt: t.timestamp(),
};
const retryAdminIdentity: ReturnType<
  typeof table<typeof adminColumns, typeof adminOptions>
> = table(adminOptions, adminColumns);

interface RetryTables {
  retryTask: ReturnType<typeof retryTaskTable>;
  retryHistory: typeof retryHistory;
  retryAdminIdentity: typeof retryAdminIdentity;
  retryJob: ReturnType<typeof retryJobTable>;
  retryJobFire: typeof retryJobFire;
  retryQueue: typeof retryQueue;
}

export type RetryTaskRow = Infer<ReturnType<typeof retryTaskTable>['rowType']>;
export type RetryHistoryRow = Infer<typeof retryHistory.rowType>;
type AdminRow = Infer<typeof retryAdminIdentity.rowType>;
type TaskInput = Pick<
  RetryTaskRow,
  'name' | 'args' | 'maxAttempts' | 'backoffSecs'
>;

/** The retry tables of a host view context. */
export interface RetryViewCtx {
  readonly sender: Identity;
  readonly db: {
    readonly retryTask: { iter(): Iterable<RetryTaskRow> };
    readonly retryHistory: {
      readonly ranAt: {
        filter(range: Range<Timestamp>): Iterable<RetryHistoryRow>;
      };
    };
    readonly retryAdminIdentity: {
      readonly identity: { find(identity: Identity): AdminRow | null };
    };
  };
}

/** The job table of a host view context. */
export interface RetryJobsViewCtx {
  readonly db: { readonly retryJob: { iter(): Iterable<RetryJobRow> } };
}

/** A host reducer context whose schema includes `retry.tables`. */
export interface RetryCtx {
  readonly sender: Identity;
  readonly timestamp: Timestamp;
  readonly db: {
    readonly retryTask: {
      iter(): Iterable<RetryTaskRow>;
      readonly name: {
        filter(name: string): IteratorObject<RetryTaskRow, undefined>;
      };
      insert(row: RetryTaskRow): RetryTaskRow;
    };
    readonly retryHistory: {
      readonly ranAt: {
        filter(
          range: Range<Timestamp>
        ): IteratorObject<RetryHistoryRow, undefined>;
      };
      insert(row: RetryHistoryRow): RetryHistoryRow;
      delete(row: RetryHistoryRow): boolean;
      count(): bigint;
    };
    readonly retryAdminIdentity: {
      readonly identity: { find(identity: Identity): AdminRow | null };
      insert(row: AdminRow): AdminRow;
      delete(row: AdminRow): boolean;
      count(): bigint;
    };
  };
}

/** Task argument types keyed by task tag. */
export type RetryTasks = VariantsObj;

export interface RetryConfig<
  Tasks extends RetryTasks,
  Jobs extends RetryTasks,
> {
  /** Tasks for `submit` and `retryReducer`. */
  tasks?: Tasks;
  /** Jobs for `enqueue` and `retryProcedure`. */
  jobs?: Jobs;
  /**
   * How long a procedure attempt may run before it counts as lost and the job
   * is retried. Set it above the slowest handler. Defaults to 300.
   */
  leaseSecs?: number;
}

/** Tagged task arguments for the configured tasks. */
export type RetryArgs<Tasks extends RetryTasks> = {
  [K in keyof Tasks & string]: Tasks[K] extends ReturnType<typeof t.unit>
    ? { tag: K }
    : { tag: K; value: Infer<Tasks[K]> };
}[keyof Tasks & string];

/** One handler per task tag. Each call runs one attempt. */
export type RetryHandlers<Tasks extends RetryTasks, Tx> = {
  [K in keyof Tasks & string]: (ctx: Tx, args: Infer<Tasks[K]>) => RetryResult;
};

/** The scheduled reducer that runs attempts. */
export type RetryReducerExport<Tx> = ModuleExport &
  ((ctx: Tx, args: { arg: RetryTaskRow }) => unknown);

/**
 * The host schema returned by `schema()`. Handler contexts are inferred from
 * its reducer exports.
 */
export interface RetrySchema<Tx extends RetryCtx = RetryCtx> {
  reducer(...args: unknown[]): RetryReducerExport<Tx>;
}

/** One procedure handler per job tag. Each call runs one attempt. */
export type RetryProcedureHandlers<Jobs extends RetryTasks, Proc> = {
  [K in keyof Jobs & string]: (
    ctx: Proc,
    args: Infer<Jobs[K]>,
    job: RetryJobInfo
  ) => RetryResult;
};

export interface RetryProcedureOptions<Jobs extends RetryTasks, Tx> {
  /**
   * Runs in the transaction that finishes a job, including `cancel`. A throw
   * rolls that transaction back.
   */
  onComplete?: (tx: Tx, outcome: RetryOutcome<RetryArgs<Jobs>>) => void;
}

/** The scheduled procedure that runs job attempts. */
export type RetryProcedureExport<Proc> = ModuleExport &
  ((ctx: Proc, args: { arg: RetryJobFireRow }) => unknown);

/** The host schema returned by `schema()`, for `retryProcedure`. */
export interface RetryProcedureSchema<
  Tx extends RetryJobCtx,
  Proc extends RetryProcedureCtx<Tx>,
> {
  reducer(
    ...args: unknown[]
  ): ModuleExport & ((ctx: Tx, args: never) => unknown);
  procedure(...args: unknown[]): RetryProcedureExport<Proc>;
}

type InternalHandler = (...args: unknown[]) => RetryResult;

const identityParams: { identity: IdentityBuilder } = {
  identity: t.identity(),
};

/** Create the retry tables and the functions that manage them. */
export function client<
  const Tasks extends RetryTasks = Record<never, never>,
  const Jobs extends RetryTasks = Record<never, never>,
>({ tasks, jobs: jobTypes, leaseSecs }: RetryConfig<Tasks, Jobs>) {
  const variants: VariantsObj = tasks ?? {};
  const retryArgs = t.enum('RetryArgs', variants);
  const retryTask = retryTaskTable(retryArgs);
  const jobVariants: VariantsObj = jobTypes ?? {};
  const retryJob = retryJobTable(t.enum('RetryJobArgs', jobVariants));
  const queue = jobs<RetryArgs<Jobs>>(retryJob, leaseSecs);
  const tables: RetryTables = {
    retryTask,
    retryHistory,
    retryAdminIdentity,
    retryJob,
    retryJobFire,
    retryQueue,
  };

  function isAdmin(ctx: RetryViewCtx): boolean {
    return ctx.db.retryAdminIdentity.identity.find(ctx.sender) != null;
  }

  function requireAdmin(ctx: RetryCtx): void {
    if (!isAdmin(ctx)) throw new SenderError(errors.notAuthorized);
  }

  function addAdmin(ctx: RetryCtx, identity: Identity): void {
    if (ctx.db.retryAdminIdentity.identity.find(identity) == null) {
      ctx.db.retryAdminIdentity.insert({ identity, addedAt: ctx.timestamp });
    }
  }

  function submitTask(ctx: RetryCtx, task: TaskInput): void {
    if (task.name.length === 0 || task.name.length > MAX_TASK_NAME_LENGTH) {
      throw new SenderError(errors.invalidTaskName);
    }
    if (
      !Number.isInteger(task.maxAttempts) ||
      task.maxAttempts < 1 ||
      task.maxAttempts > MAX_ATTEMPTS
    ) {
      throw new SenderError(errors.invalidMaxAttempts);
    }
    if (
      !Number.isInteger(task.backoffSecs) ||
      task.backoffSecs < 1 ||
      task.backoffSecs > MAX_BACKOFF_SECONDS
    ) {
      throw new SenderError(errors.invalidBackoffSeconds);
    }
    if (!ctx.db.retryTask.name.filter(task.name).next().done) {
      throw new SenderError(`${errors.taskAlreadyExists}:${task.name}`);
    }
    ctx.db.retryTask.insert({
      ...task,
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(ctx.timestamp.microsSinceUnixEpoch),
      attempt: 0,
    });
  }

  function runAttempt(
    handlers: Record<string, InternalHandler>,
    ctx: RetryCtx,
    arg: RetryTaskRow
  ): void {
    let result: RetryResult;
    let permanent = false;
    try {
      const { tag } = arg.args;
      if (!Object.prototype.hasOwnProperty.call(handlers, tag)) {
        // Only reachable when handlers bypass `RetryHandlers` typing.
        throw new Error(`retry.unknown_handler:${tag}`);
      }
      result = handlers[tag](ctx, 'value' in arg.args ? arg.args.value : {});
    } catch (error) {
      // A caught exception does not roll back the handler's earlier writes.
      // A `SenderError` rejects the task's arguments, so retrying cannot help.
      permanent = error instanceof SenderError;
      result = retryFailed(
        error instanceof Error ? error.message : String(error)
      );
    }

    const isLast =
      !result.ok && (permanent || arg.attempt + 1 >= arg.maxAttempts);
    const history = ctx.db.retryHistory;
    // Each attempt adds one row, so removing the oldest keeps the bound. Prune
    // before inserting: an index scan yields rows inserted in the current
    // transaction before committed rows.
    if (history.count() >= MAX_ROWS) {
      const oldest = history.ranAt.filter(new Range<Timestamp>()).next().value;
      if (oldest) history.delete(oldest);
    }
    history.insert({
      id: 0n,
      taskName: arg.name,
      attempt: arg.attempt,
      status: { tag: result.ok ? 'Ok' : isLast ? 'GaveUp' : 'Failed' },
      error: result.ok ? undefined : result.error.slice(0, MAX_ERROR_LENGTH),
      ranAt: ctx.timestamp,
    });
    if (result.ok || isLast) return;

    const delay =
      BigInt(arg.backoffSecs) * (1n << BigInt(arg.attempt)) * ONE_SECOND_MICROS;
    ctx.db.retryTask.insert({
      ...arg,
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(ctx.timestamp.microsSinceUnixEpoch + delay),
      attempt: arg.attempt + 1,
    });
  }

  /**
   * Register the scheduled reducer that runs attempts. Call after `schema()`
   * and export the result.
   */
  function retryReducer<Tx extends RetryCtx>(
    spacetimedb: RetrySchema<Tx>,
    handlers: RetryHandlers<Tasks, Tx>
  ): RetryReducerExport<Tx> {
    // Each stored task pairs a tag with a value of that tag's argument type,
    // so the handler for the tag accepts the value.
    const byTag = handlers as Record<string, InternalHandler>;
    return spacetimedb.reducer(
      { onSchedule: retryTask },
      { arg: retryTask.rowType },
      (ctx: Tx, { arg }: { arg: RetryTaskRow }) => {
        runAttempt(byTag, ctx, arg);
      }
    );
  }

  /**
   * Register the scheduled procedure that runs jobs from `enqueue`. Handlers
   * run outside a transaction, so they can call `ctx.http.fetch`. Call after
   * `schema()` and export the result.
   */
  function retryProcedure<
    Tx extends RetryJobCtx,
    Proc extends RetryProcedureCtx<Tx>,
  >(
    spacetimedb: RetryProcedureSchema<Tx, Proc>,
    handlers: RetryProcedureHandlers<Jobs, Proc>,
    options: RetryProcedureOptions<Jobs, Tx> = {}
  ): RetryProcedureExport<Proc> {
    const byTag = handlers as Record<string, InternalHandler>;
    queue.onComplete(
      options.onComplete as Parameters<typeof queue.onComplete>[0]
    );
    return spacetimedb.procedure(
      { name: 'retry_run_job', onSchedule: retryJobFire },
      { arg: retryJobFire.rowType },
      t.unit(),
      (ctx: Proc, { arg }: { arg: RetryJobFireRow }) => {
        queue.runAttempt(ctx, arg, byTag);
        return {};
      }
    );
  }

  return {
    tables,
    retryReducer,
    retryProcedure,
    /**
     * Add a job for `retryProcedure` and return its ID. Jobs in one queue run
     * up to the queue's parallelism, oldest first. The caller authorizes.
     */
    enqueue(ctx: RetryJobCtx, job: EnqueueInput<RetryArgs<Jobs>>): bigint {
      return queue.enqueue(ctx, job);
    },
    /** Stop a job. Returns false if it had already finished. The caller authorizes. */
    cancel: queue.cancel,
    /** A job's current row, kept for the last 1000 jobs of each final status. */
    status: queue.status,
    /** Limit how many jobs in a queue run at once. 0 pauses it. The caller authorizes. */
    setQueue: queue.setQueue,
    /** Call from the host's init reducer to seed the publishing identity as admin. */
    install(ctx: RetryCtx): void {
      addAdmin(ctx, ctx.sender);
    },
    /** Validate and schedule a task's first attempt. The caller authorizes. */
    submit(
      ctx: RetryCtx,
      task: Omit<TaskInput, 'args'> & { args: RetryArgs<Tasks> }
    ): void {
      submitTask(ctx, task);
    },
    requireAdmin,
    views: {
      /** The latest pending tasks, newest first. */
      retryTasksAdmin(ctx: RetryViewCtx): RetryTaskRow[] {
        if (!isAdmin(ctx)) return [];
        return [...ctx.db.retryTask.iter()]
          .sort((a, b) =>
            a.scheduledId > b.scheduledId
              ? -1
              : a.scheduledId < b.scheduledId
                ? 1
                : 0
          )
          .slice(0, MAX_ROWS);
      },
      /** Jobs from `enqueue`, newest first. */
      retryJobsAdmin(ctx: RetryViewCtx & RetryJobsViewCtx): RetryJobRow[] {
        if (!isAdmin(ctx)) return [];
        return [...ctx.db.retryJob.iter()]
          .sort((a, b) => (a.id > b.id ? -1 : a.id < b.id ? 1 : 0))
          .slice(0, MAX_ROWS);
      },
      /** Retained attempt history, newest first. */
      retryHistoryAdmin(ctx: RetryViewCtx): RetryHistoryRow[] {
        if (!isAdmin(ctx)) return [];
        return [
          ...ctx.db.retryHistory.ranAt.filter(new Range<Timestamp>()),
        ].reverse();
      },
    },
    reducers: {
      submitRetryTask: {
        params: {
          name: t.string(),
          args: retryArgs,
          maxAttempts: t.u8(),
          backoffSecs: t.u32(),
        },
        handler(ctx: RetryCtx, task: TaskInput): void {
          requireAdmin(ctx);
          submitTask(ctx, task);
        },
      },
      addRetryAdminIdentity: {
        params: identityParams,
        handler(ctx: RetryCtx, { identity }: { identity: Identity }): void {
          requireAdmin(ctx);
          addAdmin(ctx, identity);
        },
      },
      cancelRetryJob: {
        params: { id: t.u64() },
        handler(ctx: RetryCtx & RetryJobCtx, { id }: { id: bigint }): void {
          requireAdmin(ctx);
          queue.cancel(ctx, id);
        },
      },
      setRetryQueue: {
        params: { name: t.string(), maxParallelism: t.u32() },
        handler(
          ctx: RetryCtx & RetryJobCtx,
          args: { name: string; maxParallelism: number }
        ): void {
          requireAdmin(ctx);
          queue.setQueue(ctx, args.name, args);
        },
      },
      removeRetryAdminIdentity: {
        params: identityParams,
        handler(ctx: RetryCtx, { identity }: { identity: Identity }): void {
          requireAdmin(ctx);
          const existing = ctx.db.retryAdminIdentity.identity.find(identity);
          if (!existing) return;
          if (ctx.db.retryAdminIdentity.count() <= 1n) {
            throw new SenderError(errors.cannotRemoveLastAdmin);
          }
          ctx.db.retryAdminIdentity.delete(existing);
        },
      },
    },
  };
}
