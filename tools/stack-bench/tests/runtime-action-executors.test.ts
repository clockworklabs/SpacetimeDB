import assert from 'node:assert/strict';
import test from 'node:test';

import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';
import {
  createDatabaseWriteCapability,
  createLifecycleCapability,
  databaseWriteFailureDetail,
  RUNTIME_ACTION_IMPLEMENTATIONS,
} from '../src/actions/runtime-action-executors.js';

type UnknownRecord = Record<string, unknown>;
type Event = string | readonly [string, boolean, number];

interface ServiceOverrides {
  readonly applicationLifecycle?: unknown;
  readonly backendLifecycle?: unknown;
  readonly browser?: unknown;
  readonly clock?: unknown;
  readonly concurrency?: unknown;
  readonly databaseWrite?: unknown;
}

const sleep = async (_milliseconds?: number, _signal?: AbortSignal): Promise<void> => {};
const restartSpec = { backend: 'stub', app: '.', port: 7000, probe: '' };

function services(
  actors: ReadonlyMap<string, unknown> = new Map(),
  overrides: ServiceOverrides = {},
): Record<string, unknown> {
  return {
    actors: { get: (name: string) => actors.get(name) },
    'application-lifecycle': overrides.applicationLifecycle
      ?? createLifecycleCapability({ target: 'app-server', sleep, control: async () => {} }),
    'backend-lifecycle': overrides.backendLifecycle
      ?? createLifecycleCapability({ target: 'backend-runtime', sleep, control: async () => {} }),
    'browser-interaction': overrides.browser ?? {
      clients: { open: async () => {}, fresh: async () => 'a-fresh' },
      sleep,
    },
    clock: overrides.clock ?? { sleep },
    concurrency: overrides.concurrency ?? {
      defaultWithin: 5000,
      dispatch: async () => null,
      expand: (value: string | undefined) => value,
      sleep,
      testId: (id: string) => `[data-testid="${id}"]`,
    },
    'database-write': overrides.databaseWrite ?? { setStock: async (input: unknown) => input },
  };
}

async function run(input: UnknownRecord, capabilities: Record<string, unknown>) {
  const action = String(input.do);
  return executeAction(ACTION_REGISTRY, action, input, {
    capabilities,
  });
}

function observation(result: { readonly observation: unknown }): UnknownRecord {
  assert(result.observation !== null && typeof result.observation === 'object');
  return result.observation as UnknownRecord;
}

test('the runtime executor registry contains only registered actions', () => {
  for (const id of Object.keys(RUNTIME_ACTION_IMPLEMENTATIONS)) {
    assert(ACTION_REGISTRY.get(id).timeoutMs > 0, id);
  }
});

