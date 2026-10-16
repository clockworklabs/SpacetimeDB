// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import { act, createElement } from 'react';
import { createRoot as createReactRoot } from 'react-dom/client';
import { createApp, nextTick, shallowReactive } from 'vue';
import { createRoot, createSignal, createMemo } from 'solid-js';
import { writable, get } from 'svelte/store';
import {
  Injector,
  signal,
  runInInjectionContext,
  ɵEffectScheduler as EffectScheduler,
  ɵChangeDetectionScheduler as ChangeDetectionScheduler,
} from '@angular/core';
import { useTable as useReactTable } from '../src/react/useTable';
import { SpacetimeDBContext as ReactContext } from '../src/react/useSpacetimeDB';
import { useTable as vueTable } from '../src/vue/useTable';
import { useTable as solidTable } from '../src/solid/useTable';
import { SpacetimeDBContext as SolidContext } from '../src/solid/useSpacetimeDB';
import { SpacetimeDBProvider as SolidProvider } from '../src/solid/SpacetimeDBProvider';
import { ConnectionManager } from '../src/sdk/connection_manager';
import { useTable as svelteTable } from '../src/svelte/useTable';
import { injectTable } from '../src/angular/injectors/inject-table';
import { SPACETIMEDB_CONNECTION } from '../src/angular/connection_state';
import type { ConnectionState } from '../src/react/connection_state';
import type { ConnectionState as VueConnectionState } from '../src/vue/connection_state';
import type { ConnectionState as SvelteConnectionState } from '../src/svelte/connection_state';
import { ConnectionId } from '../src';
import { ServerMessage } from '../src/sdk/client_api/types';
import { WebsocketTestAdapterFactory } from '../src/sdk/websocket_test_adapter';
import { DbConnection, tables } from '../test-app/src/module_bindings';
import { anIdentity, encodeUser } from './utils';

vi.mock('solid-js', () =>
  vi.importActual<typeof import('solid-js')>('solid-js/dist/solid.js')
);
vi.mock('solid-js/store', () =>
  vi.importActual<typeof import('solid-js/store')>(
    'solid-js/store/dist/store.js'
  )
);
let vueState: VueConnectionState;
vi.mock('../src/vue/useSpacetimeDB', () => ({
  useSpacetimeDB: () => vueState,
}));
let svelteState = writable<SvelteConnectionState>();
let destroySvelte = () => {};
vi.mock('../src/svelte/useSpacetimeDB', () => ({
  useSpacetimeDB: () => svelteState,
}));
vi.mock('svelte', () => ({
  onDestroy: (cleanup: () => void) => {
    destroySvelte = cleanup;
  },
}));

type UserQuery = ReturnType<typeof tables.user.where>;
type Snapshot = { ready: boolean; names: readonly string[] };
type MountedHook = {
  update(state: ConnectionState): void;
  flush(): Promise<void>;
  snapshot(): Snapshot;
  destroy(): void;
  setQuery?(query: UserQuery): void;
  setEnabled?(enabled: boolean): void;
};

type ScheduledEffect = Parameters<EffectScheduler['add']>[0];
class TestEffectScheduler extends EffectScheduler {
  effects = new Set<ScheduledEffect>();
  add(effect: ScheduledEffect): void {
    this.effects.add(effect);
  }
  schedule(): void {}
  remove(effect: ScheduledEffect): void {
    this.effects.delete(effect);
  }
  flush(): void {
    for (const effect of this.effects) if (effect.dirty) effect.run();
  }
}

