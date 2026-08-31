// Compile-time API coverage. This file is checked by `pnpm typecheck`.
import { t } from 'spacetimedb/server';
import type {
  CronApi,
  CronInvocation,
  CronJobReference,
  CronSchema,
} from '../src/index';

declare const cron: CronApi;
declare const ctx: object;
declare const spacetimedb: CronSchema;

const heartbeat = cron.cronTable({ name: 'heartbeat' });
const reportDefinition = {
  name: 'report',
  args: t.object('TypeTestReportArgs', {
    tenantId: t.u64(),
    batchSize: t.u32(),
  }),
} as const;
const report = cron.cronTable(reportDefinition);

heartbeat.cronReducer(spacetimedb, (_ctx: object, invocation) => {
  const checked: CronInvocation = invocation;
  void checked;
});

report.cronReducer(spacetimedb, (_ctx: object, args, invocation) => {
  const tenantId: bigint = args.tenantId;
  const batchSize: number = args.batchSize;
  const checked: CronInvocation = invocation;
  void tenantId;
  void batchSize;
  void checked;
});

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

const core = cron.createCron([heartbeat, report], {
  reconcileEverySeconds: 300,
});
const reconcile = core.reconcileReducer(spacetimedb);
const publicViews = core.publicViews(spacetimedb);
void reconcile;
void publicViews.jobs;
