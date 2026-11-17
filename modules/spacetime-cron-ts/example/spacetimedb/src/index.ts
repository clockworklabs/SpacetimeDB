import { schema, table, t, SenderError } from 'spacetimedb/server';
import { client, cronTable, type CronJobReference } from '@spacetimedb/cron';

const digest = cronTable({ name: 'digest' });
const cleanup = cronTable({
  name: 'cleanup',
  args: t.object('CleanupCronArgs', { keep: t.u32() }),
});

const cron = client({
  jobs: [digest, cleanup],
  publicTables: true,
  reconcileEverySeconds: 300,
});

const activityLog = table(
  { name: 'activity_log', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    jobName: t.string().index(),
    message: t.string(),
    at: t.timestamp().index(),
  }
);

const spacetimedb = schema({ ...cron.tables, activityLog });
export default spacetimedb;

export const runDigest = digest.cronReducer(spacetimedb, ctx => {
  const entries = ctx.db.activityLog.count();
  ctx.db.activityLog.insert({
    id: 0n,
    jobName: 'digest',
    message: `digest generated over ${entries} log entries`,
    at: ctx.timestamp,
  });
});

export const runCleanup = cleanup.cronReducer(spacetimedb, (ctx, args) => {
  const total = Number(ctx.db.activityLog.count());
  if (total <= args.keep) return;
  const excess = total - args.keep + 1;
  const oldest = [...ctx.db.activityLog.iter()]
    .sort((left, right) => {
      const delta =
        left.at.microsSinceUnixEpoch - right.at.microsSinceUnixEpoch;
      return delta < 0n ? -1 : delta > 0n ? 1 : 0;
    })
    .slice(0, excess);
  for (const row of oldest) {
    ctx.db.activityLog.delete(row);
  }
  ctx.db.activityLog.insert({
    id: 0n,
    jobName: 'cleanup',
    message: `pruned ${oldest.length} old entries`,
    at: ctx.timestamp,
  });
});

export const cronReconcile = cron.reconcileReducer(spacetimedb);
export const { jobs: cronJobs } = cron.publicViews(spacetimedb);

export const init = spacetimedb.init(ctx => {
  // Code-declared defaults on first publish. Both are runtime state after
  // this: reschedule or disable them from the UI without republishing.
  cron.schedule(ctx, digest, '0 9 * * 1-5', { timezone: 'America/New_York' });
  cron.schedule(ctx, cleanup, { everySeconds: 300 }, { args: { keep: 50 } });
});

// These reducers are open so the local browser can exercise the submodule.
// Production applications must enforce their own admin policy.

const jobs: Record<string, CronJobReference> = { digest, cleanup };

function jobByName(name: string): CronJobReference {
  const job = jobs[name];
  if (!job) throw new SenderError(`example.unknown_job:${name}`);
  return job;
}

function cleanupKeep(value: number): number {
  if (!Number.isInteger(value) || value < 1 || value > 1_000) {
    throw new SenderError('cleanup.invalid_keep:must be between 1 and 1000');
  }
  return value;
}

export const scheduleCron = spacetimedb.reducer(
  {
    name: t.string(),
    expression: t.string(),
    timezone: t.string(),
    keep: t.u32(),
  },
  (ctx, args) => {
    try {
      if (args.name === 'cleanup') {
        cron.schedule(ctx, cleanup, args.expression, {
          timezone: args.timezone,
          args: { keep: cleanupKeep(args.keep) },
        });
      } else if (args.name === 'digest') {
        cron.schedule(ctx, digest, args.expression, {
          timezone: args.timezone,
        });
      } else {
        throw new SenderError(`example.unknown_job:${args.name}`);
      }
    } catch (err) {
      throw err instanceof SenderError
        ? err
        : new SenderError(err instanceof Error ? err.message : String(err));
    }
  }
);

export const scheduleEvery = spacetimedb.reducer(
  { name: t.string(), seconds: t.u32(), keep: t.u32() },
  (ctx, args) => {
    try {
      if (args.name === 'cleanup') {
        cron.schedule(
          ctx,
          cleanup,
          { everySeconds: args.seconds },
          { args: { keep: cleanupKeep(args.keep) } }
        );
      } else if (args.name === 'digest') {
        cron.schedule(ctx, digest, { everySeconds: args.seconds });
      } else {
        throw new SenderError(`example.unknown_job:${args.name}`);
      }
    } catch (err) {
      throw err instanceof SenderError
        ? err
        : new SenderError(err instanceof Error ? err.message : String(err));
    }
  }
);

export const unscheduleJob = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    cron.unschedule(ctx, jobByName(name));
  }
);
