import * as assert from 'node:assert/strict';
import { ScheduleAt, Timestamp, t, table } from 'spacetimedb';
import {
  makeRetryDispatch,
  retryFailed,
  retryHandler,
  retryOk,
} from '../src/handler.ts';
import { createRetrySubmodule } from '../src/submodule.ts';

const calls: string[] = [];
const handlers = {
  noArgs: retryHandler(t.unit(), () => {
    calls.push('noArgs');
    return retryOk();
  }),
  withArgs: retryHandler(
    t.object('TestArgs', { value: t.string() }),
    (_ctx, args: { value: string }) => {
      calls.push(args.value);
      return retryFailed('try again');
    }
  ),
};

assert.deepEqual(Object.keys(handlers), ['noArgs', 'withArgs']);
const dispatch = makeRetryDispatch(handlers);
assert.deepEqual(dispatch({}, { tag: 'noArgs' }), { ok: true });
assert.deepEqual(
  dispatch({}, { tag: 'withArgs', value: { value: 'payload' } }),
  {
    ok: false,
    error: 'try again',
  }
);
assert.deepEqual(calls, ['noArgs', 'payload']);
assert.throws(
  () => dispatch({}, { tag: 'missing' as keyof typeof handlers }),
  /unknown retry handler/
);
assert.throws(
  () => dispatch({}, { tag: 'toString' as keyof typeof handlers }),
  /unknown retry handler/
);

const retry = createRetrySubmodule(
  { table, t, ScheduleAt, SenderError: Error },
  {
    throws: retryHandler(t.unit(), () => {
      throw new Error('x'.repeat(3000));
    }),
    fails: retryHandler(t.unit(), () => retryFailed('unavailable')),
    succeeds: retryHandler(t.unit(), () => retryOk()),
  },
  { isAdmin: () => true }
);
type Task = Parameters<typeof retry.reducers.retryFire>[1]['arg'];
type History = ReturnType<typeof retry.views.retryHistoryAdmin>[number];
const history = new Map<bigint, History>();
const scheduled: Task[] = [];
let nextId = 0n;
const ctx = {
  timestamp: new Timestamp(10_000_000n),
  db: {
    retryTask: { insert: (row: Task) => scheduled.push(row) },
    retryHistory: {
      insert(row: History) {
        const inserted = { ...row, id: ++nextId };
        history.set(inserted.id, inserted);
        return inserted;
      },
      id: {
        update: (row: History) => history.set(row.id, row),
        delete: (id: bigint) => history.delete(id),
      },
      iter: () => history.values(),
    },
  },
};
const task: Task = {
  scheduledId: 1n,
  scheduledAt: ScheduleAt.time(0n),
  name: 'test',
  args: { tag: 'throws' },
  attempt: 0,
  maxAttempts: 3,
  backoffSecs: 2,
};
retry.reducers.retryFire(ctx, { arg: task });
assert.equal(history.get(1n)?.status.tag, 'Failed');
assert.equal(history.get(1n)?.error?.length, 2048);
assert.equal(scheduled[0].attempt, 1);
assert.deepEqual(scheduled[0].scheduledAt, ScheduleAt.time(12_000_000n));
retry.reducers.retryFire(ctx, { arg: scheduled[0] });
assert.equal(scheduled[1].attempt, 2);
assert.deepEqual(scheduled[1].scheduledAt, ScheduleAt.time(14_000_000n));
retry.reducers.retryFire(ctx, { arg: scheduled[1] });
assert.equal(history.get(3n)?.status.tag, 'GaveUp');
assert.equal(scheduled.length, 2);
retry.reducers.retryFire(ctx, {
  arg: { ...task, args: { tag: 'fails' } },
});
assert.equal(history.get(4n)?.error, 'unavailable');
assert.equal(scheduled.length, 3);
for (let i = 0; i < 1001; i++) {
  retry.reducers.retryFire(ctx, {
    arg: { ...task, args: { tag: 'succeeds' } },
  });
}
assert.equal(scheduled.length, 3);
assert.equal(history.size, 1000);
assert.equal(history.has(5n), false);
const recent = retry.views.retryHistoryAdmin(ctx);
assert.equal(recent.length, 1000);
assert.equal(recent[0].id, nextId);
assert.equal(recent[0].status.tag, 'Ok');
assert.equal(recent.at(-1)?.id, nextId - 999n);

console.log('retry tests passed');
