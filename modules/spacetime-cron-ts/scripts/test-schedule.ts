// Unit tests for calendar occurrence computation.
// Run: pnpm test:schedule
import {
  parseCronExpression,
  nextFireAfter,
  isValidTimezone,
} from '../src/parser';

let failures = 0;
function check(name: string, cond: boolean, detail?: string) {
  if (cond) {
    console.log(`ok   ${name}`);
  } else {
    failures++;
    console.error(`FAIL ${name}${detail ? `: ${detail}` : ''}`);
  }
}

const MICROS = 1000n;
function utcMicros(
  y: number,
  mo: number,
  d: number,
  h = 0,
  mi = 0,
  s = 0
): bigint {
  return BigInt(Date.UTC(y, mo - 1, d, h, mi, s)) * MICROS;
}
function fmt(micros: bigint | undefined, tz = 'UTC'): string {
  if (micros === undefined) return 'undefined';
  return new Date(Number(micros / MICROS)).toLocaleString('en-US', {
    timeZone: tz,
    hour12: false,
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}

// 1. Basic calendar step: next minute boundary, strictly after `now`.
{
  const parsed = parseCronExpression('* * * * *');
  const now = utcMicros(2026, 8, 12, 10, 0, 30); // 10:00:30
  const next = nextFireAfter(parsed, now, 'UTC');
  check(
    'next-minute boundary',
    next === utcMicros(2026, 8, 12, 10, 1, 0),
    fmt(next)
  );
}

// 2. Strictly-after semantics: asking at an exact occurrence must return the NEXT one.
{
  const parsed = parseCronExpression('* * * * *');
  const now = utcMicros(2026, 8, 12, 10, 1, 0); // exactly on an occurrence
  const next = nextFireAfter(parsed, now, 'UTC');
  check(
    'strictly-after at exact occurrence',
    next === utcMicros(2026, 8, 12, 10, 2, 0),
    fmt(next)
  );
}

// 3. Seconds granularity (6-field cron, used by the integration test).
{
  const parsed = parseCronExpression('*/2 * * * * *');
  const now = utcMicros(2026, 8, 12, 10, 0, 1);
  const next = nextFireAfter(parsed, now, 'UTC');
  check(
    'seconds-granularity */2',
    next === utcMicros(2026, 8, 12, 10, 0, 2),
    fmt(next)
  );
}

// 4. Timezone correctness: 09:00 America/New_York in August = 13:00 UTC (EDT, UTC-4).
{
  const parsed = parseCronExpression('0 9 * * *');
  const now = utcMicros(2026, 8, 12, 0, 0, 0);
  const next = nextFireAfter(parsed, now, 'America/New_York');
  check('tz offset EDT', next === utcMicros(2026, 8, 12, 13, 0, 0), fmt(next));
}

// 5. DST spring-forward: 2026-03-08 02:30 does not exist in America/New_York.
//    The job must not be lost and must not fire twice; document what it does.
{
  const parsed = parseCronExpression('30 2 * * *');
  const start = utcMicros(2026, 3, 7, 12, 0, 0);
  const a = nextFireAfter(parsed, start, 'America/New_York');
  const b =
    a !== undefined ? nextFireAfter(parsed, a, 'America/New_York') : undefined;
  const c =
    b !== undefined ? nextFireAfter(parsed, b, 'America/New_York') : undefined;
  // Starting after noon on March 7 makes the first result the March 8 slot.
  check(
    'spring-forward: occurrence exists',
    a !== undefined && b !== undefined && c !== undefined
  );
  check(
    'spring-forward: monotonic chain',
    a !== undefined && b !== undefined && c !== undefined && a < b && b < c,
    `${fmt(a, 'America/New_York')} | ${fmt(b, 'America/New_York')} | ${fmt(c, 'America/New_York')}`
  );
  console.log(
    `     info: 02:30 chain around spring-forward fires at: ${fmt(a, 'America/New_York')}, ${fmt(b, 'America/New_York')}, ${fmt(c, 'America/New_York')} (local NY time)`
  );
}

// 6. DST fall-back: 2026-11-01 01:30 occurs twice in America/New_York.
//    The chain must fire exactly once per calendar day, not twice.
{
  const parsed = parseCronExpression('30 1 * * *');
  const start = utcMicros(2026, 10, 31, 12, 0, 0);
  const fires: bigint[] = [];
  let cursor: bigint | undefined = start;
  for (let i = 0; i < 3 && cursor !== undefined; i++) {
    cursor = nextFireAfter(parsed, cursor, 'America/New_York');
    if (cursor !== undefined) fires.push(cursor);
  }
  const days = fires.map(f =>
    new Date(Number(f / MICROS)).toLocaleDateString('en-US', {
      timeZone: 'America/New_York',
    })
  );
  check(
    'fall-back: one fire per day',
    new Set(days).size === days.length,
    fires.map(f => fmt(f, 'America/New_York')).join(' | ')
  );
  console.log(
    `     info: 01:30 chain around fall-back fires at: ${fires.map(f => fmt(f, 'America/New_York')).join(', ')} (local NY time)`
  );
}

// 7a. Statically impossible expressions are rejected at PARSE time (finding:
//     cron-parser refuses Feb 30 outright, so create_job's validateSchedule
//     catches these as invalid_expression before any row exists).
{
  let threw = false;
  try {
    parseCronExpression('0 0 30 2 *');
  } catch {
    threw = true;
  }
  check('impossible Feb 30 rejected at parse', threw);
}

// 7b. nextFireAfter -> undefined is still reachable at JS date bounds, so the
//     fire reducer's disable-loudly path has a real (if exotic) trigger.
{
  const parsed = parseCronExpression('0 0 1 1 *');
  const nearMax = 8_640_000_000_000_000n * 1000n - 1_000_000n; // ~max JS date, in micros
  const next = nextFireAfter(parsed, nearMax, 'UTC');
  check('date-bound overflow -> undefined', next === undefined, fmt(next));
}

// 8. Chain-step simulation: late fire does not drift the schedule.
//    Job '0 * * * *' (hourly). Fire lands 7 minutes late; next must still be the
//    top of the NEXT hour, not lateFire+1h.
{
  const parsed = parseCronExpression('0 * * * *');
  const lateFire = utcMicros(2026, 8, 12, 9, 7, 0); // fired late at 09:07
  const next = nextFireAfter(parsed, lateFire, 'UTC');
  check(
    'late fire re-arms nominal slot',
    next === utcMicros(2026, 8, 12, 10, 0, 0),
    fmt(next)
  );
}

// 9. Catch-up semantics: an armed fire far in the past re-arms to the next FUTURE
//    occurrence, skipping intermediate ones. This is one catch-up run.
{
  const parsed = parseCronExpression('*/5 * * * *');
  const wokeUpAt = utcMicros(2026, 8, 12, 11, 3, 0); // was down for hours
  const next = nextFireAfter(parsed, wokeUpAt, 'UTC');
  check(
    'catch-up skips missed occurrences',
    next === utcMicros(2026, 8, 12, 11, 5, 0),
    fmt(next)
  );
}

// 10. Timezone validation.
{
  check('valid tz', isValidTimezone('America/New_York'));
  check('invalid tz rejected', !isValidTimezone('Mars/Olympus_Mons'));
}

console.log(
  failures === 0
    ? '\nAll schedule tests passed.'
    : `\n${failures} test(s) FAILED.`
);
process.exit(failures === 0 ? 0 : 1);
