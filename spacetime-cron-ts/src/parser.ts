import { CronExpressionParser } from 'cron-parser';

// Avoids cron-parser's Math.random fallback (blocked in STDB reducers).
const HASH_SEED = 'spacetimedb-cron-submodule';
export const MAX_CRON_EXPRESSION_LENGTH = 256;
export const MAX_TIMEZONE_LENGTH = 128;

export type ParsedCron = { expression: string };

export function parseCronExpression(expr: string): ParsedCron {
  const expression = expr.trim();
  if (expression.length === 0) throw new Error('cron expression is empty');
  if (expression.length > MAX_CRON_EXPRESSION_LENGTH) {
    throw new Error(
      `cron expression exceeds ${MAX_CRON_EXPRESSION_LENGTH} characters`
    );
  }
  CronExpressionParser.parse(expression, { hashSeed: HASH_SEED });
  return { expression };
}

// JS Date max range.
const MAX_SAFE_DATE_MS = 8_640_000_000_000_000;

export function nextFireAfter(
  parsed: ParsedCron,
  afterMicros: bigint,
  timezone: string = 'UTC'
): bigint | undefined {
  if (!isValidTimezone(timezone)) return undefined;
  const afterMs = Number(afterMicros / 1000n);
  if (!Number.isFinite(afterMs) || Math.abs(afterMs) > MAX_SAFE_DATE_MS) {
    return undefined;
  }
  try {
    const interval = CronExpressionParser.parse(parsed.expression, {
      currentDate: new Date(afterMs),
      tz: timezone,
      hashSeed: HASH_SEED,
    });
    return BigInt(interval.next().getTime()) * 1000n;
  } catch {
    return undefined;
  }
}

export function isValidTimezone(tz: string): boolean {
  if (tz.length === 0 || tz.length > MAX_TIMEZONE_LENGTH || tz !== tz.trim()) {
    return false;
  }
  if (tz === 'UTC') return true;
  try {
    new Intl.DateTimeFormat('en-US', { timeZone: tz });
    return true;
  } catch {
    return false;
  }
}
