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
  // Holds the imperative connection instance when (and only when) we're on the client.
  const connRef = React.useRef<DbConnection | null>(null);
  // Used to detect React StrictMode vs real unmounts (see cleanup comment below)
  const cleanupTimeoutRef = React.useRef<ReturnType<typeof setTimeout> | null>(
    null
  );
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
    // If we're remounting after a StrictMode unmount, cancel the pending disconnect
    if (cleanupTimeoutRef.current) {
      clearTimeout(cleanupTimeoutRef.current);
      cleanupTimeoutRef.current = null;
    }

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

      // Detect React StrictMode vs real unmounts using a deferred disconnect.
      //
      // In StrictMode, React unmounts and remounts components synchronously
      // (in the same JavaScript task) to help detect side-effect issues.
      // By deferring disconnect with setTimeout(..., 0), we push it to the
      // next task in the event loop. This lets us distinguish:
      //
      // - StrictMode (fake unmount): cleanup runs → timeout scheduled →
      //   remount happens immediately (same task) → remount cancels timeout →
      //   connection survives
      //
      // - Real unmount: cleanup runs → timeout scheduled → no remount →
      //   timeout fires → connection is properly closed
      cleanupTimeoutRef.current = setTimeout(() => {
        connRef.current?.disconnect();
        connRef.current = null;
        cleanupTimeoutRef.current = null;
      }, 0);
    };
  }, [connectionBuilder]);

  return React.createElement(
    SpacetimeDBContext.Provider,
    { value: state },
    children
  );
}
