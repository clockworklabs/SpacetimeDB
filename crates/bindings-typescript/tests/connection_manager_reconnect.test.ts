import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import { ConnectionId } from '../src';
import {
  CONNECTION_MANAGER_RECONNECT_MAX_DELAY_MS,
  connectionManagerReconnectDelayMs,
  ConnectionManager,
} from '../src/sdk/connection_manager.ts';

type ErrorContextInterface = {
  isActive: boolean;
};

class MockConnection {
  isActive = false;
  identity = undefined;
  token = undefined;
  connectionId = ConnectionId.random();
  disconnected = false;

  #onConnectCallbacks = new Set<(conn: MockConnection) => void>();
  #onDisconnectCallbacks = new Set<
    (ctx: ErrorContextInterface, error?: Error) => void
  >();
  #onConnectErrorCallbacks = new Set<
    (ctx: ErrorContextInterface, error: Error) => void
  >();

  disconnect(): void {
    if (this.disconnected) {
      return;
    }
    this.disconnected = true;
    this.isActive = false;
    for (const cb of this.#onDisconnectCallbacks) {
      cb(this as unknown as ErrorContextInterface);
    }
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

  callbackCounts(): {
    connect: number;
    disconnect: number;
    connectError: number;
  } {
    return {
      connect: this.#onConnectCallbacks.size,
      disconnect: this.#onDisconnectCallbacks.size,
      connectError: this.#onConnectErrorCallbacks.size,
    };
  }

  simulateConnect(): void {
    this.isActive = true;
    for (const cb of this.#onConnectCallbacks) {
      cb(this);
    }
  }

  simulateDisconnect(error?: Error): void {
    this.isActive = false;
    for (const cb of this.#onDisconnectCallbacks) {
      cb(this as unknown as ErrorContextInterface, error);
    }
  }

  simulateConnectError(error: Error): void {
    this.isActive = false;
    for (const cb of this.#onConnectErrorCallbacks) {
      cb(this as unknown as ErrorContextInterface, error);
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

class MockBuilder {
  buildCount = 0;
  connections: MockConnection[] = [];

  #onConnectCallbacks = new Set<(conn: MockConnection) => void>();
  #onDisconnectCallbacks = new Set<
    (ctx: ErrorContextInterface, error?: Error) => void
  >();
  #onConnectErrorCallbacks = new Set<
    (ctx: ErrorContextInterface, error: Error) => void
  >();

  build(): MockConnection {
    const connection = new MockConnection();
    this.buildCount += 1;
    this.connections.push(connection);

    for (const cb of this.#onConnectCallbacks) {
      connection.registerOnConnect(cb);
    }
    for (const cb of this.#onDisconnectCallbacks) {
      connection.registerOnDisconnect(cb);
    }
    for (const cb of this.#onConnectErrorCallbacks) {
      connection.registerOnConnectError(cb);
    }

    return connection;
  }

  onConnect(cb: (conn: MockConnection) => void): MockBuilder {
    this.#onConnectCallbacks.add(cb);
    for (const connection of this.connections) {
      connection.registerOnConnect(cb);
    }
    return this;
  }

  onDisconnect(
    cb: (ctx: ErrorContextInterface, error?: Error) => void
  ): MockBuilder {
    this.#onDisconnectCallbacks.add(cb);
    for (const connection of this.connections) {
      connection.registerOnDisconnect(cb);
    }
    return this;
  }

  onConnectError(
    cb: (ctx: ErrorContextInterface, error: Error) => void
  ): MockBuilder {
    this.#onConnectErrorCallbacks.add(cb);
    for (const connection of this.connections) {
      connection.registerOnConnectError(cb);
    }
    return this;
  }
}

let keyCounter = 0;

function nextKey(): string {
  keyCounter += 1;
  return `connection-manager-reconnect-${keyCounter}`;
}

function retainMock(key: string, builder: MockBuilder): MockConnection {
  return ConnectionManager.retain(
    key,
    builder as any
  ) as unknown as MockConnection;
}

describe('ConnectionManager retained reconnect behavior', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.runOnlyPendingTimers();
    vi.useRealTimers();
  });

  test('rebuilds a retained connection after disconnect', () => {
    const key = nextKey();
    const builder = new MockBuilder();

    const first = retainMock(key, builder);
    expect(builder.buildCount).toBe(1);

    first.simulateDisconnect();

    expect(ConnectionManager.getSnapshot(key)?.isActive).toBe(false);
    expect(ConnectionManager.getConnection(key)).toBeNull();

    vi.advanceTimersByTime(connectionManagerReconnectDelayMs(0) - 1);
    expect(builder.buildCount).toBe(1);

    vi.advanceTimersByTime(1);
    expect(builder.buildCount).toBe(2);

    const second = ConnectionManager.getConnection(
      key
    ) as unknown as MockConnection;
    expect(second).toBe(builder.connections[1]);
    expect(second).not.toBe(first);

    ConnectionManager.release(key);
  });

  test('rebuilds a retained connection after connectError', () => {
    const key = nextKey();
    const builder = new MockBuilder();
    const error = new Error('network unavailable');

    const first = retainMock(key, builder);
    first.simulateConnectError(error);

    expect(ConnectionManager.getSnapshot(key)?.isActive).toBe(false);
    expect(ConnectionManager.getSnapshot(key)?.connectionError).toBe(error);
    expect(ConnectionManager.getConnection(key)).toBeNull();

    vi.advanceTimersByTime(connectionManagerReconnectDelayMs(0));

    expect(builder.buildCount).toBe(2);
    expect(ConnectionManager.getSnapshot(key)?.connectionError).toBeUndefined();
    expect(ConnectionManager.getConnection(key)).toBe(builder.connections[1]);

    ConnectionManager.release(key);
  });

  test('same-key retain after disconnect returns a fresh connection immediately', () => {
    const key = nextKey();
    const builder = new MockBuilder();

    const first = retainMock(key, builder);
    first.simulateDisconnect();

    const second = retainMock(key, builder);

    expect(builder.buildCount).toBe(2);
    expect(second).not.toBe(first);
    expect(ConnectionManager.getConnection(key)).toBe(second);

    vi.advanceTimersByTime(connectionManagerReconnectDelayMs(0));
    expect(builder.buildCount).toBe(2);

    ConnectionManager.release(key);
    ConnectionManager.release(key);
  });

  test('reconnect uses callbacks from a replacement same-key builder', () => {
    const key = nextKey();
    const firstBuilder = new MockBuilder();
    const secondBuilder = new MockBuilder();

    const first = retainMock(key, firstBuilder);
    first.simulateConnect();

    ConnectionManager.release(key);
    const retained = retainMock(key, secondBuilder);

    expect(retained).toBe(first);
    expect(firstBuilder.buildCount).toBe(1);
    expect(secondBuilder.buildCount).toBe(0);

    first.simulateDisconnect();
    vi.advanceTimersByTime(connectionManagerReconnectDelayMs(0));

    expect(secondBuilder.buildCount).toBe(1);
    const second = secondBuilder.connections[0];
    expect(ConnectionManager.getConnection(key)).toBe(second);

    second.simulateConnect();

    expect(ConnectionManager.getSnapshot(key)?.isActive).toBe(true);
    expect(ConnectionManager.getSnapshot(key)?.connectionId).toBe(
      second.connectionId
    );

    ConnectionManager.release(key);
  });

  test('disconnect removes manager callbacks from the old connection before pending reconnect', () => {
    const key = nextKey();
    const builder = new MockBuilder();

    const first = retainMock(key, builder);
    expect(first.callbackCounts()).toEqual({
      connect: 1,
      disconnect: 1,
      connectError: 1,
    });

    first.simulateDisconnect();

    expect(first.callbackCounts()).toEqual({
      connect: 0,
      disconnect: 0,
      connectError: 0,
    });

    ConnectionManager.release(key);
  });

  test('release cancels a pending reconnect', () => {
    const key = nextKey();
    const builder = new MockBuilder();

    const first = retainMock(key, builder);
    first.simulateDisconnect();

    ConnectionManager.release(key);
    vi.advanceTimersByTime(connectionManagerReconnectDelayMs(0));

    expect(builder.buildCount).toBe(1);
    expect(ConnectionManager.getConnection(key)).toBeNull();
  });

  test('reconnect delay backs off exponentially across consecutive failures', () => {
    const key = nextKey();
    const builder = new MockBuilder();

    const first = retainMock(key, builder);
    first.simulateDisconnect();

    // First reconnect fires after the base delay.
    vi.advanceTimersByTime(connectionManagerReconnectDelayMs(0));
    expect(builder.buildCount).toBe(2);

    // Second failure: the delay doubles.
    builder.connections[1].simulateConnectError(new Error('still down'));
    vi.advanceTimersByTime(connectionManagerReconnectDelayMs(1) - 1);
    expect(builder.buildCount).toBe(2);
    vi.advanceTimersByTime(1);
    expect(builder.buildCount).toBe(3);

    // Third failure: the delay doubles again.
    builder.connections[2].simulateConnectError(new Error('still down'));
    vi.advanceTimersByTime(connectionManagerReconnectDelayMs(2) - 1);
    expect(builder.buildCount).toBe(3);
    vi.advanceTimersByTime(1);
    expect(builder.buildCount).toBe(4);

    ConnectionManager.release(key);
  });

  test('successful connect resets the reconnect backoff', () => {
    const key = nextKey();
    const builder = new MockBuilder();

    const first = retainMock(key, builder);
    first.simulateDisconnect();
    vi.advanceTimersByTime(connectionManagerReconnectDelayMs(0));

    builder.connections[1].simulateConnectError(new Error('still down'));
    vi.advanceTimersByTime(connectionManagerReconnectDelayMs(1));
    expect(builder.buildCount).toBe(3);

    // A successful connect resets the backoff to the base delay.
    builder.connections[2].simulateConnect();
    builder.connections[2].simulateDisconnect();

    vi.advanceTimersByTime(connectionManagerReconnectDelayMs(0));
    expect(builder.buildCount).toBe(4);

    ConnectionManager.release(key);
  });

  test('reconnect delay is capped at the maximum delay', () => {
    expect(connectionManagerReconnectDelayMs(0)).toBeLessThan(
      CONNECTION_MANAGER_RECONNECT_MAX_DELAY_MS
    );
    expect(connectionManagerReconnectDelayMs(100)).toBe(
      CONNECTION_MANAGER_RECONNECT_MAX_DELAY_MS
    );
  });
});
