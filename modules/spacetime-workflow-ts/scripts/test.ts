import * as assert from 'node:assert/strict';
import {
  ConnectionId,
  Identity,
  TimeDuration,
  Timestamp,
  Uuid,
} from 'spacetimedb';
import { schema, SenderError, t } from 'spacetimedb/server';
import {
  client,
  errors,
  failures,
  WorkflowStepError,
  type WorkflowCtx,
  type WorkflowFireRow as Fire,
  type WorkflowHistoryRow as History,
  type WorkflowBranchRow as Branch,
  type WorkflowJournalRow as Journal,
  type WorkflowRunRow as Run,
} from '../src';

const SECOND = 1_000_000n;
const databaseIdentity = new Identity(99n);
const admin = new Identity(1n);
const stranger = new Identity(2n);
let sender = admin;
let now = 10n * SECOND;

const Args = t.object('TestArgs', { count: t.u32() });

// Reruns count how many times each workflow function starts.
const appLog: string[] = [];
const keys: string[] = [];
let starts = 0;
let onCall: () => void = () => {};
// onComplete throws while this is above zero.
let completeFailures = 0;
// The fixable workflow's charge step fails while this is true.
let declined = false;

const logOf = (ctx: unknown) => (ctx as { appLog: string[] }).appLog;

const workflows = client();
const db = schema({ ...workflows.tables });
const workflowRunAttempt = workflows.register(db);

