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

export const CONNECTION_MANAGER_RECONNECT_BASE_DELAY_MS = 1000;
export const CONNECTION_MANAGER_RECONNECT_MAX_DELAY_MS = 30_000;

/**
 * Options controlling the auto-reconnect backoff. Set them per connection via
 * {@link DbConnectionBuilder.withReconnectOptions}. Any field left undefined
 * falls back to the module default.
 */
export type ReconnectOptions = {
  /**
   * Delay before the first reconnect attempt, in milliseconds. This is the
   * minimum backoff; each subsequent attempt doubles it up to `maxDelayMs`.
   * Defaults to {@link CONNECTION_MANAGER_RECONNECT_BASE_DELAY_MS} (1000).
   */
  baseDelayMs?: number;
  /**
   * Maximum delay between reconnect attempts, in milliseconds. The exponential
   * backoff is capped here. Defaults to
   * {@link CONNECTION_MANAGER_RECONNECT_MAX_DELAY_MS} (30000).
   */
  maxDelayMs?: number;
};

/**
 * Computes the reconnect delay for the given attempt (0-based) using
 * exponential backoff: the base delay doubles with each consecutive failed
 * attempt, capped at the maximum delay. `options` overrides the base and/or
 * maximum delay; unset fields use the module defaults.
 */
export function connectionManagerReconnectDelayMs(
  attempt: number,
  options?: ReconnectOptions
): number {
  const baseDelay =
    options?.baseDelayMs ?? CONNECTION_MANAGER_RECONNECT_BASE_DELAY_MS;
  const maxDelay =
    options?.maxDelayMs ?? CONNECTION_MANAGER_RECONNECT_MAX_DELAY_MS;
  return Math.min(baseDelay * 2 ** attempt, maxDelay);
}

