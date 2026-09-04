import * as assert from 'node:assert/strict';
import { spacetimeCron } from '../src/cron';
import type { CronSchema, CronSdk } from '../src/types';

interface DynamicValue {
  (...args: unknown[]): DynamicValue;
  readonly [key: PropertyKey]: DynamicValue;
}

function dynamicValue(): DynamicValue {
  const callable = () => dynamicValue();
  return new Proxy(callable, {
    apply: () => dynamicValue(),
    get: () => dynamicValue(),
  }) as DynamicValue;
}

function toCamelCase(value: string): string {
  const converted = value
    .replace(/[-_]+/g, '_')
    .replace(/_([a-zA-Z0-9])/g, (_match, character: string) =>
      character.toUpperCase()
    );
  return converted.charAt(0).toLowerCase() + converted.slice(1);
}

const registrationSchema = {
  reducer: (..._args: unknown[]) => ({}),
  procedure: (..._args: unknown[]) => ({}),
  anonymousView: (..._args: unknown[]) => ({}),
} as unknown as CronSchema;

function createApi() {
  return spacetimeCron({
    table: dynamicValue(),
    t: dynamicValue(),
    toCamelCase,
    ScheduleAt: dynamicValue(),
    Timestamp: dynamicValue(),
    SenderError: dynamicValue(),
  } as unknown as CronSdk);
}

{
  const owner = createApi();
  const foreign = createApi().cronTable({ name: 'foreign' });
  assert.throws(() => owner.createCron([foreign]), /cron\.foreign_job_handle/);
}

{
  const api = createApi();
  const job = api.cronTable({ name: 'duplicate' });
  assert.throws(
    () => api.createCron([job, job]),
    /cron\.duplicate_job:duplicate/
  );
}

{
  const api = createApi();
  const first = api.cronTable({ name: 'first' });
  const second = api.cronTable({ name: 'second' });
  api.createCron([first]);
  assert.throws(
    () => api.createCron([second]),
    /cron\.multiple_cores_not_supported/
  );
}

{
  const api = createApi();
  const job = api.cronTable({ name: 'not_wired' });
  assert.throws(
    () => job.cronReducer(registrationSchema, () => {}),
    /cron\.not_wired:not_wired/
  );
}

{
  const api = createApi();
  const job = api.cronTable({ name: 'missing_handler' });
  const cron = api.createCron([job], { reconcileEverySeconds: 60 });
  assert.throws(
    () => cron.reconcileReducer(registrationSchema),
    /cron\.missing_handlers:missing_handler/
  );
}

{
  const api = createApi();
  const job = api.cronTable({ name: 'unscheduled_handler' });
  api.createCron([job]);
  assert.throws(
    () => api.schedule({}, job, { everySeconds: 60 }),
    /cron\.missing_handlers:unscheduled_handler/
  );
}

{
  const api = createApi();
  const job = api.cronTable({ name: 'duplicate_handler' });
  api.createCron([job]);
  job.cronReducer(registrationSchema, () => {});
  assert.throws(
    () => job.cronProcedure(registrationSchema, () => {}),
    /cron\.handler_already_registered:duplicate_handler:reducer/
  );
}

{
  const api = createApi();
  const job = api.cronTable({ name: 'no_reconciler' });
  const cron = api.createCron([job]);
  job.cronReducer(registrationSchema, () => {});
  assert.throws(
    () => cron.reconcileReducer(registrationSchema),
    /cron\.reconcile_not_configured/
  );
}

{
  const api = createApi();
  const job = api.cronTable({ name: 'reconcile_once' });
  const cron = api.createCron([job], { reconcileEverySeconds: 60 });
  job.cronReducer(registrationSchema, () => {});
  cron.reconcileReducer(registrationSchema);
  assert.throws(
    () => cron.reconcileReducer(registrationSchema),
    /cron\.reconcile_reducer_already_registered/
  );
}

{
  const api = createApi();
  const job = api.cronTable({ name: 'views_once' });
  const cron = api.createCron([job]);
  cron.publicViews(registrationSchema);
  assert.throws(
    () => cron.publicViews(registrationSchema),
    /cron\.public_views_already_registered/
  );
}

{
  const api = createApi();
  const firstJob = api.cronTable({ name: 'first_job' });
  const secondJob = api.cronTable({ name: 'second_job' });
  const cron = api.createCron([firstJob, secondJob], {
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

{
  let viewBody: ((ctx: unknown) => unknown) | undefined;
  const schemaWithInspectableView = {
    ...registrationSchema,
    anonymousView: (...args: unknown[]) => {
      viewBody = args[2] as (ctx: unknown) => unknown;
      return {};
    },
  } as unknown as CronSchema;
  const api = createApi();
  const job = api.cronTable({ name: 'sanitized_view' });
  const cron = api.createCron([job]);
  cron.publicViews(schemaWithInspectableView);
  assert.ok(viewBody);

  const baseRow = {
    name: 'sanitized_view',
    schedule: { tag: 'every', value: { seconds: 60 } },
    args: { tag: 'sanitized_view', value: undefined },
    enabled: false,
    maxFailures: 1,
    consecutiveFailures: 1,
    fireCount: 1n,
    generation: 1n,
    lastRunAt: undefined,
    nextRunAt: undefined,
  };
  const privateReasons = [
    'failed_1_consecutive_times:secret application failure',
    'failed_1_consecutive_times:lost_fire',
    'cron.invalid_schedule_state:secret parser detail',
    'disabled_by_operator',
    'unrecognized private detail',
  ];
  const publicRows = viewBody({
    db: {
      cronJob: {
        iter: () =>
          privateReasons.map(disabledReason => ({
            ...baseRow,
            disabledReason,
          })),
      },
    },
  }) as Array<{ disabledReason: string }>;

  assert.deepEqual(
    publicRows.map(row => row.disabledReason),
    [
      'failure_threshold_reached',
      'lost_fire_threshold_reached',
      'invalid_schedule_state',
      'disabled_by_operator',
      'disabled',
    ]
  );
}

console.log('cron registration tests passed');
