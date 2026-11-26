// Tests for the Svelte bindings (`spacetimedb/svelte`). Reconnect behaviour of
// `useTable` is covered for every framework in framework_table_reconnect.test.ts;
// this file covers the rest of the Svelte surface.
//
// `svelte` is mocked with a minimal component lifecycle: `setContext` and
// `getContext` share one map per test (one provider and its children), and
// `onDestroy` callbacks registered while `component()` runs its body are
// returned as that component's `destroy`.
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import { get, writable, type Readable } from 'svelte/store';
import {
  createSpacetimeDBProvider,
  useProcedure,
  useReducer,
  useSpacetimeDB,
  useTable,
} from '../src/svelte';
import {
  SPACETIMEDB_CONTEXT_KEY,
  type ConnectionState,
} from '../src/svelte/connection_state';
import { ConnectionManager } from '../src/sdk/connection_manager';
import { ConnectionId } from '../src';
import { ServerMessage } from '../src/sdk/client_api/types';
import { WebsocketTestAdapterFactory } from '../src/sdk/websocket_test_adapter';
import { DbConnection, tables } from '../test-app/src/module_bindings';
import type { UntypedProcedureDef } from '../src/sdk/procedures';
import type { UntypedReducerDef } from '../src/sdk/reducers';
import {
  anIdentity,
  bobIdentity,
  encodeUser,
  makeQueryRows,
  makeQuerySetUpdate,
  sallyIdentity,
} from './utils';

const lifecycle = vi.hoisted(() => ({
  contexts: new Map<unknown, unknown>(),
  destroyers: [] as (() => void)[],
}));
vi.mock('svelte', () => ({
  setContext: (key: unknown, value: unknown) => {
    lifecycle.contexts.set(key, value);
    return value;
  },
  getContext: (key: unknown) => lifecycle.contexts.get(key),
  onDestroy: (fn: () => void) => {
    lifecycle.destroyers.push(fn);
  },
}));

/** Run `body` as a component's initialisation; returns its result and a destroy function. */
function component<T>(body: () => T): { value: T; destroy: () => void } {
  lifecycle.destroyers = [];
  const value = body();
  const destroyers = lifecycle.destroyers;
  lifecycle.destroyers = [];
  return { value, destroy: () => destroyers.forEach(fn => fn()) };
}

/** Let queued promise callbacks run. */
async function settle() {
  for (let i = 0; i < 5; i++) await Promise.resolve();
}

let factory: WebsocketTestAdapterFactory;
let databaseCounter = 0;

function builder() {
  return DbConnection.builder()
    .withUri('ws://localhost:1234')
    .withDatabaseName(`svelte-bindings-${databaseCounter}`)
    .withWSFn(factory.openWebSocket);
}

/** Complete the handshake for the connection currently opening. */
async function establish(connection: DbConnection) {
  await connection['wsPromise'];
  factory.current.acceptConnection();
  factory.current.sendToClient(
    ServerMessage.InitialConnection({
      identity: anIdentity,
      connectionId: ConnectionId.random(),
      token: 'token',
    })
  );
  await settle();
}

beforeEach(() => {
  vi.useFakeTimers();
  lifecycle.contexts.clear();
  lifecycle.destroyers = [];
  factory = new WebsocketTestAdapterFactory();
  databaseCounter += 1;
});
afterEach(() => {
  vi.useRealTimers();
});

describe('useSpacetimeDB', () => {
  test('throws a helpful error outside a provider', () => {
    expect(() => useSpacetimeDB()).toThrow(/createSpacetimeDBProvider/);
  });

  test('useTable also explains a missing provider', () => {
    expect(() => useTable(tables.user)).toThrow(
      /Did you forget to call `createSpacetimeDBProvider`/
    );
  });
});