type ManagedConnection = {
  connection?: DbConnectionImpl<any>;
  builder?: DbConnectionBuilder<any>;
  refCount: number;
  state: ConnectionState;
  listeners: Set<Listener>;
  pendingRelease: ReturnType<typeof setTimeout> | null;
  reconnectTimer: ReturnType<typeof setTimeout> | null;
  reconnectAttempt: number;
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
      builder: undefined,
      refCount: 0,
      state: defaultState(),
      listeners: new Set(),
      pendingRelease: null,
      reconnectTimer: null,
      reconnectAttempt: 0,
    };
    this.#connections.set(key, managed);
    return managed;
  }

  #notify(managed: ManagedConnection): void {
    for (const listener of managed.listeners) {
      listener();
    }
  }

  #updateState(
    managed: ManagedConnection,
    updates: Partial<ConnectionState>
  ): void {
    managed.state = { ...managed.state, ...updates };
    this.#notify(managed);
  }

  #ensureCallbacks(managed: ManagedConnection): void {
    if (managed.onConnect) {
      return;
    }

    managed.onConnect = conn => {
      if (conn !== managed.connection) {
        return;
      }
      managed.reconnectAttempt = 0;
      this.#updateState(managed, {
        isActive: conn.isActive,
        identity: conn.identity,
        token: conn.token,
        connectionId: conn.connectionId,
        connectionError: undefined,
      });
    };

    managed.onDisconnect = (ctx, error) => {
      if (ctx !== managed.connection) {
        return;
      }
      this.#updateState(managed, {
        isActive: false,
        connectionError: error ?? undefined,
      });
      this.#scheduleReconnect(managed);
    };

    managed.onConnectError = (ctx, error) => {
      if (ctx !== managed.connection) {
        return;
      }
      this.#updateState(managed, {
        isActive: false,
        connectionError: error,
      });
      this.#scheduleReconnect(managed);
    };
  }

  #attachCallbacks<T extends DbConnectionImpl<any>>(
    managed: ManagedConnection,
    builder: DbConnectionBuilder<T>
  ): void {
    this.#ensureCallbacks(managed);
    builder.onConnect(managed.onConnect!);
    builder.onDisconnect(managed.onDisconnect!);
    builder.onConnectError(managed.onConnectError!);
  }

  #detachCallbacks(
    managed: ManagedConnection,
    connection: DbConnectionImpl<any>
  ): void {
    if (managed.onConnect) {
      connection.removeOnConnect(managed.onConnect as any);
    }
    if (managed.onDisconnect) {
      connection.removeOnDisconnect(managed.onDisconnect as any);
    }
    if (managed.onConnectError) {
      connection.removeOnConnectError(managed.onConnectError as any);
    }
  }

  #buildManagedConnection<T extends DbConnectionImpl<any>>(
    managed: ManagedConnection,
    builder: DbConnectionBuilder<T>
  ): T {
    managed.builder = builder;
    const connection = builder.build();
    managed.connection = connection;
    this.#attachCallbacks(managed, builder);

    this.#updateState(managed, {
      isActive: connection.isActive,
      identity: connection.identity,
      token: connection.token,
      connectionId: connection.connectionId,
      connectionError: undefined,
    });

    return connection as T;
  }

  #scheduleReconnect(managed: ManagedConnection): void {
    if (
      managed.refCount <= 0 ||
      managed.pendingRelease ||
      managed.reconnectTimer ||
      !managed.builder
    ) {
      return;
    }

    const connection = managed.connection;
    if (connection) {
      this.#detachCallbacks(managed, connection);
    }
    managed.connection = undefined;

    // The application asked this connection to close; don't fight it. A
    // subsequent retain() will still build a fresh connection.
    if (connection?.isDisconnectRequested) {
      return;
    }

    const delay = connectionManagerReconnectDelayMs(
      managed.reconnectAttempt,
      managed.builder?.getReconnectOptions()
    );
    managed.reconnectAttempt += 1;
    managed.reconnectTimer = setTimeout(() => {
      managed.reconnectTimer = null;
      if (
        managed.refCount <= 0 ||
        managed.pendingRelease ||
        managed.connection ||
        !managed.builder
      ) {
        return;
      }

      this.#buildManagedConnection(managed, managed.builder);
    }, delay);
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
    if (managed.reconnectTimer) {
      clearTimeout(managed.reconnectTimer);
      managed.reconnectTimer = null;
    }

    managed.refCount += 1;
    managed.builder = builder;

    if (managed.connection) {
      return managed.connection as T;
    }

    return this.#buildManagedConnection(managed, builder);
  }

  /**
   * Tears down the current connection and builds a fresh one from `builder`,
   * preserving the entry's ref count and listener set.
   *
   * `retain()` deliberately ignores the builder once a connection is live —
   * the right behaviour for ref-counting, but it blocks "reconnect with a
   * fresh token" flows (e.g. swapping an anonymous session for a signed-in one
   * after an auth change). `rebuild()` is the supported escape hatch: pass a
   * builder carrying the new token and the pool swaps the live connection
   * under the same subscribers, so framework hooks (`useTable`, `useReducer`,
   * …) re-bind to the new connection automatically.
   *
   * The old connection's callbacks are detached before it is closed, so its
   * disconnect event never leaks into pool state, and any pending auto-reconnect
   * is cancelled (the caller is driving the reconnect explicitly). Returns the
   * newly-built connection, or `null` if the key has no retained entry.
   *
   * @param key - Unique identifier for the connection (use getKey to generate)
   * @param builder - Fresh connection builder; its handlers are rewired into the pool
   */
  rebuild<T extends DbConnectionImpl<any>>(
    key: string,
    builder: DbConnectionBuilder<T>
  ): T | null {
    const managed = this.#connections.get(key);
    if (!managed || managed.refCount <= 0) {
      return null;
    }

    // The caller is taking over the connection lifecycle explicitly; cancel a
    // deferred release or a pending auto-reconnect so neither races the fresh
    // connection, and reset the backoff so the next unexpected drop starts over.
    if (managed.pendingRelease) {
      clearTimeout(managed.pendingRelease);
      managed.pendingRelease = null;
    }
    if (managed.reconnectTimer) {
      clearTimeout(managed.reconnectTimer);
      managed.reconnectTimer = null;
    }
    managed.reconnectAttempt = 0;

    const connection = managed.connection;
    if (connection) {
      this.#detachCallbacks(managed, connection);
      connection.disconnect();
    }
    managed.connection = undefined;

    try {
      return this.#buildManagedConnection(managed, builder) as T;
    } catch (error) {
      // The old connection is already torn down, so a failed rebuild would
      // otherwise leave the pool reporting a stale "live" connection. Surface
      // the failure into pool state (matching the onConnectError shape) so
      // subscribers see a disconnected/errored connection, then re-throw so
      // the caller can handle it.
      this.#updateState(managed, {
        isActive: false,
        connectionError:
          error instanceof Error ? error : new Error(String(error)),
      });
      throw error;
    }
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

    if (managed.reconnectTimer) {
      clearTimeout(managed.reconnectTimer);
      managed.reconnectTimer = null;
    }

    managed.pendingRelease = setTimeout(() => {
      managed.pendingRelease = null;
      if (managed.refCount > 0) {
        return;
      }
      const connection = managed.connection;
      managed.connection = undefined;
      if (connection) {
        this.#detachCallbacks(managed, connection);
        connection.disconnect();
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
