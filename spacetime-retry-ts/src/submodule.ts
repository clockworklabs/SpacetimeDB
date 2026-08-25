import { makeRetryDispatch, type RetryHandler } from './kit';
import type { Identity, ScheduleAt, Timestamp } from 'spacetimedb';
import type { Infer, VariantsObj } from 'spacetimedb/server';

const ONE_SECOND_MICROS = 1_000_000n;
const MAX_ATTEMPTS = 10;
const MAX_BACKOFF_SECONDS = 3600;
const MAX_TASK_NAME_LENGTH = 128;
const MAX_ERROR_LENGTH = 2048;

export type RetryHandlers = Record<string, RetryHandler>;

export type RetrySubmoduleDeps = {
  table: (typeof import('spacetimedb/server'))['table'];
  t: (typeof import('spacetimedb/server'))['t'];
  SenderError: new (message: string) => Error;
  ScheduleAt: { time(microsSinceUnixEpoch: bigint): ScheduleAt };
};

export type RetryAuthorizationPolicy = {
  isAdmin?: (ctx: unknown) => boolean;
  requireAdmin?: (ctx: unknown) => void;
};

export function createRetrySubmodule<const H extends RetryHandlers>(
  deps: RetrySubmoduleDeps,
  handlers: H,
  auth: RetryAuthorizationPolicy = {}
) {
  const { table, t, SenderError, ScheduleAt } = deps;
  const retryArgs = t.enum('RetryArgs', handlers as unknown as VariantsObj);

  const retryTask = table(
    {
      name: 'retry_task',
      public: false,
    },
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

  const retryHistoryStatus = t.enum('RetryHistoryStatus', [
    'Attempted',
    'Ok',
    'Failed',
    'GaveUp',
  ]);
  const RetryHistoryStatus = {
    Attempted: { tag: 'Attempted' as const },
    Ok: { tag: 'Ok' as const },
    Failed: { tag: 'Failed' as const },
    GaveUp: { tag: 'GaveUp' as const },
  };

  const retryHistory = table(
    { name: 'retry_history', public: false },
    {
      id: t.u64().primaryKey().autoInc(),
      taskName: t.string().index(),
      attempt: t.u8(),
      status: retryHistoryStatus.index(),
      error: t.option(t.string()),
      ranAt: t.timestamp().index(),
    }
  );

  const retryAdminIdentity = table(
    { name: 'retry_admin_identity', public: false },
    {
      identity: t.identity().primaryKey(),
      addedAtMicros: t.i64(),
    }
  );

  type RetryDispatchArg = { tag: string; value?: unknown };
  type RetryTaskRow = Infer<typeof retryTask.rowType>;
  type RetryHistoryRow = Infer<typeof retryHistory.rowType>;
  type RetryAdminIdentityRow = Infer<typeof retryAdminIdentity.rowType>;

  interface RetryContext {
    sender: Identity;
    timestamp: Timestamp;
    db: {
      retryTask: {
        name: { filter(name: string): IterableIterator<RetryTaskRow> };
        insert(row: RetryTaskRow): RetryTaskRow;
        iter(): Iterable<RetryTaskRow>;
      };
      retryHistory: {
        id: { update(row: RetryHistoryRow): void };
        insert(row: RetryHistoryRow): RetryHistoryRow;
        iter(): Iterable<RetryHistoryRow>;
      };
      retryAdminIdentity: {
        identity: {
          find(identity: Identity): RetryAdminIdentityRow | null | undefined;
        };
        insert(row: RetryAdminIdentityRow): RetryAdminIdentityRow;
        delete(row: RetryAdminIdentityRow): void;
        count(): bigint;
      };
    };
  }

  function retryContext(ctx: unknown): RetryContext {
    return ctx as RetryContext;
  }

  const dispatchRetry = makeRetryDispatch(handlers);

  function installRetry(ctx: unknown): void {
    const retryCtx = retryContext(ctx);
    if (retryCtx.db.retryAdminIdentity.identity.find(retryCtx.sender) == null) {
      retryCtx.db.retryAdminIdentity.insert({
        identity: retryCtx.sender,
        addedAtMicros: retryCtx.timestamp.microsSinceUnixEpoch,
      });
    }
  }

  function requireAdmin(ctx: unknown): void {
    if (auth.requireAdmin) {
      auth.requireAdmin(ctx);
      return;
    }
    const retryCtx = retryContext(ctx);
    if (retryCtx.db.retryAdminIdentity.identity.find(retryCtx.sender) == null) {
      throw new SenderError('retry.not_authorized');
    }
  }

  function isAdmin(ctx: unknown): boolean {
    if (auth.isAdmin) return auth.isAdmin(ctx);
    const retryCtx = retryContext(ctx);
    return (
      retryCtx.db.retryAdminIdentity.identity.find(retryCtx.sender) != null
    );
  }

  function takeRows<T>(rows: Iterable<T>, limit = 1000): T[] {
    const out: T[] = [];
    for (const row of rows) {
      if (out.length >= limit) break;
      out.push(row);
    }
    return out;
  }

  function retryTasksAdmin(ctx: unknown): RetryTaskRow[] {
    const retryCtx = retryContext(ctx);
    return isAdmin(ctx) ? takeRows(retryCtx.db.retryTask.iter()) : [];
  }

  function retryHistoryAdmin(ctx: unknown): RetryHistoryRow[] {
    const retryCtx = retryContext(ctx);
    return isAdmin(ctx) ? takeRows(retryCtx.db.retryHistory.iter()) : [];
  }

  function retryFire(ctx: unknown, { arg }: { arg: RetryTaskRow }): void {
    const retryCtx = retryContext(ctx);
    const nowMicros = retryCtx.timestamp.microsSinceUnixEpoch as bigint;

    const inserted = retryCtx.db.retryHistory.insert({
      id: 0n,
      taskName: arg.name,
      attempt: arg.attempt,
      status: RetryHistoryStatus.Attempted,
      error: undefined,
      ranAt: retryCtx.timestamp,
    });

    const result = dispatchRetry(
      retryCtx,
      arg.args as RetryDispatchArg & {
        tag: keyof H & string;
      }
    );

    if (result.ok) {
      retryCtx.db.retryHistory.id.update({
        ...inserted,
        status: RetryHistoryStatus.Ok,
        error: undefined,
      });
      return;
    }

    const errorMessage = result.error.slice(0, MAX_ERROR_LENGTH);
    const isLast = arg.attempt + 1 >= arg.maxAttempts;
    retryCtx.db.retryHistory.id.update({
      ...inserted,
      status: isLast ? RetryHistoryStatus.GaveUp : RetryHistoryStatus.Failed,
      error: errorMessage,
    });

    if (isLast) return;

    const factor = 1n << BigInt(arg.attempt);
    const delay = BigInt(arg.backoffSecs) * factor * ONE_SECOND_MICROS;
    retryCtx.db.retryTask.insert({
      ...arg,
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(nowMicros + delay),
      attempt: arg.attempt + 1,
    });
  }

  const submitRetryTaskParams = {
    name: t.string(),
    args: retryArgs,
    maxAttempts: t.u8(),
    backoffSecs: t.u32(),
  };

  function submitRetryTask(
    ctx: unknown,
    args: {
      name: string;
      args: RetryDispatchArg;
      maxAttempts: number;
      backoffSecs: number;
    }
  ): void {
    const retryCtx = retryContext(ctx);
    requireAdmin(ctx);
    if (
      typeof args.name !== 'string' ||
      args.name.length === 0 ||
      args.name.length > MAX_TASK_NAME_LENGTH
    ) {
      throw new SenderError('retry.invalid_task_name');
    }
    if (
      !Number.isInteger(args.maxAttempts) ||
      args.maxAttempts < 1 ||
      args.maxAttempts > MAX_ATTEMPTS
    ) {
      throw new SenderError('retry.invalid_max_attempts');
    }
    if (
      !Number.isInteger(args.backoffSecs) ||
      args.backoffSecs < 1 ||
      args.backoffSecs > MAX_BACKOFF_SECONDS
    ) {
      throw new SenderError('retry.invalid_backoff_seconds');
    }
    if (retryCtx.db.retryTask.name.filter(args.name).next().value != null) {
      throw new SenderError(`retry.task_already_exists:${args.name}`);
    }
    retryCtx.db.retryTask.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(
        retryCtx.timestamp.microsSinceUnixEpoch as bigint
      ),
      name: args.name,
      args: args.args,
      attempt: 0,
      maxAttempts: args.maxAttempts,
      backoffSecs: args.backoffSecs,
    });
  }

  function addRetryAdminIdentity(
    ctx: unknown,
    { identity }: { identity: Identity }
  ): void {
    const retryCtx = retryContext(ctx);
    requireAdmin(ctx);
    if (retryCtx.db.retryAdminIdentity.identity.find(identity) == null) {
      retryCtx.db.retryAdminIdentity.insert({
        identity,
        addedAtMicros: retryCtx.timestamp.microsSinceUnixEpoch,
      });
    }
  }

  function removeRetryAdminIdentity(
    ctx: unknown,
    { identity }: { identity: Identity }
  ): void {
    const retryCtx = retryContext(ctx);
    requireAdmin(ctx);
    const existing = retryCtx.db.retryAdminIdentity.identity.find(identity);
    if (!existing) return;
    if (retryCtx.db.retryAdminIdentity.count() <= 1n) {
      throw new SenderError('retry.cannot_remove_last_admin');
    }
    retryCtx.db.retryAdminIdentity.delete(existing);
  }

  return {
    tables: {
      retryTask,
      retryHistory,
      retryAdminIdentity,
    },
    retryArgs,
    retryHistoryStatus,
    RetryHistoryStatus,
    installRetry,
    requireAdmin,
    views: {
      retryTasksAdmin,
      retryHistoryAdmin,
    },
    reducers: {
      retryFire,
      submitRetryTask: {
        params: submitRetryTaskParams,
        handler: submitRetryTask,
      },
      addRetryAdminIdentity: {
        params: { identity: t.identity() },
        handler: addRetryAdminIdentity,
      },
      removeRetryAdminIdentity: {
        params: { identity: t.identity() },
        handler: removeRetryAdminIdentity,
      },
    },
  } as const;
}
