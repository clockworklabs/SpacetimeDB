import type {
  DbConnectionBuilder,
  DbConnectionImpl,
  ErrorContextInterface,
} from './db_connection_impl';
import type { Identity } from '../lib/identity';
import { ConnectionId } from '../lib/connection_id';

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

class ConnectionManagerImpl {
  #connections = new Map<string, ManagedConnection>();

  static getKey(uri: string, moduleName: string): string {
    return `${uri}::${moduleName}`;
  }

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
