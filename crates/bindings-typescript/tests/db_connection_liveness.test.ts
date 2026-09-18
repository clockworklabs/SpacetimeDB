import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import { Identity } from '../src';
import { ServerMessage } from '../src/sdk/client_api/types';
import { WebsocketTestAdapterFactory } from '../src/sdk/websocket_test_adapter';
import { ConnectionId } from '../src';
import { DbConnection } from '../test-app/src/module_bindings';
import { anIdentity } from './utils';

// These tests exercise the page-resume liveness recovery in DbConnectionImpl:
// with automatic reconnection enabled, the connection listens for the page
// coming back to the foreground (visibilitychange/focus/online/pageshow) and
// uses the moment to notice sockets that died silently while the tab was
// frozen, and to bring a backoff-stalled reconnect forward.
//
// The listeners bind to `document`/`window` when the socket opens, so each
// test installs minimal DOM stubs first.

type ReconnectReport = {
  error?: Error;
  nextReconnectAttempt?: number;
  nextReconnectDelayMs?: number;
};

type Harness = {
  connection: DbConnection;
  factory: WebsocketTestAdapterFactory;
  connects: { identity: Identity; token: string }[];
  disconnects: ReconnectReport[];
  connectErrors: ReconnectReport[];
};

let listeners: Record<string, Array<() => void>>;
let visibilityState: 'visible' | 'hidden';

function installDomStubs(): void {
  listeners = {};
  visibilityState = 'visible';
  const record =
    (scope: string) =>
    (ev: string, h: () => void): void => {
      (listeners[`${scope}:${ev}`] ??= []).push(h);
    };
  const remove =
    (scope: string) =>
    (ev: string, h: () => void): void => {
      const bucket = listeners[`${scope}:${ev}`];
      if (bucket) {
        const at = bucket.indexOf(h);
        if (at >= 0) bucket.splice(at, 1);
      }
    };
  vi.stubGlobal('document', {
    get visibilityState() {
      return visibilityState;
    },
    addEventListener: record('doc'),
    removeEventListener: remove('doc'),
  });
  vi.stubGlobal('window', {
    addEventListener: record('win'),
    removeEventListener: remove('win'),
  });
}

function removeDomStubs(): void {
  vi.unstubAllGlobals();
}

function fire(name: string): void {
  for (const h of [...(listeners[name] ?? [])]) h();
}

function listenerCounts(): Record<string, number> {
  return Object.fromEntries(
    Object.entries(listeners).map(([name, hs]) => [name, hs.length])
  );
}

function build(options?: { automaticReconnect?: boolean }): Harness {
  const factory = new WebsocketTestAdapterFactory();
  const connects: { identity: Identity; token: string }[] = [];
  const disconnects: ReconnectReport[] = [];
  const connectErrors: ReconnectReport[] = [];

  let builder = DbConnection.builder()
    .withUri('ws://127.0.0.1:1234')
    .withDatabaseName('db')
    .withWSFn(factory.openWebSocket)
    .onConnect((_conn, identity, token) => connects.push({ identity, token }))
    .onDisconnect((_ctx, error, nextReconnectAttempt, nextReconnectDelayMs) =>
      disconnects.push({ error, nextReconnectAttempt, nextReconnectDelayMs })
    )
    .onConnectError((_ctx, error, nextReconnectAttempt, nextReconnectDelayMs) =>
      connectErrors.push({ error, nextReconnectAttempt, nextReconnectDelayMs })
    );
  if (options?.automaticReconnect ?? true) {
    builder = builder.withAutomaticReconnect();
  }

  return {
    connection: builder.build(),
    factory,
    connects,
    disconnects,
    connectErrors,
  };
}

/** Let the connection's pending socket promise settle. */
async function settle(harness: Harness): Promise<void> {
  await harness.connection['wsPromise'];
  await Promise.resolve();
}

/** Bring a connection up to an established state on its current socket. */
async function establish(harness: Harness): Promise<void> {
  await settle(harness);
  harness.factory.current.acceptConnection();
  harness.factory.current.sendToClient(
    ServerMessage.InitialConnection({
      identity: anIdentity,
      connectionId: ConnectionId.random(),
      token: 'issued-token',
    })
  );
  await Promise.resolve();
}

