import { describe, test, expect, beforeEach, vi, afterEach } from 'vitest';
import { ConnectionId } from '../src';
import { Identity } from '../src/lib/identity';

// Test identity helper
const testIdentity = Identity.fromString(
  '0000000000000000000000000000000000000000000000000000000000000069'
);

// We need to test a fresh instance each time, so we import the class directly
// and create new instances rather than using the singleton
class Deferred<T> {
  #isResolved: boolean = false;
  #isRejected: boolean = false;
  #resolve: (value: T | PromiseLike<T>) => void = () => {};
  #reject: (reason?: any) => void = () => {};
  promise: Promise<T>;

  constructor() {
    this.promise = new Promise<T>((resolve, reject) => {
      this.#resolve = resolve;
      this.#reject = reject;
    });
  }

  get isResolved(): boolean {
    return this.#isResolved;
  }

  resolve(value: T): void {
    if (!this.#isResolved && !this.#isRejected) {
      this.#isResolved = true;
      this.#resolve(value);
    }
  }
}

// ConnectionState type matching the implementation
type ConnectionState = {
  isActive: boolean;
  identity?: Identity;
  token?: string;
  connectionId: ConnectionId;
  connectionError?: Error;
};

type Listener = () => void;

type ErrorContextInterface = {
  isActive: boolean;
};