function mount(framework: string, initial: ConnectionState): MountedHook {
  if (framework === 'React') {
    let state = initial;
    let query: UserQuery = tables.user;
    let enabled = true;
    let value: Snapshot = { ready: false, names: [] };
    const root = createReactRoot(document.createElement('div'));
    function Consumer() {
      const [rows, ready] = useReactTable(query, { enabled });
      value = { ready, names: rows.map(row => row.username) };
      return null;
    }
    const render = () =>
      root.render(
        createElement(
          ReactContext.Provider,
          { value: state },
          createElement(Consumer)
        )
      );
    return {
      update(next) {
        state = next;
      },
      async flush() {
        await act(async () => render());
      },
      snapshot: () => value,
      setQuery(next) {
        query = next;
      },
      setEnabled(next) {
        enabled = next;
      },
      destroy() {
        act(() => root.unmount());
      },
    };
  }
  if (framework === 'Vue') {
    const state = shallowReactive(initial);
    vueState = state as VueConnectionState;
    let snapshot: () => Snapshot = () => ({ ready: false, names: [] });
    const app = createApp({
      setup() {
        const [rows, ready] = vueTable(tables.user);
        snapshot = () => ({
          ready: ready.value,
          names: rows.value.map(row => row.username),
        });
        return () => null;
      },
    });
    app.mount(document.createElement('div'));
    return {
      update(next) {
        Object.assign(state, next);
      },
      flush: async () => {
        await nextTick();
      },
      snapshot: () => snapshot(),
      destroy: () => app.unmount(),
    };
  }
  if (framework === 'Solid') {
    return createRoot(dispose => {
      const [state, setState] = createSignal(initial);
      const [query, setQuery] = createSignal<UserQuery>(tables.user);
      const [enabled, setEnabled] = createSignal(true);
      const connection = createMemo(() => state().getConnection());
      const context: ConnectionState = {
        get isActive() {
          return state().isActive;
        },
        get connectionId() {
          return state().connectionId;
        },
        getConnection: connection,
        reconnect: () => {},
      };
      let snapshot: () => Snapshot = () => ({ ready: false, names: [] });
      SolidContext.Provider({
        value: context,
        get children() {
          const [rows, ready] = solidTable(query, { enabled });
          snapshot = () => ({
            ready: ready(),
            names: rows.map(row => row.username),
          });
          return null;
        },
      });
      return {
        update(next) {
          setState(next);
        },
        flush: async () => {},
        snapshot: () => snapshot(),
        destroy: dispose,
        setQuery(next) {
          setQuery(next);
        },
        setEnabled,
      };
    });
  }
  if (framework === 'Svelte') {
    svelteState = writable({ ...initial, reconnect() {} });
    const [rows, ready] = svelteTable(tables.user);
    return {
      update(next) {
        svelteState.set({ ...next, reconnect() {} });
      },
      flush: async () => {},
      snapshot: () => ({
        ready: get(ready),
        names: get(rows).map(row => row.username),
      }),
      destroy: () => destroySvelte(),
    };
  }
  const state = signal(initial);
  const scheduler = new TestEffectScheduler();
  const injector = Injector.create({
    providers: [
      { provide: SPACETIMEDB_CONNECTION, useValue: state },
      { provide: EffectScheduler, useValue: scheduler },
      { provide: ChangeDetectionScheduler, useValue: { notify() {} } },
    ],
  });
  const rows = runInInjectionContext(injector, () => injectTable(tables.user));
  return {
    update(next) {
      state.set(next);
    },
    flush: async () => scheduler.flush(),
    snapshot: () => ({
      ready: !rows().isLoading,
      names: rows().rows.map(row => row.username),
    }),
    destroy: () => injector.destroy(),
  };
}

