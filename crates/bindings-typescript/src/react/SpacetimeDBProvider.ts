import {
  DbConnectionBuilder,
  type DbConnectionImpl,
  type ErrorContextInterface,
  type SubscriptionEventContextInterface,
} from '../sdk/db_connection_impl';
import * as React from 'react';
import { SpacetimeDBContext } from './useSpacetimeDB';
import type { ConnectionState } from './connection_state';
import { ConnectionId } from '../lib/connection_id';

export interface SpacetimeDBProviderProps<
  DbConnection extends DbConnectionImpl,
  ErrorContext extends ErrorContextInterface,
  SubscriptionEventContext extends SubscriptionEventContextInterface,
> {
  connectionBuilder: DbConnectionBuilder<DbConnection, ErrorContext, SubscriptionEventContext>;
  children?: React.ReactNode;
}

export function SpacetimeDBProvider<
  DbConnection extends DbConnectionImpl,
  ErrorContext extends ErrorContextInterface,
  SubscriptionEventContext extends SubscriptionEventContextInterface,
>({ connectionBuilder, children }: SpacetimeDBProviderProps<DbConnection, ErrorContext, SubscriptionEventContext>) {
  // Holds the imperative connection instance when (and only when) we’re on the client.
  const connRef = React.useRef<DbConnection | null>(null);
  const getConnection = React.useCallback(() => connRef.current, []);

  const [state, setState] = React.useState<ConnectionState<DbConnection>>({
    isActive: false,
    identity: undefined,
    token: undefined,
    connectionId: ConnectionId.random(),
    connectionError: undefined,
    getConnection,
  });

  // Build on the client only; useEffect won't run during SSR.
  React.useEffect(() => {
    // Register callback for onConnect to update state
      const onConnect = (conn: DbConnectionImpl) => {
      setState(s => ({
        ...s,
        isActive: conn.isActive,
        identity: conn.identity,
        token: conn.token,
        connectionId: conn.connectionId,
      }));
    };
    const onDisconnect = (ctx: ErrorContextInterface) => {
      setState(s => ({
        ...s,
        isActive: ctx.isActive,
      }));
    };
    const onConnectError = (ctx: ErrorContextInterface, err: Error) => {
      setState(s => ({
        ...s,
        isActive: ctx.isActive,
        connectionError: err,
      }));
    };
    connectionBuilder.onConnect(onConnect);
    connectionBuilder.onDisconnect(onDisconnect);
    connectionBuilder.onConnectError(onConnectError);

    const conn = connRef.current!;
    setState(s => ({
      ...s,
      isActive: conn.isActive,
      identity: conn.identity,
      token: conn.token,
      connectionId: conn.connectionId,
    }));

    // Lazily build once
    if (!connRef.current) {
      connRef.current = connectionBuilder.build();
    }

    return () => {
      connRef.current?.removeOnConnect(onConnect);
      connRef.current?.removeOnDisconnect(onDisconnect);
      connRef.current?.removeOnConnectError(onConnectError);
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