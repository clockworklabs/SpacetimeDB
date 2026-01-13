/**
 * ConnectionManager - A reference-counted connection manager for SpacetimeDB.
 *
 * This module implements a TanStack Query-style pattern for managing WebSocket
 * connections in React applications. It solves the React StrictMode double-mount
 * problem by using reference counting and deferred cleanup.
 *
 * ## How it works:
 *
 * 1. **Reference Counting**: Each `retain()` increments a counter, `release()` decrements it.
 *    The connection is only closed when the count reaches zero.
 *
 * 2. **Deferred Cleanup**: When refCount hits zero, cleanup is scheduled via `setTimeout(0)`.
 *    This allows React StrictMode's rapid unmount→remount cycle to cancel the cleanup.
 *
 * 3. **useSyncExternalStore Integration**: The `subscribe()` and `getSnapshot()` methods
 *    are designed to work with React's `useSyncExternalStore` hook for tear-free reads.
 *
 * ## StrictMode Lifecycle:
 *
 * ```
 * Mount   → retain()  → refCount: 0→1, connection created
 * Unmount → release() → refCount: 1→0, cleanup SCHEDULED (not executed)
 * Remount → retain()  → refCount: 0→1, cleanup CANCELLED
 * Result: Single WebSocket survives ✓
 * ```
 *
 * @module connection_manager
 */
import type {
  DbConnectionBuilder,
  DbConnectionImpl,
  ErrorContextInterface,
} from './db_connection_impl';
import type { Identity } from '../lib/identity';
import { ConnectionId } from '../lib/connection_id';

/** Represents the current state of a managed connection. */
export type ConnectionState = {
  isActive: boolean;
  identity?: Identity;
  token?: string;
  connectionId: ConnectionId;
  connectionError?: Error;
};

type Listener = () => void;

type ManagedConnection = {
  connection?: DbConnectionImpl<any>;
  refCount: number;
  state: ConnectionState;
  listeners: Set<Listener>;
  pendingRelease: ReturnType<typeof setTimeout> | null;
  onConnect?: (conn: DbConnectionImpl<any>) => void;
  onDisconnect?: (ctx: ErrorContextInterface<any>, error?: Error) => void;
  onConnectError?: (ctx: ErrorContextInterface<any>, error: Error) => void;
};

function defaultState(): ConnectionState {
  return {
    isActive: false,
    identity: undefined,
    token: undefined,
    connectionId: ConnectionId.random(),
    connectionError: undefined,
  };
}

/**
 * Singleton manager for SpacetimeDB connections.
 * Use the exported `ConnectionManager` instance rather than instantiating directly.
 */
class ConnectionManagerImpl {
  #connections = new Map<string, ManagedConnection>();

  /** Generates a unique key for a connection based on URI and module name. */
  static getKey(uri: string, moduleName: string): string {
    return `${uri}::${moduleName}`;
  }

  /** Instance method wrapper for getKey. */
  getKey(uri: string, moduleName: string): string {
    return ConnectionManagerImpl.getKey(uri, moduleName);
  }

  #ensureEntry(key: string): ManagedConnection {
    const existing = this.#connections.get(key);
    if (existing) {
      return existing;
    }
    const managed: ManagedConnection = {
      connection: undefined,
      refCount: 0,
      state: defaultState(),
      listeners: new Set(),
      pendingRelease: null,
    };
    this.#connections.set(key, managed);
    return managed;
  }

  #notify(managed: ManagedConnection): void {
    for (const listener of managed.listeners) {
      listener();
    }
  }

  /**
   * Retains a connection, incrementing its reference count.
   * Creates the connection on first call; returns existing connection on subsequent calls.
   * Cancels any pending release if the connection was about to be cleaned up.
   *
   * @param key - Unique identifier for the connection (use getKey to generate)
   * @param builder - Connection builder to create the connection if needed
   * @returns The managed connection instance
   */
  retain<T extends DbConnectionImpl<any>>(
    key: string,
    builder: DbConnectionBuilder<T>
  ): T {
    const managed = this.#ensureEntry(key);
    if (managed.pendingRelease) {
      clearTimeout(managed.pendingRelease);
      managed.pendingRelease = null;
    }
    managed.refCount += 1;
    if (managed.connection) {
      return managed.connection as T;
    }

    const connection = builder.build();
    managed.connection = connection;

    const updateState = (updates: Partial<ConnectionState>) => {
      managed.state = { ...managed.state, ...updates };
      this.#notify(managed);
    };

    updateState({
      isActive: connection.isActive,
      identity: connection.identity,
      token: connection.token,
      connectionId: connection.connectionId,
      connectionError: undefined,
    });

    managed.onConnect = conn => {
      updateState({
        isActive: conn.isActive,
        identity: conn.identity,
        token: conn.token,
        connectionId: conn.connectionId,
        connectionError: undefined,
      });
    };

    managed.onDisconnect = (ctx, error) => {
      updateState({
        isActive: ctx.isActive,
        connectionError: error ?? undefined,
      });
    };

    managed.onConnectError = (ctx, error) => {
      updateState({
        isActive: ctx.isActive,
        connectionError: error,
      });
    };

    builder.onConnect(managed.onConnect);
    builder.onDisconnect(managed.onDisconnect);
    builder.onConnectError(managed.onConnectError);

    return connection as T;
  }

  release(key: string): void {
    const managed = this.#connections.get(key);
    if (!managed) {
      return;
    }

    managed.refCount -= 1;
    if (managed.refCount > 0 || managed.pendingRelease) {
      return;
    }

    managed.pendingRelease = setTimeout(() => {
      managed.pendingRelease = null;
      if (managed.refCount > 0) {
        return;
      }
      if (managed.connection) {
        if (managed.onConnect) {
          managed.connection.removeOnConnect(managed.onConnect as any);
        }
        if (managed.onDisconnect) {
          managed.connection.removeOnDisconnect(managed.onDisconnect as any);
        }
        if (managed.onConnectError) {
          managed.connection.removeOnConnectError(managed.onConnectError as any);
        }
        managed.connection.disconnect();
      }
      this.#connections.delete(key);
    }, 0);
  }

  subscribe(key: string, listener: Listener): () => void {
    const managed = this.#ensureEntry(key);
    managed.listeners.add(listener);
    return () => {
      managed.listeners.delete(listener);
      if (
        managed.refCount <= 0 &&
        managed.listeners.size === 0 &&
        !managed.connection
      ) {
        this.#connections.delete(key);
      }
    };
  }

  getSnapshot(key: string): ConnectionState | undefined {
    return this.#connections.get(key)?.state;
  }

  getConnection<T extends DbConnectionImpl<any>>(key: string): T | null {
    return (this.#connections.get(key)?.connection as T | undefined) ?? null;
  }
}

export const ConnectionManager = new ConnectionManagerImpl();
