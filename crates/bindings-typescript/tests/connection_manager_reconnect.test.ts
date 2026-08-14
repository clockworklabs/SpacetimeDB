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
  // A real DbConnectionImpl is constructed with the builder's token and keeps
  // it in this field, so the mock takes it the same way.
  token: string | undefined;
  connectionId = ConnectionId.random();
  disconnected = false;
  isDisconnectRequested = false;

  constructor(token?: string) {
    this.token = token;
  }

  #onConnectCallbacks = new Set<(conn: MockConnection) => void>();
  #onDisconnectCallbacks = new Set<
    (ctx: ErrorContextInterface, error?: Error) => void
  >();
  #onConnectErrorCallbacks = new Set<
    (ctx: ErrorContextInterface, error: Error) => void
  >();

  disconnect(): void {
    this.isDisconnectRequested = true;
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

  /**
   * @param issuedToken - the token the server hands back on connect. Passing
   * one emulates a first-time client being issued credentials it did not have
   * when the builder was constructed.
   */
  simulateConnect(issuedToken?: string): void {
    this.isActive = true;
    if (issuedToken !== undefined) {
      this.token = issuedToken;
    }
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
  /** The token each `build()` will stamp onto its connection. */
  token: string | undefined;
  /** Every token this builder was asked to carry, oldest first. */
  tokenHistory: (string | undefined)[] = [];

  constructor(token?: string) {
    this.token = token;
  }

  #onConnectCallbacks = new Set<(conn: MockConnection) => void>();
  #onDisconnectCallbacks = new Set<
    (ctx: ErrorContextInterface, error?: Error) => void
  >();
  #onConnectErrorCallbacks = new Set<
    (ctx: ErrorContextInterface, error: Error) => void
  >();

  withToken(token?: string): MockBuilder {
    this.token = token;
    this.tokenHistory.push(token);
    return this;
  }

  build(): MockConnection {
    const connection = new MockConnection(this.token);
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

  test('manual disconnect does not trigger a reconnect', () => {
    const key = nextKey();
    const builder = new MockBuilder();

    const first = retainMock(key, builder);
    first.simulateConnect();

    first.disconnect();

    expect(ConnectionManager.getSnapshot(key)?.isActive).toBe(false);
    expect(ConnectionManager.getConnection(key)).toBeNull();

    vi.advanceTimersByTime(CONNECTION_MANAGER_RECONNECT_MAX_DELAY_MS);
    expect(builder.buildCount).toBe(1);

    ConnectionManager.release(key);
  });

  test('retain after a manual disconnect builds a fresh connection', () => {
    const key = nextKey();
    const builder = new MockBuilder();

    const first = retainMock(key, builder);
    first.simulateConnect();
    first.disconnect();

    const second = retainMock(key, builder);

    expect(builder.buildCount).toBe(2);
    expect(second).not.toBe(first);
    expect(ConnectionManager.getConnection(key)).toBe(second);

    ConnectionManager.release(key);
    ConnectionManager.release(key);
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

describe('ConnectionManager.rebuild', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.runOnlyPendingTimers();
    vi.useRealTimers();
  });

  test('swaps the live connection for one built from a fresh builder', () => {
    const key = nextKey();
    const firstBuilder = new MockBuilder();
    const secondBuilder = new MockBuilder();

    const first = retainMock(key, firstBuilder);
    first.simulateConnect();
    expect(ConnectionManager.getSnapshot(key)?.isActive).toBe(true);

    const second = ConnectionManager.rebuild(
      key,
      secondBuilder as any
    ) as unknown as MockConnection;

    // The new connection comes from the replacement builder...
    expect(secondBuilder.buildCount).toBe(1);
    expect(second).toBe(secondBuilder.connections[0]);
    expect(second).not.toBe(first);
    expect(ConnectionManager.getConnection(key)).toBe(second);
    // ...and the old one is torn down.
    expect(first.disconnected).toBe(true);

    second.simulateConnect();
    expect(ConnectionManager.getSnapshot(key)?.isActive).toBe(true);
    expect(ConnectionManager.getSnapshot(key)?.connectionId).toBe(
      second.connectionId
    );

    ConnectionManager.release(key);
  });

  test('preserves the ref count so a single release still tears down', () => {
    const key = nextKey();
    const firstBuilder = new MockBuilder();
    const secondBuilder = new MockBuilder();

    retainMock(key, firstBuilder);
    retainMock(key, secondBuilder); // refCount: 2

    const rebuilt = ConnectionManager.rebuild(
      key,
      new MockBuilder() as any
    ) as unknown as MockConnection;
    expect(rebuilt).not.toBeNull();

    // refCount was 2 and is untouched: one release leaves the entry live.
    ConnectionManager.release(key);
    vi.advanceTimersByTime(0);
    expect(ConnectionManager.getConnection(key)).toBe(rebuilt);

    ConnectionManager.release(key);
    vi.advanceTimersByTime(0);
    expect(ConnectionManager.getConnection(key)).toBeNull();
  });

  test('detaches the old connection callbacks before closing it', () => {
    const key = nextKey();
    const first = retainMock(key, new MockBuilder());
    expect(first.callbackCounts()).toEqual({
      connect: 1,
      disconnect: 1,
      connectError: 1,
    });

    ConnectionManager.rebuild(key, new MockBuilder() as any);

    expect(first.callbackCounts()).toEqual({
      connect: 0,
      disconnect: 0,
      connectError: 0,
    });

    ConnectionManager.release(key);
  });

  test('cancels a pending auto-reconnect and resets the backoff', () => {
    const key = nextKey();
    const builder = new MockBuilder();

    const first = retainMock(key, builder);
    // Two consecutive failures so the backoff has advanced past the base delay.
    first.simulateDisconnect();
    vi.advanceTimersByTime(connectionManagerReconnectDelayMs(0));
    builder.connections[1].simulateConnectError(new Error('still down'));
    expect(builder.buildCount).toBe(2);

    // rebuild() takes over: the scheduled reconnect must not also fire.
    const replacement = new MockBuilder();
    ConnectionManager.rebuild(key, replacement as any);
    expect(replacement.buildCount).toBe(1);

    vi.advanceTimersByTime(CONNECTION_MANAGER_RECONNECT_MAX_DELAY_MS);
    // No stale timer rebuilt the old builder...
    expect(builder.buildCount).toBe(2);
    expect(replacement.buildCount).toBe(1);

    // ...and the backoff was reset: a fresh drop reconnects after the base delay.
    replacement.connections[0].simulateConnect();
    replacement.connections[0].simulateDisconnect();
    vi.advanceTimersByTime(connectionManagerReconnectDelayMs(0));
    expect(replacement.buildCount).toBe(2);

    ConnectionManager.release(key);
  });

  test('surfaces a build failure into pool state and re-throws', () => {
    const key = nextKey();
    const builder = new MockBuilder();
    const buildError = new Error('build failed');

    const first = retainMock(key, builder);
    first.simulateConnect();
    expect(ConnectionManager.getSnapshot(key)?.isActive).toBe(true);

    // A builder whose build() throws (e.g. the new token is rejected synchronously).
    const failingBuilder = {
      build() {
        throw buildError;
      },
      onConnect() {
        return this;
      },
      onDisconnect() {
        return this;
      },
      onConnectError() {
        return this;
      },
    };

    expect(() => ConnectionManager.rebuild(key, failingBuilder as any)).toThrow(
      buildError
    );

    // The old connection is gone and the pool reflects the failure rather than
    // a stale "live" connection.
    expect(first.disconnected).toBe(true);
    expect(ConnectionManager.getConnection(key)).toBeNull();
    expect(ConnectionManager.getSnapshot(key)?.isActive).toBe(false);
    expect(ConnectionManager.getSnapshot(key)?.connectionError).toBe(
      buildError
    );

    ConnectionManager.release(key);
  });

  test('returns null when the key has no retained entry', () => {
    const key = nextKey();
    expect(ConnectionManager.rebuild(key, new MockBuilder() as any)).toBeNull();
  });

  test('returns null after the entry has been fully released', () => {
    const key = nextKey();
    const first = retainMock(key, new MockBuilder());
    first.simulateConnect();
    ConnectionManager.release(key);
    vi.advanceTimersByTime(0); // let the deferred release run

    expect(ConnectionManager.rebuild(key, new MockBuilder() as any)).toBeNull();
  });
});

describe('ConnectionManager session continuity across rebuilds', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.runOnlyPendingTimers();
    vi.useRealTimers();
  });

  test('auto-reconnect reuses the token issued after the builder was built', () => {
    const key = nextKey();
    // A first-time visitor: nothing in storage, so the builder carries no token.
    const builder = new MockBuilder();

    const first = retainMock(key, builder);
    expect(first.token).toBeUndefined();

    // The server issues credentials on connect. From here on, this token *is*
    // the player's identity.
    first.simulateConnect('session-token');
    expect(ConnectionManager.getSnapshot(key)?.token).toBe('session-token');

    // The socket drops and the manager auto-reconnects from the retained
    // builder — which still holds the empty token it was constructed with.
    first.simulateDisconnect();
    vi.advanceTimersByTime(connectionManagerReconnectDelayMs(0));

    const second = builder.connections[1];
    expect(builder.buildCount).toBe(2);
    // Without the session token this reconnects anonymously and the server
    // mints a brand-new identity, silently signing the user out.
    expect(second.token).toBe('session-token');
  });

  test('resumed session survives repeated reconnects', () => {
    const key = nextKey();
    const builder = new MockBuilder();

    retainMock(key, builder).simulateConnect('session-token');

    for (let attempt = 0; attempt < 3; attempt++) {
      const current = builder.connections[builder.buildCount - 1];
      current.simulateDisconnect();
      vi.advanceTimersByTime(connectionManagerReconnectDelayMs(attempt));
      const rebuilt = builder.connections[builder.buildCount - 1];
      expect(rebuilt.token).toBe('session-token');
      rebuilt.simulateConnect('session-token');
    }

    ConnectionManager.release(key);
  });

  // Precedence rule: whatever the live session settled on outranks the token
  // the builder was constructed with. Today a real client only adopts a
  // server-issued token when it presented none (see the `!this.token` guard in
  // `DbConnectionImpl#processServerMessage`), so the rotation below is
  // hypothetical — it pins the rule for any future support for rotation.
  test("the session token takes precedence over the builder's own token", () => {
    const key = nextKey();
    const builder = new MockBuilder('original-token');

    const first = retainMock(key, builder);
    // Hypothetical: the server hands back a different credential on connect.
    first.simulateConnect('rotated-token');

    first.simulateDisconnect();
    vi.advanceTimersByTime(connectionManagerReconnectDelayMs(0));

    expect(builder.connections[1].token).toBe('rotated-token');

    ConnectionManager.release(key);
  });

  test('leaves the builder alone when no session has been established', () => {
    const key = nextKey();
    const builder = new MockBuilder('stored-token');

    // Dropped before the server ever answered: there is no session to resume,
    // so the builder's own token must survive untouched.
    const first = retainMock(key, builder);
    first.simulateConnectError(new Error('server unreachable'));
    vi.advanceTimersByTime(connectionManagerReconnectDelayMs(0));

    expect(builder.connections[1].token).toBe('stored-token');

    ConnectionManager.release(key);
  });

  test('retain after a drop resumes the session rather than the stale builder', () => {
    const key = nextKey();
    const builder = new MockBuilder();

    const first = retainMock(key, builder);
    first.simulateConnect('session-token');
    first.simulateDisconnect();

    // A provider remount rebuilds through retain(), not the reconnect timer.
    const second = retainMock(key, builder);
    expect(second.token).toBe('session-token');

    ConnectionManager.release(key);
    ConnectionManager.release(key);
  });

  // A *replacement* builder arriving through retain() does not change identity,
  // even when it carries a different token. rebuild() is the supported way to
  // do that; retain() already ignores the builder outright whenever a
  // connection is live, so honouring it only in the disconnected window would
  // make identity depend on socket timing.
  test('retain() with a replacement builder keeps the session identity', () => {
    const key = nextKey();
    const anonymous = new MockBuilder();

    const first = retainMock(key, anonymous);
    first.simulateConnect('anonymous-token');
    first.simulateDisconnect();

    const signedIn = new MockBuilder('signed-in-token');
    const second = retainMock(key, signedIn);
    expect(second.token).toBe('anonymous-token');

    ConnectionManager.release(key);
    ConnectionManager.release(key);
  });

  test('auto-reconnect with a replacement builder keeps the session identity', () => {
    const key = nextKey();
    const anonymous = new MockBuilder();

    const first = retainMock(key, anonymous);
    first.simulateConnect('anonymous-token');

    // Swap the builder while the connection is live, then drop: the reconnect
    // uses the replacement's callbacks but must not adopt its token.
    ConnectionManager.release(key);
    const signedIn = new MockBuilder('signed-in-token');
    retainMock(key, signedIn);

    first.simulateDisconnect();
    vi.advanceTimersByTime(connectionManagerReconnectDelayMs(0));

    expect(signedIn.connections[0].token).toBe('anonymous-token');

    ConnectionManager.release(key);
  });

  test('rebuild() honours the replacement builder over the live session', () => {
    const key = nextKey();
    const anonymous = new MockBuilder();

    const first = retainMock(key, anonymous);
    first.simulateConnect('anonymous-token');

    // Signing in: rebuild() is the documented way to change identity, so the
    // session-resume must not drag the anonymous token along.
    const signedIn = new MockBuilder('signed-in-token');
    const second = ConnectionManager.rebuild(
      key,
      signedIn as any
    ) as unknown as MockConnection;

    expect(second.token).toBe('signed-in-token');
    expect(signedIn.tokenHistory).toEqual([]);

    ConnectionManager.release(key);
  });

  test('auto-reconnect after rebuild() keeps the new identity', () => {
    const key = nextKey();
    const anonymous = new MockBuilder();

    retainMock(key, anonymous).simulateConnect('anonymous-token');

    const signedIn = new MockBuilder('signed-in-token');
    const second = ConnectionManager.rebuild(
      key,
      signedIn as any
    ) as unknown as MockConnection;

    // Drop *before* the new connection completes its handshake: the manager
    // must not fall back to the identity rebuild() just replaced.
    second.simulateDisconnect();
    vi.advanceTimersByTime(connectionManagerReconnectDelayMs(0));

    expect(signedIn.connections[1].token).toBe('signed-in-token');

    ConnectionManager.release(key);
  });
});