describe('createSpacetimeDBProvider', () => {
  test('shares a read-only connection store with its children', () => {
    const provider = component(() => createSpacetimeDBProvider(builder()));
    try {
      const store = provider.value;
      expect(useSpacetimeDB()).toBe(store);
      expect('set' in store).toBe(false);
      expect('update' in store).toBe(false);
      expect(get(store).isActive).toBe(false);
    } finally {
      provider.destroy();
    }
  });

  test('reflects the connection becoming active', async () => {
    const provider = component(() => createSpacetimeDBProvider(builder()));
    try {
      const store = provider.value;
      const connection = get(store).getConnection() as DbConnection;
      expect(connection).toBeInstanceOf(DbConnection);

      await establish(connection);

      const state = get(store);
      expect(state.isActive).toBe(true);
      expect(state.identity?.isEqual(anIdentity)).toBe(true);
      expect(state.token).toBe('token');
    } finally {
      provider.destroy();
    }
  });

  test('reconnect(builder) replaces the connection', async () => {
    const provider = component(() => createSpacetimeDBProvider(builder()));
    try {
      const store = provider.value;
      const first = get(store).getConnection() as DbConnection;
      await establish(first);

      get(store).reconnect(builder().withToken('new-token'));

      const second = get(store).getConnection() as DbConnection;
      expect(second).toBeInstanceOf(DbConnection);
      expect(second).not.toBe(first);
      expect(first.isActive).toBe(false);
    } finally {
      provider.destroy();
    }
  });

  test('releases the connection when destroyed', async () => {
    const b = builder();
    const key = ConnectionManager.getKey(b.getUri(), b.getModuleName());
    const provider = component(() => createSpacetimeDBProvider(b));
    const connection = get(provider.value).getConnection() as DbConnection;
    await establish(connection);

    provider.destroy();
    // Release is deferred so a quick remount (HMR, {#key}) can reuse it.
    expect(ConnectionManager.getConnection(key)).toBe(connection);
    await vi.runOnlyPendingTimersAsync();

    expect(ConnectionManager.getConnection(key)).toBeNull();
    expect(connection.isActive).toBe(false);
  });
});

/** A connection store driven by hand, for testing the call hooks in isolation. */
function fakeConnectionStore() {
  const calls: [string, unknown][] = [];
  const connection = {
    reducers: {
      createPlayer: (args: unknown) => {
        calls.push(['createPlayer', args]);
        return Promise.resolve();
      },
      failingReducer: () => Promise.reject(new Error('reducer failed')),
    },
    procedures: {
      getScore: (args: { name: string }) => {
        calls.push(['getScore', args]);
        return Promise.resolve(`score for ${args.name}`);
      },
      failingProcedure: () => Promise.reject(new Error('procedure failed')),
    },
  };
  const state = (live: boolean): ConnectionState => ({
    isActive: live,
    identity: undefined,
    token: undefined,
    connectionId: ConnectionId.random(),
    connectionError: undefined,
    getConnection: () => (live ? (connection as never) : null),
    reconnect: () => {},
  });
  const store = writable(state(false));
  lifecycle.contexts.set(
    SPACETIMEDB_CONTEXT_KEY,
    store satisfies Readable<ConnectionState>
  );
  return {
    calls,
    connect: () => store.set(state(true)),
    disconnect: () => store.set(state(false)),
  };
}

const createPlayer = { accessorName: 'createPlayer' } as UntypedReducerDef;
const failingReducer = { accessorName: 'failingReducer' } as UntypedReducerDef;
const getScore = { accessorName: 'getScore' } as UntypedProcedureDef;
const failingProcedure = {
  accessorName: 'failingProcedure',
} as UntypedProcedureDef;

