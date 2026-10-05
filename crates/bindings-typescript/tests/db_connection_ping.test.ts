import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import { ConnectionId, TimeDuration, Timestamp } from '../src';
import { ServerMessage, type ClientMessage } from '../src/sdk/client_api/types';
import WebsocketTestAdapter, {
  WebsocketTestAdapterFactory,
} from '../src/sdk/websocket_test_adapter';
import { WebsocketDecompressAdapter } from '../src/sdk/websocket_decompress_adapter';
import { V2_WS_PROTOCOL, V3_WS_PROTOCOL } from '../src/sdk/websocket_protocols';
import { decodeClientMessagesV3 } from '../src/sdk/websocket_v3_frames.ts';
import { DbConnection } from '../test-app/src/module_bindings';
import { anIdentity } from './utils';

type Builder = ReturnType<typeof DbConnection.builder>;

async function connect(
  opts: {
    protocols?: string[];
    configure?: (builder: Builder) => Builder;
  } = {}
) {
  const wsAdapter = new WebsocketTestAdapter();
  if (opts.protocols) {
    wsAdapter.supportedProtocols = opts.protocols;
  }
  const builder = DbConnection.builder()
    .withUri('ws://127.0.0.1:1234')
    .withDatabaseName('db')
    .withWSFn(wsAdapter.openWebSocket);
  const client = (opts.configure?.(builder) ?? builder).build();
  await client['wsPromise'];
  wsAdapter.acceptConnection();
  return { wsAdapter, client };
}

function sendInitialConnection(wsAdapter: WebsocketTestAdapter): void {
  wsAdapter.sendToClient(
    ServerMessage.InitialConnection({
      identity: anIdentity,
      token: 'a-token',
      connectionId: ConnectionId.random(),
    })
  );
}

function pings(
  wsAdapter: WebsocketTestAdapter
): Extract<ClientMessage, { tag: 'Ping' }>[] {
  return wsAdapter.outgoingMessages.filter(
    (m): m is Extract<ClientMessage, { tag: 'Ping' }> => m.tag === 'Ping'
  );
}

function answerPing(
  wsAdapter: WebsocketTestAdapter,
  ping: Extract<ClientMessage, { tag: 'Ping' }>
): void {
  wsAdapter.sendToClient(
    ServerMessage.Pong({
      requestId: ping.value.requestId,
      clientSendTime: ping.value.clientSendTime,
      serverReceiveTime: new Timestamp(1_700_000_000_000_000n),
      serverHoldDuration: new TimeDuration(0n),
    })
  );
}

