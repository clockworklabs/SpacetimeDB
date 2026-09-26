import { schema, table, t, SenderError } from 'spacetimedb/server';
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
  throwOnFailure: t.bool(),
});

const flaky = retryHandler(flakyArgs, (ctx, args): RetryResult => {
  const tx = ctx as FlakyTransaction;
  const task = tx.db.retryTask.name.filter(args.taskName).next().value;
  const attempt = Number(task?.attempt ?? 0);
  if (attempt < args.succeedAtAttempt) {
    const message = `simulated failure at attempt ${attempt}`;
    if (args.throwOnFailure) throw new Error(message);
    return retryFailed(message);
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

export const retryTasksAdmin = spacetimedb.view(
  { name: 'retry_tasks_admin', public: true },
  t.array(retryTask.rowType),
  retry.views.retryTasksAdmin
);

export const retryHistoryAdmin = spacetimedb.view(
  { name: 'retry_history_admin', public: true },
  t.array(retryHistory.rowType),
  retry.views.retryHistoryAdmin
);

export const init = spacetimedb.init(ctx => {
  retry.install(ctx);
});

export const retryFire = spacetimedb.reducer(
  { onSchedule: retryTask },
  { arg: retryTask.rowType },
  retry.reducers.retryFire
);

export const submitRetryTask = spacetimedb.reducer(
  retry.reducers.submitRetryTask.params,
  retry.reducers.submitRetryTask.handler
);

export const addRetryAdminIdentity = spacetimedb.reducer(
  retry.reducers.addRetryAdminIdentity.params,
  retry.reducers.addRetryAdminIdentity.handler
);

export const removeRetryAdminIdentity = spacetimedb.reducer(
  retry.reducers.removeRetryAdminIdentity.params,
  retry.reducers.removeRetryAdminIdentity.handler
);
