import {
  MAX_CRON_EXPRESSION_LENGTH,
  MAX_TIMEZONE_LENGTH,
  isValidTimezone,
  nextFireAfter,
  parseCronExpression,
} from './parser';
import type { CronSchedule, ScheduleSpec } from './types';

export const ONE_SECOND_MICROS = 1_000_000n;
export const MAX_INTERVAL_SECONDS = 31_536_000;
export const MAX_HISTORY_CAP = 1_000;
export const MAX_FAILURES = 4_294_967_295;
export const MAX_ERROR_LENGTH = 1_024;

// The host limit is roughly 795 days. Annual checkpoints leave ample room for
// execution delay and keep valid sparse schedules, such as February 29, armed.
export const CHECKPOINT_DELAY_MICROS =
  365n * 24n * 60n * 60n * ONE_SECOND_MICROS;

const JOB_NAME_PATTERN = /^[a-z][a-z0-9]*(?:_[a-z0-9]+)*$/;
const MAX_JOB_NAME_LENGTH = 48;

export class CronInputError extends Error {}

export interface NormalizedSchedule {
  schedule: CronSchedule;
  firstAt: bigint | undefined;
}

export function normalizeJobName(name: string): string {
  const normalized = name.trim();
  if (
    normalized.length === 0 ||
    normalized.length > MAX_JOB_NAME_LENGTH ||
    !JOB_NAME_PATTERN.test(normalized)
  ) {
    throw new CronInputError(
      'cron.invalid_job_name:use 1-48 lowercase snake_case characters'
    );
  }
  return normalized;
}

export function normalizeHistoryCap(value: number | undefined): number {
  const cap = value ?? 5;
  if (!Number.isSafeInteger(cap) || cap < 0 || cap > MAX_HISTORY_CAP) {
    throw new CronInputError(
      `cron.invalid_history_cap:must be an integer between 0 and ${MAX_HISTORY_CAP}`
    );
  }
  return cap;
}

export function normalizeReconcileEverySeconds(
  value: number | undefined
): number | undefined {
  if (value === undefined) return undefined;
  if (
    !Number.isSafeInteger(value) ||
    value < 1 ||
    value > MAX_INTERVAL_SECONDS
  ) {
    throw new CronInputError(
      `cron.invalid_reconcile_interval:seconds must be an integer between 1 and ${MAX_INTERVAL_SECONDS}`
    );
  }
  return value;
}

export function normalizeMaxFailures(value: number | undefined): number {
  const failures = value ?? 0;
  if (
    !Number.isSafeInteger(failures) ||
    failures < 0 ||
    failures > MAX_FAILURES
  ) {
    throw new CronInputError(
      `cron.invalid_max_failures:must be an integer between 0 and ${MAX_FAILURES}`
    );
  }
  return failures;
}

export function normalizeJobArgs(
  jobName: string,
  hasArgs: boolean,
  opts: { args?: unknown } | undefined
): unknown {
  const supplied = Object.prototype.hasOwnProperty.call(opts ?? {}, 'args');
  if (hasArgs && !supplied) {
    throw new CronInputError(`cron.missing_args:${jobName}`);
  }
  if (!hasArgs && supplied) {
    throw new CronInputError(`cron.unexpected_args:${jobName}`);
  }
  return hasArgs ? opts?.args : {};
}

export function normalizeSchedule(
  spec: ScheduleSpec,
  opts: { timezone?: string } | undefined,
  nowMicros: bigint
): NormalizedSchedule {
  if (typeof spec !== 'string') {
    const seconds = spec.everySeconds;
    if (
      !Number.isSafeInteger(seconds) ||
      seconds < 1 ||
      seconds > MAX_INTERVAL_SECONDS
    ) {
      throw new CronInputError(
        `cron.invalid_interval:seconds must be an integer between 1 and ${MAX_INTERVAL_SECONDS}`
      );
    }
    return {
      schedule: { tag: 'every', value: { seconds } },
      firstAt: nowMicros + BigInt(seconds) * ONE_SECOND_MICROS,
    };
  }

  const expression = spec.trim();
  const timezone = (opts?.timezone ?? 'UTC').trim() || 'UTC';
  if (expression.length === 0) {
    throw new CronInputError('cron.invalid_expression:empty');
  }
  if (expression.length > MAX_CRON_EXPRESSION_LENGTH) {
    throw new CronInputError('cron.invalid_expression:too_long');
  }
  if (timezone.length > MAX_TIMEZONE_LENGTH) {
    throw new CronInputError('cron.invalid_timezone:too_long');
  }
  if (!isValidTimezone(timezone)) {
    throw new CronInputError(`cron.invalid_timezone:${timezone}`);
  }

  let firstAt: bigint | undefined;
  try {
    firstAt = nextFireAfter(
      parseCronExpression(expression),
      nowMicros,
      timezone
    );
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    throw new CronInputError(`cron.invalid_expression:${detail}`);
  }
  if (firstAt === undefined) {
    throw new CronInputError('cron.unsatisfiable_expression');
  }
  return {
    schedule: { tag: 'cron', value: { expression, timezone } },
    firstAt,
  };
}

export function nextOccurrence(
  schedule: CronSchedule,
  afterMicros: bigint
): bigint | undefined {
  if (schedule.tag === 'every') {
    return afterMicros + BigInt(schedule.value.seconds) * ONE_SECOND_MICROS;
  }
  return nextFireAfter(
    parseCronExpression(schedule.value.expression),
    afterMicros,
    schedule.value.timezone
  );
}

export function boundedScheduleTime(
  nowMicros: bigint,
  targetMicros: bigint
): bigint {
  const checkpoint = nowMicros + CHECKPOINT_DELAY_MICROS;
  return targetMicros < checkpoint ? targetMicros : checkpoint;
}

export function truncateError(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  return message.length <= MAX_ERROR_LENGTH
    ? message
    : `${message.slice(0, MAX_ERROR_LENGTH - 3)}...`;
}
