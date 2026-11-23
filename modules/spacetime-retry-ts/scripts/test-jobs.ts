import * as assert from 'node:assert/strict';
import { Timestamp } from 'spacetimedb';
import { schema, SenderError, t } from 'spacetimedb/server';
import {
  client,
  errors,
  retryFailed,
  retryOk,
  type RetryJobCtx,
  type RetryJobFireRow as Fire,
  type RetryJobRow as Job,
  type RetryOutcome,
} from '../src';

type History = Parameters<RetryJobCtx['db']['retryHistory']['insert']>[0];
type Queue = Parameters<RetryJobCtx['db']['retryQueue']['insert']>[0];

let now = 1_000_000_000n;
let internal = true;
let random = 0.5;
const jobs = new Map<bigint, Job>();
const fires = new Map<bigint, Fire>();
const queues = new Map<string, Queue>();
const history: History[] = [];
let nextId = 0n;

const sortBy = <T>(rows: T[], key: (row: T) => bigint) =>
  rows.sort((a, b) => (key(a) < key(b) ? -1 : key(a) > key(b) ? 1 : 0));

const tx: RetryJobCtx = {
  get timestamp() {
    return new Timestamp(now);
  },
  get senderAuth() {
    return { isInternal: internal };
  },
  random: () => random,
  db: {
    retryJob: {
      id: {
        find: id => jobs.get(id) ?? null,
        update(row) {
          jobs.set(row.id, row);
          return row;
        },
      },
      name: {
        filter: name =>
          [...jobs.values()].filter(j => j.name === name).values(),
      },
      byQueueStatus: {
        filter: ([queue, status]) =>
          sortBy(
            [...jobs.values()].filter(
              j => j.queue === queue && j.status.tag === status.tag
            ),
            j => j.id
          ).values(),
      },
      byStatusUpdatedAt: {
        filter: ([status]) =>
          sortBy(
            [...jobs.values()].filter(j => j.status.tag === status.tag),
            j => j.updatedAt.microsSinceUnixEpoch
          ).values(),
      },
      insert(row) {
        const inserted = { ...row, id: ++nextId };
        jobs.set(inserted.id, inserted);
        return inserted;
      },
      delete: row => jobs.delete(row.id),
    },
    retryJobFire: {
      jobId: {
        filter: jobId =>
          [...fires.values()].filter(f => f.jobId === jobId).values(),
      },
      insert(row) {
        const inserted = { ...row, scheduledId: ++nextId };
        fires.set(inserted.scheduledId, inserted);
        return inserted;
      },
      delete: row => fires.delete(row.scheduledId),
    },
    retryQueue: {
      name: {
        find: name => queues.get(name) ?? null,
        update(row) {
          queues.set(row.name, row);
          return row;
        },
      },
      insert(row) {
        queues.set(row.name, row);
        return row;
      },
    },
    retryHistory: {
      ranAt: { filter: () => [...history].values() },
      insert(row) {
        history.push(row);
        return row;
      },
      delete: row => {
        history.splice(history.indexOf(row), 1);
        return true;
      },
      count: () => BigInt(history.length),
    },
  },
};

const calls: string[] = [];
const outcomes: RetryOutcome<unknown>[] = [];
// Set by a test to act while an attempt is in flight.
let during: (() => void) | undefined;
const retry = client({
  leaseSecs: 10,
  jobs: {
    send: t.object('SendArgs', { to: t.string() }),
    fail: t.unit(),
    reject: t.unit(),
  },
});
const run = retry.retryProcedure(
  schema({ ...retry.tables }) as never,
  {
    send(_ctx: unknown, { to }: { to: string }, job) {
      calls.push(`${to}:${job.attempt}:${job.idempotencyKey}`);
      during?.();
      return retryOk();
    },
    fail: () => retryFailed('down'),
    reject() {
      throw new SenderError('bad.input');
    },
  },
  { onComplete: (_tx: unknown, outcome) => outcomes.push(outcome) }
);
const proc = { withTx: <T>(body: (ctx: RetryJobCtx) => T) => body(tx) };
const fire = (row: Fire) => {
  fires.delete(row.scheduledId);
  (run as (ctx: unknown, args: { arg: Fire }) => void)(proc, { arg: row });
};
const at = (f: Fire) => (f.scheduledAt.value as Timestamp).microsSinceUnixEpoch;
const firesOf = (id: bigint) =>
  sortBy(
    [...fires.values()].filter(f => f.jobId === id),
    at
  );
const send = (name: string, queue?: string) =>
  retry.enqueue(tx, {
    name,
    queue,
    args: { tag: 'send', value: { to: name } },
    maxAttempts: 3,
    backoffSecs: 4,
  });

// Input is checked, and an unfinished job's name can't be reused.
const valid = {
  name: 'a',
  args: { tag: 'fail' as const },
  maxAttempts: 3,
  backoffSecs: 1,
};
assert.throws(() => retry.enqueue(tx, { ...valid, name: '' }), {
  message: errors.invalidTaskName,
});
assert.throws(() => retry.enqueue(tx, { ...valid, queue: '' }), {
  message: errors.invalidQueueName,
});
assert.throws(() => retry.enqueue(tx, { ...valid, maxAttempts: 0 }), {
  message: errors.invalidMaxAttempts,
});
assert.throws(() => retry.enqueue(tx, { ...valid, backoffSecs: 3601 }), {
  message: errors.invalidBackoffSeconds,
});

