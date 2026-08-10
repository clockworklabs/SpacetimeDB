import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import { ConnectionId } from '../src';

// A reconnect must present the identity we were already issued.
//
// A client that first connects anonymously builds its connection builder BEFORE
// it has a token, so the builder's token stays `undefined` for the life of the
// page. Rebuilding from it presents no credentials, the host mints a NEW
// identity, and every row owned by the previous one silently disappears — no
// error, and no callback in between for the application to intervene. Saving
// the token diligently does not help: the stale builder is what gets rebuilt.
//
// Not hypothetical. The pattern the client skill doc prescribes constructs the
// builder inside `useMemo(..., [])`, reading `localStorage` once at first
// render — when, for a first-ever visitor, it is empty.
//
// Structured like connection_manager_liveness.test.ts: the manager wires itself
// to `document`/`window` in its constructor, so each test installs DOM stubs and
// re-imports the module to get a fresh singleton bound to them.

type ErrorContextInterface = { isActive: boolean };

class MockConnection {
  isActive = false;
  identity = undefined;
  token: string | undefined = undefined;
  connectionId = ConnectionId.random();
  isDisconnectRequested = false;
  socketClosed = false;

  #onConnect = new Set<(conn: MockConnection) => void>();
  #onDisconnect = new Set<(ctx: ErrorContextInterface, e?: Error) => void>();
  #onConnectError = new Set<(ctx: ErrorContextInterface, e: Error) => void>();

  /** What the host hands back once this connection is accepted. */
  constructor(readonly issuedToken: string) {}

  get isSocketClosed(): boolean {
    return this.socketClosed;
  }

  disconnect(): void {
    this.isDisconnectRequested = true;
    this.isActive = false;
  }

  registerOnConnect(cb: (conn: MockConnection) => void): void {
    this.#onConnect.add(cb);
  }
  registerOnDisconnect(cb: (c: ErrorContextInterface, e?: Error) => void): void {
    this.#onDisconnect.add(cb);
  }
  registerOnConnectError(cb: (c: ErrorContextInterface, e: Error) => void): void {
    this.#onConnectError.add(cb);
  }
  removeOnConnect(cb: (conn: MockConnection) => void): void {
    this.#onConnect.delete(cb);
  }
  removeOnDisconnect(cb: (c: ErrorContextInterface, e?: Error) => void): void {
    this.#onDisconnect.delete(cb);
  }
  removeOnConnectError(cb: (c: ErrorContextInterface, e: Error) => void): void {
    this.#onConnectError.delete(cb);
  }

  /** The host accepts the connection and issues a token. */
  simulateConnect(): void {
    this.isActive = true;
    this.token = this.issuedToken;
    for (const cb of this.#onConnect) cb(this);
  }
}

/** Records the token it was asked to present on every build — the thing under test. */
class MockBuilder {
  buildCount = 0;
  presented: Array<string | undefined> = [];
  connections: MockConnection[] = [];
  #token: string | undefined;

  #onConnect = new Set<(conn: MockConnection) => void>();
  #onDisconnect = new Set<(c: ErrorContextInterface, e?: Error) => void>();
  #onConnectError = new Set<(c: ErrorContextInterface, e: Error) => void>();

  constructor(private readonly issuedToken = 'token-from-host') {}

  withToken(token?: string): this {
    this.#token = token;
    return this;
  }

  build(): MockConnection {
    this.buildCount += 1;
    this.presented.push(this.#token);
    const c = new MockConnection(this.issuedToken);
    this.connections.push(c);
    for (const cb of this.#onConnect) c.registerOnConnect(cb);
    for (const cb of this.#onDisconnect) c.registerOnDisconnect(cb);
    for (const cb of this.#onConnectError) c.registerOnConnectError(cb);
    return c;
  }

  onConnect(cb: (conn: MockConnection) => void): this {
    this.#onConnect.add(cb);
    for (const c of this.connections) c.registerOnConnect(cb);
    return this;
  }
  onDisconnect(cb: (c: ErrorContextInterface, e?: Error) => void): this {
    this.#onDisconnect.add(cb);
    for (const c of this.connections) c.registerOnDisconnect(cb);
    return this;
  }
  onConnectError(cb: (c: ErrorContextInterface, e: Error) => void): this {
    this.#onConnectError.add(cb);
    for (const c of this.connections) c.registerOnConnectError(cb);
    return this;
  }

  get lastPresented(): string | undefined {
    return this.presented[this.presented.length - 1];
  }
}

let ConnectionManager: typeof import('../src/sdk/connection_manager.ts').ConnectionManager;
let listeners: Record<string, Array<() => void>>;
let keyCounter = 0;

const nextKey = () => `connection-manager-token-reuse-${++keyCounter}`;
const fire = (name: string) => {
  for (const h of listeners[name] ?? []) h();
};
const retain = (key: string, b: MockBuilder) =>
  ConnectionManager.retain(key, b as any) as unknown as MockConnection;

describe('ConnectionManager reconnects as the identity it was issued', () => {
  beforeEach(async () => {
    vi.useFakeTimers();
    listeners = {};
    (globalThis as any).document = {
      visibilityState: 'visible',
      addEventListener: (ev: string, h: () => void) => {
        (listeners[`doc:${ev}`] ??= []).push(h);
      },
    };
    (globalThis as any).window = {
      addEventListener: (ev: string, h: () => void) => {
        (listeners[`win:${ev}`] ??= []).push(h);
      },
    };
    vi.resetModules();
    ({ ConnectionManager } = await import('../src/sdk/connection_manager.ts'));
  });

  afterEach(() => {
    vi.runOnlyPendingTimers();
    vi.useRealTimers();
    delete (globalThis as any).document;
    delete (globalThis as any).window;
  });

  test('re-presents the issued token when a dead socket is revived', () => {
    const key = nextKey();
    const builder = new MockBuilder('token-from-host');
    const first = retain(key, builder);

    // First connection: a brand-new visitor has nothing to present.
    expect(builder.presented).toEqual([undefined]);

    first.simulateConnect();
    expect(ConnectionManager.getSnapshot(key)?.token).toBe('token-from-host');

    // The socket dies silently — a host restart, or a drop while backgrounded.
    first.socketClosed = true;
    fire('win:online');

    expect(builder.buildCount).toBe(2);
    // The reconnect must NOT have gone out anonymous again. Before this fix it
    // presented `undefined` and the host issued a second, unrelated identity.
    expect(builder.lastPresented).toBe('token-from-host');

    ConnectionManager.release(key);
  });

  test('an explicit rebuild still uses the token it was handed', () => {
    const key = nextKey();
    const anon = new MockBuilder('anonymous-token');
    retain(key, anon).simulateConnect();

    // Swapping an anonymous session for a signed-in one is exactly what
    // rebuild() is for, so the manager must not force the old token back on.
    const signedIn = new MockBuilder('signed-in-token');
    signedIn.withToken('a-real-users-token');
    ConnectionManager.rebuild(key, signedIn as any);

    expect(signedIn.presented).toEqual(['a-real-users-token']);

    ConnectionManager.release(key);
  });
});
