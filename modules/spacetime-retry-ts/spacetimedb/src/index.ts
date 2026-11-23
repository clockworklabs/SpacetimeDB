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
  jobs: {
    fetchUrl: t.object('FetchUrlArgs', { url: t.string() }),
  },
});
const { retryTask, retryHistory } = retry.tables;

const retryMetric = table(
  { name: 'retry_metric', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
    value: t.f64(),
    recordedAt: t.timestamp(),
  }
);

const spacetimedb = schema({ ...retry.tables, retryMetric });
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

export const retryRunJob = retry.retryProcedure(
  spacetimedb,
  {
    fetchUrl(ctx, { url }, job) {
      const response = ctx.http.fetch(url, {
        headers: { 'Idempotency-Key': job.idempotencyKey },
      });
      return response.ok
        ? retryOk()
        : retryFailed(`status ${response.status} on attempt ${job.attempt}`);
    },
  },
  {
    onComplete(tx, outcome) {
      tx.db.retryMetric.insert({
        id: 0n,
        name: `job-${outcome.status}-${outcome.name}`,
        value: outcome.attempts,
        recordedAt: tx.timestamp,
      });
    },
  }
);

export const enqueueFetch = spacetimedb.reducer(
  {
    name: t.string(),
    url: t.string(),
    queue: t.option(t.string()),
    maxAttempts: t.u8(),
    backoffSecs: t.u32(),
  },
  (ctx, { name, url, queue, maxAttempts, backoffSecs }) => {
    retry.requireAdmin(ctx);
    retry.enqueue(ctx, {
      name,
      queue,
      args: { tag: 'fetchUrl', value: { url } },
      maxAttempts,
      backoffSecs,
    });
  }
);

export const retryJobsAdmin = spacetimedb.view(
  { name: 'retry_jobs_admin', public: true },
  t.array(retry.tables.retryJob.rowType),
  retry.views.retryJobsAdmin
);

export const cancelRetryJob = spacetimedb.reducer(
  retry.reducers.cancelRetryJob.params,
  retry.reducers.cancelRetryJob.handler
);

export const setRetryQueue = spacetimedb.reducer(
  retry.reducers.setRetryQueue.params,
  retry.reducers.setRetryQueue.handler
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