describe('useReducer', () => {
  test('queues calls made before connecting and sends them once connected', async () => {
    const conn = fakeConnectionStore();
    const hook = component(() => useReducer(createPlayer));
    const call = hook.value as (args: unknown) => Promise<void>;

    const pending = call({ name: 'Alice' });
    await settle();
    expect(conn.calls).toEqual([]);

    conn.connect();
    await expect(pending).resolves.toBeUndefined();
    expect(conn.calls).toEqual([['createPlayer', { name: 'Alice' }]]);
    hook.destroy();
  });

  test('calls straight through when connected', async () => {
    const conn = fakeConnectionStore();
    conn.connect();
    const hook = component(() => useReducer(createPlayer));
    const call = hook.value as (args: unknown) => Promise<void>;

    await call({ name: 'Bob' });
    expect(conn.calls).toEqual([['createPlayer', { name: 'Bob' }]]);
    hook.destroy();
  });

  test('rejects a queued call when the reducer fails', async () => {
    const conn = fakeConnectionStore();
    const hook = component(() => useReducer(failingReducer));
    const pending = (hook.value as () => Promise<void>)();
    conn.connect();
    await expect(pending).rejects.toThrow('reducer failed');
    hook.destroy();
  });

  test('stops sending queued calls once destroyed', async () => {
    const conn = fakeConnectionStore();
    const hook = component(() => useReducer(createPlayer));
    void (hook.value as (args: unknown) => Promise<void>)({ name: 'Ghost' });

    hook.destroy();
    conn.connect();
    await settle();
    expect(conn.calls).toEqual([]);
  });
});

describe('useProcedure', () => {
  test('queues calls made before connecting and resolves with the return value', async () => {
    const conn = fakeConnectionStore();
    const hook = component(() => useProcedure(getScore));
    const call = hook.value as (args: { name: string }) => Promise<string>;

    const pending = call({ name: 'Alice' });
    await settle();
    expect(conn.calls).toEqual([]);

    conn.connect();
    await expect(pending).resolves.toBe('score for Alice');
    hook.destroy();
  });

  test('calls straight through when connected', async () => {
    const conn = fakeConnectionStore();
    conn.connect();
    const hook = component(() => useProcedure(getScore));
    const call = hook.value as (args: { name: string }) => Promise<string>;

    await expect(call({ name: 'Bob' })).resolves.toBe('score for Bob');
    hook.destroy();
  });

  test('rejects a queued call when the procedure fails', async () => {
    const conn = fakeConnectionStore();
    const hook = component(() => useProcedure(failingProcedure));
    const pending = (hook.value as () => Promise<unknown>)();
    conn.connect();
    await expect(pending).rejects.toThrow('procedure failed');
    hook.destroy();
  });

  test('stops sending queued calls once destroyed', async () => {
    const conn = fakeConnectionStore();
    const hook = component(() => useProcedure(getScore));
    void (hook.value as (args: { name: string }) => Promise<string>)({
      name: 'Ghost',
    });

    hook.destroy();
    conn.connect();
    await settle();
    expect(conn.calls).toEqual([]);
  });
});

