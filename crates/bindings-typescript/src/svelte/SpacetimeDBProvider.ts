import { setContext, onDestroy } from 'svelte';
import { writable, type Writable } from 'svelte/store';
import {
  DbConnectionBuilder,
  type DbConnectionImpl,
} from '../sdk/db_connection_impl';
import { ConnectionId } from '../lib/connection_id';
import {
  ConnectionManager,
  type ConnectionState as ManagerConnectionState,
} from '../sdk/connection_manager';
import {
  SPACETIMEDB_CONTEXT_KEY,
  type ConnectionState,
} from './connection_state';

/**
 * Establish a SpacetimeDB connection for the current component subtree and make
 * it available to `useSpacetimeDB`, `useTable` and `useReducer`.
 *
 * The connection is owned by the shared `ConnectionManager` (the same pool the
 * React and Solid bindings use), keyed by uri + database name. The manager's
 * reference counting and deferred cleanup absorb rapid mount/unmount cycles
 * (HMR, `{#key}` blocks), and it reconnects automatically with exponential
 * backoff if the socket drops unexpectedly.
 *
 * To swap the connection's auth token without reloading the page (e.g. after a
 * sign-in), call `reconnect(builder)` from the context value with a builder
 * carrying the new token.
 */
export function createSpacetimeDBProvider<
  DbConnection extends DbConnectionImpl<any>,
>(
  connectionBuilder: DbConnectionBuilder<DbConnection>
): Writable<ConnectionState> {
  const key = ConnectionManager.getKey(
    connectionBuilder.getUri(),
    connectionBuilder.getModuleName()
  );

  const fallback: ManagerConnectionState = {
    isActive: false,
    identity: undefined,
    token: undefined,
    connectionId: ConnectionId.random(),
    connectionError: undefined,
  };

  const getConnection = () =>
    ConnectionManager.getConnection<DbConnection>(key);
  const reconnect = (builder: DbConnectionBuilder<DbConnection>): void => {
    ConnectionManager.rebuild(key, builder);
  };

  const snapshot = (): ConnectionState => ({
    ...(ConnectionManager.getSnapshot(key) ?? fallback),
    getConnection,
    reconnect,
  });

  // Retain for this provider's lifetime, then mirror the manager's external
  // store into a Svelte store. `getConnection` / `reconnect` are stable across
  // updates; only the plain state fields change.
  ConnectionManager.retain(key, connectionBuilder);
  const store = writable<ConnectionState>(snapshot());
  const unsubscribe = ConnectionManager.subscribe(key, () =>
    store.set(snapshot())
  );

  onDestroy(() => {
    unsubscribe();
    ConnectionManager.release(key);
  });

  setContext(SPACETIMEDB_CONTEXT_KEY, store);
  return store;
}