// A job gets a fire row at runAt and a backup one lease later.
const ok = send('ok');
assert.throws(() => send('ok'), {
  message: `${errors.taskAlreadyExists}:ok`,
});
assert.deepEqual(firesOf(ok).map(at), [now, now + 10_000_000n]);

// Only the host may fire jobs.
internal = false;
assert.throws(
  () =>
    (run as (ctx: unknown, args: { arg: Fire }) => void)(proc, {
      arg: firesOf(ok)[0],
    }),
  { message: errors.notAuthorized }
);
internal = true;

// A claim swaps both rows for a guard row, and success finishes the job.
const [first, backup] = firesOf(ok);
fire(first);
assert.deepEqual(calls, [`ok:1:retry-job-${ok}`]);
assert.equal(retry.status(tx, ok)?.status.tag, 'Succeeded');
assert.equal(fires.size, 0);
assert.equal(outcomes.at(-1)?.status, 'Succeeded');
assert.deepEqual(outcomes.at(-1)?.args, { tag: 'send', value: { to: 'ok' } });
// The stale backup does nothing.
fire(backup);
assert.equal(calls.length, 1);
// A finished job's name can be used again.
retry.cancel(tx, send('ok'));

// Failures back off 4s, then 8s, within 25% jitter, then give up.
const failing = retry.enqueue(tx, { ...valid, name: 'f', backoffSecs: 4 });
random = 0;
fire(firesOf(failing)[0]);
assert.equal(retry.status(tx, failing)?.status.tag, 'Queued');
assert.equal(at(firesOf(failing)[0]), now + 3_000_000n);
random = 0.999;
fire(firesOf(failing)[0]);
assert.ok(at(firesOf(failing)[0]) > now + 9_990_000n);
assert.ok(at(firesOf(failing)[0]) <= now + 10_000_000n);
fire(firesOf(failing)[0]);
assert.equal(retry.status(tx, failing)?.status.tag, 'Failed');
assert.deepEqual(
  history.slice(-3).map(h => [h.attempt, h.status.tag, h.error]),
  [
    [0, 'Failed', 'down'],
    [1, 'Failed', 'down'],
    [2, 'GaveUp', 'down'],
  ]
);
assert.equal(outcomes.at(-1)?.attempts, 3);

// A SenderError gives up at once.
const rejected = retry.enqueue(tx, {
  ...valid,
  name: 'r',
  args: { tag: 'reject' },
});
fire(firesOf(rejected)[0]);
assert.equal(retry.status(tx, rejected)?.error, 'bad.input');
assert.equal(retry.status(tx, rejected)?.attempt, 1);

// The guard row fires when an attempt never records a result.
const lost = send('lost');
const claim = firesOf(lost)[0];
during = () => {
  during = undefined;
  // The guard fires while the attempt is still running.
  now += 10_000_000n;
  fire(firesOf(lost)[0]);
};
fire(claim);
assert.equal(history.at(-1)?.error, errors.leaseExpired);
// The late result was dropped and the job is queued for attempt 2.
assert.equal(retry.status(tx, lost)?.status.tag, 'Queued');
fire(firesOf(lost)[0]);
assert.equal(retry.status(tx, lost)?.status.tag, 'Succeeded');
assert.equal(retry.status(tx, lost)?.attempt, 2);

// A paused queue parks due jobs, and room starts them oldest first.
retry.setQueue(tx, 'mail', { maxParallelism: 0 });
const m1 = send('m1', 'mail');
const m2 = send('m2', 'mail');
fire(firesOf(m1)[0]);
fire(firesOf(m2)[0]);
assert.equal(retry.status(tx, m1)?.status.tag, 'Parked');
assert.equal(fires.size, 0);
retry.setQueue(tx, 'mail', { maxParallelism: 1 });
assert.equal(retry.status(tx, m1)?.status.tag, 'Queued');
assert.equal(retry.status(tx, m2)?.status.tag, 'Parked');
// m1 finishing makes room for m2.
fire(firesOf(m1)[0]);
assert.equal(retry.status(tx, m2)?.status.tag, 'Queued');
assert.throws(() => retry.setQueue(tx, 'mail', { maxParallelism: -1 }), {
  message: errors.invalidParallelism,
});

// Cancelling runs onComplete, and drops a running attempt's result.
const c = send('c');
during = () => {
  during = undefined;
  assert.equal(retry.cancel(tx, c), true);
};
fire(firesOf(c)[0]);
assert.equal(retry.status(tx, c)?.status.tag, 'Cancelled');
assert.equal(outcomes.at(-1)?.status, 'Cancelled');
assert.equal(retry.cancel(tx, c), false);
assert.equal(firesOf(c).length, 0);

// Finished jobs are kept up to 1000 per status.
for (let i = 0; i < 1001; i++) {
  now += 1n;
  retry.cancel(tx, send(`bulk-${i}`));
}
const cancelled = [...jobs.values()].filter(j => j.status.tag === 'Cancelled');
assert.equal(cancelled.length, 1000);
assert.equal(
  cancelled.some(j => j.name === 'bulk-1000'),
  true
);

process.stdout.write('retry job tests passed\n');