test('race preserves branch ordering while overlapping branches through registered dispatch', async () => {
  const events: string[] = [];
  const capability = services(new Map(), { concurrency: {
    defaultWithin: 5000,
    expand: (value: string | undefined) => value,
    sleep,
    testId: (id: string) => id,
    dispatch: async (step: UnknownRecord) => {
      events.push(`start-${String(step.actor)}-${String(step.ms)}`);
      if (step.ms === 1) await new Promise(resolve => setImmediate(resolve));
      events.push(`end-${String(step.actor)}-${String(step.ms)}`);
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
    defaultWithin: 5000, expand: (value: string | undefined) => value, sleep,
    testId: (id: string) => id,
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
  assert.match(result.summary ?? '', /fewer than two/);
});

test('lifecycle operations distinguish missing control, unsafe refusal, and success', async () => {
  const missing = await run({ do: 'restartBackend', settleMs: 0 }, services());
  assert.equal(missing.status, 'inconclusive');
  assert.match(missing.summary ?? '', /no backend control/);

  const refusedError = Object.assign(new Error('refused'), { status: 3 });
  const refusedCapability = createLifecycleCapability({ restartSpec,
    target: 'backend-runtime', sleep,
    control: async () => { throw refusedError; } });
  const refused = await run({ do: 'restartBackend', settleMs: 0 },
    services(new Map(), { backendLifecycle: refusedCapability }));
  assert.equal(refused.status, 'inconclusive');
  assert.match(refused.summary ?? '', /benchmark-owned instance/);

  const calls: Array<readonly [unknown, string]> = [];
  const successful = createLifecycleCapability({ restartSpec,
    target: 'backend-runtime', sleep,
    control: async (spec, mode) => { calls.push([spec, mode]); } });
  const passed = await run({ do: 'restartBackend', settleMs: 0 },
    services(new Map(), { backendLifecycle: successful }));
  assert.equal(passed.status, 'passed');
  assert.deepEqual(calls, [[restartSpec, 'restart']]);
});

test('a generated app server timing out is an application failure, not a harness failure', async () => {
  const timedOut = Object.assign(new Error('app start timed out'), { code: 'ETIMEDOUT' });
  const applicationLifecycle = createLifecycleCapability({ restartSpec,
    target: 'app-server', sleep, control: async () => { throw timedOut; } });
  const appResult = await run({ do: 'startAppServer', settleMs: 0 },
    services(new Map(), { applicationLifecycle }));
  assert.equal(appResult.status, 'failed');
  assert.match(appResult.summary ?? '', /could not start the app server/);

  const backendLifecycle = createLifecycleCapability({ restartSpec,
    target: 'backend-runtime', sleep,
    control: async () => { throw timedOut; } });
  const backendResult = await run({ do: 'restartBackend', settleMs: 0 },
    services(new Map(), { backendLifecycle }));
  assert.equal(backendResult.status, 'harness_failure');
});

test('direct PostgreSQL stock writes quote names and require exactly one updated row', async () => {
  const calls: Array<readonly [string, readonly string[]]> = [];
  const waits: number[] = [];
  const databaseLease = { resources: { database: 'bench',
    container: { name: 'leased-postgres', id: 'postgres-id' } } };
  const capability = createDatabaseWriteCapability({
    backend: 'postgres',
    databaseLease,
    expand: value => value,
    exec: (command, args) => {
      calls.push([command, args]);
      return args[0] === 'inspect' ? 'postgres-id\n' : 'UPDATE 1\n';
    },
  });
  const passed = await run({ do: 'dbSetStock', item: "Kid's Keyboard", warehouse: 'Main',
    quantity: 7, settleMs: 17 }, {
    ...services(new Map(), { databaseWrite: capability, clock: {
      sleep: async (ms: number) => { waits.push(ms); },
    } }),
  });
  assert.equal(passed.status, 'passed');
  assert.match(calls.find(([, args]) => args.includes('psql'))?.[1].at(-1) ?? '', /Kid''s Keyboard/);
  assert.deepEqual(waits, [17]);

  const missed = createDatabaseWriteCapability({ backend: 'postgres', databaseLease,
    expand: value => value, exec: (_command, args) => args[0] === 'inspect'
      ? 'postgres-id\n' : 'UPDATE 0\n' });
  const failed = await run({ do: 'dbSetStock', item: 'Missing', warehouse: 'Main',
    quantity: 7, settleMs: 0 }, services(new Map(), { databaseWrite: missed }));
  assert.equal(failed.status, 'failed');
  assert.match(failed.summary ?? '', /was not updated/);
});

test('database-write feedback preserves the actionable schema error ahead of terse process output', async () => {
  const error = Object.assign(new Error('direct stock correction requires singular collections '
    + '`item`, `warehouse`, and `stock`'), { stdout: 'MISSING\n' });
  assert.match(databaseWriteFailureDetail(error), /singular collections/);
  assert.match(databaseWriteFailureDetail(error), /MISSING/);

  const capability = createDatabaseWriteCapability({ backend: 'mongodb',
    databaseLease: { resources: { database: 'bench',
      container: { name: 'leased-mongodb', id: 'mongodb-id' } } },
    expand: value => value, exec: (_command, args) => {
      if (args[0] === 'inspect') return 'mongodb-id\n';
      throw error;
    } });
  const result = await run({ do: 'dbSetStock', item: 'Desk Lamp', warehouse: 'East',
    quantity: 5, settleMs: 0 }, services(new Map(), { databaseWrite: capability }));
  assert.equal(result.status, 'failed');
  assert.match(result.summary ?? '', /singular collections `item`, `warehouse`, and `stock`/);
  assert.match(result.summary ?? '', /MISSING/);
});

test('direct database writes target the container selected by the run lease', async () => {
  const calls: Array<readonly [string, readonly string[]]> = [];
  const capability = createDatabaseWriteCapability({
    backend: 'mongodb',
    databaseLease: { resources: { database: 'bench',
      container: { name: 'leased-mongodb', id: 'mongodb-id' } } },
    expand: value => value,
    exec: (command, args) => {
      calls.push([command, args]);
      return args[0] === 'inspect' ? 'mongodb-id\n' : 'OK\n';
    },
  });
  const result = await run({ do: 'dbSetStock', item: 'Desk Lamp', warehouse: 'East',
    quantity: 5, settleMs: 0 }, services(new Map(), { databaseWrite: capability }));
  assert.equal(result.status, 'passed');
  assert.deepEqual(calls.find(([, args]) => args.includes('mongosh'))?.[1].slice(0, 2),
    ['exec', 'mongodb-id']);
});

test('direct database writes fail as harness errors without lease authority', async () => {
  const capability = createDatabaseWriteCapability({
    backend: 'postgres', expand: value => value, exec: () => 'UPDATE 1\n',
  });
  const result = await run({ do: 'dbSetStock', item: 'Desk Lamp', warehouse: 'East',
    quantity: 5, settleMs: 0 }, services(new Map(), { databaseWrite: capability }));
  assert.equal(result.status, 'harness_failure');
  assert.match(result.summary ?? '', /authenticated backend lease/);
});

test('the null control can skip direct database writes', async () => {
  const capability = createDatabaseWriteCapability({
    backend: 'postgres', skip: true, expand: value => value,
    exec: () => { throw new Error('must not execute'); },
  });
  const result = await run({ do: 'dbSetStock', item: 'Desk Lamp', warehouse: 'East',
    quantity: 5, settleMs: 0 }, services(new Map(), { databaseWrite: capability }));
  assert.equal(result.status, 'passed');
});

test('offline lifecycle preserves settling time and verifies browser network state', async () => {
  const offlineStates: boolean[] = [];
  const waits: number[] = [];
  let browserOnline = true;
  const actor = { page: {
    evaluate: async () => browserOnline,
    context: () => ({
    setOffline: async (value: boolean) => {
      offlineStates.push(value);
      browserOnline = !value;
    },
  }) } };
  const capabilities = services(new Map([['a', actor]]), { browser: {
    clients: { open: async () => {}, fresh: async () => 'a-fresh' },
    sleep: async (ms: number) => { waits.push(ms); },
  } });
  const disconnected = await run({ do: 'setOffline', actor: 'a', offline: true, settleMs: 10 },
    capabilities);
  const reconnected = await run({ do: 'setOffline', actor: 'a', offline: false, settleMs: 20 },
    capabilities);
  assert.equal(disconnected.status, 'passed');
  assert.equal(observation(disconnected).browserOnline, false);
  assert.equal(reconnected.status, 'passed');
  assert.equal(observation(reconnected).browserOnline, true);
  assert.deepEqual(offlineStates, [true, false]);
  assert.deepEqual(waits, [10, 20]);
});

test('offline lifecycle fails closed when browser network state does not change', async () => {
  const actor = { page: { evaluate: async () => true,
    context: () => ({ setOffline: async () => {} }) } };
  const result = await run({ do: 'setOffline', actor: 'a', offline: true, settleMs: 1 },
    services(new Map([['a', actor]])));
  assert.equal(result.status, 'harness_failure');
  assert.match(result.summary ?? '', /navigator\.onLine remained true/);
});

test('client lifecycle delegates through the narrow browser capability', async () => {
  const events: Event[] = [];
  const actor = { page: { close: async () => events.push('close') } };
  const capabilities = services(new Map([['a', actor]]), { browser: {
    clients: {
      open: async (value: unknown, settleMs: number) => {
        events.push(['open', value === actor, settleMs]);
      },
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
