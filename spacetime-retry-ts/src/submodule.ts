import {
  Range,
  ScheduleAt,
  SenderError,
  t,
  table,
  type Infer,
  type ModuleExport,
  type SumBuilder,
  type VariantsObj,
} from 'spacetimedb/server';
import type { Identity, Timestamp } from 'spacetimedb';
import { retryFailed, type RetryResult } from './handler';

const ONE_SECOND_MICROS = 1_000_000n;
const MAX_ATTEMPTS = 10;
const MAX_BACKOFF_SECONDS = 3600;
const MAX_TASK_NAME_LENGTH = 128;
const MAX_ERROR_LENGTH = 2048;
const MAX_ROWS = 1000;

export const errors = {
  notAuthorized: 'retry.not_authorized',
  invalidTaskName: 'retry.invalid_task_name',
  invalidMaxAttempts: 'retry.invalid_max_attempts',
  invalidBackoffSeconds: 'retry.invalid_backoff_seconds',
  taskAlreadyExists: 'retry.task_already_exists',
  cannotRemoveLastAdmin: 'retry.cannot_remove_last_admin',
  unknownHandler: 'retry.unknown_handler',
} as const;

function retryTaskTable(args: SumBuilder<VariantsObj>) {
  return table(
    { name: 'retry_task', public: false },
    {
      scheduledId: t.u64().primaryKey().autoInc(),
      scheduledAt: t.scheduleAt(),
      name: t.string().index(),
      args,
      attempt: t.u8(),
      maxAttempts: t.u8(),
      backoffSecs: t.u32(),
    }
  );
}

const retryHistory = table(
  { name: 'retry_history', public: false },
  {
    id: t.u64().primaryKey().autoInc(),
    taskName: t.string(),
    attempt: t.u8(),
    status: t.enum('RetryHistoryStatus', ['Ok', 'Failed', 'GaveUp']),
    error: t.option(t.string()),
    ranAt: t.timestamp().index(),
  }
);

const retryAdminIdentity = table(
  { name: 'retry_admin_identity', public: false },
  {
    identity: t.identity().primaryKey(),
    addedAt: t.timestamp(),
  }
);

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

export interface RetryConfig<Tasks extends RetryTasks> {
  tasks: Tasks;
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

type InternalHandler = (...args: unknown[]) => RetryResult;

/** Create the retry tables and the functions that manage them. */
export function client<const Tasks extends RetryTasks>({
  tasks,
}: RetryConfig<Tasks>) {
  const variants: VariantsObj = tasks;
  const retryArgs = t.enum('RetryArgs', variants);
  const retryTask = retryTaskTable(retryArgs);

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
    try {
      const { tag } = arg.args;
      if (!Object.prototype.hasOwnProperty.call(handlers, tag)) {
        throw new Error(`${errors.unknownHandler}:${tag}`);
      }
      result = handlers[tag](ctx, 'value' in arg.args ? arg.args.value : {});
    } catch (error) {
      // A caught exception does not roll back the handler's earlier writes.
      result = retryFailed(
        error instanceof Error ? error.message : String(error)
      );
    }

    const isLast = !result.ok && arg.attempt + 1 >= arg.maxAttempts;
    const history = ctx.db.retryHistory;
    history.insert({
      id: 0n,
      taskName: arg.name,
      attempt: arg.attempt,
      status: { tag: result.ok ? 'Ok' : isLast ? 'GaveUp' : 'Failed' },
      error: result.ok ? undefined : result.error.slice(0, MAX_ERROR_LENGTH),
      ranAt: ctx.timestamp,
    });
    // Each attempt adds one row, so removing the oldest keeps the bound.
    if (history.count() > MAX_ROWS) {
      const oldest = history.ranAt.filter(new Range<Timestamp>()).next().value;
      if (oldest) history.delete(oldest);
    }
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

  return {
    tables: { retryTask, retryHistory, retryAdminIdentity },
    retryReducer,
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
        params: { identity: t.identity() },
        handler(ctx: RetryCtx, { identity }: { identity: Identity }): void {
          requireAdmin(ctx);
          addAdmin(ctx, identity);
        },
      },
      removeRetryAdminIdentity: {
        params: { identity: t.identity() },
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
