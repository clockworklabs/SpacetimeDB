import { Timestamp } from 'spacetimedb';
import { SenderError } from 'spacetimedb/server';
import {
  client,
  errors,
  MAX_SWEEP_BATCH,
  resetRegisteredScopes,
  sweepRateLimits,
  type RateLimitBucketRow,
} from '../src/limit';

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

function assertThrows(
  fn: () => void,
  expected: string,
  name: string,
  type: new (message: string) => Error = Error
): void {
  try {
    fn();
    assert(false, name, `expected ${expected}`);
  } catch (error) {
    assert(error instanceof type && error.message === expected, name);
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
      },
    },
    rows,
  };
  return tx;
}

function byExpiry(rows: Map<string, RateLimitBucketRow>) {
  return [...rows.values()].sort((a, b) =>
    a.expiresAt.microsSinceUnixEpoch < b.expiresAt.microsSinceUnixEpoch ? -1 : 1
  );
}

process.stdout.write('\nrate limiter\n');

{
  const tx = makeTx();
  const tap = client({ scope: 'tap', windowSeconds: 60, limit: 1 });
  const other = client({ scope: 'other', windowSeconds: 60, limit: 1 });
  assert(tap.consume(tx, { key: 'alice' }).allowed, 'first call allowed');
  assert(!tap.consume(tx, { key: 'alice' }).allowed, 'limit enforced');
  assert(
    other.consume(tx, { key: 'alice' }).allowed,
    'scopes have independent buckets'
  );
  assert(
    tap.consume(tx, { key: 'bob' }).allowed,
    'actors have independent buckets'
  );
  assertThrows(
    () => client({ scope: 'tap', windowSeconds: 30, limit: 5 }),
    'rate_limit.duplicate_scope',
    'a scope can be configured once'
  );
  resetRegisteredScopes();
  assert(
    client({ scope: 'tap', windowSeconds: 30, limit: 5 }).limit === 5,
    'reset lets a scope be configured again'
  );
  assertThrows(
    () => tap.consume(tx, { key: '' }),
    errors.invalidActorKey,
    'empty actor key rejected',
    SenderError
  );
}

{
  const tx = makeTx();
  const upload = client({ scope: 'upload', limit: 3, windowSeconds: 60 });
  assertThrows(
    () => upload.consume(tx, { key: 'alice', cost: 4 }),
    errors.invalidCost,
    'cost above the limit rejected',
    SenderError
  );
  assertThrows(
    () => upload.consume(tx, { key: 'alice', cost: 0 }),
    errors.invalidCost,
    'non-positive cost rejected',
    SenderError
  );
  assert(tx.rows.size === 0, 'rejected cost leaves the bucket untouched');
  const full = upload.consume(tx, { key: 'alice', cost: 3 });
  assert(
    full.allowed && full.remaining === 0,
    'cost equal to the limit allowed'
  );
}

{
  const tx = makeTx();
  const login = client({ scope: 'auth.login', limit: 2, windowSeconds: 60 });
  const one = login.consume(tx, { key: 'ip:1' });
  const two = login.consume(tx, { key: 'ip:1' });
  const three = login.consume(tx, { key: 'ip:1' });
  assert(one.allowed && one.remaining === 1, 'first request allowed');
  assert(two.allowed && two.remaining === 0, 'second request allowed');
  assert(
    !three.allowed && three.retryAfterSeconds === 60,
    'third request blocked'
  );
  const status = login.peek(tx.db, 'ip:1', tx.timestamp);
  assert(
    status.used === 2 && status.remaining === 0 && status.limit === 2,
    'peek reports the open window'
  );
  assert(
    login.peek(tx.db, 'ip:2').used === 0,
    'peek reports an unused bucket as fresh'
  );
  tx.timestamp = new Timestamp(61_000_000n);
  const expired = login.peek(tx.db, 'ip:1', tx.timestamp);
  assert(
    expired.used === 0 && expired.resetAt === undefined,
    'peek reports an expired window as fresh'
  );
  const next = login.consume(tx, { key: 'ip:1' });
  assert(next.allowed && next.used === 1, 'expired window resets');
}

{
  const tx = makeTx();
  const a = client({ scope: 'a:actor:b', limit: 1, windowSeconds: 1 });
  const b = client({ scope: 'a', limit: 1, windowSeconds: 1 });
  assert(
    a.consume(tx, { key: 'c' }).key !== b.consume(tx, { key: 'b:actor:c' }).key,
    'compound keys cannot collide through delimiters'
  );
}

{
  const tx = makeTx();
  const long = client({ scope: 'sweep.long', limit: 1, windowSeconds: 100 });
  const short = client({ scope: 'sweep.short', limit: 1, windowSeconds: 1 });
  long.consume(tx, { key: 'a' });
  long.consume(tx, { key: 'b' });
  const expiredKey = short.consume(tx, { key: 'a' }).key;
  tx.timestamp = new Timestamp(2_000_000n);
  assert(
    sweepRateLimits(tx, byExpiry(tx.rows), 2) === 1,
    'sweep removes only expired buckets'
  );
  assert(!tx.rows.has(expiredKey), 'sweep removes the expired bucket');
  assertThrows(
    () => sweepRateLimits(tx, [], MAX_SWEEP_BATCH + 1),
    errors.invalidSweepBatch,
    'sweep rejects an excessive batch size'
  );
}

process.stdout.write(`\n${pass} passed, ${fail} failed\n`);
process.exit(fail === 0 ? 0 : 1);
