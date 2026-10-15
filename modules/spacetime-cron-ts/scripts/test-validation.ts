import * as assert from 'node:assert/strict';
import {
  boundedScheduleTime,
  CHECKPOINT_DELAY_MICROS,
  MAX_ERROR_LENGTH,
  MAX_FAILURES,
  MAX_HISTORY_CAP,
  MAX_INTERVAL_SECONDS,
  normalizeHistoryCap,
  normalizeJobArgs,
  normalizeJobName,
  normalizeMaxFailures,
  normalizeReconcileEverySeconds,
  normalizeSchedule,
  truncateError,
} from '../src/schedule';

const utcMicros = (iso: string) => BigInt(Date.parse(iso)) * 1_000n;

assert.equal(normalizeJobName('daily_report'), 'daily_report');
for (const invalid of [
  '',
  'DailyReport',
  'daily-report',
  '_daily',
  'daily__report',
]) {
  assert.throws(() => normalizeJobName(invalid), /cron\.invalid_job_name/);
}
assert.throws(() => normalizeJobName(`a${'b'.repeat(48)}`), /invalid_job_name/);

const now = utcMicros('2028-03-01T00:00:00.000Z');
for (const everySeconds of [0, -1, 1.5, MAX_INTERVAL_SECONDS + 1]) {
  assert.throws(
    () => normalizeSchedule({ everySeconds }, undefined, now),
    /cron\.invalid_interval/
  );
}
assert.equal(
  normalizeSchedule({ everySeconds: 30 }, undefined, now).firstAt,
  now + 30_000_000n
);

for (const maxFailures of [-1, 1.5, MAX_FAILURES + 1]) {
  assert.throws(
    () => normalizeMaxFailures(maxFailures),
    /invalid_max_failures/
  );
}
assert.equal(normalizeMaxFailures(undefined), 0);
assert.equal(normalizeMaxFailures(MAX_FAILURES), MAX_FAILURES);

assert.deepEqual(normalizeJobArgs('heartbeat', false, undefined), {});
assert.deepEqual(
  normalizeJobArgs('report', true, { args: { tenantId: 42n } }),
  { tenantId: 42n }
);
assert.equal(
  normalizeJobArgs('optional', true, { args: undefined }),
  undefined
);
assert.throws(
  () => normalizeJobArgs('report', true, undefined),
  /cron\.missing_args:report/
);
assert.throws(
  () => normalizeJobArgs('heartbeat', false, { args: {} }),
  /cron\.unexpected_args:heartbeat/
);

for (const historyCap of [-1, 1.5, MAX_HISTORY_CAP + 1]) {
  assert.throws(() => normalizeHistoryCap(historyCap), /invalid_history_cap/);
}
assert.equal(normalizeHistoryCap(undefined), 5);

for (const seconds of [0, -1, 1.5, MAX_INTERVAL_SECONDS + 1]) {
  assert.throws(
    () => normalizeReconcileEverySeconds(seconds),
    /invalid_reconcile_interval/
  );
}
assert.equal(normalizeReconcileEverySeconds(undefined), undefined);
assert.equal(normalizeReconcileEverySeconds(300), 300);

const leapDay = normalizeSchedule('0 0 29 2 *', { timezone: 'UTC' }, now);
assert.ok(leapDay.firstAt !== undefined);
assert.ok(leapDay.firstAt - now > CHECKPOINT_DELAY_MICROS);
assert.equal(
  boundedScheduleTime(now, leapDay.firstAt),
  now + CHECKPOINT_DELAY_MICROS
);

const longError = 'x'.repeat(MAX_ERROR_LENGTH + 100);
assert.equal(truncateError(longError).length, MAX_ERROR_LENGTH);
assert.match(truncateError(longError), /\.\.\.$/);

console.log('cron validation tests passed');