describe('DbConnection pings', () => {
  beforeEach(() => {
    // Only fake intervals, so promises and the v3 microtask flush stay real.
    vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] });
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  test('sends no Ping before InitialConnection', async () => {
    const { wsAdapter } = await connect();
    vi.advanceTimersByTime(5000);
    expect(pings(wsAdapter)).toHaveLength(0);
  });

  test('pings once on InitialConnection, then once per second', async () => {
    const { wsAdapter } = await connect();
    sendInitialConnection(wsAdapter);
    expect(pings(wsAdapter)).toHaveLength(1);

    vi.advanceTimersByTime(999);
    expect(pings(wsAdapter)).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(pings(wsAdapter)).toHaveLength(2);
    vi.advanceTimersByTime(3000);
    expect(pings(wsAdapter)).toHaveLength(5);
  });

  test('sends each Ping in its own v3 websocket message', async () => {
    const { wsAdapter, client } = await connect();
    expect(wsAdapter.protocol).toEqual(V3_WS_PROTOCOL);
    sendInitialConnection(wsAdapter);

    // Queue reducer calls for the v3 same-tick batch, then ping before the
    // batch flushes.
    void client.reducers.createPlayer({
      name: 'Player One',
      location: { x: 1, y: 2 },
    });
    void client.reducers.createPlayer({
      name: 'Player Two',
      location: { x: 3, y: 4 },
    });
    vi.advanceTimersByTime(1000);
    await Promise.resolve();

    const frames = wsAdapter.messageQueue.map(frame =>
      decodeClientMessagesV3(frame)
    );
    expect(frames.map(frame => frame.length)).toEqual([1, 1, 2]);
    // Outgoing messages are recorded frame by frame, so this pairs each
    // message with its frame: both Pings travel alone, and the reducer calls
    // share the batched frame.
    expect(wsAdapter.outgoingMessages.map(m => m.tag)).toEqual([
      'Ping',
      'Ping',
      'CallReducer',
      'CallReducer',
    ]);
  });

  test('sends each Ping in its own message on v2', async () => {
    const { wsAdapter } = await connect({ protocols: [V2_WS_PROTOCOL] });
    expect(wsAdapter.protocol).toEqual(V2_WS_PROTOCOL);
    sendInitialConnection(wsAdapter);
    vi.advanceTimersByTime(2000);

    expect(pings(wsAdapter)).toHaveLength(3);
    expect(wsAdapter.messageQueue).toHaveLength(3);
  });

  test('withPingInterval(0) disables pinging', async () => {
    const { wsAdapter, client } = await connect({
      configure: b => b.withPingInterval(0),
    });
    sendInitialConnection(wsAdapter);
    vi.advanceTimersByTime(10_000);
    expect(pings(wsAdapter)).toHaveLength(0);
    expect(client.networkStats).toBeUndefined();
  });

  test('withPingInterval sets the cadence', async () => {
    const { wsAdapter } = await connect({
      configure: b => b.withPingInterval(250),
    });
    sendInitialConnection(wsAdapter);
    vi.advanceTimersByTime(1000);
    expect(pings(wsAdapter)).toHaveLength(5);
  });

  test('withPingInterval rejects invalid intervals', () => {
    expect(() => DbConnection.builder().withPingInterval(-1)).toThrow(
      RangeError
    );
    expect(() => DbConnection.builder().withPingInterval(NaN)).toThrow(
      RangeError
    );
    expect(() => DbConnection.builder().withPingInterval(Infinity)).toThrow(
      RangeError
    );
    expect(() => DbConnection.builder().withPingInterval(250.5)).toThrow(
      RangeError
    );
  });

  test('records a Pong using its arrival time', async () => {
    vi.spyOn(performance, 'now').mockReturnValue(1000);
    const { wsAdapter, client } = await connect();
    sendInitialConnection(wsAdapter);
    expect(client.networkStats).toBeUndefined();

    const [ping] = pings(wsAdapter);
    expect(ping.value.clientSendTime).toBe(1_000_000n);

    wsAdapter.sendToClient(
      ServerMessage.Pong({
        requestId: ping.value.requestId,
        clientSendTime: ping.value.clientSendTime,
        serverReceiveTime: new Timestamp(1_700_000_000_000_000n),
        serverHoldDuration: new TimeDuration(5_000n),
      }),
      { receivedAt: 1050 }
    );

    const stats = client.networkStats;
    expect(stats).toBeDefined();
    expect(stats!.rttLatest).toBe(45);
    expect(stats!.serverNow).toBeInstanceOf(Timestamp);
  });

  test('ping() resolves with the timings of its own Pong', async () => {
    const now = vi.spyOn(performance, 'now').mockReturnValue(1000);
    const { wsAdapter, client } = await connect({
      configure: b => b.withPingInterval(0),
    });
    sendInitialConnection(wsAdapter);

    const first = client.ping();
    const second = client.ping();
    const [ping1, ping2] = pings(wsAdapter);
    // Both Pings went out in the same microsecond, but stay distinguishable.
    expect(ping1.value.clientSendTime).toBe(1_000_000n);
    expect(ping2.value.clientSendTime).toBe(1_000_000n);
    expect(ping1.value.requestId).not.toBe(ping2.value.requestId);

    now.mockReturnValue(1080);
    const serverReceiveTime = new Timestamp(1_700_000_000_000_000n);
    // Answer out of order, to check each result matches its own Pong.
    wsAdapter.sendToClient(
      ServerMessage.Pong({
        requestId: ping2.value.requestId,
        clientSendTime: ping2.value.clientSendTime,
        serverReceiveTime,
        serverHoldDuration: new TimeDuration(30_000n),
      }),
      { receivedAt: 1060 }
    );
    wsAdapter.sendToClient(
      ServerMessage.Pong({
        requestId: ping1.value.requestId,
        clientSendTime: ping1.value.clientSendTime,
        serverReceiveTime,
        serverHoldDuration: new TimeDuration(5_000n),
      }),
      { receivedAt: 1080 }
    );

    await expect(first).resolves.toEqual({
      rtt: 75,
      roundTrip: 80,
      serverHold: 5,
      serverReceiveTime,
      sentAt: 1000,
      receivedAt: 1080,
    });
    const result2 = await second;
    expect(result2.roundTrip).toBe(60);
    expect(result2.serverHold).toBe(30);
    // The manual Pings feed the stats too.
    expect(client.networkStats?.rttLatest).toBe(75);
  });

  test('ping() rejects before InitialConnection and on close', async () => {
    const { wsAdapter, client } = await connect();
    await expect(client.ping()).rejects.toThrow(/established/);

    sendInitialConnection(wsAdapter);
    const pending = client.ping();
    wsAdapter.close();
    await expect(pending).rejects.toThrow(/closed/);
    await expect(client.ping()).rejects.toThrow(/closed/);
  });

  test('stops pinging and clears stats when the socket closes', async () => {
    const { wsAdapter, client } = await connect();
    sendInitialConnection(wsAdapter);
    const [ping] = pings(wsAdapter);
    wsAdapter.sendToClient(
      ServerMessage.Pong({
        requestId: ping.value.requestId,
        clientSendTime: ping.value.clientSendTime,
        serverReceiveTime: new Timestamp(1_700_000_000_000_000n),
        serverHoldDuration: new TimeDuration(0n),
      })
    );
    expect(client.networkStats).toBeDefined();

    wsAdapter.close();
    vi.advanceTimersByTime(5000);
    expect(pings(wsAdapter)).toHaveLength(1);
    expect(client.networkStats).toBeUndefined();
  });
});

