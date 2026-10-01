import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import {
  createRenderer,
  defineComponent,
  h,
  isProxy,
  isReactive,
  nextTick,
  watch,
  type App,
} from 'vue';
import { t } from '../src/lib/type_builders';
import { ConnectionId } from '../src/lib/connection_id';
import {
  ConnectionManager,
  connectionManagerReconnectDelayMs,
} from '../src/sdk/connection_manager';
import { ServerMessage } from '../src/sdk/client_api/types';
import WebsocketTestAdapter from '../src/sdk/websocket_test_adapter';
import {
  SpacetimeDBProvider,
  useSpacetimeDB,
  useSpacetimeDBProvider,
  useTable,
  useReducer,
  useProcedure,
} from '../src/vue';
import type { ConnectionState } from '../src/vue/connection_state';
import {
  DbConnection,
  tables,
  reducers,
} from '../test-app/src/module_bindings';
import { anIdentity, bobIdentity, encodeUser, makeQueryRows } from './utils';

// Exercise real Vue component lifecycles in Node without a DOM dependency.
interface HostNode {
  parent: HostNode | null;
  children: HostNode[];
}
const node = (): HostNode => ({ parent: null, children: [] });
const renderer = createRenderer<HostNode, HostNode>({
  createElement: node,
  createText: node,
  createComment: node,
  setText() {},
  setElementText() {},
  patchProp() {},
  insert(child, parent, anchor) {
    if (child.parent) {
      child.parent.children.splice(child.parent.children.indexOf(child), 1);
    }
    child.parent = parent;
    const index = anchor ? parent.children.indexOf(anchor) : -1;
    if (index < 0) parent.children.push(child);
    else parent.children.splice(index, 0, child);
  },
  remove(child) {
    child.parent?.children.splice(child.parent.children.indexOf(child), 1);
    child.parent = null;
  },
  parentNode: child => child.parent,
  nextSibling: child => {
    const siblings = child.parent?.children ?? [];
    return siblings[siblings.indexOf(child) + 1] ?? null;
  },
});

let databaseId = 0;
let apps: App[];
let sockets: WebsocketTestAdapter[];

function builder(database: string, token?: string) {
  return DbConnection.builder()
    .withUri('ws://127.0.0.1:1234')
    .withDatabaseName(database)
    .withToken(token)
    .withWSFn(async options => {
      const socket = new WebsocketTestAdapter();
      sockets.push(socket);
      return socket.openWebSocket(options);
    });
}

function mountProvider(
  connectionBuilder: ReturnType<typeof builder>,
  consume: (state: ConnectionState) => void = () => {},
  composable = false
): ConnectionState {
  let state!: ConnectionState;
  const Consumer = defineComponent({
    setup() {
      state = useSpacetimeDB();
      consume(state);
      return () => null;
    },
  });
  const Root = composable
    ? defineComponent({
        setup() {
          useSpacetimeDBProvider(connectionBuilder);
          return () => h(Consumer);
        },
      })
    : defineComponent({
        setup: () => () =>
          h(SpacetimeDBProvider, { connectionBuilder }, () => h(Consumer)),
      });
  const app = renderer.createApp(Root);
  app.mount(node());
  apps.push(app);
  return state;
}

async function connect(
  state: ConnectionState,
  token: string,
  identity = anIdentity
) {
  const connection = state.getConnection<DbConnection>()!;
  await connection['wsPromise'];
  const socket = sockets.at(-1)!;
  const connected = new Promise<void>(resolve => {
    const stop = watch(
      () => state.connectionId,
      () => {
        stop();
        resolve();
      },
      { flush: 'sync' }
    );
  });
  socket.acceptConnection();
  socket.sendToClient(
    ServerMessage.InitialConnection({
      identity,
      token,
      connectionId: ConnectionId.random(),
    })
  );
  await connected;
  await nextTick();
  return { connection, socket };
}

function unmountLast() {
  apps.pop()!.unmount();
}