beforeEach(() => {
  vi.useFakeTimers();
  installDomStubs();
});

afterEach(() => {
  vi.useRealTimers();
  removeDomStubs();
});

describe('liveness listeners', () => {
  test('are installed when the socket opens with automatic reconnect enabled', async () => {
    const harness = build();
    await establish(harness);

    expect(listenerCounts()).toEqual({
      'doc:visibilitychange': 1,
      'win:focus': 1,
      'win:online': 1,
      'win:pageshow': 1,
    });
  });

  test('are not installed without automatic reconnect', async () => {
    const harness = build({ automaticReconnect: false });
    await establish(harness);

    expect(listeners).toEqual({});
  });

  test('are removed when the connection ends', async () => {
    const harness = build();
    await establish(harness);

    harness.connection.disconnect();
    await settle(harness);

    expect(listenerCounts()).toEqual({
      'doc:visibilitychange': 0,
      'win:focus': 0,
      'win:online': 0,
      'win:pageshow': 0,
    });
  });
});

describe('liveness recovery on page resume', () => {
  test('treats a silently-dead socket as a lost connection when the network returns', async () => {
    const harness = build();
    await establish(harness);
    const firstSocket = harness.factory.current;

    // Socket dies while backgrounded: no close event is ever delivered, but
    // the underlying readyState is now CLOSED.
    firstSocket.dieSilently();
    expect(harness.disconnects).toHaveLength(0);

    fire('win:online');

    // The loss is reported like any mid-session drop, announcing a retry...
    expect(harness.disconnects).toHaveLength(1);
    expect(harness.disconnects[0].nextReconnectAttempt).toBe(1);

    // ...and the scheduled attempt builds a fresh socket the connection can
    // re-establish on.
    await vi.runOnlyPendingTimersAsync();
    await settle(harness);
    expect(harness.factory.current).not.toBe(firstSocket);
    await establish(harness);
    expect(harness.connects).toHaveLength(2);
    expect(harness.connection.isActive).toBe(true);
  });

  test('does not disturb a healthy connection on resume', async () => {
    const harness = build();
    await establish(harness);
    const firstSocket = harness.factory.current;

    fire('win:focus');
    fire('doc:visibilitychange');
    await settle(harness);

    expect(harness.disconnects).toHaveLength(0);
    expect(harness.factory.current).toBe(firstSocket);
    expect(harness.connection.isActive).toBe(true);
  });

  test('does not revive a connection after an explicit disconnect', async () => {
    const harness = build();
    await establish(harness);

    harness.connection.disconnect();
    await settle(harness);
    const disconnectsBefore = harness.disconnects.length;

    harness.factory.current.dieSilently();
    fire('win:online');
    await vi.runOnlyPendingTimersAsync();

    expect(harness.disconnects).toHaveLength(disconnectsBefore);
    expect(harness.connection.isActive).toBe(false);
  });

  test('brings a backoff-stalled reconnect forward on resume', async () => {
    const harness = build();
    await establish(harness);
    const firstSocket = harness.factory.current;

    // Drop the connection; a reconnect is now waiting out its backoff delay
    // (simulating a background tab whose timers are throttled/frozen).
    firstSocket.serverClose(1006);
    expect(harness.disconnects).toHaveLength(1);
    const delay = harness.disconnects[0].nextReconnectDelayMs!;
    vi.advanceTimersByTime(delay - 1);
    expect(harness.factory.current).toBe(firstSocket);

    // Regaining visibility retries immediately instead of waiting out the
    // remaining delay.
    fire('doc:visibilitychange');
    await settle(harness);
    expect(harness.factory.current).not.toBe(firstSocket);

    await establish(harness);
    expect(harness.connects).toHaveLength(2);
  });

  test('visibilitychange while still hidden does nothing', async () => {
    const harness = build();
    await establish(harness);
    const firstSocket = harness.factory.current;

    firstSocket.serverClose(1006);
    visibilityState = 'hidden';
    fire('doc:visibilitychange');
    await settle(harness);

    expect(harness.factory.current).toBe(firstSocket);
  });
});