describe('DbConnection pings with automatic reconnect', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  async function connectReconnecting() {
    const factory = new WebsocketTestAdapterFactory();
    const client = DbConnection.builder()
      .withUri('ws://127.0.0.1:1234')
      .withDatabaseName('db')
      .withWSFn(factory.openWebSocket)
      .withAutomaticReconnect()
      .build();
    await establish(client, factory);
    return { factory, client };
  }

  async function establish(
    client: DbConnection,
    factory: WebsocketTestAdapterFactory
  ): Promise<void> {
    await client['wsPromise'];
    factory.current.acceptConnection();
    sendInitialConnection(factory.current);
    await Promise.resolve();
  }

  /** Drop the socket, then run the reconnect timer to open a new one. */
  async function dropAndReopen(
    client: DbConnection,
    factory: WebsocketTestAdapterFactory
  ): Promise<WebsocketTestAdapter> {
    const dropped = factory.current;
    dropped.close();
    // Pinging stopped with the socket, so only the reconnect timer is pending.
    await vi.runOnlyPendingTimersAsync();
    await client['wsPromise'];
    expect(factory.current).not.toBe(dropped);
    return dropped;
  }

  test('stops pinging the lost socket and resumes on the new one', async () => {
    const { factory, client } = await connectReconnecting();
    expect(pings(factory.current)).toHaveLength(1);

    const dropped = await dropAndReopen(client, factory);
    expect(client.isReconnecting).toBe(true);
    // No Ping before the new socket's handshake completes.
    expect(pings(factory.current)).toHaveLength(0);

    await establish(client, factory);
    expect(pings(factory.current)).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(1000);
    expect(pings(factory.current)).toHaveLength(2);
    expect(pings(dropped)).toHaveLength(1);
  });

  test('clears stats on loss and measures afresh after reconnect', async () => {
    const { factory, client } = await connectReconnecting();
    answerPing(factory.current, pings(factory.current)[0]);
    expect(client.networkStats).toBeDefined();

    await dropAndReopen(client, factory);
    expect(client.networkStats).toBeUndefined();

    await establish(client, factory);
    expect(client.networkStats).toBeUndefined();
    answerPing(factory.current, pings(factory.current)[0]);
    expect(client.networkStats).toBeDefined();
  });

  test('ping() rejects on loss and while reconnecting', async () => {
    const { factory, client } = await connectReconnecting();
    const pending = client.ping();
    const rejected = expect(pending).rejects.toThrow(/closed/);

    await dropAndReopen(client, factory);
    await rejected;
    await expect(client.ping()).rejects.toThrow(/established/);

    await establish(client, factory);
    const resumed = client.ping();
    answerPing(factory.current, pings(factory.current).at(-1)!);
    await expect(resumed).resolves.toMatchObject({ serverHold: 0 });
  });

  test('disconnect() stops pinging', async () => {
    const { factory, client } = await connectReconnecting();
    client.disconnect();
    await vi.advanceTimersByTimeAsync(5000);
    expect(pings(factory.current)).toHaveLength(1);
    expect(factory.sockets).toHaveLength(1);
  });
});