const w = {
  chain: workflows.define(db, 'chain', { args: Args }, (step, args) => {
    starts++;
    const one = step.tx('one', ctx => {
      logOf(ctx).push('one');
      return args.count + 1;
    });
    const two = step.call('two', () => {
      appLog.push('two');
      return one + 1;
    });
    step.sleep(60);
    step.tx('three', ctx => {
      logOf(ctx).push(`three:${two}`);
    });
  }),
  flaky: workflows.define(
    db,
    'flaky',
    { args: Args, retry: { maxAttempts: 3, backoffSeconds: 2 } },
    (step, args) => {
      step.tx('attempt', (ctx, run) => {
        logOf(ctx).push(`write:${run.attempt}`);
        keys.push(run.idempotencyKey);
        if (run.attempt <= args.count) throw new Error('x'.repeat(3000));
      });
      step.tx('after', () => {});
    }
  ),
  rejected: workflows.define(
    db,
    'rejected',
    { args: Args, retry: { maxAttempts: 3, backoffSeconds: 1 } },
    step => {
      step.tx('attempt', () => {
        throw new SenderError('order.not_found');
      });
    }
  ),
  caught: workflows.define(db, 'caught', { args: Args }, step => {
    try {
      step.tx('attempt', () => {
        throw new Error('broke');
      });
    } catch (error) {
      assert.ok(error instanceof WorkflowStepError);
      const failed = error.step;
      step.sleep(1);
      step.tx('undo', ctx => {
        logOf(ctx).push(`undo:${failed}:${error.message}`);
      });
    }
  }),
  // Swallowing the pause signal must not let the function run on.
  swallows: workflows.define(db, 'swallows', { args: Args }, step => {
    try {
      step.sleep(30);
    } catch {
      appLog.push('caught the signal');
    }
    appLog.push('after sleep');
    step.tx('late', ctx => {
      logOf(ctx).push('late');
    });
  }),
  uncaught: workflows.define(db, 'uncaught', { args: Args }, step => {
    step.tx('attempt', () => {
      throw new Error('broke');
    });
  }),
  plainThrow: workflows.define(db, 'plainThrow', { args: Args }, () => {
    throw new Error('bug in the function');
  }),
  waits: workflows.define(db, 'waits', { args: Args }, step => {
    step.waitFor('go');
    step.tx('resumed', ctx => {
      logOf(ctx).push('resumed');
    });
  }),
  slowCall: workflows.define(
    db,
    'slowCall',
    {
      args: Args,
      retry: { maxAttempts: 2, backoffSeconds: 1 },
      stepTimeoutSeconds: 5,
    },
    step => {
      step.call('call', (_ctx, run) => {
        appLog.push(`call:${run.attempt}`);
        keys.push(run.idempotencyKey);
        if (run.attempt === 1) onCall();
      });
      step.tx('after', ctx => {
        logOf(ctx).push('after');
      });
    }
  ),
  slowCallOnce: workflows.define(
    db,
    'slowCallOnce',
    { args: Args, stepTimeoutSeconds: 5 },
    step => {
      step.call('call', () => {
        onCall();
      });
    }
  ),
  lostTx: workflows.define(
    db,
    'lostTx',
    { args: Args, stepTimeoutSeconds: 5 },
    step => {
      step.sleep(1);
      step.tx('work', ctx => {
        logOf(ctx).push('work');
      });
    }
  ),
  changing: workflows.define(db, 'changing', { args: Args }, step => {
    step.tx('one', () => {});
    step.sleep(1);
    step.tx('two', () => {});
  }),
  results: workflows.define(db, 'results', { args: Args }, step => {
    const value = step.tx('value', () => ({
      big: 12345678901234567890n,
      list: [1, 'two', { nested: 3n }],
      at: new Timestamp(5n),
      took: new TimeDuration(6n),
      who: new Identity(7n),
      conn: new ConnectionId(8n),
      id: new Uuid(9n),
      bytes: new Uint8Array([0, 15, 255]),
    }));
    const empty = step.tx('empty', () => {});
    step.sleep(1);
    step.tx('check', ctx => {
      logOf(ctx).push(`${value.big}:${value.list.length}:${empty}`);
      // Saved before the sleep, so these come back from the journal.
      assert.ok(value.at instanceof Timestamp);
      assert.equal(value.at.microsSinceUnixEpoch, 5n);
      assert.ok(value.took instanceof TimeDuration);
      assert.equal(value.took.micros, 6n);
      assert.ok(value.who.isEqual(new Identity(7n)));
      assert.ok(value.conn.isEqual(new ConnectionId(8n)));
      assert.ok(value.id instanceof Uuid);
      assert.equal(value.id.asBigInt(), 9n);
      assert.deepEqual(value.bytes, new Uint8Array([0, 15, 255]));
    });
  }),
  overLimit: workflows.define(db, 'overLimit', { args: Args }, step => {
    try {
      step.tx('huge', () => 'x'.repeat(70_000));
    } catch (error) {
      appLog.push((error as Error).message);
    }
    try {
      step.tx('async', async () => 1);
    } catch (error) {
      appLog.push((error as Error).message);
    }
    // Values that would not come back as the same type are refused.
    for (const value of [new Map(), { when: new Date(0) }, NaN]) {
      try {
        step.tx('unsupported', () => value);
      } catch (error) {
        appLog.push((error as Error).message);
      }
    }
  }),
  loops: workflows.define(db, 'loops', { args: Args }, step => {
    for (;;) step.tx('again', () => {});
  }),
  // Item "flaky" fails once, item "bad" always fails, item "slow" calls onCall.
  fanOut: workflows.define(
    db,
    'fanOut',
    {
      args: Args,
      retry: { maxAttempts: 2, backoffSeconds: 1 },
      stepTimeoutSeconds: 5,
    },
    (step, args) => {
      starts++;
      const before = step.tx('before', () => args.count);
      const results = step.parallel(
        'fetch',
        ['a', 'flaky', 'bad', 'slow'],
        (_ctx, item, run) => {
          appLog.push(`${item}:${run.attempt}`);
          keys.push(run.idempotencyKey);
          if (item === 'flaky' && run.attempt === 1) throw new Error('flaky');
          if (item === 'bad') throw new SenderError('bad.item');
          if (item === 'slow' && run.attempt === 1) onCall();
          return { item, big: BigInt(before) + 1n };
        }
      );
      step.tx('after', ctx => {
        logOf(ctx).push(
          results
            .map(r => (r.ok ? `${r.value.item}:${r.value.big}` : r.error))
            .join(',')
        );
      });
    }
  ),
  emptyFanOut: workflows.define(db, 'emptyFanOut', { args: Args }, step => {
    const results = step.parallel('none', [] as string[], () => 1);
    step.tx('after', ctx => {
      logOf(ctx).push(`empty:${results.length}`);
    });
  }),
  wideFanOut: workflows.define(db, 'wideFanOut', { args: Args }, step => {
    step.parallel(
      'wide',
      Array.from({ length: 33 }, (_, i) => i),
      () => 1
    );
  }),
  // The first answer arrives before the wait, the second during it.
  answers: workflows.define(db, 'answers', { args: Args }, step => {
    const first = step.waitFor<{ at: Timestamp }>('answer');
    const second = step.waitFor<number>('answer');
    step.sleep(1);
    step.tx('log', ctx => {
      logOf(ctx).push(
        `${first.at instanceof Timestamp}:${first.at.microsSinceUnixEpoch}:${second}`
      );
    });
  }),
  // A count of 99 fails the run.
  finishes: workflows.define(
    db,
    'finishes',
    { args: Args, retry: { maxAttempts: 2, backoffSeconds: 1 } },
    (step, args) => {
      const count = step.tx('count', () => args.count);
      if (count === 99) throw new Error('bad count');
      return { doubled: count * 2 };
    },
    (ctx, outcome) => {
      if (completeFailures-- > 0) throw new Error('callback broke');
      const doubled: number | undefined = outcome.result?.doubled;
      logOf(ctx).push(
        `${outcome.status}:${outcome.args.count}:${doubled}:${outcome.error}`
      );
    }
  ),
  // The workflow does not retry, but some of its steps do.
  mixed: workflows.define(db, 'mixed', { args: Args }, step => {
    step.call(
      'flaky',
      (_ctx, run) => {
        appLog.push(`flaky:${run.attempt}`);
        if (run.attempt < 3) throw new Error('try again');
      },
      { retry: { maxAttempts: 3, backoffSeconds: 5 } }
    );
    const items = step.parallel(
      'items',
      ['a'],
      (_ctx, item, run) => {
        appLog.push(`${item}:${run.attempt}`);
        if (run.attempt === 1) throw new Error('item failed');
        return item;
      },
      { retry: { maxAttempts: 2, backoffSeconds: 1 } }
    );
    step.tx('once', ctx => {
      logOf(ctx).push(`once:${items.map(r => r.ok).join()}`);
      throw new Error('no retry');
    });
  }),
  badOptions: workflows.define(db, 'badOptions', { args: Args }, step => {
    step.tx('bad', () => {}, { retry: { maxAttempts: 0, backoffSeconds: 1 } });
  }),
  fixable: workflows.define(db, 'fixable', { args: Args }, step => {
    step.tx('reserve', ctx => {
      logOf(ctx).push('reserve');
    });
    step.call('charge', () => {
      appLog.push('charge');
      if (declined) throw new Error('card declined');
    });
    step.tx('ship', ctx => {
      logOf(ctx).push('ship');
    });
  }),
};

