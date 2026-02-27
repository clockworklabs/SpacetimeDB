import {
  DbConnectionBuilder,
  type DbConnectionImpl,
} from '../sdk/db_connection_impl';
import { createSignal, createEffect, onCleanup, createMemo } from 'solid-js';
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

export function SpacetimeDBProvider<
  DbConnection extends DbConnectionImpl<any>,
>(props: SpacetimeDBProviderProps<DbConnection>) {
  const uri = () => props.connectionBuilder.getUri();
  const moduleName = () => props.connectionBuilder.getModuleName();

  const key = createMemo(() =>
    ConnectionManager.getKey(uri(), moduleName())
  );

  const fallbackState: ManagerConnectionState = {
    isActive: false,
    identity: undefined,
    token: undefined,
    connectionId: ConnectionId.random(),
    connectionError: undefined,
  };

  const [state, setState] =
    createSignal<ManagerConnectionState>(fallbackState);

  // Subscription to ConnectionManager
  createEffect(() => {
    const unsubscribe = ConnectionManager.subscribe(key(), () => {
      const snapshot =
        ConnectionManager.getSnapshot(key()) ?? fallbackState;
      setState(snapshot);
    });

    // initial snapshot
    const snapshot =
      ConnectionManager.getSnapshot(key()) ?? fallbackState;
    setState(snapshot);

    onCleanup(() => {
      unsubscribe();
    });
  });

  const getConnection = () =>
    ConnectionManager.getConnection<DbConnection>(key());

  const contextValue = createMemo<ConnectionState>(() => ({
    ...state(),
    getConnection,
  }));

  // retain / release lifecycle
  createEffect(() => {
    ConnectionManager.retain(key(), props.connectionBuilder);

    onCleanup(() => {
      ConnectionManager.release(key());
    });
  });


  return SpacetimeDBContext.Provider({
      value: contextValue(),
      children: props.children,
    });
}