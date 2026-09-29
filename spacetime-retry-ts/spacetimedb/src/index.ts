import { schema, table, t } from 'spacetimedb/server';
import { client, retryFailed, retryOk } from '@spacetimedb/retry';

const retry = client({
  tasks: {
    flaky: t.object('FlakyArgs', {
      taskName: t.string(),
      succeedAtAttempt: t.u8(),
      throwOnFailure: t.bool(),
    }),
  },
});
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

export const retryFire = retry.retryReducer(spacetimedb, {
  flaky(ctx, args) {
    const task = ctx.db.retryTask.name.filter(args.taskName).next().value;
    const attempt = task?.attempt ?? 0;
    if (attempt < args.succeedAtAttempt) {
      const message = `simulated failure at attempt ${attempt}`;
      if (args.throwOnFailure) throw new Error(message);
      return retryFailed(message);
    }
    ctx.db.retryMetric.insert({
      id: 0n,
      name: `flaky-success-${args.taskName}`,
      value: attempt,
      recordedAt: ctx.timestamp,
    });
    return retryOk();
  },
});

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
