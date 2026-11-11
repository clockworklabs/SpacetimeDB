import {
  DbConnectionBuilder,
  type DbConnectionImpl,
} from '../sdk/db_connection_impl.ts';
import { onCleanup, createMemo, createComputed, createSignal } from 'solid-js';
import { createStore } from 'solid-js/store';
import { SpacetimeDBContext } from './useSpacetimeDB.ts';
import type { ConnectionState } from './connection_state.ts';
import { ConnectionId } from '../lib/connection_id.ts';
import {
  ConnectionManager,
  type ConnectionState as ManagerConnectionState,
} from '../sdk/connection_manager.ts';

export interface SpacetimeDBProviderProps<
  DbConnection extends DbConnectionImpl<any>,
> {
  connectionBuilder: DbConnectionBuilder<DbConnection>;
  children?: any;
}

export function SpacetimeDBProvider<DbConnection extends DbConnectionImpl<any>>(
  props: SpacetimeDBProviderProps<DbConnection>
) {
  const uri = () => props.connectionBuilder.getUri();
  const moduleName = () => props.connectionBuilder.getModuleName();

  const key = createMemo(() => ConnectionManager.getKey(uri(), moduleName()));

  const fallbackState: ManagerConnectionState = {
    isActive: false,
    identity: undefined,
    token: undefined,
    connectionId: ConnectionId.random(),
    connectionError: undefined,
  };

  const [state, setState] = createStore<ManagerConnectionState>(fallbackState);

  const [connection, setConnection] = createSignal<DbConnection | null>(null);

  // Subscribe to ConnectionManager state changes
  createComputed(() => {
    const currentKey = key();

    const unsubscribe = ConnectionManager.subscribe(currentKey, () => {
      const snapshot =
        ConnectionManager.getSnapshot(currentKey) ?? fallbackState;
      setConnection(() =>
        ConnectionManager.getConnection<DbConnection>(currentKey)
      );
      setState(snapshot);
    });

    // Load initial snapshot
    const snapshot = ConnectionManager.getSnapshot(currentKey) ?? fallbackState;
    setConnection(() =>
      ConnectionManager.getConnection<DbConnection>(currentKey)
    );
    setState(snapshot);

    onCleanup(() => {
      unsubscribe();
    });
  });

  const getConnection = () => connection();

  const contextValue: ConnectionState = {
    get isActive() {
      return state.isActive;
    },
    get identity() {
      return state.identity;
    },
    get token() {
      return state.token;
    },
    get connectionId() {
      return state.connectionId;
    },
    get connectionError() {
      return state.connectionError;
    },
    getConnection,
  };

  // Retain / release lifecycle
  createComputed(() => {
    const currentKey = key();
    ConnectionManager.retain(currentKey, props.connectionBuilder);

    onCleanup(() => {
      ConnectionManager.release(currentKey);
    });
  });

  return SpacetimeDBContext.Provider({
    value: contextValue,
    get children() {
      return props.children;
    },
  });
}