describe('Vue managed provider', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    apps = [];
    sockets = [];
    databaseId += 1;
  });

  afterEach(async () => {
    while (apps.length) unmountLast();
    await vi.runOnlyPendingTimersAsync();
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  test.each([false, true])(
    'swaps anonymous auth and signs out through a stable reactive context (composable=%s)',
    async composable => {
      const database = `vue-auth-${databaseId}`;
      const observed: (string | undefined)[] = [];
      const state = mountProvider(
        builder(database),
        state =>
          watch(
            () => state.token,
            token => observed.push(token)
          ),
        composable
      );
      const getConnection = state.getConnection;
      const { connection: anonymous, socket: anonymousSocket } = await connect(
        state,
        'anonymous-token'
      );
      expect(isReactive(state)).toBe(true);
      expect(isProxy(state.identity)).toBe(false);
      expect(isProxy(state.connectionId)).toBe(false);
      expect(state.getConnection()).toBe(anonymous);

      state.reconnect(builder(database, 'authenticated-token'));
      expect(anonymousSocket.closed).toBe(true);
      expect(anonymous.isDisconnectRequested).toBe(true);
      expect(state.isActive).toBe(false);
      expect(state.token).toBe('authenticated-token');
      const { connection: authenticated } = await connect(
        state,
        'authenticated-token',
        bobIdentity
      );
      expect(state.identity).toEqual(bobIdentity);
      expect(state.identity).toBe(authenticated.identity);
      expect(state.getConnection()).toBe(authenticated);
      expect(state.getConnection).toBe(getConnection);
      expect(authenticated).not.toBe(anonymous);
      expect(sockets.filter(socket => !socket.closed)).toHaveLength(1);

      state.reconnect(builder(database));
      expect(state.token).toBeUndefined();
      await connect(state, 'signed-out-token');
      expect(state.token).toBe('signed-out-token');
      expect(observed).toContain('authenticated-token');
      expect(sockets.filter(socket => !socket.closed)).toHaveLength(1);
    }
  );

  test('shares one connection and replacement, releasing only after the last provider', async () => {
    const database = `vue-shared-${databaseId}`;
    const first = mountProvider(builder(database));
    await connect(first, 'anonymous-token');
    const second = mountProvider(builder(database), undefined, true);
    expect(second.getConnection()).toBe(first.getConnection());
    expect(second.isActive).toBe(true);
    expect(sockets).toHaveLength(1);

    first.reconnect(builder(database, 'signed-in-token'));
    await connect(first, 'signed-in-token', bobIdentity);
    expect(second.getConnection()).toBe(first.getConnection());
    expect(second.token).toBe('signed-in-token');
    expect(second.identity).toEqual(bobIdentity);
    expect(sockets.filter(socket => !socket.closed)).toHaveLength(1);

    unmountLast();
    await vi.advanceTimersByTimeAsync(0);
    expect(sockets.at(-1)!.closed).toBe(false);
    unmountLast();
    await vi.advanceTimersByTimeAsync(0);
    expect(sockets.every(socket => socket.closed)).toBe(true);
    const key = ConnectionManager.getKey('ws://127.0.0.1:1234/', database);
    expect(ConnectionManager.getSnapshot(key)).toBeUndefined();
    first.reconnect(builder(database, 'stale-token'));
    expect(sockets).toHaveLength(2);
  });

  test('isolates providers for different databases', async () => {
    const first = mountProvider(builder(`vue-one-${databaseId}`));
    await connect(first, 'first-token');
    const second = mountProvider(builder(`vue-two-${databaseId}`));
    await connect(second, 'second-token', bobIdentity);
    expect(first.getConnection()).not.toBe(second.getConnection());
    expect(first.token).toBe('first-token');
    expect(second.token).toBe('second-token');
    unmountLast();
    await vi.advanceTimersByTimeAsync(0);
    expect(sockets[0].closed).toBe(false);
    expect(sockets[1].closed).toBe(true);
  });

  test('absorbs rapid remounts without opening a duplicate socket', async () => {
    const database = `vue-remount-${databaseId}`;
    const first = mountProvider(builder(database));
    const { connection } = await connect(first, 'session-token');
    unmountLast();
    const second = mountProvider(builder(database));
    await vi.advanceTimersByTimeAsync(0);
    expect(second.getConnection()).toBe(connection);
    expect(second.token).toBe('session-token');
    expect(sockets).toHaveLength(1);
    expect(sockets[0].closed).toBe(false);
  });

  test('automatically reconnects with the issued session token', async () => {
    const state = mountProvider(builder(`vue-auto-${databaseId}`));
    const { connection, socket } = await connect(state, 'session-token');
    socket.close();
    await nextTick();
    expect(state.isActive).toBe(false);
    expect(state.getConnection()).toBeNull();
    await vi.advanceTimersByTimeAsync(connectionManagerReconnectDelayMs(0));
    expect(state.getConnection()).not.toBe(connection);
    expect(state.getConnection()!.token).toBe('session-token');
    await connect(state, 'session-token');
    expect(state.isActive).toBe(true);
    expect(sockets.filter(socket => !socket.closed)).toHaveLength(1);
  });

  test('token swap cancels a pending reconnect and unmount cancels further retries', async () => {
    const database = `vue-pending-${databaseId}`;
    const state = mountProvider(builder(database));
    const { socket } = await connect(state, 'anonymous-token');
    socket.close();
    state.reconnect(builder(database, 'signed-in-token'));
    const { socket: signedInSocket } = await connect(state, 'signed-in-token');
    await vi.advanceTimersByTimeAsync(connectionManagerReconnectDelayMs(0));
    expect(sockets).toHaveLength(2);
    signedInSocket.close();
    unmountLast();
    await vi.advanceTimersByTimeAsync(connectionManagerReconnectDelayMs(0));
    expect(sockets).toHaveLength(2);
  });

  test('rebinds tables when active connections are replaced in the same Vue tick', async () => {
    const database = `vue-batched-${databaseId}`;
    const state = mountProvider(builder(database), () => useTable(tables.user));
    const { connection: first } = await connect(state, 'anonymous-token');
    const removeListener = vi.spyOn(first.db.user, 'removeOnInsert');
    const replacement = builder(database, 'signed-in-token');
    const build = replacement.build.bind(replacement);
    vi.spyOn(replacement, 'build').mockImplementation(() => {
      const connection = build();
      // Model an already-active replacement: the boolean stays true across
      // Vue's batched update, so only tracking connection identity can rebind.
      connection.isActive = true;
      return connection;
    });
    state.reconnect(replacement);
    const second = state.getConnection<DbConnection>()!;
    const addListener = vi.spyOn(second.db.user, 'onInsert');
    await nextTick();
    expect(state.isActive).toBe(true);
    expect(removeListener).toHaveBeenCalledOnce();
    expect(addListener).toHaveBeenCalledOnce();
  });

  test('composables rebind subscriptions and calls to the authenticated connection', async () => {
    const database = `vue-hooks-${databaseId}`;
    let rows!: ReturnType<typeof useTable>[0];
    let isReady!: ReturnType<typeof useTable>[1];
    let reduce!: ReturnType<typeof useReducer<typeof reducers.createPlayer>>;
    let procedure!: ReturnType<typeof useProcedure>;
    const state = mountProvider(builder(database), () => {
      [rows, isReady] = useTable(tables.user);
      reduce = useReducer(reducers.createPlayer);
      procedure = useProcedure({
        name: 'test',
        accessorName: 'test',
        params: {},
        returnType: t.u32(),
      });
    });
    const { connection: anonymous } = await connect(state, 'anonymous-token');
    const anonymousSocket = sockets.at(-1)!;
    const subscribe = anonymousSocket.outgoingMessages.find(
      message => message.tag === 'Subscribe'
    )!;
    if (subscribe.tag !== 'Subscribe') throw new Error('Missing subscription');
    const applied = new Promise<void>(resolve => {
      const stop = watch(isReady, ready => {
        if (ready) {
          stop();
          resolve();
        }
      });
    });
    anonymousSocket.sendToClient(
      ServerMessage.SubscribeApplied({
        requestId: subscribe.value.requestId,
        querySetId: subscribe.value.querySetId,
        rows: makeQueryRows(
          'user',
          encodeUser({ identity: anIdentity, username: 'anonymous' })
        ),
      })
    );
    await applied;
    expect(rows.value).toEqual([
      { identity: anIdentity, username: 'anonymous' },
    ]);
    const removeListener = vi.spyOn(anonymous.db.user, 'removeOnInsert');
    const unsubscribe = vi.spyOn(anonymous, 'unregisterSubscription');
    state.reconnect(builder(database, 'signed-in-token'));
    await nextTick();
    expect(isReady.value).toBe(false);
    expect(rows.value).toEqual([]);
    expect(removeListener).toHaveBeenCalledOnce();
    expect(unsubscribe).toHaveBeenCalledOnce();
    const { connection: authenticated, socket } = await connect(
      state,
      'signed-in-token'
    );
    expect(
      socket.outgoingMessages.some(message => message.tag === 'Subscribe')
    ).toBe(true);
    const callReducer = vi
      .spyOn(authenticated.reducers, 'createPlayer')
      .mockResolvedValue();
    const callProcedure = vi.fn().mockResolvedValue(42);
    Object.assign(authenticated.procedures, { test: callProcedure });
    await reduce({ name: 'player', location: { x: 0, y: 0 } });
    expect(callReducer).toHaveBeenCalledOnce();
    expect(await procedure()).toBe(42);
    expect(callProcedure).toHaveBeenCalledOnce();
  });
});
