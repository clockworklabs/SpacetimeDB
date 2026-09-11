import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import { ConnectionId, Identity } from '../src';
import {
  DisconnectedError,
  IdentityChangedError,
  UnknownCallResultError,
} from '../src/lib/errors';
import {
  computeReconnectDelayMs,
  tokenNeedsRefresh,
  RECONNECT_INITIAL_DELAY_MS,
  RECONNECT_MAX_DELAY_MS,
} from '../src/sdk/db_connection_impl';
import {
  ServerMessage,
  type SubscribeBatch,
} from '../src/sdk/client_api/types';
import { WebSocketTokenError } from '../src/sdk/ws';
import { WebsocketTestAdapterFactory } from '../src/sdk/websocket_test_adapter';
import { DbConnection } from '../test-app/src/module_bindings';
import { anIdentity, bobIdentity, encodeUser } from './utils';

/** The disconnect/connect-error reports an application sees. */
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

const TOKEN = 'issued-token';

function build(options?: {
  automaticReconnect?: boolean;
  token?: string;
  tokenProvider?: () => Promise<string>;
}): Harness {
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
  if (options?.token) {
    builder = builder.withToken(options.token);
  }
  if (options?.automaticReconnect ?? true) {
    builder = builder.withAutomaticReconnect();
  }
  if (options?.tokenProvider) {
    builder = builder.withTokenProvider(options.tokenProvider);
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

function initialConnection(
  identity: Identity = anIdentity,
  connectionId: ConnectionId = ConnectionId.random()
): ServerMessage {
  return ServerMessage.InitialConnection({
    identity,
    connectionId,
    token: TOKEN,
  });
}

/** Bring a connection up to an established state on its current socket. */
async function establish(
  harness: Harness,
  identity: Identity = anIdentity
): Promise<void> {
  await settle(harness);
  harness.factory.current.acceptConnection();
  harness.factory.current.sendToClient(initialConnection(identity));
  await Promise.resolve();
}

/** Run the scheduled reconnect timer and let its socket be created. */
async function runReconnectTimer(harness: Harness): Promise<void> {
  await vi.runOnlyPendingTimersAsync();
  await settle(harness);
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('reconnect policy', () => {
  test('delays grow exponentially from the initial delay', () => {
    const noJitter = () => 0.5;
    expect(computeReconnectDelayMs(1, noJitter)).toBe(
      RECONNECT_INITIAL_DELAY_MS
    );
    expect(computeReconnectDelayMs(2, noJitter)).toBe(
      RECONNECT_INITIAL_DELAY_MS * 2
    );
    expect(computeReconnectDelayMs(3, noJitter)).toBe(
      RECONNECT_INITIAL_DELAY_MS * 4
    );
  });

  test('delays are capped', () => {
    const noJitter = () => 0.5;
    expect(computeReconnectDelayMs(20, noJitter)).toBe(RECONNECT_MAX_DELAY_MS);
  });

  test('jitter spreads the delay around the base but never exceeds the cap', () => {
    expect(computeReconnectDelayMs(3, () => 0)).toBeLessThan(
      computeReconnectDelayMs(3, () => 1)
    );
    expect(computeReconnectDelayMs(30, () => 1)).toBeLessThanOrEqual(
      RECONNECT_MAX_DELAY_MS
    );
    expect(computeReconnectDelayMs(1, () => 0)).toBeGreaterThanOrEqual(0);
  });
});

describe('token refresh', () => {
  const nowSeconds = 1_000_000;
  const nowMs = nowSeconds * 1000;

  function jwt(claims: object): string {
    const payload = Buffer.from(JSON.stringify(claims)).toString('base64url');
    return `header.${payload}.signature`;
  }

  test('a token with plenty of life left is not refreshed', () => {
    const token = jwt({ iat: nowSeconds - 60, exp: nowSeconds + 3600 });
    expect(tokenNeedsRefresh(token, nowMs)).toBe(false);
  });

  test('a token close to expiring is refreshed', () => {
    const token = jwt({ iat: nowSeconds - 3590, exp: nowSeconds + 10 });
    expect(tokenNeedsRefresh(token, nowMs)).toBe(true);
  });

  test('a short-lived token is refreshed on the 30 second floor', () => {
    // 5% of a 60 second lifetime is only 3 seconds, so the floor applies.
    const token = jwt({ iat: nowSeconds - 40, exp: nowSeconds + 20 });
    expect(tokenNeedsRefresh(token, nowMs)).toBe(true);
  });

  test('an expired token is refreshed', () => {
    const token = jwt({ iat: nowSeconds - 3600, exp: nowSeconds - 1 });
    expect(tokenNeedsRefresh(token, nowMs)).toBe(true);
  });

  test('a token whose expiry cannot be read is always refreshed', () => {
    expect(tokenNeedsRefresh('not-a-jwt', nowMs)).toBe(true);
    expect(tokenNeedsRefresh(jwt({ sub: 'no-exp' }), nowMs)).toBe(true);
    expect(tokenNeedsRefresh(undefined, nowMs)).toBe(true);
  });
});

describe('losing an established connection', () => {
  test('onDisconnect announces the first reconnect attempt', async () => {
    const harness = build();
    await establish(harness);

    harness.factory.current.close();
    await Promise.resolve();

    expect(harness.disconnects).toHaveLength(1);
    expect(harness.disconnects[0].nextReconnectAttempt).toBe(1);
    expect(harness.disconnects[0].nextReconnectDelayMs).toBeGreaterThan(0);
    expect(harness.disconnects[0].error).toBeInstanceOf(Error);
  });

  test('a reconnect opens a new socket and fires onConnect again', async () => {
    const harness = build();
    await establish(harness);
    expect(harness.factory.sockets).toHaveLength(1);

    harness.factory.current.close();
    await runReconnectTimer(harness);
    expect(harness.factory.sockets).toHaveLength(2);

    harness.factory.current.acceptConnection();
    harness.factory.current.sendToClient(initialConnection());
    await Promise.resolve();

    expect(harness.connects).toHaveLength(2);
    expect(harness.connection.isActive).toBe(true);
  });

  test('the connection object and its cache survive a reconnect', async () => {
    const harness = build();
    await establish(harness);
    const cacheBefore = harness.connection.db;

    harness.factory.current.close();
    await runReconnectTimer(harness);
    harness.factory.current.acceptConnection();
    harness.factory.current.sendToClient(initialConnection());
    await Promise.resolve();

    expect(harness.connection.db).toBe(cacheBefore);
  });

  test('the retained token is used to reconnect, keeping the identity stable', async () => {
    // Connect anonymously; the server issues a token.
    const harness = build();
    await establish(harness);
    expect(harness.connects[0].token).toBe(TOKEN);

    harness.factory.current.close();
    await runReconnectTimer(harness);

    expect(harness.factory.current.connectArgs?.authToken).toBe(TOKEN);
  });

  test('a session id is sent so the server can supersede the old connection', async () => {
    const harness = build();
    await establish(harness);
    const sessionId = harness.factory.sockets[0].connectArgs?.sessionId;
    expect(sessionId).toBeTruthy();

    harness.factory.current.close();
    await runReconnectTimer(harness);

    // The same session id identifies both connections as one client session.
    expect(harness.factory.current.connectArgs?.sessionId).toBe(sessionId);
    // Each connection still has its own connection id.
    expect(harness.factory.current.connectArgs?.connectionId).toBeTruthy();
  });

  test('reconnection is off unless requested', async () => {
    const harness = build({ automaticReconnect: false });
    await establish(harness);

    harness.factory.current.close();
    await vi.runOnlyPendingTimersAsync();

    expect(harness.disconnects).toHaveLength(1);
    expect(harness.disconnects[0].nextReconnectAttempt).toBeUndefined();
    expect(harness.factory.sockets).toHaveLength(1);
  });
});

describe('failed reconnect attempts', () => {
  test('onConnectError announces the next attempt', async () => {
    const harness = build();
    await establish(harness);

    harness.factory.current.close();
    await runReconnectTimer(harness);
    // The attempt's socket dies before completing its handshake.
    harness.factory.current.close();
    await Promise.resolve();

    expect(harness.connectErrors).toHaveLength(1);
    expect(harness.connectErrors[0].nextReconnectAttempt).toBe(2);
    // A failed attempt is not a lost connection, so no second onDisconnect.
    expect(harness.disconnects).toHaveLength(1);
  });

  test('the attempt number grows across consecutive failures', async () => {
    const harness = build();
    await establish(harness);

    harness.factory.current.close();
    for (let expected = 2; expected <= 4; expected++) {
      await runReconnectTimer(harness);
      harness.factory.current.close();
      await Promise.resolve();
      expect(
        harness.connectErrors[harness.connectErrors.length - 1]
          .nextReconnectAttempt
      ).toBe(expected);
    }
  });

  test('the delay grows across consecutive failures', async () => {
    const harness = build();
    await establish(harness);

    harness.factory.current.close();
    const delays: number[] = [harness.disconnects[0].nextReconnectDelayMs!];
    for (let i = 0; i < 3; i++) {
      await runReconnectTimer(harness);
      harness.factory.current.close();
      await Promise.resolve();
      delays.push(
        harness.connectErrors[harness.connectErrors.length - 1]
          .nextReconnectDelayMs!
      );
    }

    // Jitter makes individual steps noisy, so compare the ends of the run.
    expect(delays[delays.length - 1]).toBeGreaterThan(delays[0]);
  });

  test('a failure to open the socket at all counts as a failed attempt', async () => {
    const harness = build();
    await establish(harness);

    harness.factory.current.close();
    harness.factory.connectError = new Error('server unreachable');
    await runReconnectTimer(harness);

    expect(harness.connectErrors).toHaveLength(1);
    expect(harness.connectErrors[0].error?.message).toBe('server unreachable');
    expect(harness.connectErrors[0].nextReconnectAttempt).toBe(2);
  });

  test('the attempt counter resets once a connection is established', async () => {
    const harness = build();
    await establish(harness);

    harness.factory.current.close();
    await runReconnectTimer(harness);
    harness.factory.current.close();
    await Promise.resolve();
    expect(
      harness.connectErrors[harness.connectErrors.length - 1]
        .nextReconnectAttempt
    ).toBe(2);

    await runReconnectTimer(harness);
    harness.factory.current.acceptConnection();
    harness.factory.current.sendToClient(initialConnection());
    await Promise.resolve();

    // A later drop starts again at attempt 1.
    harness.factory.current.close();
    await Promise.resolve();
    expect(
      harness.disconnects[harness.disconnects.length - 1].nextReconnectAttempt
    ).toBe(1);
  });

  test('an initial connection failure is not retried', async () => {
    const harness = build();
    await settle(harness);

    // The socket dies before ever completing a handshake.
    harness.factory.current.close();
    await vi.runOnlyPendingTimersAsync();

    expect(harness.connectErrors).toHaveLength(1);
    expect(harness.connectErrors[0].nextReconnectAttempt).toBeUndefined();
    expect(harness.factory.sockets).toHaveLength(1);
  });
});

describe('terminal failures', () => {
  test('a reconnect under a different identity stops the SDK', async () => {
    const harness = build();
    await establish(harness, anIdentity);

    harness.factory.current.close();
    await runReconnectTimer(harness);
    harness.factory.current.acceptConnection();
    // The server hands us a different identity: the token was replaced.
    harness.factory.current.sendToClient(initialConnection(bobIdentity));
    await Promise.resolve();

    const lastError =
      harness.connectErrors[harness.connectErrors.length - 1].error;
    expect(lastError).toBeInstanceOf(IdentityChangedError);
    expect(
      harness.connectErrors[harness.connectErrors.length - 1]
        .nextReconnectAttempt
    ).toBeUndefined();

    // No further attempts are scheduled.
    const socketsBefore = harness.factory.sockets.length;
    await vi.runOnlyPendingTimersAsync();
    expect(harness.factory.sockets).toHaveLength(socketsBefore);
  });
});

describe('explicit disconnect()', () => {
  test('fires onDisconnect and stops reconnecting', async () => {
    const harness = build();
    await establish(harness);

    harness.connection.disconnect();
    harness.factory.current.close();
    await vi.runOnlyPendingTimersAsync();

    expect(harness.disconnects).toHaveLength(1);
    expect(harness.disconnects[0].nextReconnectAttempt).toBeUndefined();
    expect(harness.factory.sockets).toHaveLength(1);
  });

  test('fires onDisconnect when called while reconnecting', async () => {
    const harness = build();
    await establish(harness);

    harness.factory.current.close();
    await Promise.resolve();
    expect(harness.disconnects[0].nextReconnectAttempt).toBe(1);

    // Called between attempts, when there is no live socket whose close event
    // would otherwise end the connection.
    harness.connection.disconnect();
    await vi.runOnlyPendingTimersAsync();

    expect(harness.disconnects).toHaveLength(2);
    expect(harness.disconnects[1].nextReconnectAttempt).toBeUndefined();
    // The scheduled attempt was cancelled.
    expect(harness.factory.sockets).toHaveLength(1);
  });

  test('cancels a scheduled attempt even after several failures', async () => {
    const harness = build();
    await establish(harness);

    harness.factory.current.close();
    await runReconnectTimer(harness);
    harness.factory.current.close();
    await Promise.resolve();

    harness.connection.disconnect();
    const socketsBefore = harness.factory.sockets.length;
    await vi.runOnlyPendingTimersAsync();

    expect(harness.factory.sockets).toHaveLength(socketsBefore);
    expect(
      harness.disconnects[harness.disconnects.length - 1].nextReconnectAttempt
    ).toBeUndefined();
  });
});

describe('calls while reconnecting', () => {
  test('a reducer call fails immediately rather than queueing', async () => {
    const harness = build();
    await establish(harness);
    harness.factory.current.close();
    await Promise.resolve();

    await expect(
      harness.connection.reducers.createPlayer({
        name: 'Alice',
        location: { x: 1, y: 2 },
      })
    ).rejects.toBeInstanceOf(DisconnectedError);
  });

  test('an in-flight call settles with an unknown-result error', async () => {
    const harness = build();
    await establish(harness);

    const pending = harness.connection.reducers.createPlayer({
      name: 'Alice',
      location: { x: 1, y: 2 },
    });
    // The connection drops before the server acknowledges the call.
    harness.factory.current.close();
    await Promise.resolve();

    await expect(pending).rejects.toBeInstanceOf(UnknownCallResultError);
  });

  test('calls are not rejected without automatic reconnection', async () => {
    const harness = build({ automaticReconnect: false });
    await establish(harness);
    harness.factory.current.close();
    await Promise.resolve();

    // Legacy behavior: the call queues on the dead socket rather than failing.
    let settled = false;
    void harness.connection.reducers
      .createPlayer({ name: 'Alice', location: { x: 1, y: 2 } })
      .then(
        () => (settled = true),
        () => (settled = true)
      );
    await Promise.resolve();
    expect(settled).toBe(false);
  });
});

describe('replaying subscriptions', () => {
  /** The last batch-subscribe message the connection sent, if any. */
  function lastSubscribeBatch(harness: Harness): SubscribeBatch | undefined {
    const messages = harness.factory.current.outgoingMessages;
    for (let i = messages.length - 1; i >= 0; i--) {
      const message = messages[i];
      if (message.tag === 'SubscribeBatch') {
        return message.value;
      }
    }
    return undefined;
  }

  async function establishWithSubscription(): Promise<Harness> {
    const harness = build();
    await establish(harness);
    harness.connection.subscriptionBuilder().subscribe(['SELECT * FROM user']);
    await Promise.resolve();
    return harness;
  }

  test('a reconnect replays live subscriptions in one batch', async () => {
    const harness = await establishWithSubscription();

    harness.factory.current.close();
    await runReconnectTimer(harness);
    harness.factory.current.acceptConnection();
    harness.factory.current.sendToClient(initialConnection());
    await Promise.resolve();

    const batch = lastSubscribeBatch(harness);
    expect(batch).toBeDefined();
    expect(batch!.sets).toHaveLength(1);
  });

  test('replayed sets are registered under fresh query set ids', async () => {
    const harness = await establishWithSubscription();
    const originalSubscribe = harness.factory.current.outgoingMessages.find(
      message => message.tag === 'Subscribe'
    );
    const originalQuerySetId = originalSubscribe!.value.querySetId.id;

    harness.factory.current.close();
    await runReconnectTimer(harness);
    harness.factory.current.acceptConnection();
    harness.factory.current.sendToClient(initialConnection());
    await Promise.resolve();

    const batch = lastSubscribeBatch(harness);
    expect(batch!.sets[0].querySetId.id).not.toBe(originalQuerySetId);
  });

  test('nothing is replayed when there are no subscriptions', async () => {
    const harness = build();
    await establish(harness);

    harness.factory.current.close();
    await runReconnectTimer(harness);
    harness.factory.current.acceptConnection();
    harness.factory.current.sendToClient(initialConnection());
    await Promise.resolve();

    expect(lastSubscribeBatch(harness)).toBeUndefined();
  });

  test('rows unchanged across the outage produce no callbacks', async () => {
    const harness = await establishWithSubscription();
    const querySetId = harness.factory.current.outgoingMessages.find(
      message => message.tag === 'Subscribe'
    )!.value.querySetId.id;

    // The server delivers one row for the subscription.
    harness.factory.current.sendToClient(
      ServerMessage.SubscribeApplied({
        requestId: 1,
        querySetId: { id: querySetId },
        rows: {
          tables: [
            {
              table: 'user',
              rows: {
                sizeHint: { tag: 'RowOffsets', value: [0n] },
                rowsData: encodeUser({
                  identity: anIdentity,
                  username: 'Alice',
                }),
              },
            },
          ],
        },
      })
    );
    await Promise.resolve();

    const inserts: string[] = [];
    const updates: string[] = [];
    const deletes: string[] = [];
    harness.connection.db.user.onInsert((_ctx, row) =>
      inserts.push(row.username)
    );
    harness.connection.db.user.onUpdate((_ctx, _old, row) =>
      updates.push(row.username)
    );
    harness.connection.db.user.onDelete((_ctx, row) =>
      deletes.push(row.username)
    );

    harness.factory.current.close();
    await runReconnectTimer(harness);
    harness.factory.current.acceptConnection();
    harness.factory.current.sendToClient(initialConnection());
    await Promise.resolve();

    // The replay returns the same row it had before.
    const batch = lastSubscribeBatch(harness)!;
    harness.factory.current.sendToClient(
      ServerMessage.SubscribeBatchApplied({
        requestId: batch.requestId,
        results: [
          {
            querySetId: batch.sets[0].querySetId,
            outcome: {
              tag: 'Applied',
              value: {
                tables: [
                  {
                    table: 'user',
                    rows: {
                      sizeHint: { tag: 'RowOffsets', value: [0n] },
                      rowsData: encodeUser({
                        identity: anIdentity,
                        username: 'Alice',
                      }),
                    },
                  },
                ],
              },
            },
          },
        ],
      })
    );
    await Promise.resolve();

    expect(inserts).toEqual([]);
    expect(updates).toEqual([]);
    expect(deletes).toEqual([]);
    // The row is still readable from the cache.
    expect(harness.connection.db.user.count()).toBe(1n);
  });

  test('a row which changed during the outage produces one update callback', async () => {
    const harness = await establishWithSubscription();
    const querySetId = harness.factory.current.outgoingMessages.find(
      message => message.tag === 'Subscribe'
    )!.value.querySetId.id;

    harness.factory.current.sendToClient(
      ServerMessage.SubscribeApplied({
        requestId: 1,
        querySetId: { id: querySetId },
        rows: {
          tables: [
            {
              table: 'user',
              rows: {
                sizeHint: { tag: 'RowOffsets', value: [0n] },
                rowsData: encodeUser({
                  identity: anIdentity,
                  username: 'Alice',
                }),
              },
            },
          ],
        },
      })
    );
    await Promise.resolve();

    const updates: { from: string; to: string }[] = [];
    harness.connection.db.user.onUpdate((_ctx, oldRow, newRow) =>
      updates.push({ from: oldRow.username, to: newRow.username })
    );

    harness.factory.current.close();
    await runReconnectTimer(harness);
    harness.factory.current.acceptConnection();
    harness.factory.current.sendToClient(initialConnection());
    await Promise.resolve();

    const batch = lastSubscribeBatch(harness)!;
    harness.factory.current.sendToClient(
      ServerMessage.SubscribeBatchApplied({
        requestId: batch.requestId,
        results: [
          {
            querySetId: batch.sets[0].querySetId,
            outcome: {
              tag: 'Applied',
              value: {
                tables: [
                  {
                    table: 'user',
                    rows: {
                      sizeHint: { tag: 'RowOffsets', value: [0n] },
                      // The same identity, renamed while we were away.
                      rowsData: encodeUser({
                        identity: anIdentity,
                        username: 'Alicia',
                      }),
                    },
                  },
                ],
              },
            },
          },
        ],
      })
    );
    await Promise.resolve();

    expect(updates).toEqual([{ from: 'Alice', to: 'Alicia' }]);
  });

  test('a row deleted during the outage produces a delete callback', async () => {
    const harness = await establishWithSubscription();
    const querySetId = harness.factory.current.outgoingMessages.find(
      message => message.tag === 'Subscribe'
    )!.value.querySetId.id;

    harness.factory.current.sendToClient(
      ServerMessage.SubscribeApplied({
        requestId: 1,
        querySetId: { id: querySetId },
        rows: {
          tables: [
            {
              table: 'user',
              rows: {
                sizeHint: { tag: 'RowOffsets', value: [0n] },
                rowsData: encodeUser({
                  identity: anIdentity,
                  username: 'Alice',
                }),
              },
            },
          ],
        },
      })
    );
    await Promise.resolve();

    const deletes: string[] = [];
    harness.connection.db.user.onDelete((_ctx, row) =>
      deletes.push(row.username)
    );

    harness.factory.current.close();
    await runReconnectTimer(harness);
    harness.factory.current.acceptConnection();
    harness.factory.current.sendToClient(initialConnection());
    await Promise.resolve();

    // The replay returns no rows: the row is gone.
    const batch = lastSubscribeBatch(harness)!;
    harness.factory.current.sendToClient(
      ServerMessage.SubscribeBatchApplied({
        requestId: batch.requestId,
        results: [
          {
            querySetId: batch.sets[0].querySetId,
            outcome: { tag: 'Applied', value: { tables: [] } },
          },
        ],
      })
    );
    await Promise.resolve();

    expect(deletes).toEqual(['Alice']);
    expect(harness.connection.db.user.count()).toBe(0n);
  });

  test('a rejected replayed query reports its error while the rest apply', async () => {
    const harness = build();
    await establish(harness);

    const errors: string[] = [];
    const applied: number[] = [];
    harness.connection
      .subscriptionBuilder()
      .onApplied(() => applied.push(1))
      .onError(ctx => errors.push(ctx.event!.message))
      .subscribe(['SELECT * FROM user']);
    harness.connection
      .subscriptionBuilder()
      .onApplied(() => applied.push(2))
      .onError(ctx => errors.push(ctx.event!.message))
      .subscribe(['SELECT * FROM no_such_table']);
    await Promise.resolve();

    harness.factory.current.close();
    await runReconnectTimer(harness);
    harness.factory.current.acceptConnection();
    harness.factory.current.sendToClient(initialConnection());
    await Promise.resolve();

    const batch = lastSubscribeBatch(harness)!;
    expect(batch.sets).toHaveLength(2);
    harness.factory.current.sendToClient(
      ServerMessage.SubscribeBatchApplied({
        requestId: batch.requestId,
        results: [
          {
            querySetId: batch.sets[0].querySetId,
            outcome: { tag: 'Applied', value: { tables: [] } },
          },
          {
            querySetId: batch.sets[1].querySetId,
            outcome: { tag: 'Error', value: 'no such table: no_such_table' },
          },
        ],
      })
    );
    await Promise.resolve();

    expect(errors).toEqual(['no such table: no_such_table']);
    // The healthy set applied, firing its onApplied again on the new connection.
    expect(applied).toContain(1);
  });
});

describe('token provider', () => {
  test('is not called while the retained token has life left', async () => {
    const nowSeconds = Math.floor(Date.now() / 1000);
    const longLived = `header.${Buffer.from(
      JSON.stringify({ iat: nowSeconds, exp: nowSeconds + 3600 })
    ).toString('base64url')}.sig`;

    const provider = vi.fn(async () => 'fresh-token');
    const harness = build({ token: longLived, tokenProvider: provider });
    await establish(harness);

    harness.factory.current.close();
    await runReconnectTimer(harness);

    expect(provider).not.toHaveBeenCalled();
    expect(harness.factory.current.connectArgs?.authToken).toBe(longLived);
  });

  test('supplies a fresh token when the retained one is close to expiring', async () => {
    const nowSeconds = Math.floor(Date.now() / 1000);
    const expiring = `header.${Buffer.from(
      JSON.stringify({ iat: nowSeconds - 3595, exp: nowSeconds + 5 })
    ).toString('base64url')}.sig`;

    const provider = vi.fn(async () => 'fresh-token');
    const harness = build({ token: expiring, tokenProvider: provider });
    await establish(harness);

    harness.factory.current.close();
    await runReconnectTimer(harness);

    expect(provider).toHaveBeenCalled();
    expect(harness.factory.current.connectArgs?.authToken).toBe('fresh-token');
  });

  test('a provider failure counts as a failed attempt, not a terminal error', async () => {
    const provider = vi.fn(async () => {
      throw new Error('token endpoint down');
    });
    const harness = build({ token: 'opaque', tokenProvider: provider });
    await establish(harness);

    harness.factory.current.close();
    await runReconnectTimer(harness);

    expect(harness.connectErrors).toHaveLength(1);
    expect(harness.connectErrors[0].error?.message).toBe('token endpoint down');
    // The SDK keeps trying.
    expect(harness.connectErrors[0].nextReconnectAttempt).toBe(2);
  });
});

describe('reconnect regressions', () => {
  test('uses a fresh connection id for every attempt and retains the session id', async () => {
    const harness = build();
    await establish(harness);
    const first = harness.factory.current.connectArgs!;
    const establishedId = harness.connection.connectionId.toHexString();
    harness.factory.current.close();
    await runReconnectTimer(harness);
    const second = harness.factory.current.connectArgs!;
    expect(second.connectionId).not.toBe(establishedId);
    expect(second.connectionId).not.toBe(first.connectionId);
    expect(second.sessionId).toBe(first.sessionId);
    harness.factory.current.close();
    await runReconnectTimer(harness);
    expect(harness.factory.current.connectArgs!.connectionId).not.toBe(
      second.connectionId
    );
  });

  test('session-busy responses retry without advancing the backoff', async () => {
    const harness = build();
    await establish(harness);
    harness.factory.current.close();
    for (let i = 0; i < 3; i++) {
      await runReconnectTimer(harness);
      harness.factory.current.acceptConnection();
      harness.factory.current.serverClose(4000, 'session busy');
      expect(harness.connectErrors.at(-1)?.nextReconnectAttempt).toBe(1);
      expect(
        harness.connectErrors.at(-1)?.nextReconnectDelayMs
      ).toBeLessThanOrEqual(1500);
    }
  });

  test('ignores all late events from a discarded socket', async () => {
    const harness = build();
    await establish(harness);
    const old = harness.factory.current;
    old.error(new Error('lost'));
    expect(old.closed).toBe(true);
    old.acceptConnection();
    old.sendToClient(initialConnection(bobIdentity));
    old.close();
    expect(harness.connection.isActive).toBe(false);
    expect(harness.disconnects).toHaveLength(1);
    expect(harness.connects).toHaveLength(1);
    await runReconnectTimer(harness);
    await establish(harness);
    old.sendToClient(initialConnection(bobIdentity));
    expect(harness.connection.identity).toEqual(anIdentity);
    expect(harness.connects).toHaveLength(2);
  });

  test('waits for InitialConnection before allowing calls on a reconnect', async () => {
    const harness = build();
    await establish(harness);
    harness.factory.current.close();
    await runReconnectTimer(harness);
    harness.factory.current.acceptConnection();
    expect(harness.connection.isActive).toBe(false);
    expect(harness.connection.isReconnecting).toBe(true);
    await expect(
      harness.connection.callReducer('test', new Uint8Array())
    ).rejects.toBeInstanceOf(DisconnectedError);
    expect(harness.factory.current.outgoingMessages).toEqual([]);
  });

  test('does not send a queued call on the next socket after rejecting it', async () => {
    const harness = build();
    await establish(harness);
    const pending = harness.connection.callReducer('test', new Uint8Array());
    const rejected = expect(pending).rejects.toBeInstanceOf(
      UnknownCallResultError
    );
    harness.factory.current.close();
    await rejected;
    await runReconnectTimer(harness);
    await establish(harness);
    expect(harness.factory.current.outgoingMessages).toEqual([]);
  });

  test('explicit disconnect reports no error and settles in-flight procedures', async () => {
    const harness = build();
    await establish(harness);
    const rejected = expect(
      harness.connection.callProcedure('test', new Uint8Array())
    ).rejects.toBeInstanceOf(UnknownCallResultError);
    harness.connection.disconnect();
    await rejected;
    expect(harness.disconnects).toEqual([
      {
        error: undefined,
        nextReconnectAttempt: undefined,
        nextReconnectDelayMs: undefined,
      },
    ]);
  });

  test('disconnect from onDisconnect cancels retries immediately', async () => {
    const harness = build();
    await establish(harness);
    harness.connection['onDisconnect'](() => {
      if (!harness.connection.isDisconnectRequested)
        harness.connection.disconnect();
    });
    harness.factory.current.close();
    expect(vi.getTimerCount()).toBe(0);
    expect(harness.disconnects).toHaveLength(2);
  });

  test('disconnect during token refresh prevents opening another socket', async () => {
    let resolveToken!: (token: string) => void;
    const provider = vi.fn(
      () =>
        new Promise<string>(resolve => {
          resolveToken = resolve;
        })
    );
    const harness = build({ tokenProvider: provider });
    await establish(harness);
    harness.factory.current.close();
    await vi.runOnlyPendingTimersAsync();
    expect(provider).toHaveBeenCalledOnce();
    harness.connection.disconnect();
    resolveToken('fresh-token');
    await settle(harness);
    expect(harness.factory.sockets).toHaveLength(1);
    expect(harness.connection.isReconnecting).toBe(false);
  });

  test('subscribes during the outage and onConnect are sent only in the replay batch', async () => {
    const harness = build();
    await establish(harness);
    harness.connection.subscriptionBuilder().subscribe('SELECT * FROM user');
    harness.factory.current.close();
    harness.connection.subscriptionBuilder().subscribe('SELECT * FROM player');
    harness.connection['onConnect'](conn =>
      conn.subscriptionBuilder().subscribe('SELECT * FROM user')
    );
    await runReconnectTimer(harness);
    await establish(harness);
    const messages = harness.factory.current.outgoingMessages;
    expect(messages.map(m => m.tag)).toEqual(['SubscribeBatch']);
    const batch = messages[0];
    if (batch.tag !== 'SubscribeBatch') throw new Error('Expected replay');
    expect(batch.value.sets).toHaveLength(3);
  });

  test.each(['before drop', 'during outage'] as const)(
    'unsubscribe %s ends locally and removes stale rows after reconnect',
    async timing => {
      const harness = build();
      await establish(harness);
      const handle = harness.connection
        .subscriptionBuilder()
        .subscribe('SELECT * FROM user');
      await Promise.resolve();
      const subscribe = harness.factory.current.outgoingMessages[0];
      if (subscribe.tag !== 'Subscribe')
        throw new Error('Expected subscription');
      harness.factory.current.sendToClient(
        ServerMessage.SubscribeApplied({
          ...subscribe.value,
          rows: {
            tables: [
              {
                table: 'user',
                rows: {
                  sizeHint: { tag: 'RowOffsets', value: [0n] },
                  rowsData: encodeUser({
                    identity: anIdentity,
                    username: 'Alice',
                  }),
                },
              },
            ],
          },
        })
      );
      const onEnd = vi.fn();
      if (timing === 'before drop') handle.unsubscribeThen(onEnd);
      harness.factory.current.close();
      if (timing === 'during outage') handle.unsubscribeThen(onEnd);
      expect(handle.isEnded()).toBe(true);
      expect(handle.isActive()).toBe(false);
      expect(onEnd).toHaveBeenCalledOnce();
      expect(harness.connection.db.user.count()).toBe(1n);
      await runReconnectTimer(harness);
      await establish(harness);
      expect(harness.factory.current.outgoingMessages).toEqual([]);
      expect(harness.connection.db.user.count()).toBe(0n);
    }
  );

  test('legacy sockets still emit both error and close events', async () => {
    const harness = build({ automaticReconnect: false });
    await establish(harness);
    harness.factory.current.error(new Error('network error'));
    harness.factory.current.close();
    expect(harness.connectErrors).toHaveLength(1);
    expect(harness.disconnects).toHaveLength(1);
  });

  test.each([1002, 1003, 1007, 1008])(
    'protocol/policy close code %i is terminal',
    async code => {
      const harness = build();
      await establish(harness);
      harness.factory.current.serverClose(code);
      expect(harness.disconnects[0].nextReconnectAttempt).toBeUndefined();
      expect(harness.connection.isReconnecting).toBe(false);
      expect(vi.getTimerCount()).toBe(0);
    }
  );
});

describe('token rejection classification', () => {
  test.each([401, 403])(
    'status %i without a provider is terminal',
    async status => {
      const harness = build();
      await establish(harness);
      harness.factory.current.close();
      harness.factory.connectError = new WebSocketTokenError(
        status,
        'Rejected'
      );
      await runReconnectTimer(harness);
      expect(harness.connectErrors[0].nextReconnectAttempt).toBeUndefined();
      expect(harness.connection.isReconnecting).toBe(false);
    }
  );

  test('refreshes a rejected retained token once, then stops if the fresh token is rejected', async () => {
    const now = Date.now() / 1000;
    const token = `header.${Buffer.from(JSON.stringify({ iat: now, exp: now + 3600 })).toString('base64url')}.sig`;
    const provider = vi.fn(async () => 'replacement');
    const harness = build({ token, tokenProvider: provider });
    await establish(harness);
    harness.factory.current.close();
    harness.factory.connectError = new WebSocketTokenError(401, 'Unauthorized');
    await runReconnectTimer(harness);
    expect(provider).not.toHaveBeenCalled();
    expect(harness.connectErrors[0].nextReconnectAttempt).toBe(2);
    await runReconnectTimer(harness);
    expect(provider).toHaveBeenCalledOnce();
    expect(harness.connectErrors[1].nextReconnectAttempt).toBeUndefined();
  });

  test('a token exchange server error retries without classifying it as bad credentials', async () => {
    const harness = build();
    await establish(harness);
    harness.factory.current.close();
    harness.factory.connectError = new WebSocketTokenError(503, 'Unavailable');
    await runReconnectTimer(harness);
    expect(harness.connectErrors[0].nextReconnectAttempt).toBe(2);
  });
});

describe('handshake and replay boundaries', () => {
  test('subscriptions cancelled before the initial handshake are never sent', async () => {
    const harness = build();
    const cancelled = harness.connection
      .subscriptionBuilder()
      .subscribe('SELECT * FROM user');
    cancelled.unsubscribe();
    harness.connection.subscriptionBuilder().subscribe('SELECT * FROM player');
    await establish(harness);
    expect(cancelled.isEnded()).toBe(true);
    const messages = harness.factory.current.outgoingMessages;
    expect(messages).toHaveLength(1);
    expect(messages[0]).toMatchObject({
      tag: 'Subscribe',
      value: { queryStrings: ['SELECT * FROM player'] },
    });
  });

  test('an incomplete replay response is terminal', async () => {
    const harness = build();
    await establish(harness);
    harness.connection.subscriptionBuilder().subscribe('SELECT * FROM user');
    harness.factory.current.close();
    await runReconnectTimer(harness);
    await establish(harness);
    const message = harness.factory.current.outgoingMessages[0];
    if (message.tag !== 'SubscribeBatch') throw new Error('Expected replay');
    harness.factory.current.sendToClient(
      ServerMessage.SubscribeBatchApplied({
        requestId: message.value.requestId,
        results: [],
      })
    );
    expect(harness.disconnects.at(-1)?.nextReconnectAttempt).toBeUndefined();
    expect(harness.connection.isReconnecting).toBe(false);
    expect(harness.factory.current.closed).toBe(true);
  });

  test('disconnect in onConnect prevents subscription replay', async () => {
    const harness = build();
    await establish(harness);
    harness.connection.subscriptionBuilder().subscribe('SELECT * FROM user');
    harness.factory.current.close();
    harness.connection['onConnect'](() => harness.connection.disconnect());
    await runReconnectTimer(harness);
    await establish(harness);
    expect(harness.factory.current.closed).toBe(true);
    expect(harness.factory.current.outgoingMessages).toEqual([]);
    expect(vi.getTimerCount()).toBe(0);
  });
});
