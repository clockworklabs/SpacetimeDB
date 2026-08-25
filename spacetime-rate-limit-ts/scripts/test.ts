import { Timestamp } from 'spacetimedb';
import {
  consumeRateLimit,
  sweepRateLimits,
  type RateLimitBucketRow,
} from '../src/limit.ts';
import { buildRateLimitKey } from '../src/key.ts';

let pass = 0;
let fail = 0;

function assert(cond: boolean, name: string, detail = ''): void {
  if (cond) {
    pass++;
    process.stdout.write(`  ok   ${name}\n`);
  } else {
    fail++;
    process.stdout.write(
      `  FAIL ${name}${detail ? `\n       ${detail}` : ''}\n`
    );
  }
}

function makeTx(nowMicros = 0n) {
  const rows = new Map<string, RateLimitBucketRow>();
  const tx = {
    timestamp: new Timestamp(nowMicros),
    db: {
      rateLimitBucket: {
        key: {
          find: (key: string) => rows.get(key),
          update: (row: RateLimitBucketRow) => rows.set(row.key, row),
        },
        insert: (row: RateLimitBucketRow) => rows.set(row.key, row),
        delete: (row: RateLimitBucketRow) => rows.delete(row.key),
        expiresAt: {
          filter: function* () {
            yield* [...rows.values()].sort((a, b) =>
              a.expiresAt.microsSinceUnixEpoch <
              b.expiresAt.microsSinceUnixEpoch
                ? -1
                : 1
            );
          },
        },
      },
    },
    rows,
  };
  return tx;
}

{
  const tx = makeTx();
  consumeRateLimit(tx, {
    key: 'fresh-a',
    scope: 's',
    limit: 1,
    windowSeconds: 100,
  });
  consumeRateLimit(tx, {
    key: 'fresh-b',
    scope: 's',
    limit: 1,
    windowSeconds: 100,
  });
  consumeRateLimit(tx, {
    key: 'expired',
    scope: 's',
    limit: 1,
    windowSeconds: 1,
  });
  tx.timestamp = new Timestamp(2_000_000n);
  const deleted = sweepRateLimits(
    tx,
    tx.db.rateLimitBucket.expiresAt.filter(),
    2
  );
  assert(deleted === 1, 'sweep reaches expired buckets beyond fresh inserts');
  assert(!tx.rows.has('expired'), 'indexed sweep removes the expired bucket');
}

process.stdout.write('\nrate limiter\n');

assert(
  buildRateLimitKey('a:actor:b', 'c') !== buildRateLimitKey('a', 'b:actor:c'),
  'compound keys cannot collide through delimiters'
);

{
  const tx = makeTx();
  const one = consumeRateLimit(tx, {
    key: 'auth.login:ip:1',
    scope: 'auth.login',
    limit: 2,
    windowSeconds: 60,
  });
  const two = consumeRateLimit(tx, {
    key: 'auth.login:ip:1',
    scope: 'auth.login',
    limit: 2,
    windowSeconds: 60,
  });
  const three = consumeRateLimit(tx, {
    key: 'auth.login:ip:1',
    scope: 'auth.login',
    limit: 2,
    windowSeconds: 60,
  });
  assert(one.allowed && one.remaining === 1, 'first request allowed');
  assert(two.allowed && two.remaining === 0, 'second request allowed');
  assert(
    !three.allowed && three.retryAfterSeconds === 60,
    'third request blocked'
  );
}

{
  const tx = makeTx();
  consumeRateLimit(tx, {
    key: 'k',
    scope: 's',
    limit: 1,
    windowSeconds: 60,
  });
  tx.timestamp = new Timestamp(61_000_000n);
  const next = consumeRateLimit(tx, {
    key: 'k',
    scope: 's',
    limit: 1,
    windowSeconds: 60,
  });
  assert(next.allowed && next.used === 1, 'expired window resets');
}

{
  const tx = makeTx();
  consumeRateLimit(tx, {
    key: 'k',
    scope: 's',
    limit: 1,
    windowSeconds: 1,
  });
  tx.timestamp = new Timestamp(2_000_000n);
  const deleted = sweepRateLimits(tx, tx.db.rateLimitBucket.expiresAt.filter());
  assert(deleted === 1 && tx.rows.size === 0, 'sweep removes expired buckets');
}

process.stdout.write(`\n${pass} passed, ${fail} failed\n`);
process.exit(fail === 0 ? 0 : 1);
