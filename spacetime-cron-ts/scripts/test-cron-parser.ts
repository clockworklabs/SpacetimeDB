// Pure-Node sanity test. No STDB needed.

import {
  MAX_CRON_EXPRESSION_LENGTH,
  MAX_TIMEZONE_LENGTH,
  isValidTimezone,
  nextFireAfter,
  parseCronExpression,
} from '../src/parser';

function assert(cond: boolean, msg: string): void {
  if (!cond) {
    process.stderr.write(`FAIL: ${msg}\n`);
    process.exit(1);
  }
}

function check(
  expr: string,
  fromIso: string,
  expectIso: string,
  timezone = 'UTC'
): void {
  const parsed = parseCronExpression(expr);
  const fromMicros = BigInt(new Date(fromIso).getTime()) * 1000n;
  const next = nextFireAfter(parsed, fromMicros, timezone);
  if (next === undefined) {
    assert(
      false,
      `${expr} [${timezone}] from ${fromIso} → undefined (expected ${expectIso})`
    );
    return;
  }
  const got = new Date(Number(next / 1000n)).toISOString();
  assert(
    got === expectIso,
    `${expr} [${timezone}] from ${fromIso} → ${got} (expected ${expectIso})`
  );
  process.stdout.write(
    `  ${expr.padEnd(15)}  [${timezone.padEnd(20)}]  from ${fromIso}  →  ${got}  ✓\n`
  );
}

process.stdout.write('cron parser tests\n');

// "every minute"
check('* * * * *', '2026-05-04T12:00:00.000Z', '2026-05-04T12:01:00.000Z');
check('* * * * *', '2026-05-04T12:00:30.000Z', '2026-05-04T12:01:00.000Z');

// every 15 minutes
check('*/15 * * * *', '2026-05-04T12:00:00.000Z', '2026-05-04T12:15:00.000Z');
check('*/15 * * * *', '2026-05-04T12:50:00.000Z', '2026-05-04T13:00:00.000Z');

// daily at 9:00 UTC
check('0 9 * * *', '2026-05-04T08:00:00.000Z', '2026-05-04T09:00:00.000Z');
check('0 9 * * *', '2026-05-04T09:00:00.000Z', '2026-05-05T09:00:00.000Z');

// Mondays at 9:00 UTC (Mon = 1)
check('0 9 * * 1', '2026-05-04T08:00:00.000Z', '2026-05-04T09:00:00.000Z'); // Mon May 4
check('0 9 * * MON', '2026-05-04T10:00:00.000Z', '2026-05-11T09:00:00.000Z'); // next Mon

// First of the month at midnight
check('0 0 1 * *', '2026-05-04T00:00:00.000Z', '2026-06-01T00:00:00.000Z');

// Range + list: weekdays at quarter past 9
check('15 9 * * 1-5', '2026-05-02T00:00:00.000Z', '2026-05-04T09:15:00.000Z'); // skip weekend

// Step inside range: every 5 minutes between :00 and :30
check('0-30/5 * * * *', '2026-05-04T12:00:00.000Z', '2026-05-04T12:05:00.000Z');
check('0-30/5 * * * *', '2026-05-04T12:30:00.000Z', '2026-05-04T13:00:00.000Z');

// DOM + DOW union (Vixie cron); next Mon (May 11) wins over next 1st (Jun 1).
check('0 0 1 * 1', '2026-05-04T00:00:00.000Z', '2026-05-11T00:00:00.000Z');

// Named months
check('0 0 1 JAN *', '2026-05-04T00:00:00.000Z', '2027-01-01T00:00:00.000Z');

// Unsatisfiable (Feb 31): cron-parser rejects up-front in strict mode.
let unsatThrew = false;
try {
  parseCronExpression('0 0 31 2 *');
} catch {
  unsatThrew = true;
}
assert(unsatThrew, '0 0 31 2 * should be rejected as unsatisfiable');
process.stdout.write(
  '  0 0 31 2 *           rejected as unsatisfiable             ✓\n'
);