// Steps, results, and arguments are typed.
{
  const typed = client();
  const typedDb = schema({ ...typed.tables });
  const only = typed.define(typedDb, 'only', { args: Args }, (step, args) => {
    const count: number = step.tx('value', () => args.count);
    // @ts-expect-error A step's result keeps its type.
    const wrong: string = step.tx('value', () => args.count);
    void [count, wrong];
  });
  void ((ctx: WorkflowCtx) =>
    // @ts-expect-error Arguments must match the workflow.
    typed.start(ctx, only, { count: 'one' }));
  assert.throws(
    () => typed.define(typedDb, 'only', { args: Args }, () => {}),
    /workflow\.invalid_definition:only:duplicate_name/
  );
}

const runs = new Map<bigint, Run>();
const journal = new Map<bigint, Journal>();
const fires = new Map<bigint, Fire>();
const branches = new Map<bigint, Branch>();
let nextBranchId = 0n;
const finished = new Map<bigint, { runId: bigint; finishedAt: Timestamp }>();
const history = new Map<bigint, History>();
const admins = new Map<string, { identity: Identity; addedAt: Timestamp }>();
let nextRunId = 0n;
let nextJournalId = 0n;
let nextFireId = 0n;
let nextHistoryId = 0n;

const byTime = <T>(rows: Iterable<T>, at: (row: T) => Timestamp) =>
  [...rows]
    .sort((a, b) =>
      Number(at(a).microsSinceUnixEpoch - at(b).microsSinceUnixEpoch)
    )
    .values();

