import {
  DbConnectionBuilder,
  type DbConnectionImpl,
} from '../sdk/db_connection_impl';
import * as React from 'react';
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
  children?: React.ReactNode;
}

export function SpacetimeDBProvider<
  DbConnection extends DbConnectionImpl<any>,
>({
  connectionBuilder,
  children,
}: SpacetimeDBProviderProps<DbConnection>): React.JSX.Element {
  const uri = connectionBuilder.getUri();
  const moduleName = connectionBuilder.getModuleName();
  const key = React.useMemo(
    () => ConnectionManager.getKey(uri, moduleName),
    [uri, moduleName]
  );

  const fallbackStateRef = React.useRef<ManagerConnectionState>({
    isActive: false,
    identity: undefined,
    token: undefined,
    connectionId: ConnectionId.random(),
    connectionError: undefined,
  });

  const subscribe = React.useCallback(
    (onStoreChange: () => void) =>
      ConnectionManager.subscribe(key, onStoreChange),
    [key]
  );
  const getSnapshot = React.useCallback(
    () => ConnectionManager.getSnapshot(key) ?? fallbackStateRef.current,
    [key]
  );
  const getServerSnapshot = React.useCallback(
    () => fallbackStateRef.current,
    []
  );

  const state = React.useSyncExternalStore(
    subscribe,
    getSnapshot,
    getServerSnapshot
  );

  const getConnection = React.useCallback(
    () => ConnectionManager.getConnection<DbConnection>(key),
    [key]
  );

  const contextValue = React.useMemo<ConnectionState>(
    () => ({ ...state, getConnection }),
    [state, getConnection]
  );

  React.useEffect(() => {
    ConnectionManager.retain(key, connectionBuilder);
    return () => {
      ConnectionManager.release(key);
    };
  }, [key, connectionBuilder]);

  return React.createElement(
    SpacetimeDBContext.Provider,
    { value: contextValue },
    children
  );
}
