import {
  DbConnectionBuilder,
  type DbConnectionImpl,
  type ErrorContextInterface,
  type RemoteModuleOf,
} from '../sdk/db_connection_impl';
import * as React from 'react';
import { SpacetimeDBContext } from './useSpacetimeDB';
import type { ConnectionState } from './connection_state';
import { ConnectionId } from '../lib/connection_id';

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
  // Holds the imperative connection instance when (and only when) we’re on the client.
  const connRef = React.useRef<DbConnection | null>(null);
  const getConnection = React.useCallback(() => connRef.current, []);

  const [state, setState] = React.useState<ConnectionState>({
    isActive: false,
    identity: undefined,
    token: undefined,
    connectionId: ConnectionId.random(),
    connectionError: undefined,
    getConnection: getConnection as ConnectionState['getConnection'],
  });

  // Build on the client only; useEffect won't run during SSR.
  React.useEffect(() => {
    if (!connRef.current) {
      connRef.current = connectionBuilder.build();
    }
    // Register callback for onConnect to update state
    const onConnect = (conn: DbConnection) => {
      setState(s => ({
        ...s,
        isActive: conn.isActive,
        identity: conn.identity,
        token: conn.token,
        connectionId: conn.connectionId,
      }));
    };
    const onDisconnect = (
      ctx: ErrorContextInterface<RemoteModuleOf<DbConnection>>
    ) => {
      setState(s => ({
        ...s,
        isActive: ctx.isActive,
      }));
    };
    const onConnectError = (
      ctx: ErrorContextInterface<RemoteModuleOf<DbConnection>>,
      err: Error
    ) => {
      setState(s => ({
        ...s,
        isActive: ctx.isActive,
        connectionError: err,
      }));
    };
    connectionBuilder.onConnect(onConnect);
    connectionBuilder.onDisconnect(onDisconnect);
    connectionBuilder.onConnectError(onConnectError);

    const conn = connRef.current;
    setState(s => ({
      ...s,
      isActive: conn.isActive,
      identity: conn.identity,
      token: conn.token,
      connectionId: conn.connectionId,
    }));

    return () => {
      connRef.current?.removeOnConnect(onConnect as any);
      connRef.current?.removeOnDisconnect(onDisconnect as any);
      connRef.current?.removeOnConnectError(onConnectError as any);
      connRef.current?.disconnect();
      connRef.current = null;
    };
  }, [connectionBuilder]);

  return React.createElement(
    SpacetimeDBContext.Provider,
    { value: state },
    children
  );
}