const ctx = {
  get sender() {
    return sender;
  },
  get databaseIdentity() {
    return databaseIdentity;
  },
  get timestamp() {
    return new Timestamp(now);
  },
  get appLog() {
    return appLog;
  },
  db: {
    workflowRun: {
      iter: () => runs.values(),
      id: {
        find: (id: bigint) => runs.get(id) ?? null,
        update(row: Run) {
          runs.set(row.id, row);
          return row;
        },
        delete: (id: bigint) => runs.delete(id),
      },
      insert(row: Run) {
        const inserted = { ...row, id: ++nextRunId };
        runs.set(inserted.id, inserted);
        return inserted;
      },
    },
    workflowJournal: {
      runId: {
        filter: (runId: bigint) =>
          [...journal.values()].filter(row => row.runId === runId).values(),
      },
      insert(row: Journal) {
        const inserted = { ...row, id: ++nextJournalId };
        journal.set(inserted.id, inserted);
        return inserted;
      },
      delete: (row: Journal) => journal.delete(row.id),
    },
    workflowFire: {
      runId: {
        filter: (runId: bigint) =>
          [...fires.values()].filter(row => row.runId === runId).values(),
      },
      insert(row: Fire) {
        const inserted = { ...row, scheduledId: ++nextFireId };
        fires.set(inserted.scheduledId, inserted);
        return inserted;
      },
      delete: (row: Fire) => fires.delete(row.scheduledId),
    },
    workflowBranch: {
      id: {
        update(row: Branch) {
          branches.set(row.id, row);
          return row;
        },
      },
      runId: {
        filter: (runId: bigint) =>
          [...branches.values()].filter(row => row.runId === runId).values(),
      },
      insert(row: Branch) {
        const inserted = { ...row, id: ++nextBranchId };
        branches.set(inserted.id, inserted);
        return inserted;
      },
      delete: (row: Branch) => branches.delete(row.id),
    },
    workflowFinished: {
      runId: { delete: (runId: bigint) => finished.delete(runId) },
      finishedAt: {
        filter: () => byTime(finished.values(), row => row.finishedAt),
      },
      insert(row: { runId: bigint; finishedAt: Timestamp }) {
        finished.set(row.runId, row);
        return row;
      },
      delete: (row: { runId: bigint }) => finished.delete(row.runId),
      count: () => BigInt(finished.size),
    },
    workflowHistory: {
      ranAt: { filter: () => byTime(history.values(), row => row.ranAt) },
      insert(row: History) {
        const inserted = { ...row, id: ++nextHistoryId };
        history.set(inserted.id, inserted);
        return inserted;
      },
      delete: (row: History) => history.delete(row.id),
      count: () => BigInt(history.size),
    },
    workflowAdminIdentity: {
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
  // Like the host, a throwing body leaves nothing behind.
  withTx<T>(body: (tx: never) => T): T {
    const tables = [runs, journal, fires, branches, finished, history] as Map<
      bigint,
      never
    >[];
    const saved = tables.map(table => new Map(table));
    const savedLog = [...appLog];
    try {
      return body(ctx as never);
    } catch (error) {
      tables.forEach((table, i) => {
        table.clear();
        for (const [key, value] of saved[i]) table.set(key, value);
      });
      appLog.length = 0;
      appLog.push(...savedLog);
      throw error;
    }
  },
};
const tx = ctx as unknown as WorkflowCtx;

// Runs due fires as the host does: delete the row, then call the procedure.
function runDue(): void {
  for (;;) {
    const due = [...fires.values()]
      .filter(
        row =>
          row.scheduledAt.tag === 'Time' &&
          row.scheduledAt.value.microsSinceUnixEpoch <= now
      )
      .sort((a, b) => Number(a.scheduledId - b.scheduledId))[0];
    if (!due) return;
    fires.delete(due.scheduledId);
    const previous = sender;
    sender = databaseIdentity;
    workflowRunAttempt(ctx as never, { arg: due });
    sender = previous;
  }
}

const advance = (seconds: bigint) => {
  now += seconds * SECOND;
  runDue();
};
const run = (id: bigint) => {
  const row = runs.get(id);
  assert.ok(row);
  return row;
};
const statusOf = (id: bigint) => run(id).status.tag;
const attemptsOf = (id: bigint) =>
  [...history.values()].filter(row => row.runId === id);
const firesOf = (id: bigint) =>
  [...fires.values()].filter(row => row.runId === id);
const journalOf = (id: bigint) =>
  [...journal.values()].filter(row => row.runId === id);
const reset = () => {
  appLog.length = 0;
  keys.length = 0;
  starts = 0;
  onCall = () => {};
};

workflows.install(tx);
assert.ok(admins.has(admin.toHexString()));

// Steps run in order, pass results along, and sleep without running early. The
// function only reruns after the sleep, not after every step.
{
  reset();
  const id = workflows.start(tx, w.chain, { count: 0 });
  assert.equal(run(id).startedBy, admin);
  runDue();
  assert.deepEqual(appLog, ['one', 'two']);
  assert.equal(starts, 1);
  assert.equal(run(id).lastStep, 'sleep');
  assert.equal(run(id).stepCount, 3);
  advance(59n);
  assert.equal(starts, 1);
  advance(1n);
  assert.equal(starts, 2);
  assert.deepEqual(appLog, ['one', 'two', 'three:2'], 'no step ran twice');
  assert.equal(statusOf(id), 'Completed');
  assert.equal(firesOf(id).length, 0);
  assert.equal(journalOf(id).length, 0, 'the journal is removed when finished');
  assert.deepEqual(
    attemptsOf(id).map(row => [row.step, row.status.tag]),
    [
      ['one', 'Ok'],
      ['two', 'Ok'],
      ['three', 'Ok'],
    ]
  );
  advance(600n);
  assert.equal(starts, 2);
}

// Results survive a rerun, including bigints and undefined.
{
  reset();
  const id = workflows.start(tx, w.results, { count: 0 });
  runDue();
  advance(1n);
  assert.deepEqual(appLog, ['12345678901234567890:3:undefined']);
  assert.equal(statusOf(id), 'Completed');
}

// A failed attempt rolls back its writes, then retries with growing backoff.
{
  reset();
  const id = workflows.start(tx, w.flaky, { count: 2 });
  runDue();
  assert.deepEqual(appLog, []);
  assert.equal(run(id).attempt, 1);
  assert.equal(statusOf(id), 'Running');
  advance(1n);
  assert.equal(run(id).attempt, 1, 'first backoff is two seconds');
  advance(1n);
  assert.equal(run(id).attempt, 2);
  advance(3n);
  assert.equal(run(id).attempt, 2, 'second backoff is four seconds');
  advance(1n);
  assert.deepEqual(appLog, ['write:3']);
  assert.equal(statusOf(id), 'Completed');
  assert.equal(new Set(keys).size, 1, 'attempts share an idempotency key');
  const failed = attemptsOf(id).filter(row => row.status.tag === 'Failed');
  assert.equal(failed.length, 2);
  assert.equal(failed[0].error?.length, 2048);
}

// Running out of attempts fails the run if the function does not catch it.
{
  reset();
  const id = workflows.start(tx, w.flaky, { count: 3 });
  advance(0n);
  advance(2n);
  advance(4n);
  assert.equal(statusOf(id), 'Failed');
  assert.equal(run(id).error?.length, 2048);
  assert.equal(attemptsOf(id).at(-1)?.status.tag, 'GaveUp');
  assert.equal(firesOf(id).length, 0);
}

// A SenderError means the input is bad, so it does not retry.
{
  reset();
  const id = workflows.start(tx, w.rejected, { count: 0 });
  runDue();
  assert.equal(statusOf(id), 'Failed');
  assert.equal(run(id).error, 'order.not_found');
  assert.equal(attemptsOf(id).length, 1);
}

// A function can catch a step that gave up. The same error comes back on a rerun.
{
  reset();
  const id = workflows.start(tx, w.caught, { count: 0 });
  runDue();
  assert.equal(statusOf(id), 'Running');
  advance(1n);
  assert.deepEqual(appLog, ['undo:attempt:broke']);
  assert.equal(statusOf(id), 'Completed');
  assert.equal(run(id).error, undefined);
}

// Catching the pause signal does not keep the function going.
{
  reset();
  const id = workflows.start(tx, w.swallows, { count: 0 });
  runDue();
  assert.equal(statusOf(id), 'Running', 'a swallowed signal must not finish');
  assert.equal(firesOf(id).length, 2);
  assert.deepEqual(appLog, ['caught the signal', 'after sleep']);
  assert.equal(run(id).stepCount, 1, 'the late step must not run');
  advance(30n);
  assert.equal(statusOf(id), 'Completed');
  assert.equal(run(id).stepCount, 2);
}

// Anything the function does not catch fails the run.
{
  reset();
  const id = workflows.start(tx, w.uncaught, { count: 0 });
  runDue();
  assert.equal(statusOf(id), 'Failed');
  assert.equal(run(id).error, 'broke');

  const thrower = workflows.start(tx, w.plainThrow, { count: 0 });
  runDue();
  assert.equal(statusOf(thrower), 'Failed');
  assert.equal(run(thrower).error, 'bug in the function');
}

// A waiting run holds no scheduled work and resumes on its event.
{
  reset();
  const id = workflows.start(tx, w.waits, { count: 0 });
  runDue();
  assert.equal(statusOf(id), 'Waiting');
  assert.equal(run(id).waitingFor, 'go');
  assert.equal(firesOf(id).length, 0);
  assert.equal(workflows.sendEvent(tx, id, 'other'), true);
  assert.equal(statusOf(id), 'Waiting');
  assert.equal(workflows.sendEvent(tx, id, 'go'), true);
  runDue();
  assert.deepEqual(appLog, ['resumed']);
  assert.equal(statusOf(id), 'Completed');
  assert.equal(workflows.sendEvent(tx, id, 'go'), false);
  assert.equal(workflows.sendEvent(tx, 12345n, 'go'), false);
  assert.throws(() => workflows.sendEvent(tx, id, ''), {
    message: errors.invalidEvent,
  });
}

// An event that arrives before the wait is kept, and pending events are capped.
{
  reset();
  const id = workflows.start(tx, w.waits, { count: 0 });
  assert.equal(workflows.sendEvent(tx, id, 'go'), true);
  assert.equal(workflows.sendEvent(tx, id, 'go'), true);
  assert.deepEqual(
    run(id).pendingEvents.map(e => e.name),
    ['go', 'go']
  );
  runDue();
  assert.deepEqual(appLog, ['resumed']);
  assert.equal(statusOf(id), 'Completed');
  // Each wait takes one event, oldest first.
  assert.deepEqual(
    run(id).pendingEvents.map(e => e.name),
    ['go']
  );

  const capped = workflows.start(tx, w.waits, { count: 0 });
  for (let i = 0; i < 32; i++) workflows.sendEvent(tx, capped, `event${i}`);
  assert.throws(() => workflows.sendEvent(tx, capped, 'one-too-many'), {
    message: errors.tooManyEvents,
  });
  workflows.cancel(tx, capped);
}

// Events carry values, whether they arrive before or during the wait.
{
  reset();
  const id = workflows.start(tx, w.answers, { count: 0 });
  workflows.sendEvent(tx, id, 'answer', { at: new Timestamp(5n) });
  runDue();
  assert.equal(run(id).waitingFor, 'answer');
  workflows.sendEvent(tx, id, 'answer', 7);
  runDue();
  advance(1n);
  assert.deepEqual(appLog, ['true:5:7']);
  assert.equal(statusOf(id), 'Completed');
  assert.throws(() => workflows.sendEvent(tx, id, 'answer', new Map()), {
    message: `${failures.unsupportedValue}:Map`,
  });
}

// onComplete runs once after a run ends, with its result or error.
{
  reset();
  const done = workflows.start(tx, w.finishes, { count: 2 });
  const failed = workflows.start(tx, w.finishes, { count: 99 });
  const cancelled = workflows.start(tx, w.finishes, { count: 5 });
  workflows.cancel(tx, cancelled);
  assert.equal(run(cancelled).completing, true);
  assert.ok(!finished.has(cancelled), 'kept until onComplete runs');
  runDue();
  assert.deepEqual(appLog.sort(), [
    'Cancelled:5:undefined:undefined',
    'Completed:2:4:undefined',
    'Failed:99:undefined:bad count',
  ]);
  for (const id of [done, failed, cancelled]) {
    assert.equal(run(id).completing, false);
    assert.equal(firesOf(id).length, 0);
    assert.ok(finished.has(id));
  }
  assert.equal(run(done).result, '{"doubled":4}');
}

// A failing onComplete is retried, then given up on.
{
  reset();
  completeFailures = 1;
  const retried = workflows.start(tx, w.finishes, { count: 1 });
  runDue();
  assert.deepEqual(appLog, []);
  assert.equal(run(retried).completing, true);
  advance(1n);
  assert.deepEqual(appLog, ['Completed:1:2:undefined']);
  assert.deepEqual(
    attemptsOf(retried)
      .filter(row => row.step === 'onComplete')
      .map(row => [row.status.tag, row.error]),
    [
      ['Failed', 'callback broke'],
      ['Ok', undefined],
    ]
  );

  reset();
  completeFailures = 2;
  const abandoned = workflows.start(tx, w.finishes, { count: 1 });
  runDue();
  advance(1n);
  assert.deepEqual(appLog, []);
  assert.equal(run(abandoned).completing, false);
  assert.equal(statusOf(abandoned), 'Completed');
  assert.deepEqual(
    attemptsOf(abandoned)
      .filter(row => row.step === 'onComplete')
      .map(row => row.status.tag),
    ['Failed', 'GaveUp']
  );
  completeFailures = 0;
}

// A step's own retry settings replace the workflow's.
{
  reset();
  const id = workflows.start(tx, w.mixed, { count: 0 });
  runDue();
  assert.deepEqual(appLog, ['flaky:1']);
  advance(5n);
  assert.deepEqual(appLog, ['flaky:1', 'flaky:2']);
  advance(9n);
  assert.deepEqual(appLog, ['flaky:1', 'flaky:2'], 'backoff doubles');
  advance(1n);
  assert.deepEqual(appLog, ['flaky:1', 'flaky:2', 'flaky:3', 'a:1']);
  advance(1n);
  // The item succeeds on its second attempt. The last step has one attempt.
  assert.deepEqual(appLog, ['flaky:1', 'flaky:2', 'flaky:3', 'a:1', 'a:2']);
  assert.equal(statusOf(id), 'Failed');
  assert.equal(run(id).error, 'no retry');
  assert.equal(attemptsOf(id).filter(row => row.step === 'once').length, 1);

  const bad = workflows.start(tx, w.badOptions, { count: 0 });
  runDue();
  assert.equal(statusOf(bad), 'Failed');
  assert.equal(
    run(bad).error,
    'workflow.invalid_step_options:bad:max_attempts'
  );
}

// A failed run restarts from the step that failed it, keeping earlier steps.
{
  reset();
  declined = true;
  const id = workflows.start(tx, w.fixable, { count: 0 });
  runDue();
  assert.equal(statusOf(id), 'Failed');
  assert.equal(run(id).error, 'card declined');
  assert.deepEqual(appLog, ['reserve', 'charge']);
  assert.ok(finished.has(id));

  declined = false;
  assert.equal(workflows.restart(tx, id), true);
  assert.equal(statusOf(id), 'Running');
  assert.equal(run(id).error, undefined);
  assert.equal(run(id).lastStep, 'reserve');
  assert.ok(!finished.has(id));
  runDue();
  assert.deepEqual(appLog, ['reserve', 'charge', 'charge', 'ship']);
  assert.equal(statusOf(id), 'Completed');
  assert.equal(journalOf(id).length, 0);
  assert.equal(workflows.restart(tx, id), false, 'only failed or cancelled');
}

// A restart can start from an earlier step, by name or position.
{
  reset();
  declined = true;
  const id = workflows.start(tx, w.fixable, { count: 0 });
  runDue();
  assert.throws(() => workflows.restart(tx, id, 'missing'), {
    message: errors.unknownStep,
  });
  assert.throws(() => workflows.restart(tx, id, 3), {
    message: errors.unknownStep,
  });
  declined = false;
  assert.equal(workflows.restart(tx, id, 'reserve'), true);
  runDue();
  assert.deepEqual(appLog, ['reserve', 'charge', 'reserve', 'charge', 'ship']);
  assert.equal(statusOf(id), 'Completed');
}

// A cancelled run restarts where it stopped. A run waits for its onComplete.
{
  reset();
  const id = workflows.start(tx, w.waits, { count: 0 });
  runDue();
  workflows.cancel(tx, id);
  assert.equal(workflows.restart(tx, id), true);
  runDue();
  assert.equal(statusOf(id), 'Waiting');
  assert.equal(run(id).waitingFor, 'go');
  workflows.cancel(tx, id);

  completeFailures = 1;
  const pending = workflows.start(tx, w.finishes, { count: 99 });
  runDue();
  assert.equal(run(pending).completing, true);
  assert.equal(workflows.restart(tx, pending), false);
  advance(1n);
  assert.equal(workflows.restart(tx, pending), true);
  completeFailures = 0;
  runDue();
  assert.equal(statusOf(pending), 'Failed');
}

// Cancelling removes scheduled work, and only applies to active runs.
{
  reset();
  const id = workflows.start(tx, w.chain, { count: 0 });
  assert.equal(workflows.cancel(tx, id), true);
  assert.equal(statusOf(id), 'Cancelled');
  assert.equal(firesOf(id).length, 0);
  assert.equal(workflows.cancel(tx, id), false);
  runDue();
  assert.deepEqual(appLog, []);
}

// A result that arrives after cancelling is ignored, and later steps do not run.
{
  reset();
  const id = workflows.start(tx, w.slowCall, { count: 0 });
  onCall = () => workflows.cancel(tx, id);
  runDue();
  assert.equal(statusOf(id), 'Cancelled');
  assert.deepEqual(appLog, ['call:1']);
  assert.equal(firesOf(id).length, 0);
  assert.equal(attemptsOf(id).length, 0);
}

// An attempt that outlives its timeout is retried. Its late result is ignored.
{
  reset();
  const id = workflows.start(tx, w.slowCall, { count: 0 });
  onCall = () => {
    onCall = () => {};
    advance(5n);
  };
  runDue();
  assert.deepEqual(appLog, ['call:1', 'call:2', 'after']);
  assert.equal(new Set(keys).size, 1);
  assert.deepEqual(
    attemptsOf(id).map(row => [row.step, row.status.tag, row.error]),
    [
      ['call', 'Failed', failures.stepLost],
      ['call', 'Ok', undefined],
      ['after', 'Ok', undefined],
    ]
  );
  assert.equal(statusOf(id), 'Completed');
}

// A lost `call` with no attempts left gives up, which fails an uncaught run.
{
  reset();
  const id = workflows.start(tx, w.slowCallOnce, { count: 0 });
  onCall = () => {
    onCall = () => {};
    advance(5n);
  };
  runDue();
  assert.equal(statusOf(id), 'Failed');
  assert.equal(run(id).error, failures.stepLost);
}

// A lost `tx` step never committed, so it repeats even with one attempt.
{
  reset();
  const id = workflows.start(tx, w.lostTx, { count: 0 });
  runDue();
  now += SECOND;
  const row = run(id);
  // As if the host stopped during the transaction, after the claim committed.
  const [fire] = firesOf(id);
  fires.delete(fire.scheduledId);
  runs.set(id, {
    ...row,
    generation: row.generation + 1n,
    attempt: 1,
    inFlight: true,
  });
  for (const other of firesOf(id)) fires.delete(other.scheduledId);
  fires.set(1_000_000n, {
    ...fire,
    scheduledId: 1_000_000n,
    generation: row.generation + 1n,
  });
  runDue();
  assert.deepEqual(appLog, ['work']);
  assert.equal(statusOf(id), 'Completed');
}

// If the host stops before claiming a fire, the second fire runs it and no
// attempt is counted as lost.
{
  reset();
  const id = workflows.start(tx, w.slowCall, { count: 0 });
  const [first, second] = firesOf(id);
  fires.delete(first.scheduledId);
  assert.ok(second);
  runDue();
  assert.equal(statusOf(id), 'Running');
  advance(5n);
  assert.deepEqual(appLog, ['call:1', 'after']);
  assert.equal(statusOf(id), 'Completed');
  assert.ok(attemptsOf(id).every(row => row.error === undefined));
}

// Code that no longer matches the saved steps fails the run with a clear error.
{
  reset();
  const id = workflows.start(tx, w.changing, { count: 0 });
  runDue();
  const saved = journalOf(id)[0];
  journal.set(saved.id, { ...saved, name: 'renamed' });
  advance(1n);
  assert.equal(statusOf(id), 'Failed');
  assert.equal(run(id).error, `${failures.replayMismatch}:0:renamed:one`);
}

// Oversized, async, and unsupported results give up without retrying.
{
  reset();
  const id = workflows.start(tx, w.overLimit, { count: 0 });
  runDue();
  assert.deepEqual(appLog, [
    failures.resultTooLarge,
    failures.asyncStep,
    `${failures.unsupportedValue}:Map`,
    `${failures.unsupportedValue}:Date`,
    `${failures.unsupportedValue}:NaN`,
  ]);
  assert.equal(statusOf(id), 'Completed');
}

// A function that never stops adding steps is stopped.
{
  reset();
  const id = workflows.start(tx, w.loops, { count: 0 });
  runDue();
  assert.equal(statusOf(id), 'Failed');
  assert.equal(run(id).error, failures.tooManySteps);
  // A failed run keeps its steps, so it can be restarted.
  assert.equal(journalOf(id).length, 10_000);
}

// Only the scheduler can run an attempt.
{
  reset();
  const id = workflows.start(tx, w.chain, { count: 0 });
  const [fire] = firesOf(id);
  assert.throws(() => workflowRunAttempt(ctx as never, { arg: fire }), {
    message: errors.notAuthorized,
  });
  assert.deepEqual(appLog, []);
  workflows.cancel(tx, id);
}

// Administration requires an admin and keeps at least one.
{
  const {
    cancelWorkflowRun,
    addWorkflowAdminIdentity,
    removeWorkflowAdminIdentity,
  } = workflows.reducers;
  const id = workflows.start(tx, w.waits, { count: 0 });
  sender = stranger;
  assert.throws(() => cancelWorkflowRun.handler(tx, { runId: id }), {
    message: errors.notAuthorized,
  });
  assert.deepEqual(workflows.views.workflowRunsAdmin(ctx as never), []);
  assert.deepEqual(workflows.views.workflowHistoryAdmin(ctx as never), []);
  sender = admin;
  assert.equal(workflows.views.workflowRunsAdmin(ctx as never)[0].id, id);
  cancelWorkflowRun.handler(tx, { runId: id });
  assert.equal(statusOf(id), 'Cancelled');
  addWorkflowAdminIdentity.handler(tx, { identity: stranger });
  removeWorkflowAdminIdentity.handler(tx, { identity: stranger });
  assert.throws(
    () => removeWorkflowAdminIdentity.handler(tx, { identity: admin }),
    { message: errors.cannotRemoveLastAdmin }
  );
}

// Finished runs and history stay capped. Active runs are not removed.
{
  reset();
  const active = workflows.start(tx, w.waits, { count: 0 });
  runDue();
  const first = workflows.start(tx, w.chain, { count: 0 });
  workflows.cancel(tx, first);
  for (let i = 0; i < 1100; i++) {
    now += SECOND;
    workflows.cancel(tx, workflows.start(tx, w.chain, { count: 0 }));
  }
  assert.equal(finished.size, 1000);
  assert.equal(runs.has(first), false);
  assert.equal(statusOf(active), 'Waiting');

  for (let i = 0; i < 1100; i++) {
    now += SECOND;
    workflows.start(tx, w.rejected, { count: 0 });
    runDue();
  }
  assert.equal(history.size, 1000);
}

// Parallel items are all scheduled at once, run separately, and come back in
// item order. Failures are returned, not thrown, and the run carries on.
{
  reset();
  const id = workflows.start(tx, w.fanOut, { count: 41 });
  const startsBefore = starts;
  now += SECOND;
  // Run only the main fire: it saves `before`, then hands off to the items.
  const [main] = firesOf(id).sort((a, b) =>
    Number(a.scheduledId - b.scheduledId)
  );
  fires.delete(main.scheduledId);
  sender = databaseIdentity;
  workflowRunAttempt(ctx as never, { arg: main });
  sender = admin;
  assert.equal(statusOf(id), 'Waiting');
  assert.equal(run(id).waitingFor, 'parallel:fetch');
  const itemFires = firesOf(id).filter(fire => fire.branch !== undefined);
  assert.equal(itemFires.length, 8, 'every item is scheduled at once');
  assert.deepEqual(
    new Set(itemFires.map(fire => fire.branch)),
    new Set([0, 1, 2, 3])
  );

  // The slow item outlives its timeout. Its retry wins, its late result is ignored.
  onCall = () => {
    onCall = () => {};
    advance(5n);
  };
  runDue();
  advance(1n);
  assert.equal(statusOf(id), 'Completed');
  assert.deepEqual(appLog.at(-1), 'a:42,flaky:42,bad.item,slow:42');
  assert.ok((appLog as string[]).includes('flaky:2'), 'the flaky item retried');
  assert.equal(
    appLog.filter(entry => entry === 'bad:1').length,
    1,
    'a SenderError item does not retry'
  );
  assert.ok((appLog as string[]).includes('slow:2'));
  assert.equal(
    new Set(keys).size,
    4,
    'each item keeps one idempotency key across attempts'
  );
  assert.equal(branches.size, 0, 'item rows are removed once saved');
  assert.equal(journalOf(id).length, 0);
  assert.ok(starts - startsBefore >= 2, 'the function resumed after the items');
  const failed = attemptsOf(id)
    .filter(row => row.step.startsWith('fetch') && row.status.tag !== 'Ok')
    .map(row => `${row.step} ${row.status.tag} ${row.error}`)
    .sort();
  assert.deepEqual(failed, [
    'fetch #2 Failed flaky',
    'fetch #3 GaveUp bad.item',
    `fetch #4 Failed ${failures.stepLost}`,
  ]);
}

// Cancelling during a parallel step drops the items and their late results.
{
  reset();
  const id = workflows.start(tx, w.fanOut, { count: 0 });
  onCall = () => workflows.cancel(tx, id);
  runDue();
  assert.equal(statusOf(id), 'Cancelled');
  assert.equal(branches.size, 0);
  assert.equal(firesOf(id).length, 0);
}

// No items finishes straight away. Too many items fails the run.
{
  reset();
  const empty = workflows.start(tx, w.emptyFanOut, { count: 0 });
  runDue();
  assert.deepEqual(appLog, ['empty:0']);
  assert.equal(statusOf(empty), 'Completed');

  const wide = workflows.start(tx, w.wideFanOut, { count: 0 });
  runDue();
  assert.equal(statusOf(wide), 'Failed');
  assert.equal(run(wide).error, failures.tooManyBranches);
}

// A host cannot wake a parallel step by sending its internal event name.
{
  reset();
  const id = workflows.start(tx, w.waits, { count: 0 });
  assert.throws(() => workflows.sendEvent(tx, id, 'parallel:fetch'), {
    message: errors.invalidEvent,
  });
  workflows.cancel(tx, id);
}

// Definitions are checked when the module loads.
for (const [detail, def] of [
  ['max_attempts', { retry: { maxAttempts: 0, backoffSeconds: 1 } }],
  ['max_attempts', { retry: { maxAttempts: 11, backoffSeconds: 1 } }],
  ['backoff_seconds', { retry: { maxAttempts: 1, backoffSeconds: 0 } }],
  ['step_timeout_seconds', { stepTimeoutSeconds: 0 }],
] as const) {
  assert.throws(
    () => workflows.define(db, 'bad', { args: Args, ...def }, () => {}),
    { message: `workflow.invalid_definition:bad:${detail}` }
  );
}
assert.throws(() => workflows.define(db, '', { args: Args }, () => {}), {
  message: 'workflow.invalid_definition::name',
});

// A run whose workflow was removed from the code fails with a clear error.
{
  reset();
  const id = workflows.start(tx, w.waits, { count: 0 });
  runs.set(id, { ...run(id), workflow: 'removed' });
  runDue();
  assert.equal(statusOf(id), 'Failed');
  assert.equal(run(id).error, `${failures.unknownWorkflow}:removed`);
  assert.throws(() => workflows.start(tx, { name: 'other' }, {}), {
    message: `${failures.unknownWorkflow}:other`,
  });
}
