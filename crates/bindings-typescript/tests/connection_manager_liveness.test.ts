import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import { ConnectionId } from '../src';
import { connectionManagerReconnectDelayMs } from '../src/sdk/connection_manager.ts';

// These tests exercise the page-resume + zombie-socket liveness recovery in the
// ConnectionManager. That logic wires itself to `document`/`window` events in
// its constructor, so each test installs minimal DOM stubs and re-imports the
// module to get a fresh singleton bound to those stubs.

type ErrorContextInterface = { isActive: boolean };

class MockConnection {
  isActive = false;
  identity = undefined;
  // A real DbConnectionImpl is constructed with the builder's token and keeps
  // it in this field, so the mock takes it the same way.
  token: string | undefined;
  connectionId = ConnectionId.random();
  isDisconnectRequested = false;
  disconnected = false;
  // Controls the `isSocketClosed` signal the manager reads to detect a socket
  // that died silently (CLOSING/CLOSED without a clean `onclose`).
  socketClosed = false;

  #onConnect = new Set<(conn: MockConnection) => void>();
  #onDisconnect = new Set<
    (ctx: ErrorContextInterface, error?: Error) => void
  >();
  #onConnectError = new Set<
    (ctx: ErrorContextInterface, error: Error) => void
  >();

  get isSocketClosed(): boolean {
    return this.socketClosed;
  }

  disconnect(): void {
    this.isDisconnectRequested = true;
    this.disconnected = true;
    this.isActive = false;
  }

  removeOnConnect(cb: (conn: MockConnection) => void): void {
    this.#onConnect.delete(cb);
  }
  removeOnDisconnect(
    cb: (ctx: ErrorContextInterface, error?: Error) => void
  ): void {
    this.#onDisconnect.delete(cb);
  }
  removeOnConnectError(
    cb: (ctx: ErrorContextInterface, error: Error) => void
  ): void {
    this.#onConnectError.delete(cb);
  }

  register(
    type: 'connect' | 'disconnect' | 'connectError',
    cb: (...args: any[]) => void
  ): void {
    if (type === 'connect') this.#onConnect.add(cb);
    else if (type === 'disconnect') this.#onDisconnect.add(cb);
    else this.#onConnectError.add(cb);
  }

