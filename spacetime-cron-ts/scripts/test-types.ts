// Compile-time API coverage. This file is checked by `pnpm typecheck`.
import { schema, t, table } from 'spacetimedb/server';
import {
  client,
  cronTable,
  type CronInvocation,
  type CronJobReference,
} from '../src/index';

const heartbeat = cronTable({ name: 'heartbeat' });
const report = cronTable({
  name: 'report',
  args: t.object('TypeTestReportArgs', {
    tenantId: t.u64(),
    batchSize: t.u32(),
  }),
});
const cron = client({
  jobs: [heartbeat, report],
  reconcileEverySeconds: 300,
});
const log = table({ name: 'log' }, { id: t.u64().primaryKey() });
const spacetimedb = schema({ ...cron.tables, log });

export const beat = heartbeat.cronReducer(spacetimedb, (ctx, invocation) => {
  ctx.db.log.insert({ id: invocation.sequence });
  ctx.db.heartbeatFire.jobName.find('heartbeat');
  const checked: CronInvocation = invocation;
  void checked;
});

export const runReport = report.cronProcedure(
  spacetimedb,
  (ctx, args, invocation) => {
    const tenantId: bigint = args.tenantId;
    const batchSize: number = args.batchSize;
    ctx.withTx(tx => tx.db.log.insert({ id: tenantId }));
    void batchSize;
    void invocation;
  }
);

export const init = spacetimedb.init(ctx => {
  cron.schedule(ctx, heartbeat, { everySeconds: 30 });
  cron.schedule(ctx, heartbeat, '0 * * * *', { timezone: 'UTC' });
  cron.schedule(ctx, report, '0 9 * * *', {
    timezone: 'UTC',
    args: { tenantId: 42n, batchSize: 100 },
  });

  // @ts-expect-error Argument-bearing jobs require scheduling options with args.
  cron.schedule(ctx, report, '0 9 * * *');
  // @ts-expect-error Argument-bearing jobs require an args property.
  cron.schedule(ctx, report, '0 9 * * *', { timezone: 'UTC' });
  cron.schedule(ctx, report, '0 9 * * *', {
    // @ts-expect-error tenantId is a u64 and therefore a bigint.
    args: { tenantId: 42, batchSize: 100 },
  });
  cron.schedule(
    ctx,
    heartbeat,
    { everySeconds: 30 },
    {
      // @ts-expect-error Argumentless jobs do not accept an args property.
      args: {},
    }
  );

  const reference: CronJobReference = report;
  cron.unschedule(ctx, reference);
});

export const cronReconcile = cron.reconcileReducer(spacetimedb);
export const { jobs: cronJobs } = cron.publicViews(spacetimedb);

// A schema without the cron tables cannot register cron handlers.
// @ts-expect-error The host schema must include cron.tables.
heartbeat.cronReducer(schema({ log }), () => {});
