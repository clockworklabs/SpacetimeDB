import {
  Range,
  ScheduleAt,
  SenderError,
  t,
  table,
  type Infer,
  type VariantsObj,
} from 'spacetimedb/server';
import type { Identity, Timestamp } from 'spacetimedb';
import { retryFailed, type RetryHandler, type RetryResult } from './handler';

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

export type RetryHandlers = Record<string, RetryHandler>;

export interface RetryConfig<H extends RetryHandlers> {
  /** Task variants keyed by tag. Each handler runs one attempt. */
  handlers: H;
}

/** Tagged task arguments for the configured handlers. */
export type RetryArgs<H extends RetryHandlers> = {
  [K in keyof H & string]: H[K]['args'] extends ReturnType<typeof t.unit>
    ? { tag: K }
    : { tag: K; value: Infer<H[K]['args']> };
}[keyof H & string];

/** Create the retry tables, reducers, and views from the host's handlers. */
export function client<const H extends RetryHandlers>({
  handlers,
}: RetryConfig<H>) {
  const variants: VariantsObj = {};
  for (const [tag, handler] of Object.entries(handlers)) {
    variants[tag] = handler.args;
  }
  const retryArgs = t.enum('RetryArgs', variants);

  const retryTask = table(
    { name: 'retry_task', public: false },
    {
      scheduledId: t.u64().primaryKey().autoInc(),
      scheduledAt: t.scheduleAt(),
      name: t.string().index(),
      args: retryArgs,
      attempt: t.u8(),
      maxAttempts: t.u8(),
      backoffSecs: t.u32(),
    }
  );

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

  type TaskRow = Infer<typeof retryTask.rowType>;
  type HistoryRow = Infer<typeof retryHistory.rowType>;
  type AdminRow = Infer<typeof retryAdminIdentity.rowType>;
  type TaskInput = Pick<
    TaskRow,
    'name' | 'args' | 'maxAttempts' | 'backoffSecs'
  >;

  /** The parts of a host view context that retry reads. */
  interface RetryViewCtx {
    readonly sender: Identity;
    readonly db: {
      readonly retryTask: { iter(): Iterable<TaskRow> };
      readonly retryHistory: {
        readonly ranAt: {
          filter(range: Range<Timestamp>): Iterable<HistoryRow>;
        };
      };
      readonly retryAdminIdentity: {
        readonly identity: { find(identity: Identity): AdminRow | null };
      };
    };
  }

  /** The parts of a host reducer context that retry reads and writes. */
  interface RetryCtx {
    readonly sender: Identity;
    readonly timestamp: Timestamp;
    readonly db: {
      readonly retryTask: {
        iter(): Iterable<TaskRow>;
        readonly name: {
          filter(name: string): IteratorObject<TaskRow, undefined>;
        };
        insert(row: TaskRow): TaskRow;
      };
      readonly retryHistory: {
        readonly ranAt: {
          filter(
            range: Range<Timestamp>
          ): IteratorObject<HistoryRow, undefined>;
        };
        insert(row: HistoryRow): HistoryRow;
        delete(row: HistoryRow): boolean;
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

  /** Call from the host's init reducer to seed the publishing identity as admin. */
  function install(ctx: RetryCtx): void {
    addAdmin(ctx, ctx.sender);
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

  function dispatch(ctx: RetryCtx, args: TaskRow['args']): RetryResult {
    if (!Object.prototype.hasOwnProperty.call(handlers, args.tag)) {
      throw new Error(`${errors.unknownHandler}:${args.tag}`);
    }
    const value = 'value' in args ? args.value : undefined;
    return handlers[args.tag].run(ctx, value);
  }

  function retryFire(ctx: RetryCtx, { arg }: { arg: TaskRow }): void {
    let result: RetryResult;
    try {
      result = dispatch(ctx, arg.args);
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
    // Each fire adds one row, so removing the oldest keeps the bound.
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

  /** The latest pending tasks, newest first. */
  function retryTasksAdmin(ctx: RetryViewCtx): TaskRow[] {
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
  }

  /** Retained attempt history, newest first. */
  function retryHistoryAdmin(ctx: RetryViewCtx): HistoryRow[] {
    if (!isAdmin(ctx)) return [];
    return [
      ...ctx.db.retryHistory.ranAt.filter(new Range<Timestamp>()),
    ].reverse();
  }

  return {
    tables: { retryTask, retryHistory, retryAdminIdentity },
    install,
    /** Validate and schedule a task's first attempt. The caller authorizes. */
    submit(
      ctx: RetryCtx,
      task: Omit<TaskInput, 'args'> & { args: RetryArgs<H> }
    ): void {
      submitTask(ctx, task);
    },
    requireAdmin,
    views: { retryTasksAdmin, retryHistoryAdmin },
    reducers: {
      retryFire,
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
  } as const;
}