describe('DbConnection ping timeout', () => {
  beforeEach(() => {
    // performance.now() must follow the fake clock for Pings to age.
    vi.useFakeTimers({
      toFake: [
        'setInterval',
        'clearInterval',
        'setTimeout',
        'clearTimeout',
        'performance',
      ],
    });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  async function connectSilent(configure?: (builder: Builder) => Builder) {
    const factory = new WebsocketTestAdapterFactory();
    const disconnects: (Error | undefined)[] = [];
    let builder = DbConnection.builder()
      .withUri('ws://127.0.0.1:1234')
      .withDatabaseName('db')
      .withWSFn(factory.openWebSocket)
      .onDisconnect((_ctx, error) => disconnects.push(error));
    builder = configure?.(builder) ?? builder;
    const client = builder.build();
    await client['wsPromise'];
    factory.current.acceptConnection();
    sendInitialConnection(factory.current);
    return { factory, client, disconnects };
  }

  test('networkStats.pongWait rises while the link is silent', async () => {
    const { factory, client } = await connectSilent();
    answerPing(factory.current, pings(factory.current)[0]);
    expect(client.networkStats!.pongWait).toBe(0);

    await vi.advanceTimersByTimeAsync(3500);
    // The Ping sent at 1 s is the oldest unanswered one.
    expect(client.networkStats!.pongWait).toBe(2500);
  });

  test('a silent link disconnects with an error after the timeout', async () => {
    const { factory, client, disconnects } = await connectSilent();
    const pending = client.ping();
    const rejected = expect(pending).rejects.toThrow(/closed/);

    await vi.advanceTimersByTimeAsync(10_000);
    expect(disconnects).toEqual([]);
    expect(client.isActive).toBe(true);

    // The first Ping went out at 0 s; the tick at 11 s finds it over 10 s old.
    await vi.advanceTimersByTimeAsync(1000);
    expect(disconnects).toHaveLength(1);
    expect(disconnects[0]?.message).toMatch(
      /unresponsive: no Pong for 11000ms/
    );
    expect(client.isActive).toBe(false);
    expect(factory.current.closed).toBe(true);
    await rejected;

    const sent = pings(factory.current).length;
    await vi.advanceTimersByTimeAsync(10_000);
    expect(pings(factory.current)).toHaveLength(sent);
  });

  test('a silent link reconnects when automatic reconnect is on', async () => {
    const { factory, disconnects } = await connectSilent(b =>
      b.withAutomaticReconnect()
    );
    await vi.advanceTimersByTimeAsync(11_000);
    expect(disconnects).toHaveLength(1);
    expect(factory.sockets).toHaveLength(1);

    // The backoff delay is jittered, but well under 5 s for the first retry.
    await vi.advanceTimersByTimeAsync(5000);
    expect(factory.sockets).toHaveLength(2);
  });

  test('withPingTimeout sets the timeout', async () => {
    const { disconnects } = await connectSilent(b => b.withPingTimeout(3000));
    await vi.advanceTimersByTimeAsync(3000);
    expect(disconnects).toEqual([]);
    await vi.advanceTimersByTimeAsync(1000);
    expect(disconnects).toHaveLength(1);
  });

  test('an answered link never times out', async () => {
    const { factory, disconnects } = await connectSilent();
    for (let second = 0; second < 60; second++) {
      answerPing(factory.current, pings(factory.current).at(-1)!);
      await vi.advanceTimersByTimeAsync(1000);
    }
    expect(disconnects).toEqual([]);
    expect(pings(factory.current)).toHaveLength(61);
  });

  test('withPingTimeout rejects invalid timeouts', () => {
    for (const bad of [0, -1, NaN, Infinity, 2500.5]) {
      expect(() => DbConnection.builder().withPingTimeout(bad)).toThrow(
        RangeError
      );
    }
  });
});

describe('WebsocketDecompressAdapter', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  test('captures receivedAt on arrival, before decompression', async () => {
    const fakeWs = {} as { onmessage?: (msg: MessageEvent) => Promise<void> };
    const adapter = new WebsocketDecompressAdapter(
      fakeWs as unknown as WebSocket
    );
    const received: { data: Uint8Array; receivedAt?: number }[] = [];
    adapter.onmessage = msg => received.push(msg);

    vi.spyOn(performance, 'now').mockReturnValueOnce(111).mockReturnValue(999);
    // Tag 0 means uncompressed.
    await fakeWs.onmessage!({
      data: new Uint8Array([0, 42]).buffer,
    } as MessageEvent);

    expect(received).toEqual([{ data: new Uint8Array([42]), receivedAt: 111 }]);
  });
});
