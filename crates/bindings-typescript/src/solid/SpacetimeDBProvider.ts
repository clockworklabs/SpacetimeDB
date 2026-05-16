import {
  DbConnectionBuilder,
  type DbConnectionImpl,
} from '../sdk/db_connection_impl';
import { createEffect, onCleanup, createMemo, createComputed } from 'solid-js';
import { createStore } from 'solid-js/store';
import { SpacetimeDBContext } from './useSpacetimeDB';
import type { ConnectionState } from './connection_state';
import { ConnectionId } from '../lib/connection_id';
import {
  ConnectionManager,
  type ConnectionState as ManagerConnectionState,
} from '../sdk/connection_manager';

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

  // Subscribe to ConnectionManager state changes
  createComputed(() => {
    const currentKey = key();

    const unsubscribe = ConnectionManager.subscribe(currentKey, () => {
      const snapshot =
        ConnectionManager.getSnapshot(currentKey) ?? fallbackState;
      setState(snapshot);
    });

    // Load initial snapshot
    const snapshot = ConnectionManager.getSnapshot(currentKey) ?? fallbackState;
    setState(snapshot);

    onCleanup(() => {
      unsubscribe();
    });
  });

  const getConnection = () =>
    ConnectionManager.getConnection<DbConnection>(key());

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
