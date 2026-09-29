import * as assert from 'node:assert/strict';
import { schema } from 'spacetimedb/server';
import { client, cronTable, publicDisabledReason } from '../src/cron';
import { errors } from '../src/errors';

const expectCode = (code: string) => ({
  message: new RegExp(`^${code}(:|$)`),
});

{
  const job = cronTable({ name: 'owned' });
  const other = cronTable({ name: 'other' });
  const cron = client({ jobs: [job] });
  client({ jobs: [other] });
  assert.throws(
    () => cron.unschedule({} as never, other),
    expectCode(errors.foreignJobHandle)
  );
  assert.throws(
    () => client({ jobs: [{ jobName: 'plain' }] }),
    expectCode(errors.foreignJobHandle)
  );
}

assert.throws(() => client({ jobs: [] }), expectCode(errors.noJobs));

{
  const job = cronTable({ name: 'duplicate' });
  assert.throws(() => client({ jobs: [job, job] }), {
    message: /^cron\.duplicate_job:duplicate$/,
  });
}

{
  const job = cronTable({ name: 'wired_once' });
  client({ jobs: [job] });
  assert.throws(() => client({ jobs: [job] }), {
    message: /^cron\.job_already_wired:wired_once$/,
  });
}

{
  const job = cronTable({ name: 'not_wired' });
  const other = client({ jobs: [cronTable({ name: 'other_client' })] });
  const spacetimedb = schema({ ...other.tables });
  assert.throws(() => job.cronReducer(spacetimedb, () => {}), {
    message: /^cron\.not_wired:not_wired:/,
  });
}

{
  const job = cronTable({ name: 'missing_handler' });
  const cron = client({ jobs: [job], reconcileEverySeconds: 60 });
  const spacetimedb = schema({ ...cron.tables });
  assert.throws(() => cron.reconcileReducer(spacetimedb), {
    message: /^cron\.missing_handlers:missing_handler$/,
  });
}

{
  const job = cronTable({ name: 'unscheduled_handler' });
  const cron = client({ jobs: [job] });
  assert.throws(() => cron.schedule({} as never, job, { everySeconds: 60 }), {
    message: /^cron\.missing_handlers:unscheduled_handler$/,
  });
}

{
  const job = cronTable({ name: 'duplicate_handler' });
  const cron = client({ jobs: [job] });
  const spacetimedb = schema({ ...cron.tables });
  job.cronReducer(spacetimedb, () => {});
  assert.throws(() => job.cronProcedure(spacetimedb, () => {}), {
    message: /^cron\.handler_already_registered:duplicate_handler:reducer$/,
  });
}

{
  const job = cronTable({ name: 'no_reconciler' });
  const cron = client({ jobs: [job] });
  const spacetimedb = schema({ ...cron.tables });
  job.cronReducer(spacetimedb, () => {});
  assert.throws(
    () => cron.reconcileReducer(spacetimedb),
    expectCode(errors.reconcileNotConfigured)
  );
}

{
  const job = cronTable({ name: 'reconcile_once' });
  const cron = client({ jobs: [job], reconcileEverySeconds: 60 });
  const spacetimedb = schema({ ...cron.tables });
  job.cronReducer(spacetimedb, () => {});
  cron.reconcileReducer(spacetimedb);
  assert.throws(
    () => cron.reconcileReducer(spacetimedb),
    expectCode(errors.reconcileReducerAlreadyRegistered)
  );
}

{
  const job = cronTable({ name: 'views_once' });
  const cron = client({ jobs: [job] });
  const spacetimedb = schema({ ...cron.tables });
  cron.publicViews(spacetimedb);
  assert.throws(
    () => cron.publicViews(spacetimedb),
    expectCode(errors.publicViewsAlreadyRegistered)
  );
}

{
  const firstJob = cronTable({ name: 'first_job' });
  const secondJob = cronTable({ name: 'second_job' });
  const cron = client({
    jobs: [firstJob, secondJob],
    reconcileEverySeconds: 60,
  });
  assert.deepEqual(Object.keys(cron.tables).sort(), [
    'cronJob',
    'cronReconcileTick',
    'cronRun',
    'firstJobFire',
    'secondJobFire',
  ]);
}

assert.deepEqual(
  [
    'failed_1_consecutive_times:secret application failure',
    'failed_1_consecutive_times:lost_fire',
    `${errors.invalidScheduleState}:secret parser detail`,
    'disabled_by_operator',
    'unrecognized private detail',
    undefined,
  ].map(publicDisabledReason),
  [
    'failure_threshold_reached',
    'lost_fire_threshold_reached',
    'invalid_schedule_state',
    'disabled_by_operator',
    'disabled',
    undefined,
  ]
);

process.stdout.write('cron registration tests passed\n');
