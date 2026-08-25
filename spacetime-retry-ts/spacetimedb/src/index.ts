import {
  schema,
  table,
  t,
  SenderError,
  type InferSchema,
  type ReducerCtx,
  type ViewCtx,
} from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';
import {
  createRetrySubmodule,
  retryFailed,
  retryHandler,
  retryOk,
  type RetryResult,
} from '@spacetimedb/retry';

interface FlakyTransaction {
  timestamp: import('spacetimedb').Timestamp;
  db: {
    retryTask: {
      name: {
        filter(name: string): IterableIterator<{ attempt: number }>;
      };
    };
    retryMetric: {
      insert(row: {
        id: bigint;
        name: string;
        value: number;
        recordedAt: import('spacetimedb').Timestamp;
      }): unknown;
    };
  };
}

const flakyArgs = t.object('FlakyArgs', {
  taskName: t.string(),
  succeedAtAttempt: t.u8(),
});

const flaky = retryHandler(flakyArgs, (ctx, args): RetryResult => {
  const tx = ctx as FlakyTransaction;
  const task = tx.db.retryTask.name.filter(args.taskName).next().value;
  const attempt = Number(task?.attempt ?? 0);
  if (attempt < args.succeedAtAttempt) {
    return retryFailed(`simulated failure at attempt ${attempt}`);
  }
  tx.db.retryMetric.insert({
    id: 0n,
    name: `flaky-success-${args.taskName}`,
    value: attempt,
    recordedAt: tx.timestamp,
  });
  return retryOk();
});

const retryHandlers = {
  flaky,
};

const retry = createRetrySubmodule(
  { table, t, SenderError, ScheduleAt },
  retryHandlers
);
const { retryTask, retryHistory, retryAdminIdentity } = retry.tables;

const retryMetric = table(
  { name: 'retry_metric', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
    value: t.f64(),
    recordedAt: t.timestamp(),
  }
);

const spacetimedb = schema({
  retryTask,
  retryHistory,
  retryAdminIdentity,
  retryMetric,
});
export default spacetimedb;

type Schema = InferSchema<typeof spacetimedb>;
type Tx = ReducerCtx<Schema>;

function isAdmin(ctx: ViewCtx<Schema>): boolean {
  return ctx.db.retryAdminIdentity.identity.find(ctx.sender) != null;
}

export const retryTasksAdmin = spacetimedb.view(
  { name: 'retry_tasks_admin', public: true },
  t.array(retryTask.rowType),
  ctx => (isAdmin(ctx) ? retry.views.retryTasksAdmin(ctx) : [])
);

export const retryHistoryAdmin = spacetimedb.view(
  { name: 'retry_history_admin', public: true },
  t.array(retryHistory.rowType),
  ctx => (isAdmin(ctx) ? retry.views.retryHistoryAdmin(ctx) : [])
);

export const init = spacetimedb.init(ctx => {
  retry.installRetry(ctx as Tx);
});

export const retry_fire = spacetimedb.reducer(
  { onSchedule: retryTask },
  { arg: retryTask.rowType },
  retry.reducers.retryFire
);

export const submit_retry_task = spacetimedb.reducer(
  retry.reducers.submitRetryTask.params,
  retry.reducers.submitRetryTask.handler
);

export const add_retry_admin_identity = spacetimedb.reducer(
  retry.reducers.addRetryAdminIdentity.params,
  retry.reducers.addRetryAdminIdentity.handler
);

export const remove_retry_admin_identity = spacetimedb.reducer(
  retry.reducers.removeRetryAdminIdentity.params,
  retry.reducers.removeRetryAdminIdentity.handler
);
