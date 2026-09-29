import * as assert from 'node:assert/strict';
import { Identity, Timestamp } from 'spacetimedb';
import { ScheduleAt, t } from 'spacetimedb/server';
import { client, errors, retryFailed, retryHandler, retryOk } from '../src';

const calls: string[] = [];
const sharedArgs = t.object('TestArgs', { value: t.string() });
const retry = client({
  handlers: {
    throws: retryHandler(t.unit(), () => {
      throw new Error('x'.repeat(3000));
    }),
    fails: retryHandler(t.unit(), () => retryFailed('unavailable')),
    succeeds: retryHandler(t.unit(), () => retryOk()),
    // One argument builder may back several handlers.
    first: retryHandler(sharedArgs, (_ctx, args) => {
      calls.push(`first:${args.value}`);
      return retryOk();
    }),
    second: retryHandler(sharedArgs, (_ctx, args) => {
      calls.push(`second:${args.value}`);
      return retryOk();
    }),
  },
});

type Task = Parameters<typeof retry.reducers.retryFire>[1]['arg'];
type History = ReturnType<typeof retry.views.retryHistoryAdmin>[number];
type Ctx = Parameters<typeof retry.install>[0];

const admin = Identity.zero();
const stranger = new Identity(1n);
const admins = new Map<string, { identity: Identity; addedAt: Timestamp }>();
const history = new Map<bigint, History>();
const tasks = new Map<bigint, Task>();
let nextHistoryId = 0n;
let nextTaskId = 0n;
let sender = admin;
let now = 10_000_000n;

const byRanAt = () =>
  [...history.values()]
    .sort((a, b) =>
      a.ranAt.microsSinceUnixEpoch === b.ranAt.microsSinceUnixEpoch
        ? Number(a.id - b.id)
        : Number(a.ranAt.microsSinceUnixEpoch - b.ranAt.microsSinceUnixEpoch)
    )
    .values();

const ctx: Ctx = {
  get sender() {
    return sender;
  },
  get timestamp() {
    return new Timestamp(now);
  },
  db: {
    retryTask: {
      name: {
        filter: (name: string) =>
          [...tasks.values()].filter(row => row.name === name).values(),
      },
      iter: () => tasks.values(),
      insert(row: Task) {
        const inserted = { ...row, scheduledId: ++nextTaskId };
        tasks.set(inserted.scheduledId, inserted);
        return inserted;
      },
    },
    retryHistory: {
      ranAt: { filter: byRanAt },
      insert(row: History) {
        const inserted = { ...row, id: ++nextHistoryId };
        history.set(inserted.id, inserted);
        return inserted;
      },
      delete: (row: History) => history.delete(row.id),
      count: () => BigInt(history.size),
    },
    retryAdminIdentity: {
      identity: {
        find: (identity: Identity) =>
          admins.get(identity.toHexString()) ?? null,
      },
      insert(row: { identity: Identity; addedAt: Timestamp }) {
        admins.set(row.identity.toHexString(), row);
        return row;
      },
      delete: (row: { identity: Identity }) =>
        admins.delete(row.identity.toHexString()),
      count: () => BigInt(admins.size),
    },
  },
};

retry.install(ctx);
assert.equal(
  admins.get(admin.toHexString())?.addedAt.microsSinceUnixEpoch,
  now
);

// Submission validates input and requires an admin.
const submit = retry.reducers.submitRetryTask.handler;
const valid = {
  name: 'job',
  args: { tag: 'first', value: { value: 'payload' } },
  maxAttempts: 3,
  backoffSecs: 2,
};
assert.throws(() => submit(ctx, { ...valid, name: '' }), {
  message: errors.invalidTaskName,
});
assert.throws(() => submit(ctx, { ...valid, maxAttempts: 11 }), {
  message: errors.invalidMaxAttempts,
});
assert.throws(() => submit(ctx, { ...valid, backoffSecs: 0 }), {
  message: errors.invalidBackoffSeconds,
});
submit(ctx, valid);
assert.throws(() => submit(ctx, valid), {
  message: `${errors.taskAlreadyExists}:job`,
});
sender = stranger;
assert.throws(() => submit(ctx, { ...valid, name: 'other' }), {
  message: errors.notAuthorized,
});
assert.deepEqual(retry.views.retryTasksAdmin(ctx), []);

// Handlers sharing one argument builder dispatch independently. `submit`
// skips the admin check for host reducers that authorize callers themselves.
sender = stranger;
retry.submit(ctx, {
  name: 'job2',
  args: { tag: 'second', value: { value: 'b' } },
  maxAttempts: 1,
  backoffSecs: 1,
});
sender = admin;
retry.reducers.retryFire(ctx, { arg: tasks.get(1n)! });
retry.reducers.retryFire(ctx, { arg: tasks.get(2n)! });
assert.deepEqual(calls, ['first:payload', 'second:b']);

// Failures back off exponentially, then give up.
tasks.clear();
const task: Task = {
  scheduledId: 0n,
  scheduledAt: ScheduleAt.time(0n),
  name: 'test',
  args: { tag: 'throws' },
  attempt: 0,
  maxAttempts: 3,
  backoffSecs: 2,
};
history.clear();
nextHistoryId = 0n;
retry.reducers.retryFire(ctx, { arg: task });
assert.equal(history.get(1n)?.status.tag, 'Failed');
assert.equal(history.get(1n)?.error?.length, 2048);
const [second] = tasks.values();
assert.equal(second.attempt, 1);
assert.deepEqual(second.scheduledAt, ScheduleAt.time(now + 2_000_000n));
retry.reducers.retryFire(ctx, { arg: second });
const third = [...tasks.values()][1];
assert.equal(third.attempt, 2);
assert.deepEqual(third.scheduledAt, ScheduleAt.time(now + 4_000_000n));
retry.reducers.retryFire(ctx, { arg: third });
assert.equal(history.get(3n)?.status.tag, 'GaveUp');
assert.equal(tasks.size, 2);

retry.reducers.retryFire(ctx, { arg: { ...task, args: { tag: 'fails' } } });
assert.equal(history.get(4n)?.error, 'unavailable');
retry.reducers.retryFire(ctx, { arg: { ...task, args: { tag: 'toString' } } });
assert.equal(history.get(5n)?.error, `${errors.unknownHandler}:toString`);
assert.equal(tasks.size, 4);

// The newest pending tasks come first.
assert.deepEqual(
  retry.views.retryTasksAdmin(ctx).map(row => row.scheduledId),
  [...tasks.keys()].reverse()
);

// History keeps the latest 1,000 attempts.
for (let i = 0; i < 1000; i++) {
  now += 1n;
  retry.reducers.retryFire(ctx, {
    arg: { ...task, args: { tag: 'succeeds' } },
  });
}
assert.equal(tasks.size, 4);
assert.equal(history.size, 1000);
assert.equal(history.has(5n), false);
assert.equal(history.has(6n), true);
const recent = retry.views.retryHistoryAdmin(ctx);
assert.equal(recent.length, 1000);
assert.equal(recent[0].id, nextHistoryId);
assert.equal(recent[0].status.tag, 'Ok');
assert.equal(recent.at(-1)?.id, nextHistoryId - 999n);

// The last admin cannot be removed.
assert.throws(
  () =>
    retry.reducers.removeRetryAdminIdentity.handler(ctx, { identity: admin }),
  { message: errors.cannotRemoveLastAdmin }
);

process.stdout.write('retry tests passed\n');