function fixture(framework: string) {
  const factory = new WebsocketTestAdapterFactory();
  let connection: DbConnection;
  const send = (message: ServerMessage) => {
    act(() => factory.current.sendToClient(message));
  };
  const snapshot = (): ConnectionState => ({
    isActive: connection.isActive,
    connectionId: connection.connectionId,
    getConnection: () => connection,
    reconnect: () => {},
  });
  const notify = () => hook?.update(snapshot());
  const build = () =>
    DbConnection.builder()
      .withUri('ws://localhost:1234')
      .withDatabaseName('db')
      .withAutomaticReconnect()
      .withWSFn(factory.openWebSocket)
      .onConnect(notify)
      .onAutomaticReconnect(notify)
      .onDisconnect(notify)
      .onConnectError(notify)
      .build();
  connection = build();
  const hook = mount(framework, snapshot());
  const mounted = hook;
  const establish = async () => {
    await connection['wsPromise'];
    factory.current.acceptConnection();
    send(
      ServerMessage.InitialConnection({
        identity: anIdentity,
        connectionId: ConnectionId.random(),
        token: 'token',
      })
    );
    await Promise.resolve();
  };
  const rows = {
    tables: [
      {
        table: 'user',
        rows: {
          sizeHint: { tag: 'RowOffsets' as const, value: [0n] },
          rowsData: encodeUser({ identity: anIdentity, username: 'Alice' }),
        },
      },
    ],
  };
  const apply = () => {
    const message = factory.current.outgoingMessages.at(-1);
    if (message?.tag === 'Subscribe') {
      send(
        ServerMessage.SubscribeApplied({
          requestId: message.value.requestId,
          querySetId: message.value.querySetId,
          rows,
        })
      );
    } else if (message?.tag === 'SubscribeBatch') {
      send(
        ServerMessage.SubscribeBatchApplied({
          requestId: message.value.requestId,
          results: message.value.sets.map(set => ({
            querySetId: set.querySetId,
            outcome: { tag: 'Applied' as const, value: rows },
          })),
        })
      );
    } else throw new Error('Expected a subscription request');
  };
  return {
    factory,
    hook: mounted,
    establish,
    apply,
    send,
    connection: () => connection,
    async replace() {
      connection.disconnect();
      connection = build();
      notify();
      await mounted.flush();
    },
    async destroy() {
      mounted.destroy();
      await Promise.resolve();
      connection.disconnect();
    },
  };
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe.each(['React', 'Vue', 'Solid', 'Svelte', 'Angular'])(
  '%s table reconnect',
  framework => {
    test.each([false, true])(
      'retains its subscription and cached rows (batched recovery: %s)',
      async batched => {
        const f = fixture(framework);
        try {
          await f.hook.flush();
          await f.establish();
          await f.hook.flush();
          expect(f.factory.current.outgoingMessages.map(m => m.tag)).toEqual([
            'Subscribe',
          ]);
          f.apply();
          await f.hook.flush();
          expect(f.hook.snapshot()).toEqual({ ready: true, names: ['Alice'] });

          for (let attempt = 0; attempt < 2; attempt++) {
            f.factory.current.close();
            if (!batched) {
              await f.hook.flush();
              expect(f.hook.snapshot()).toEqual({
                ready: false,
                names: ['Alice'],
              });
            }
            await vi.runOnlyPendingTimersAsync();
            await f.establish();
            if (!batched) {
              await f.hook.flush();
              expect(f.hook.snapshot().ready).toBe(false);
            }
            expect(f.factory.current.outgoingMessages.map(m => m.tag)).toEqual([
              'SubscribeBatch',
            ]);
            f.apply();
            await f.hook.flush();
            expect(f.hook.snapshot()).toEqual({
              ready: true,
              names: ['Alice'],
            });
            expect(f.factory.current.outgoingMessages.map(m => m.tag)).toEqual([
              'SubscribeBatch',
            ]);
          }
        } finally {
          await f.destroy();
        }
      }
    );

    test('subscribes on a replacement connection and cleans up on unmount', async () => {
      const f = fixture(framework);
      try {
        await f.hook.flush();
        await f.establish();
        await f.hook.flush();
        f.apply();
        await f.hook.flush();
        await f.replace();
        await f.establish();
        await f.hook.flush();
        expect(f.hook.snapshot()).toEqual({ ready: false, names: [] });
        expect(f.factory.current.outgoingMessages.map(m => m.tag)).toEqual([
          'Subscribe',
        ]);
        f.apply();
        await f.hook.flush();
        expect(f.hook.snapshot().ready).toBe(true);
      } finally {
        await f.destroy();
      }
      expect(f.factory.current.outgoingMessages.at(-1)?.tag).toBe(
        'Unsubscribe'
      );
    });
  }
);

describe.each(['React', 'Vue', 'Solid', 'Svelte', 'Angular'])(
  '%s replay failure',
  framework => {
    test('stays not ready when replay is rejected', async () => {
      const f = fixture(framework);
      try {
        await f.hook.flush();
        await f.establish();
        await f.hook.flush();
        f.apply();
        await f.hook.flush();
        f.factory.current.close();
        await vi.runOnlyPendingTimersAsync();
        await f.establish();
        const batch = f.factory.current.outgoingMessages[0];
        if (batch.tag !== 'SubscribeBatch') throw new Error('Expected replay');
        f.send(
          ServerMessage.SubscribeBatchApplied({
            requestId: batch.value.requestId,
            results: batch.value.sets.map(set => ({
              querySetId: set.querySetId,
              outcome: { tag: 'Error' as const, value: 'permission denied' },
            })),
          })
        );
        await f.hook.flush();
        expect(f.hook.snapshot().ready).toBe(false);
        expect(f.factory.current.outgoingMessages.map(m => m.tag)).toEqual([
          'SubscribeBatch',
        ]);
      } finally {
        await f.destroy();
      }
    });
  }
);

describe.each(['React', 'Solid'])('%s reactive query', framework => {
  test('replaces the query during an outage and replays only the new query', async () => {
    const f = fixture(framework);
    try {
      await f.hook.flush();
      await f.establish();
      await f.hook.flush();
      f.apply();
      await f.hook.flush();
      f.factory.current.close();
      f.hook.setQuery!(tables.user.where(row => row.username.eq('Alice')));
      await f.hook.flush();
      await vi.runOnlyPendingTimersAsync();
      await f.establish();
      await f.hook.flush();
      const messages = f.factory.current.outgoingMessages;
      expect(messages.map(m => m.tag)).toEqual(['SubscribeBatch']);
      const batch = messages[0];
      if (batch.tag !== 'SubscribeBatch') throw new Error('Expected replay');
      expect(batch.value.sets).toHaveLength(1);
      expect(batch.value.sets[0].queryStrings[0]).toContain('Alice');
      expect(f.hook.snapshot().ready).toBe(false);
      f.apply();
      await f.hook.flush();
      expect(f.hook.snapshot().ready).toBe(true);
    } finally {
      await f.destroy();
    }
  });

  test('does not replay a subscription disabled during an outage', async () => {
    const f = fixture(framework);
    try {
      await f.hook.flush();
      await f.establish();
      await f.hook.flush();
      f.apply();
      await f.hook.flush();
      f.factory.current.close();
      f.hook.setEnabled!(false);
      await f.hook.flush();
      await vi.runOnlyPendingTimersAsync();
      await f.establish();
      await f.hook.flush();
      expect(f.factory.current.outgoingMessages).toEqual([]);
      f.hook.setEnabled!(true);
      await f.hook.flush();
      await Promise.resolve();
      expect(f.factory.current.outgoingMessages.map(m => m.tag)).toEqual([
        'Subscribe',
      ]);
      expect(f.hook.snapshot().ready).toBe(false);
      f.apply();
      await f.hook.flush();
      expect(f.hook.snapshot().ready).toBe(true);
    } finally {
      await f.destroy();
    }
  });
});

test('Solid provider exposes replacement connections without rebinding on reconnect', async () => {
  const factory = new WebsocketTestAdapterFactory();
  const builder = DbConnection.builder()
    .withUri('ws://localhost:1234')
    .withDatabaseName('solid-provider-reconnect')
    .withWSFn(factory.openWebSocket);
  const key = ConnectionManager.getKey(
    builder.getUri(),
    builder.getModuleName()
  );
  const dispose = createRoot(dispose => {
    SolidProvider({
      connectionBuilder: builder,
      get children() {
        solidTable(() => tables.user);
        return null;
      },
    });
    return dispose;
  });
  const establish = async () => {
    const connection = ConnectionManager.getConnection<DbConnection>(key)!;
    await connection['wsPromise'];
    factory.current.acceptConnection();
    factory.current.sendToClient(
      ServerMessage.InitialConnection({
        identity: anIdentity,
        connectionId: ConnectionId.random(),
        token: 'token',
      })
    );
    await Promise.resolve();
  };
  try {
    await establish();
    expect(factory.current.outgoingMessages.map(m => m.tag)).toEqual([
      'Subscribe',
    ]);
    factory.current.close();
    await vi.runOnlyPendingTimersAsync();
    await establish();
    expect(factory.current.outgoingMessages.map(m => m.tag)).toEqual([
      'SubscribeBatch',
    ]);
    ConnectionManager.rebuild(key, builder);
    await establish();
    expect(factory.current.outgoingMessages.map(m => m.tag)).toEqual([
      'Subscribe',
    ]);
  } finally {
    dispose();
    await vi.runOnlyPendingTimersAsync();
  }
});