  /**
   * @param issuedToken - the token the server hands back on connect, emulating
   * a client being issued credentials it did not have when the builder was
   * constructed.
   */
  simulateConnect(issuedToken?: string): void {
    this.isActive = true;
    if (issuedToken !== undefined) this.token = issuedToken;
    for (const cb of this.#onConnect) cb(this);
  }
  simulateDisconnect(error?: Error): void {
    this.isActive = false;
    for (const cb of this.#onDisconnect)
      cb(this as unknown as ErrorContextInterface, error);
  }
}

class MockBuilder {
  buildCount = 0;
  connections: MockConnection[] = [];
  /** The token each `build()` will stamp onto its connection. */
  token: string | undefined;

  #onConnect = new Set<(conn: MockConnection) => void>();
  #onDisconnect = new Set<
    (ctx: ErrorContextInterface, error?: Error) => void
  >();
  #onConnectError = new Set<
    (ctx: ErrorContextInterface, error: Error) => void
  >();

  withToken(token?: string): MockBuilder {
    this.token = token;
    return this;
  }

  build(): MockConnection {
    const connection = new MockConnection();
    connection.token = this.token;
    this.buildCount += 1;
    this.connections.push(connection);
    for (const cb of this.#onConnect) connection.register('connect', cb);
    for (const cb of this.#onDisconnect) connection.register('disconnect', cb);
    for (const cb of this.#onConnectError)
      connection.register('connectError', cb);
    return connection;
  }

  onConnect(cb: (conn: MockConnection) => void): MockBuilder {
    this.#onConnect.add(cb);
    for (const c of this.connections) c.register('connect', cb);
    return this;
  }
  onDisconnect(
    cb: (ctx: ErrorContextInterface, error?: Error) => void
  ): MockBuilder {
    this.#onDisconnect.add(cb);
    for (const c of this.connections) c.register('disconnect', cb);
    return this;
  }
  onConnectError(
    cb: (ctx: ErrorContextInterface, error: Error) => void
  ): MockBuilder {
    this.#onConnectError.add(cb);
    for (const c of this.connections) c.register('connectError', cb);
    return this;
  }
}

let keyCounter = 0;
function nextKey(): string {
  keyCounter += 1;
  return `connection-manager-liveness-${keyCounter}`;
}

type DocStub = {
  visibilityState: 'visible' | 'hidden';
  addEventListener: (ev: string, h: () => void) => void;
};

let ConnectionManager: typeof import('../src/sdk/connection_manager.ts').ConnectionManager;
let doc: DocStub;
let listeners: Record<string, Array<() => void>>;

function retain(key: string, builder: MockBuilder): MockConnection {
  return ConnectionManager.retain(
    key,
    builder as any
  ) as unknown as MockConnection;
}

function fire(name: string): void {
  for (const h of listeners[name] ?? []) h();
}

async function loadManager(): Promise<void> {
  listeners = {};
  doc = {
    visibilityState: 'visible',
    addEventListener: (ev, h) => {
      (listeners[`doc:${ev}`] ??= []).push(h);
    },
  };
  const win = {
    addEventListener: (ev: string, h: () => void) => {
      (listeners[`win:${ev}`] ??= []).push(h);
    },
  };
  (globalThis as any).document = doc;
  (globalThis as any).window = win;
  vi.resetModules();
  ({ ConnectionManager } = await import('../src/sdk/connection_manager.ts'));
}

describe('ConnectionManager liveness recovery', () => {
  beforeEach(async () => {
    // Fake timers let us drive the reconnect backoff deterministically.
    vi.useFakeTimers();
    await loadManager();
  });

  afterEach(() => {
    vi.runOnlyPendingTimers();
    vi.useRealTimers();
    delete (globalThis as any).document;
    delete (globalThis as any).window;
  });

  test('registers resume + network listeners on construction', () => {
    expect(listeners['doc:visibilitychange']?.length).toBe(1);
    expect(listeners['win:focus']?.length).toBe(1);
    expect(listeners['win:online']?.length).toBe(1);
    expect(listeners['win:pageshow']?.length).toBe(1);
  });

  test('revives a silently-dead socket when the network returns', () => {
    const key = nextKey();
    const builder = new MockBuilder();
    const first = retain(key, builder);
    first.simulateConnect();
    expect(ConnectionManager.getSnapshot(key)?.isActive).toBe(true);

    // Socket dies while backgrounded: no disconnect event ever fires, but the
    // underlying readyState is now CLOSED.
    first.socketClosed = true;
    expect(ConnectionManager.getConnection(key)).toBe(first);

    fire('win:online');

    expect(builder.buildCount).toBe(2);
    expect(ConnectionManager.getConnection(key)).toBe(builder.connections[1]);
    ConnectionManager.release(key);
  });

  test('does not rebuild a healthy connection on resume', () => {
    const key = nextKey();
    const builder = new MockBuilder();
    const first = retain(key, builder);
    first.simulateConnect();

    fire('win:focus');
    fire('doc:visibilitychange');

    expect(builder.buildCount).toBe(1);
    expect(ConnectionManager.getConnection(key)).toBe(first);
    ConnectionManager.release(key);
  });

  test('does not revive a connection that was intentionally disconnected', () => {
    const key = nextKey();
    const builder = new MockBuilder();
    const first = retain(key, builder);
    first.simulateConnect();
    first.isDisconnectRequested = true;
    first.socketClosed = true;

    fire('win:online');

    expect(builder.buildCount).toBe(1);
    ConnectionManager.release(key);
  });

  test('brings a stalled reconnect forward on resume and resets backoff', () => {
    const key = nextKey();
    const builder = new MockBuilder();
    const first = retain(key, builder);
    first.simulateDisconnect();

    // The reconnect timer is scheduled but has not fired yet (simulating a
    // background tab whose timers are throttled/frozen).
    vi.advanceTimersByTime(connectionManagerReconnectDelayMs(0) - 1);
    expect(builder.buildCount).toBe(1);

    // Regaining focus rebuilds immediately instead of waiting out the delay.
    fire('doc:visibilitychange');
    expect(builder.buildCount).toBe(2);

    // Backoff was reset: the next failure reconnects after the base delay.
    builder.connections[1].simulateDisconnect();
    vi.advanceTimersByTime(connectionManagerReconnectDelayMs(0));
    expect(builder.buildCount).toBe(3);

    ConnectionManager.release(key);
  });

  test('visibilitychange while still hidden does not reconnect', () => {
    const key = nextKey();
    const builder = new MockBuilder();
    const first = retain(key, builder);
    first.simulateDisconnect();

    doc.visibilityState = 'hidden';
    fire('doc:visibilitychange');

    expect(builder.buildCount).toBe(1);
    ConnectionManager.release(key);
  });

  // The resume paths rebuild from the retained builder, whose token is a
  // snapshot from before the session existed. Reconnecting anonymously here
  // makes the server mint a new identity, so a user who merely switched tabs
  // comes back as a stranger.
  test('reviving a dead socket on resume keeps the session identity', () => {
    const key = nextKey();
    // A first-time visitor: no stored credentials when the builder was made.
    const builder = new MockBuilder();
    const first = retain(key, builder);
    first.simulateConnect('session-token');

    // Tab is backgrounded and the socket dies silently.
    first.socketClosed = true;
    fire('doc:visibilitychange');

    expect(builder.buildCount).toBe(2);
    expect(builder.connections[1].token).toBe('session-token');
    ConnectionManager.release(key);
  });

  test('a stalled reconnect brought forward on resume keeps the session identity', () => {
    const key = nextKey();
    const builder = new MockBuilder();
    const first = retain(key, builder);
    first.simulateConnect('session-token');
    first.simulateDisconnect();

    // Timer still pending behind background throttling; focus fires it early.
    vi.advanceTimersByTime(connectionManagerReconnectDelayMs(0) - 1);
    fire('win:focus');

    expect(builder.buildCount).toBe(2);
    expect(builder.connections[1].token).toBe('session-token');
    ConnectionManager.release(key);
  });
});
