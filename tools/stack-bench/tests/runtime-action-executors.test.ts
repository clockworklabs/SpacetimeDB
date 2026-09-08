import assert from 'node:assert/strict';
import test from 'node:test';
import { runInNewContext } from 'node:vm';

import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';
import { describesMissingStockInterface } from '../src/stacks/stock-interface.js';
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
  assert.match(result.summary ?? '', /never contended/);
});

test('lifecycle operations distinguish missing control, unsafe refusal, and success', async () => {
  const missing = await run({ do: 'restartBackend', settleMs: 0 }, services());
  assert.equal(missing.status, 'inconclusive');
  assert.match(missing.summary ?? '', /no control over the database runtime/);

  const refusedError = Object.assign(new Error('refused'), { status: 3 });
  const refusedCapability = createLifecycleCapability({ restartSpec,
    target: 'backend-runtime', sleep,
    control: async () => { throw refusedError; } });
  const refused = await run({ do: 'restartBackend', settleMs: 0 },
    services(new Map(), { backendLifecycle: refusedCapability }));
  assert.equal(refused.status, 'inconclusive');
  assert.match(refused.summary ?? '', /was refused on this host/);

  const calls: Array<readonly [unknown, string]> = [];
  const operated: Array<'restart' | 'start' | 'stop'> = [];
  const successful = createLifecycleCapability({ restartSpec,
    target: 'backend-runtime', sleep,
    control: async (spec, mode) => { calls.push([spec, mode]); },
    onOperated: mode => operated.push(mode) });
  const passed = await run({ do: 'restartBackend', settleMs: 0 },
    services(new Map(), { backendLifecycle: successful }));
  assert.equal(passed.status, 'passed');
  assert.deepEqual(calls, [[restartSpec, 'restart']]);
  assert.deepEqual(operated, ['restart']);
});

test('a lifecycle operation reports the state it left only when the control completed', async () => {
  const operated: Array<'restart' | 'start' | 'stop'> = [];
  const failing = createLifecycleCapability({ restartSpec, target: 'app-server', sleep,
    control: async () => { throw Object.assign(new Error('exit 1'), { status: 1, stdout: 'port busy' }); },
    onOperated: mode => operated.push(mode) });
  const failed = await run({ do: 'stopAppServer', settleMs: 0 },
    services(new Map(), { applicationLifecycle: failing }));
  assert.equal(failed.status, 'failed');
  assert.equal(operated.length, 0);

  const working = createLifecycleCapability({ restartSpec, target: 'app-server', sleep,
    control: async () => {}, onOperated: mode => operated.push(mode) });
  const capabilities = services(new Map(), { applicationLifecycle: working });
  assert.equal((await run({ do: 'stopAppServer', settleMs: 0 }, capabilities)).status, 'passed');
  assert.equal((await run({ do: 'startAppServer', settleMs: 0 }, capabilities)).status, 'passed');
  assert.deepEqual(operated, ['stop', 'start']);
});

test('a generated app server timing out is an application failure, not a harness failure', async () => {
  const timedOut = Object.assign(new Error('app start timed out'), { code: 'ETIMEDOUT' });
  const applicationLifecycle = createLifecycleCapability({ restartSpec,
    target: 'app-server', sleep, control: async () => { throw timedOut; } });
  const appResult = await run({ do: 'startAppServer', settleMs: 0 },
    services(new Map(), { applicationLifecycle }));
  assert.equal(appResult.status, 'failed');
  assert.match(appResult.summary ?? '', /application server could not start/);

  const backendLifecycle = createLifecycleCapability({ restartSpec,
    target: 'backend-runtime', sleep,
    control: async () => { throw timedOut; } });
  const backendResult = await run({ do: 'restartBackend', settleMs: 0 },
    services(new Map(), { backendLifecycle }));
  assert.equal(backendResult.status, 'harness_failure');
});

test('direct PostgreSQL stock writes quote names and require exactly one updated row', async () => {
  const waits: number[] = [];
  let stockSql = '';
  const databaseLease = { resources: { database: 'bench',
    container: { name: 'leased-postgres', id: 'postgres-id' } } };
  const capability = createDatabaseWriteCapability({
    backend: 'postgres',
    databaseLease,
    expand: value => value,
    exec: (_command, args, options) => {
      if (args[0] === 'inspect') return 'postgres-id\n';
      stockSql = options.input ?? '';
      return 'UPDATE 1\n';
    },
  });
  const passed = await run({ do: 'dbSetStock', item: "Kid's Keyboard", warehouse: 'Main',
    quantity: 7, settleMs: 17 }, {
    ...services(new Map(), { databaseWrite: capability, clock: {
      sleep: async (ms: number) => { waits.push(ms); },
    } }),
  });
  assert.equal(passed.status, 'passed');
  assert.match(stockSql, /Kid''s Keyboard/);
  assert.match(stockSql, /\\gexec/);
  assert.deepEqual(waits, [17]);

  const missed = createDatabaseWriteCapability({ backend: 'postgres', databaseLease,
    expand: value => value, exec: (_command, args) => {
      if (args[0] === 'inspect') return 'postgres-id\n';
      return 'UPDATE 0\n';
    } });
  const failed = await run({ do: 'dbSetStock', item: 'Missing', warehouse: 'Main',
    quantity: 7, settleMs: 0 }, services(new Map(), { databaseWrite: missed }));
  assert.equal(failed.status, 'failed');
  assert.equal(failed.finding?.kind, 'stock-interface-missing');
  assert.match(String(failed.finding?.fields.detail), /could not locate one relational stock row/);
  assert.equal(failed.finding?.fields.missingRow, 'stock');
});

