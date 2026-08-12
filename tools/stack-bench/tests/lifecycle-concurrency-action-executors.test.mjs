import assert from 'node:assert/strict';
import test from 'node:test';

import { ACTION_REGISTRY } from '../action-catalog.mjs';
import { createActionRunContext, executeAction } from '../action-contract.mjs';
import {
  createDatabaseWriteCapability,
  createLifecycleCapability,
  LIFECYCLE_CONCURRENCY_ACTION_IDS,
  LIFECYCLE_CONCURRENCY_ACTION_IMPLEMENTATIONS,
} from '../lifecycle-concurrency-action-executors.mjs';

const sleep = async () => {};

function services(actors = new Map(), overrides = {}) {
  return {
    actors: { get: name => actors.get(name) },
    'application-lifecycle': overrides.applicationLifecycle
      ?? createLifecycleCapability({ sleep }),
    'backend-lifecycle': overrides.backendLifecycle
      ?? createLifecycleCapability({ sleep }),
    'browser-interaction': overrides.browser ?? {
      clients: { open: async () => {}, fresh: async () => 'a-fresh' },
      sleep,
    },
    concurrency: overrides.concurrency ?? {
      defaultWithin: 5000,
      dispatch: async () => null,
      expand: value => value,
      sleep,
      testId: id => `[data-testid="${id}"]`,
    },
    'database-write': overrides.databaseWrite ?? { setStock: async input => input },
  };
}

async function run(input, capabilities) {
  return executeAction(ACTION_REGISTRY, input.do, input, createActionRunContext({
    capabilities,
    implementations: LIFECYCLE_CONCURRENCY_ACTION_IMPLEMENTATIONS,
    attempt: { id: `test-${input.do}` },
  }));
}

test('the final executor registry covers exactly the lifecycle/concurrency action ids', () => {
  assert.deepEqual(Object.keys(LIFECYCLE_CONCURRENCY_ACTION_IMPLEMENTATIONS).sort(),
    LIFECYCLE_CONCURRENCY_ACTION_IDS);
  for (const id of LIFECYCLE_CONCURRENCY_ACTION_IDS) {
    assert(ACTION_REGISTRY.get(id).deadline.timeoutMs > 0, id);
  }
});

test('race preserves branch ordering while overlapping branches through registered dispatch', async () => {
  const events = [];
  const capability = services(new Map(), { concurrency: {
    defaultWithin: 5000,
    expand: value => value,
    sleep,
    testId: id => id,
    dispatch: async step => {
      events.push(`start-${step.actor}-${step.ms}`);
      if (step.ms === 1) await new Promise(resolve => setImmediate(resolve));
      events.push(`end-${step.actor}-${step.ms}`);
    },
  } });
  const result = await run({ do: 'race', settleMs: 0, branches: [
    [{ do: 'wait', actor: 'a', ms: 1 }, { do: 'wait', actor: 'a', ms: 2 }],
    [{ do: 'wait', actor: 'b', ms: 3 }],
  ] }, capability);
  assert.equal(result.status, 'passed');
  assert(events.indexOf('start-b-3') < events.indexOf('start-a-2'));
  assert(events.indexOf('end-a-1') < events.indexOf('start-a-2'));

  const nestedFailure = new Error('nested action failed');
  Object.defineProperty(nestedFailure, 'actionEvidence', { value: {
    status: 'failed', summary: 'nested application mismatch',
  } });
  const failed = await run({ do: 'race', settleMs: 0, branches: [
    [{ do: 'wait', actor: 'a', ms: 1 }], [{ do: 'wait', actor: 'b', ms: 1 }],
  ] }, services(new Map(), { concurrency: {
    defaultWithin: 5000, expand: value => value, sleep, testId: id => id,
    dispatch: async () => { throw nestedFailure; },
  } }));
  assert.equal(failed.status, 'failed');
  assert.equal(failed.summary, 'nested application mismatch');
});