describe('useTable', () => {
  const alice = { identity: anIdentity, username: 'Alice' };
  const bob = { identity: bobIdentity, username: 'Bob' };
  const sally = { identity: sallyIdentity, username: 'Sally' };

  /** Mount a provider with a live connection, then a component calling useTable. */
  async function mountTable(
    query: Parameters<typeof useTable>[0],
    initialRows: (typeof alice)[]
  ) {
    const events: string[] = [];
    const provider = component(() => createSpacetimeDBProvider(builder()));
    const connection = get(provider.value).getConnection() as DbConnection;
    await establish(connection);

    const hook = component(() =>
      useTable(query, {
        onInsert: row => events.push(`insert ${row.username}`),
        onDelete: row => events.push(`delete ${row.username}`),
        onUpdate: (oldRow, newRow) =>
          events.push(`update ${oldRow.username} -> ${newRow.username}`),
      })
    );
    const [rows, ready] = hook.value;
    await settle();

    const subscribe = factory.current.outgoingMessages.at(-1);
    if (subscribe?.tag !== 'Subscribe') {
      throw new Error('Expected useTable to subscribe');
    }
    const querySetId = subscribe.value.querySetId.id;
    factory.current.sendToClient(
      ServerMessage.SubscribeApplied({
        requestId: subscribe.value.requestId,
        querySetId: subscribe.value.querySetId,
        rows: makeQueryRows('user', initialRows.map(encodeUser)),
      })
    );
    await settle();

    const emissions: string[][] = [];
    const stopWatching = rows.subscribe(value =>
      emissions.push(value.map(row => row.username as string))
    );
    emissions.length = 0; // drop the immediate emission from subscribing

    const transaction = async (
      inserts: (typeof alice)[],
      deletes: (typeof alice)[] = []
    ) => {
      factory.current.sendToClient(
        ServerMessage.TransactionUpdate({
          querySets: [
            makeQuerySetUpdate(
              querySetId,
              'user',
              inserts.map(encodeUser),
              deletes.map(encodeUser)
            ),
          ],
        })
      );
      await settle();
    };
    const names = () => get(rows).map(row => row.username);
    const destroy = () => {
      stopWatching();
      hook.destroy();
      provider.destroy();
    };
    return { rows, ready, names, events, emissions, transaction, destroy };
  }

  test('becomes ready with the subscribed rows', async () => {
    const t = await mountTable(tables.user, [alice, bob]);
    try {
      expect(get(t.ready)).toBe(true);
      expect(t.names()).toEqual(['Alice', 'Bob']);
    } finally {
      t.destroy();
    }
  });

  test('updates rows once per transaction, firing callbacks per row', async () => {
    const t = await mountTable(tables.user, [alice]);
    try {
      t.events.length = 0;
      await t.transaction([bob, sally]);

      expect(t.names()).toEqual(['Alice', 'Bob', 'Sally']);
      expect(t.events).toEqual(['insert Bob', 'insert Sally']);
      expect(t.emissions).toEqual([['Alice', 'Bob', 'Sally']]);
    } finally {
      t.destroy();
    }
  });

  test('filters rows and callbacks by the where clause', async () => {
    const t = await mountTable(
      tables.user.where(row => row.username.eq('Alice')),
      [alice, bob]
    );
    try {
      expect(t.names()).toEqual(['Alice']);
      t.events.length = 0;

      // Bob is renamed to Alice: he enters the filtered set, so onInsert.
      await t.transaction([{ ...bob, username: 'Alice' }], [bob]);
      expect(t.events).toEqual(['insert Alice']);
      expect(t.names()).toEqual(['Alice', 'Alice']);

      // Alice is renamed: she leaves the filtered set, so onDelete.
      t.events.length = 0;
      await t.transaction([{ ...alice, username: 'Alicia' }], [alice]);
      expect(t.events).toEqual(['delete Alice']);
      expect(t.names()).toEqual(['Alice']);

      // A change entirely outside the filter fires nothing.
      t.events.length = 0;
      t.emissions.length = 0;
      await t.transaction([sally]);
      expect(t.events).toEqual([]);
      expect(t.emissions).toEqual([]);
    } finally {
      t.destroy();
    }
  });

  test('reports an in-filter change as an update', async () => {
    const t = await mountTable(tables.user, [alice]);
    try {
      t.events.length = 0;
      await t.transaction([{ ...alice, username: 'Alicia' }], [alice]);
      expect(t.events).toEqual(['update Alice -> Alicia']);
      expect(t.names()).toEqual(['Alicia']);
    } finally {
      t.destroy();
    }
  });

  test('unsubscribes and stops listening when destroyed', async () => {
    const t = await mountTable(tables.user, [alice]);
    t.destroy();
    await settle();
    expect(factory.current.outgoingMessages.at(-1)?.tag).toBe('Unsubscribe');

    // Later transactions no longer reach the destroyed hook.
    t.events.length = 0;
    t.emissions.length = 0;
    await t.transaction([bob]);
    expect(t.events).toEqual([]);
    expect(t.emissions).toEqual([]);
  });
});