test('a database without the stock interface is an application failure that keeps its diagnostic', async () => {
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
  // The contract names the stock tables; an app without them has failed the
  // interface, and the writer's diagnostic travels as detail.
  assert.equal(result.status, 'failed');
  assert.equal(result.finding?.kind, 'stock-interface-missing');
  assert.match(String(result.finding?.fields.detail), /singular collections/);
  assert.match(String(result.finding?.fields.detail), /MISSING/);
  for (const row of ['ITEM', 'WAREHOUSE'] as const) {
    const missing = createDatabaseWriteCapability({ backend: 'mongodb',
      databaseLease: { resources: { database: 'bench', container: { name: 'leased-mongodb', id: 'mongodb-id' } } },
      expand: value => value, exec: (_command, args) => {
        if (args[0] === 'inspect') return 'mongodb-id\n';
        throw Object.assign(new Error('missing row'), { stdout: `MISSING_${row}\n` });
      } });
    const result = await run({ do: 'dbSetStock', item: 'Desk Lamp', warehouse: 'East', quantity: 5, settleMs: 0 },
      services(new Map(), { databaseWrite: missing }));
    assert.equal(result.finding?.kind, 'stock-interface-missing');
    assert.equal(result.finding?.fields.missingRow, row.toLowerCase());
  }

});

test('direct MongoDB writes use public stock IDs and the container selected by the run lease', async () => {
  const calls: Array<readonly [string, readonly string[]]> = [];
  const stock = { item_id: 0, warehouse_id: 7, quantity: 1 };
  const capability = createDatabaseWriteCapability({
    backend: 'mongodb',
    databaseLease: { resources: { database: 'bench',
      container: { name: 'leased-mongodb', id: 'mongodb-id' } } },
    expand: value => value,
    exec: (command, args) => {
      calls.push([command, args]);
      if (args[0] === 'inspect') return 'mongodb-id\n';
      let output = '';
      runInNewContext(args.at(-1)!, {
        db: {
          getCollectionNames: () => ['item', 'warehouse', 'stock'],
          item: { findOne: () => ({ id: stock.item_id, _id: 'generated-item-object-id' }) },
          warehouse: { findOne: () => ({ id: stock.warehouse_id, _id: 'generated-warehouse-object-id' }) },
          stock: { updateOne: (query: { $or: Array<{ item_id?: number; warehouse_id?: number }> },
            update: { $set: { quantity: number } }) => {
            const matched = query.$or.some(row => row.item_id === stock.item_id
              && row.warehouse_id === stock.warehouse_id);
            if (matched) stock.quantity = update.$set.quantity;
            return { matchedCount: matched ? 1 : 0 };
          } },
        },
        print: (value: string) => { output += value + '\n'; },
      });
      return output;
    },
  });
  const result = await run({ do: 'dbSetStock', item: 'Desk Lamp', warehouse: 'East',
    quantity: 5, settleMs: 0 }, services(new Map(), { databaseWrite: capability }));
  assert.equal(result.status, 'passed');
  assert.equal(stock.quantity, 5);
  assert.deepEqual(calls.find(([, args]) => args.includes('mongosh'))?.[1].slice(0, 2),
    ['exec', 'mongodb-id']);
});

test('MongoDB auth and command failures cannot be scored as missing stock data', async () => {
  const databaseLease = { resources: { database: 'bench',
    container: { name: 'leased-mongodb', id: 'mongodb-id' } } };
  for (const error of [
    Object.assign(new Error('mongosh --password SENTINEL_PRIVATE_PASSWORD exited 1'),
      { stderr: 'MongoServerError: Command find requires authentication' }),
    Object.assign(new Error('mongosh exited 1'),
      { stderr: 'MongoServerError: Authentication failed' }),
    Object.assign(new Error('docker exec exited 126'),
      { stderr: 'OCI runtime exec failed: executable file not found in $PATH' }),
    Object.assign(new Error('docker timed out'), { code: 'ETIMEDOUT' }),
  ]) {
    const capability = createDatabaseWriteCapability({ backend: 'mongodb', databaseLease,
      expand: value => value, exec: (_command, args) => {
        if (args[0] === 'inspect') return 'mongodb-id\n';
        throw error;
      } });
    const result = await run({ do: 'dbSetStock', item: 'Desk Lamp', warehouse: 'East',
      quantity: 5, settleMs: 0 }, services(new Map(), { databaseWrite: capability }));
    assert.equal(result.status, 'harness_failure', error.message);
    assert.equal(result.finding, null);
    assert.match(result.summary ?? '', /authentication|Authentication|executable file|timed out/);
    assert.doesNotMatch(JSON.stringify(result), /SENTINEL_PRIVATE_PASSWORD/);
  }
  for (const [output, status] of [['NOMATCH\n', 'failed'], ['unexpected output\n', 'harness_failure']]) {
    const capability = createDatabaseWriteCapability({ backend: 'mongodb', databaseLease,
      expand: value => value, exec: (_command, args) => args[0] === 'inspect' ? 'mongodb-id\n' : output! });
    const result = await run({ do: 'dbSetStock', item: 'Desk Lamp', warehouse: 'East',
      quantity: 5, settleMs: 0 }, services(new Map(), { databaseWrite: capability }));
    assert.equal(result.status, status);
  }
});

test('stock-interface detection requires database evidence, not a missing executable', () => {
  for (const message of ['OCI runtime exec failed: executable file not found in $PATH',
    'FATAL: role "appuser" does not exist', 'FATAL: database "bench" does not exist']) {
    assert.equal(describesMissingStockInterface(message), false, message);
  }
  for (const message of ['Table stock not found', 'relation "stock" does not exist',
    'no such column: quantity', 'field item_id not found']) {
    assert.equal(describesMissingStockInterface(message), true, message);
  }
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