test('concurrent replay refuses to invent contention when fewer than two writes exist', async () => {
  const actor = { writes: [], lastWrites: {}, lastWrite: null };
  const result = await run({ do: 'replayConcurrently', actors: ['a', 'b'],
    settleMs: 0 }, services(new Map([['a', actor], ['b', actor]])));
  assert.equal(result.status, 'inconclusive');
  assert.match(result.summary, /fewer than two/);
});

test('lifecycle operations distinguish missing control, unsafe refusal, and success', async () => {
  const missing = await run({ do: 'restartBackend', settleMs: 0 }, services());
  assert.equal(missing.status, 'inconclusive');
  assert.match(missing.summary, /no backend control/);

  const refusedError = Object.assign(new Error('refused'), { status: 3 });
  const refusedCapability = createLifecycleCapability({ restartSpec: { kind: 'test' }, sleep,
    control: async () => { throw refusedError; } });
  const refused = await run({ do: 'restartBackend', settleMs: 0 },
    services(new Map(), { backendLifecycle: refusedCapability }));
  assert.equal(refused.status, 'inconclusive');
  assert.match(refused.summary, /benchmark-owned instance/);

  const calls = [];
  const successful = createLifecycleCapability({ restartSpec: { kind: 'test' }, sleep,
    control: async (spec, mode) => calls.push([spec, mode]) });
  const passed = await run({ do: 'restartBackend', settleMs: 0 },
    services(new Map(), { backendLifecycle: successful }));
  assert.equal(passed.status, 'passed');
  assert.deepEqual(calls, [[{ kind: 'test' }, 'restart']]);
});

test('direct PostgreSQL stock writes quote names and require exactly one updated row', async () => {
  const calls = [];
  const capability = createDatabaseWriteCapability({
    backend: 'postgres',
    dbName: 'bench',
    expand: value => value,
    exec: (command, args) => { calls.push([command, args]); return 'UPDATE 1\n'; },
  });
  const passed = await run({ do: 'dbSetStock', item: "Kid's Keyboard", warehouse: 'Main',
    quantity: 7, settleMs: 0 }, services(new Map(), { databaseWrite: capability }));
  assert.equal(passed.status, 'passed');
  assert.match(calls[0][1].at(-1), /Kid''s Keyboard/);

  const missed = createDatabaseWriteCapability({ backend: 'postgres', dbName: 'bench',
    expand: value => value, exec: () => 'UPDATE 0\n' });
  const failed = await run({ do: 'dbSetStock', item: 'Missing', warehouse: 'Main',
    quantity: 7, settleMs: 0 }, services(new Map(), { databaseWrite: missed }));
  assert.equal(failed.status, 'failed');
  assert.match(failed.summary, /was not updated/);
});

test('client lifecycle delegates through the narrow browser capability', async () => {
  const events = [];
  const actor = { page: { close: async () => events.push('close') } };
  const capabilities = services(new Map([['a', actor]]), { browser: {
    clients: {
      open: async (value, settleMs) => events.push(['open', value === actor, settleMs]),
      fresh: async () => { events.push('fresh'); return 'a-fresh'; },
    },
    sleep,
  } });
  assert.equal((await run({ do: 'closeClient', actor: 'a' }, capabilities)).status, 'passed');
  assert.equal((await run({ do: 'openClient', actor: 'a', settleMs: 9 }, capabilities)).status, 'passed');
  const fresh = await run({ do: 'freshClient', actor: 'a' }, capabilities);
  assert.equal(fresh.status, 'passed');
  assert.deepEqual(events, ['close', ['open', true, 9], 'fresh']);
  assert.deepEqual(fresh.observation, { actor: 'a-fresh' });
});

test('a crashed page during a concurrency barrier remains a harness failure', async () => {
  const actor = { loc: () => ({ waitFor: async () => {
    throw new Error('locator.waitFor: Target page, context or browser has been closed');
  } }) };
  const result = await run({ do: 'clickConcurrently', actors: ['a', 'b'],
    testid: 'buy', settleMs: 0 }, services(new Map([['a', actor], ['b', actor]])));
  assert.equal(result.status, 'harness_failure');
  assert.equal(result.code, 'unclassified_exception');
});
