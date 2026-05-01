import { setContext, onDestroy } from 'svelte';
import { writable, type Writable } from 'svelte/store';
import {
  DbConnectionBuilder,
  type DbConnectionImpl,
  type ErrorContextInterface,
  type RemoteModuleOf,
} from '../sdk/db_connection_impl';
import { ConnectionId } from '../lib/connection_id';
import {
  SPACETIMEDB_CONTEXT_KEY,
  type ConnectionState,
} from './connection_state';

let connRef: DbConnectionImpl<any> | null = null;
let cleanupTimeoutId: ReturnType<typeof setTimeout> | null = null;

export function createSpacetimeDBProvider<
  DbConnection extends DbConnectionImpl<any>,
>(
  connectionBuilder: DbConnectionBuilder<DbConnection>
): Writable<ConnectionState> {
  const getConnection = () => connRef as DbConnection | null;

  const store = writable<ConnectionState>({
    isActive: false,
    identity: undefined,
    token: undefined,
    connectionId: ConnectionId.random(),
    connectionError: undefined,
    getConnection: getConnection as ConnectionState['getConnection'],
  });

  if (cleanupTimeoutId) {
    clearTimeout(cleanupTimeoutId);
    cleanupTimeoutId = null;
  }

  if (!connRef) {
    connRef = connectionBuilder.build();
  }

  const onConnect = (conn: DbConnection) => {
    store.update(s => ({
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
    store.update(s => ({
      ...s,
      isActive: ctx.isActive,
    }));
  };

  const onConnectError = (
    ctx: ErrorContextInterface<RemoteModuleOf<DbConnection>>,
    err: Error
  ) => {
    store.update(s => ({
      ...s,
      isActive: ctx.isActive,
      connectionError: err,
    }));
  };

  connectionBuilder.onConnect(onConnect);
  connectionBuilder.onDisconnect(onDisconnect);
  connectionBuilder.onConnectError(onConnectError);

  const conn = connRef;
  store.update(s => ({
    ...s,
    isActive: conn.isActive,
    identity: conn.identity,
    token: conn.token,
    connectionId: conn.connectionId,
  }));

  setContext(SPACETIMEDB_CONTEXT_KEY, store);

  onDestroy(() => {
    connRef?.removeOnConnect(onConnect as any);
    connRef?.removeOnDisconnect(onDisconnect as any);
    connRef?.removeOnConnectError(onConnectError as any);

    cleanupTimeoutId = setTimeout(() => {
      connRef?.disconnect();
      connRef = null;
      cleanupTimeoutId = null;
    }, 0);
  });

  return store;
}