// Validation
let threw = false;
try {
  parseCronExpression('invalid');
} catch {
  threw = true;
}
assert(threw, 'invalid expression should throw');

assert(
  parseCronExpression('  0 9 * * *  ').expression === '0 9 * * *',
  'expression should be normalized'
);
for (const invalid of ['', '   ', '*'.repeat(MAX_CRON_EXPRESSION_LENGTH + 1)]) {
  let invalidThrew = false;
  try {
    parseCronExpression(invalid);
  } catch {
    invalidThrew = true;
  }
  assert(
    invalidThrew,
    `expression should be rejected: ${JSON.stringify(invalid.slice(0, 20))}`
  );
}

const hashed = parseCronExpression('H * * * *');
const hashedFrom =
  BigInt(new Date('2026-05-04T12:00:00.000Z').getTime()) * 1000n;
assert(
  nextFireAfter(hashed, hashedFrom, 'UTC') ===
    nextFireAfter(hashed, hashedFrom, 'UTC'),
  'hashed expressions should resolve deterministically'
);

process.stdout.write('\ntimezone tests\n');

// 9am Pacific: 17:00 UTC in PST, 16:00 UTC in PDT.
check(
  '0 9 * * *',
  '2026-01-15T00:00:00.000Z',
  '2026-01-15T17:00:00.000Z',
  'America/Los_Angeles'
);
check(
  '0 9 * * *',
  '2026-07-15T00:00:00.000Z',
  '2026-07-15T16:00:00.000Z',
  'America/Los_Angeles'
);

// 9am Tokyo = 00:00 UTC same day.
check(
  '0 9 * * *',
  '2026-05-03T23:59:00.000Z',
  '2026-05-04T00:00:00.000Z',
  'Asia/Tokyo'
);
// 00:00 UTC IS 9am Tokyo today; strict-after means next is tomorrow.
check(
  '0 9 * * *',
  '2026-05-04T00:00:00.000Z',
  '2026-05-05T00:00:00.000Z',
  'Asia/Tokyo'
);

// "Every Monday 9am" in NY (EST/EDT)
check(
  '0 9 * * MON',
  '2026-01-04T00:00:00.000Z',
  '2026-01-05T14:00:00.000Z',
  'America/New_York'
); // Mon Jan 5 9am EST = 14:00 UTC

// During the New York DST spring-forward, 02:30 resolves to the next available local time.
check(
  '30 2 * * *',
  '2026-03-08T00:00:00.000Z',
  '2026-03-08T07:30:00.000Z',
  'America/New_York'
);

// DST fall-back: 01:30 NY happens twice, fires once at first occurrence.
check(
  '30 1 * * *',
  '2026-10-31T23:59:59.000Z',
  '2026-11-01T05:30:00.000Z',
  'America/New_York'
);
check(
  '30 1 * * *',
  '2026-11-01T05:30:00.000Z',
  '2026-11-02T06:30:00.000Z',
  'America/New_York'
);

// IANA validation
assert(isValidTimezone('UTC'), 'UTC should be valid');
assert(
  isValidTimezone('America/Los_Angeles'),
  'America/Los_Angeles should be valid'
);
assert(!isValidTimezone('Not_A_Real/Timezone'), 'bogus tz should be invalid');
assert(
  !isValidTimezone(' UTC'),
  'timezone with surrounding whitespace should be invalid'
);
assert(
  !isValidTimezone('x'.repeat(MAX_TIMEZONE_LENGTH + 1)),
  'oversized timezone should be invalid'
);
assert(
  nextFireAfter(parseCronExpression('* * * * *'), 0n, 'Not_A_Real/Timezone') ===
    undefined,
  'nextFireAfter should reject an invalid timezone'
);
process.stdout.write(
  '  isValidTimezone gate                                       ✓\n'
);

process.stdout.write('\nall parser tests passed.\n');