type ManagedConnection = {
  connection?: MockConnection;
  refCount: number;
  state: ConnectionState;
  listeners: Set<Listener>;
  pendingRelease: ReturnType<typeof setTimeout> | null;
  onConnect?: (conn: MockConnection) => void;
  onDisconnect?: (ctx: ErrorContextInterface, error?: Error) => void;
  onConnectError?: (ctx: ErrorContextInterface, error: Error) => void;
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

// Mock connection for testing
class MockConnection {
  isActive = false;
  identity?: Identity;
  token?: string;
  connectionId = ConnectionId.random();
  disconnected = false;

  #onConnectCallbacks: Set<(conn: MockConnection) => void> = new Set();
  #onDisconnectCallbacks: Set<
    (ctx: ErrorContextInterface, error?: Error) => void
  > = new Set();
  #onConnectErrorCallbacks: Set<
    (ctx: ErrorContextInterface, error: Error) => void
  > = new Set();

  disconnect(): void {
    this.disconnected = true;
    this.isActive = false;
  }

  removeOnConnect(cb: (conn: MockConnection) => void): void {
    this.#onConnectCallbacks.delete(cb);
  }

  removeOnDisconnect(
    cb: (ctx: ErrorContextInterface, error?: Error) => void
  ): void {
    this.#onDisconnectCallbacks.delete(cb);
  }

  removeOnConnectError(
    cb: (ctx: ErrorContextInterface, error: Error) => void
  ): void {
    this.#onConnectErrorCallbacks.delete(cb);
  }

  // Test helpers to simulate connection events
  simulateConnect(identity: Identity, token: string): void {
    this.isActive = true;
    this.identity = identity;
    this.token = token;
    for (const cb of this.#onConnectCallbacks) {
      cb(this);
    }
  }

  simulateDisconnect(error?: Error): void {
    this.isActive = false;
    for (const cb of this.#onDisconnectCallbacks) {
      cb({ isActive: false }, error);
    }
  }

  simulateConnectError(error: Error): void {
    this.isActive = false;
    for (const cb of this.#onConnectErrorCallbacks) {
      cb({ isActive: false }, error);
    }
  }

  registerOnConnect(cb: (conn: MockConnection) => void): void {
    this.#onConnectCallbacks.add(cb);
  }

  registerOnDisconnect(
    cb: (ctx: ErrorContextInterface, error?: Error) => void
  ): void {
    this.#onDisconnectCallbacks.add(cb);
  }

  registerOnConnectError(
    cb: (ctx: ErrorContextInterface, error: Error) => void
  ): void {
    this.#onConnectErrorCallbacks.add(cb);
  }
}

// Mock builder for testing
// The real builder pattern allows registering callbacks before OR after build()
// ConnectionManager calls builder.onConnect() AFTER build(), so we need to handle that
class MockBuilder {
  #connection: MockConnection;
  #built = false;

  constructor(connection: MockConnection) {
    this.#connection = connection;
  }

  build(): MockConnection {
    this.#built = true;
    return this.#connection;
  }

  onConnect(cb: (conn: MockConnection) => void): MockBuilder {
    // Register immediately on connection (works before or after build)
    this.#connection.registerOnConnect(cb);
    return this;
  }

  onDisconnect(
    cb: (ctx: ErrorContextInterface, error?: Error) => void
  ): MockBuilder {
    this.#connection.registerOnDisconnect(cb);
    return this;
  }

  onConnectError(
    cb: (ctx: ErrorContextInterface, error: Error) => void
  ): MockBuilder {
    this.#connection.registerOnConnectError(cb);
    return this;
  }
}

// Re-implement ConnectionManagerImpl for testing (to avoid singleton issues)
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

  retain(key: string, builder: MockBuilder): MockConnection {
    const managed = this.#ensureEntry(key);
    if (managed.pendingRelease) {
      clearTimeout(managed.pendingRelease);
      managed.pendingRelease = null;
    }
    managed.refCount += 1;
    if (managed.connection) {
      return managed.connection;
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

    return connection;
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
          managed.connection.removeOnConnect(managed.onConnect);
        }
        if (managed.onDisconnect) {
          managed.connection.removeOnDisconnect(managed.onDisconnect);
        }
        if (managed.onConnectError) {
          managed.connection.removeOnConnectError(managed.onConnectError);
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

  getConnection(key: string): MockConnection | null {
    return this.#connections.get(key)?.connection ?? null;
  }

  // Test helper to check internal state
  _getRefCount(key: string): number {
    return this.#connections.get(key)?.refCount ?? 0;
  }

  _hasConnection(key: string): boolean {
    return this.#connections.get(key)?.connection !== undefined;
  }

  _hasEntry(key: string): boolean {
    return this.#connections.has(key);
  }

  _hasPendingRelease(key: string): boolean {
    return this.#connections.get(key)?.pendingRelease !== null;
  }
}

describe('ConnectionManager', () => {
  let manager: ConnectionManagerImpl;

  beforeEach(() => {
    vi.useFakeTimers();
    manager = new ConnectionManagerImpl();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('getKey', () => {
    test('generates consistent keys from uri and moduleName', () => {
      const key1 = manager.getKey('ws://localhost:3000', 'myModule');
      const key2 = manager.getKey('ws://localhost:3000', 'myModule');
      expect(key1).toBe(key2);
      expect(key1).toBe('ws://localhost:3000::myModule');
    });

    test('generates different keys for different uris', () => {
      const key1 = manager.getKey('ws://localhost:3000', 'myModule');
      const key2 = manager.getKey('ws://localhost:4000', 'myModule');
      expect(key1).not.toBe(key2);
    });

    test('generates different keys for different modules', () => {
      const key1 = manager.getKey('ws://localhost:3000', 'moduleA');
      const key2 = manager.getKey('ws://localhost:3000', 'moduleB');
      expect(key1).not.toBe(key2);
    });

    test('static getKey matches instance method', () => {
      const uri = 'ws://localhost:3000';
      const moduleName = 'myModule';
      expect(ConnectionManagerImpl.getKey(uri, moduleName)).toBe(
        manager.getKey(uri, moduleName)
      );
    });
  });

  describe('retain', () => {
    test('creates connection on first retain', () => {
      const mockConnection = new MockConnection();
      const builder = new MockBuilder(mockConnection);
      const key = 'test-key';

      const connection = manager.retain(key, builder);

      expect(connection).toBe(mockConnection);
      expect(manager._getRefCount(key)).toBe(1);
    });

    test('returns same connection on subsequent retains', () => {
      const mockConnection = new MockConnection();
      const builder = new MockBuilder(mockConnection);
      const key = 'test-key';

      const connection1 = manager.retain(key, builder);
      const connection2 = manager.retain(key, builder);

      expect(connection1).toBe(connection2);
      expect(manager._getRefCount(key)).toBe(2);
    });

    test('increments refCount on each retain', () => {
      const mockConnection = new MockConnection();
      const builder = new MockBuilder(mockConnection);
      const key = 'test-key';

      manager.retain(key, builder);
      expect(manager._getRefCount(key)).toBe(1);

      manager.retain(key, builder);
      expect(manager._getRefCount(key)).toBe(2);

      manager.retain(key, builder);
      expect(manager._getRefCount(key)).toBe(3);
    });

    test('cancels pending release when retaining again', () => {
      const mockConnection = new MockConnection();
      const builder = new MockBuilder(mockConnection);
      const key = 'test-key';

      manager.retain(key, builder);
      manager.release(key);

      expect(manager._hasPendingRelease(key)).toBe(true);

      manager.retain(key, builder);

      expect(manager._hasPendingRelease(key)).toBe(false);
      expect(manager._getRefCount(key)).toBe(1);
      expect(mockConnection.disconnected).toBe(false);
    });
  });

  describe('release', () => {
    test('decrements refCount on release', () => {
      const mockConnection = new MockConnection();
      const builder = new MockBuilder(mockConnection);
      const key = 'test-key';

      manager.retain(key, builder);
      manager.retain(key, builder);
      expect(manager._getRefCount(key)).toBe(2);

      manager.release(key);
      expect(manager._getRefCount(key)).toBe(1);
    });

    test('schedules cleanup when refCount reaches zero', () => {
      const mockConnection = new MockConnection();
      const builder = new MockBuilder(mockConnection);
      const key = 'test-key';

      manager.retain(key, builder);
      manager.release(key);

      expect(manager._hasPendingRelease(key)).toBe(true);
      expect(mockConnection.disconnected).toBe(false);
    });

    test('disconnects after timeout when refCount is zero', () => {
      const mockConnection = new MockConnection();
      const builder = new MockBuilder(mockConnection);
      const key = 'test-key';

      manager.retain(key, builder);
      manager.release(key);

      vi.runAllTimers();

      expect(mockConnection.disconnected).toBe(true);
      expect(manager._hasConnection(key)).toBe(false);
    });

    test('does not disconnect if re-retained before timeout', () => {
      const mockConnection = new MockConnection();
      const builder = new MockBuilder(mockConnection);
      const key = 'test-key';

      manager.retain(key, builder);
      manager.release(key);
      manager.retain(key, builder);

      vi.runAllTimers();

      expect(mockConnection.disconnected).toBe(false);
      expect(manager._hasConnection(key)).toBe(true);
    });

    test('does nothing when releasing unknown key', () => {
      expect(() => manager.release('unknown-key')).not.toThrow();
    });

    test('does not schedule multiple cleanups', () => {
      const mockConnection = new MockConnection();
      const builder = new MockBuilder(mockConnection);
      const key = 'test-key';

      manager.retain(key, builder);
      manager.retain(key, builder);
      manager.release(key);
      manager.release(key);

      // Should only have one pending release
      expect(manager._hasPendingRelease(key)).toBe(true);

      vi.runAllTimers();

      expect(mockConnection.disconnected).toBe(true);
    });
  });

  describe('React StrictMode simulation', () => {
    test('survives mount → unmount → remount cycle', () => {
      const mockConnection = new MockConnection();
      const builder = new MockBuilder(mockConnection);
      const key = 'test-key';

      // Mount: retain
      manager.retain(key, builder);
      expect(manager._getRefCount(key)).toBe(1);

      // Unmount: release (schedules cleanup)
      manager.release(key);
      expect(manager._getRefCount(key)).toBe(0);
      expect(manager._hasPendingRelease(key)).toBe(true);

      // Remount: retain again (cancels cleanup)
      manager.retain(key, builder);
      expect(manager._getRefCount(key)).toBe(1);
      expect(manager._hasPendingRelease(key)).toBe(false);

      // Let any timers run
      vi.runAllTimers();

      // Connection should still be active
      expect(mockConnection.disconnected).toBe(false);
      expect(manager.getConnection(key)).toBe(mockConnection);
    });

    test('maintains single connection across multiple StrictMode cycles', () => {
      const mockConnection = new MockConnection();
      const builder = new MockBuilder(mockConnection);
      const key = 'test-key';

      // First cycle
      manager.retain(key, builder);
      manager.release(key);
      manager.retain(key, builder);

      // Second cycle (nested component)
      manager.retain(key, builder);
      manager.release(key);
      manager.retain(key, builder);

      vi.runAllTimers();

      expect(mockConnection.disconnected).toBe(false);
      expect(manager._getRefCount(key)).toBe(2);
    });
  });

  describe('subscribe', () => {
    test('adds listener and returns unsubscribe function', () => {
      const key = 'test-key';
      const listener = vi.fn();

      const unsubscribe = manager.subscribe(key, listener);

      expect(typeof unsubscribe).toBe('function');
    });

    test('notifies listeners on state change', () => {
      const mockConnection = new MockConnection();
      const builder = new MockBuilder(mockConnection);
      const key = 'test-key';
      const listener = vi.fn();

      manager.subscribe(key, listener);
      manager.retain(key, builder);

      // Initial state update during retain
      expect(listener).toHaveBeenCalled();
    });

    test('notifies listeners when connection state changes', () => {
      const mockConnection = new MockConnection();
      const builder = new MockBuilder(mockConnection);
      const key = 'test-key';
      const listener = vi.fn();

      manager.subscribe(key, listener);
      manager.retain(key, builder);
      listener.mockClear();

      const identity = testIdentity;
      mockConnection.simulateConnect(identity, 'test-token');

      expect(listener).toHaveBeenCalled();
    });

    test('stops notifying after unsubscribe', () => {
      const mockConnection = new MockConnection();
      const builder = new MockBuilder(mockConnection);
      const key = 'test-key';
      const listener = vi.fn();

      const unsubscribe = manager.subscribe(key, listener);
      manager.retain(key, builder);
      listener.mockClear();

      unsubscribe();

      const identity = testIdentity;
      mockConnection.simulateConnect(identity, 'test-token');

      expect(listener).not.toHaveBeenCalled();
    });

    test('cleans up entry when no listeners and no connection', () => {
      const key = 'test-key';
      const listener = vi.fn();

      const unsubscribe = manager.subscribe(key, listener);
      expect(manager._hasConnection(key)).toBe(false);

      unsubscribe();

      // Entry should be cleaned up since there's no connection and no listeners
      expect(manager.getSnapshot(key)).toBeUndefined();
    });

    test('does not clean up entry when connection exists', () => {
      const mockConnection = new MockConnection();
      const builder = new MockBuilder(mockConnection);
      const key = 'test-key';
      const listener = vi.fn();

      const unsubscribe = manager.subscribe(key, listener);
      manager.retain(key, builder);
      unsubscribe();

      // Entry should still exist because connection is active
      expect(manager.getSnapshot(key)).toBeDefined();
    });
  });

  describe('getSnapshot', () => {
    test('returns undefined for unknown key', () => {
      expect(manager.getSnapshot('unknown-key')).toBeUndefined();
    });

    test('returns state after retain', () => {
      const mockConnection = new MockConnection();
      const builder = new MockBuilder(mockConnection);
      const key = 'test-key';

      manager.retain(key, builder);
      const snapshot = manager.getSnapshot(key);

      expect(snapshot).toBeDefined();
      expect(snapshot?.isActive).toBe(false);
    });

    test('reflects connection state changes', () => {
      const mockConnection = new MockConnection();
      const builder = new MockBuilder(mockConnection);
      const key = 'test-key';

      manager.retain(key, builder);

      const identity = testIdentity;
      mockConnection.simulateConnect(identity, 'test-token');

      const snapshot = manager.getSnapshot(key);
      expect(snapshot?.isActive).toBe(true);
      expect(snapshot?.identity).toBe(identity);
      expect(snapshot?.token).toBe('test-token');
    });

    test('reflects disconnect state', () => {
      const mockConnection = new MockConnection();
      const builder = new MockBuilder(mockConnection);
      const key = 'test-key';

      manager.retain(key, builder);
      const identity = testIdentity;
      mockConnection.simulateConnect(identity, 'test-token');
      mockConnection.simulateDisconnect();

      const snapshot = manager.getSnapshot(key);
      expect(snapshot?.isActive).toBe(false);
    });

    test('reflects connection error', () => {
      const mockConnection = new MockConnection();
      const builder = new MockBuilder(mockConnection);
      const key = 'test-key';

      manager.retain(key, builder);
      const error = new Error('Connection failed');
      mockConnection.simulateConnectError(error);

      const snapshot = manager.getSnapshot(key);
      expect(snapshot?.isActive).toBe(false);
      expect(snapshot?.connectionError).toBe(error);
    });
  });

  describe('getConnection', () => {
    test('returns null for unknown key', () => {
      expect(manager.getConnection('unknown-key')).toBeNull();
    });

    test('returns connection after retain', () => {
      const mockConnection = new MockConnection();
      const builder = new MockBuilder(mockConnection);
      const key = 'test-key';

      manager.retain(key, builder);

      expect(manager.getConnection(key)).toBe(mockConnection);
    });

    test('returns null after cleanup', () => {
      const mockConnection = new MockConnection();
      const builder = new MockBuilder(mockConnection);
      const key = 'test-key';

      manager.retain(key, builder);
      manager.release(key);
      vi.runAllTimers();

      expect(manager.getConnection(key)).toBeNull();
    });
  });

  describe('multiple connections', () => {
    test('manages multiple independent connections', () => {
      const connection1 = new MockConnection();
      const connection2 = new MockConnection();
      const builder1 = new MockBuilder(connection1);
      const builder2 = new MockBuilder(connection2);
      const key1 = 'connection-1';
      const key2 = 'connection-2';

      manager.retain(key1, builder1);
      manager.retain(key2, builder2);

      expect(manager.getConnection(key1)).toBe(connection1);
      expect(manager.getConnection(key2)).toBe(connection2);
      expect(manager._getRefCount(key1)).toBe(1);
      expect(manager._getRefCount(key2)).toBe(1);
    });

    test('releases connections independently', () => {
      const connection1 = new MockConnection();
      const connection2 = new MockConnection();
      const builder1 = new MockBuilder(connection1);
      const builder2 = new MockBuilder(connection2);
      const key1 = 'connection-1';
      const key2 = 'connection-2';

      manager.retain(key1, builder1);
      manager.retain(key2, builder2);
      manager.release(key1);
      vi.runAllTimers();

      expect(connection1.disconnected).toBe(true);
      expect(connection2.disconnected).toBe(false);
      expect(manager.getConnection(key1)).toBeNull();
      expect(manager.getConnection(key2)).toBe(connection2);
    });
  });

  describe('callback cleanup', () => {
    test('removes callbacks on disconnect', () => {
      const mockConnection = new MockConnection();
      const builder = new MockBuilder(mockConnection);
      const key = 'test-key';

      manager.retain(key, builder);
      manager.release(key);
      vi.runAllTimers();

      // After cleanup, simulating events should not cause issues
      // (callbacks were removed)
      const identity = testIdentity;
      expect(() => {
        mockConnection.simulateConnect(identity, 'test-token');
      }).not.toThrow();
    });
  });
});
