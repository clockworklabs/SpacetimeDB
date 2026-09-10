import { afterEach, describe, expect, it } from 'vitest';
import { createServer, type Server, type ServerResponse } from 'node:http';
import { once } from 'node:events';
import type { AddressInfo, Socket } from 'node:net';
import { WebSocketServer, WebSocket } from 'ws';
import {
  BinaryReader,
  BinaryWriter,
  ConnectionId,
  Identity,
  Timestamp,
} from '../src';
import {
  Container,
  ContainerSession,
  ContainerSessionError,
  ContainerSessionCallError,
} from '../src/sdk/node';
import { ClientMessage, ServerMessage } from '../src/sdk/client_api/types';
import { INTERNAL_MANAGED_SESSION } from '../src/sdk/managed_session_lifecycle';
import WebsocketTestAdapter from '../src/sdk/websocket_test_adapter';
import { V2_WS_PROTOCOL, V3_WS_PROTOCOL } from '../src/sdk/websocket_protocols';
import {
  decodeClientMessagesV3,
  encodeServerMessagesV3,
} from '../src/sdk/websocket_v3_frames';
import { DbConnection } from '../test-app/src/module_bindings';
import { encodePlayer, makeQueryRows, makeQuerySetUpdate } from './utils';

const OWN = '1'.repeat(64);
const TARGET = '2'.repeat(64);
const sessions: ContainerSession<DbConnection>[] = [];
const fixtures: Fixture[] = [];
class Deferred<T> {
  resolve!: (value: T) => void;
  reject!: (reason?: unknown) => void;
  promise = new Promise<T>((resolve, reject) => {
    this.resolve = resolve;
    this.reject = reject;
  });
}
function send(socket: WebSocket, message: ServerMessage): void {
  const writer = new BinaryWriter(1024);
  ServerMessage.serialize(writer, message);
  const payload =
    socket.protocol === V3_WS_PROTOCOL
      ? encodeServerMessagesV3(new BinaryWriter(1024), [writer.getBuffer()])
      : writer.getBuffer();
  socket.send(Buffer.concat([Buffer.from([0]), payload]));
}
function credential(
  response: ServerResponse,
  expiry: number,
  token = 'fixture-session-token'
): void {
  const body = JSON.stringify({ token, expires_unix_seconds: expiry });
  response
    .writeHead(200, {
      'Content-Type': 'application/json',
      'Cache-Control': 'no-store',
      'Content-Length': Buffer.byteLength(body),
    })
    .end(body);
}
class Fixture {
  server: Server;
  ws = new WebSocketServer({
    noServer: true,
    handleProtocols: () => this.protocol,
  });
  protocol: string = V2_WS_PROTOCOL;
  sockets = new Set<Socket>();
  peerClosures: Promise<void>[] = [];
  live = 0;
  maxLive = 0;
  connections = 0;
  requests = 0;
  calls: { generation: number; message: ClientMessage }[] = [];
  routes: string[] = [];
  targets: string[] = [];
  serverUri = '';
  broker = '';
  source!: Container;
  onRequest: (response: ServerResponse, count: number) => void = response =>
    credential(response, Math.floor(Date.now() / 1000) + 15);
  onConnection?: (socket: WebSocket, generation: number) => void;
  onMessage?: (
    socket: WebSocket,
    message: ClientMessage,
    generation: number
  ) => void;
  sender = OWN;
  initial = true;
  constructor() {
    this.server = createServer((request, response) => {
      if (request.url !== '/v1/credentials') {
        response.writeHead(500).end();
        return;
      }
      let body = '';
      request.on('data', data => {
        body += data;
      });
      request.on('end', () => {
        this.targets.push(JSON.parse(body).target_database);
        this.onRequest(response, ++this.requests);
      });
    });
    this.server.on('connection', socket => {
      this.sockets.add(socket);
      socket.once('close', () => this.sockets.delete(socket));
    });
    this.server.on('upgrade', (request, socket, head) => {
      this.routes.push(request.url!);
      this.ws.handleUpgrade(request, socket, head, client => {
        const generation = ++this.connections;
        this.live++;
        this.maxLive = Math.max(this.live, this.maxLive);
        this.peerClosures.push(
          new Promise<void>(resolve =>
            client.once('close', () => {
              this.live--;
              resolve();
            })
          )
        );
        client.on('error', () => {});
        client.on('message', bytes => {
          const data = new Uint8Array(bytes as Buffer);
          const messages =
            this.protocol === V3_WS_PROTOCOL
              ? decodeClientMessagesV3(data)
              : [data];
          for (const bytes of messages) {
            const message = ClientMessage.deserialize(new BinaryReader(bytes));
            this.calls.push({ generation, message });
            this.onMessage?.(client, message, generation);
            if (message.tag === 'Subscribe') {
              send(
                client,
                ServerMessage.SubscribeApplied({
                  requestId: message.value.requestId,
                  querySetId: message.value.querySetId,
                  rows: makeQueryRows(
                    'player',
                    encodePlayer({
                      id: generation,
                      userId: new Identity(OWN),
                      name: `generation-${generation}`,
                      location: { x: 0, y: 0 },
                    })
                  ),
                })
              );
            }
          }
        });
        if (this.initial) {
          const id = new URL(request.url!, 'http://127.0.0.1').searchParams.get(
            'connection_id'
          )!;
          send(
            client,
            ServerMessage.InitialConnection({
              identity: new Identity(this.sender),
              token: '',
              connectionId: new ConnectionId(BigInt('0x' + id)),
            })
          );
        }
        this.onConnection?.(client, generation);
      });
    });
  }
  async start(): Promise<this> {
    this.server.listen(0, '127.0.0.1');
    await once(this.server, 'listening');
    this.serverUri = `http://127.0.0.1:${(this.server.address() as AddressInfo).port}/`;
    this.broker = this.serverUri + 'v1/credentials';
    this.source = new Container({
      databaseIdentity: OWN,
      serverUri: this.serverUri,
      credentialBroker: this.broker,
    });
    fixtures.push(this);
    return this;
  }
  async joinPeers(): Promise<void> {
    await Promise.all(this.peerClosures);
  }
  async close(): Promise<void> {
    for (const socket of this.sockets) socket.destroy();
    await new Promise<void>(resolve => this.ws.close(() => resolve()));
    await new Promise<void>((resolve, reject) =>
      this.server.close(error => (error ? reject(error) : resolve()))
    );
  }
}
afterEach(async () => {
  await Promise.all(sessions.splice(0).map(session => session.shutdown()));
  await Promise.all(fixtures.splice(0).map(fixture => fixture.close()));
});
function own(
  fixture: Fixture,
  onConnect: (connection: DbConnection, info: { generation: number }) => void,
  onEvent?: ConstructorParameters<
    typeof ContainerSession<DbConnection>
  >[1]['onEvent']
): ContainerSession<DbConnection> {
  const session = new ContainerSession(DbConnection, {
    container: fixture.source,
    target: new Identity(TARGET),
    onConnect,
    onEvent,
    compression: 'none',
  });
  sessions.push(session);
  return session;
}

describe('managed Node container sessions', () => {
  it('renews with a fresh cache/subscription and no overlapping sockets or replay', async () => {
    const fixture = await new Fixture().start();
    fixture.onRequest = (response, count) =>
      credential(
        response,
        Math.floor(Date.now() / 1000) + (count === 1 ? 3 : 15)
      );
    const renewed = new Deferred<void>();
    const connections: DbConnection[] = [];
    const rows: number[][] = [];
    let uncertain: Promise<unknown> | undefined;
    const session = own(fixture, (connection, info) => {
      connections.push(connection);
      expect(connection.db.player.count()).toBe(0n);
      connection
        .subscriptionBuilder()
        .onApplied(() => {
          rows.push([...connection.db.player.iter()].map(row => row.id));
          if (info.generation === 2) renewed.resolve();
        })
        .subscribe('SELECT * FROM player');
      if (info.generation === 1)
        uncertain = connection.reducers
          .createPlayer({ name: 'once', location: { x: 0, y: 0 } })
          .catch(error => error);
    });
    const run = session.run();
    await renewed.promise;
    expect(await uncertain).toMatchObject({ outcome: 'unknown' });
    expect(rows).toEqual([[1], [2]]);
    expect(fixture.maxLive).toBe(1);
    expect(
      fixture.calls.filter(call => call.message.tag === 'CallReducer')
    ).toHaveLength(1);
    expect(fixture.targets.every(target => target === TARGET)).toBe(true);
    expect(
      fixture.routes.every(route =>
        route.startsWith(`/v1/database/${TARGET}/subscribe?`)
      )
    ).toBe(true);
    await expect(
      connections[0].reducers.createPlayer({
        name: 'late',
        location: { x: 0, y: 0 },
      })
    ).rejects.toMatchObject({ outcome: 'not_sent' });
    await expect(
      connections[0].callProcedure('late', new Uint8Array())
    ).rejects.toMatchObject({ outcome: 'not_sent' });
    expect(() =>
      connections[0].subscriptionBuilder().subscribe('SELECT * FROM player')
    ).toThrow(ContainerSessionError);
    await session.shutdown();
    await run;
  }, 10000);

  it('does not rotate repeatedly on the same lease expiry and denial is terminal', async () => {
    const fixture = await new Fixture().start();
    const expiry = Math.floor(Date.now() / 1000) + 4;
    fixture.onRequest = (response, count) =>
      count < 3
        ? credential(response, expiry)
        : response.writeHead(403, { 'Content-Length': 0 }).end();
    let connected = 0;
    const session = own(fixture, () => {
      connected++;
    });
    await expect(session.run()).rejects.toMatchObject({ code: 'denied' });
    expect(fixture.requests).toBe(3);
    expect(connected).toBe(1);
    await fixture.joinPeers();
    expect(fixture.live).toBe(0);
    await expect(session.run()).rejects.toMatchObject({ code: 'terminated' });
  }, 10000);

  it('expires and closes the old socket while a refresh response is stalled', async () => {
    const fixture = await new Fixture().start();
    fixture.onRequest = (response, count) => {
      if (count === 1) credential(response, Math.floor(Date.now() / 1000) + 3);
    };
    const expired = new Deferred<void>();
    const pause = new AbortController();
    const session = own(
      fixture,
      () => {},
      event => {
        if (event.type === 'disconnected' && event.reason === 'expired') {
          expired.resolve();
          pause.abort();
        }
      }
    );
    const run = session.run(pause.signal);
    await expired.promise;
    await run;
    await fixture.joinPeers();
    expect(fixture.live).toBe(0);
    expect(fixture.connections).toBe(1);
  }, 10000);

  it('joins cancellation and can resume with another fresh generation', async () => {
    const fixture = await new Fixture().start();
    let pause = new AbortController();
    const session = own(fixture, () => pause.abort());
    await session.run(pause.signal);
    await fixture.joinPeers();
    expect(fixture.live).toBe(0);
    pause = new AbortController();
    await session.run(pause.signal);
    expect(fixture.connections).toBe(2);
    expect(fixture.requests).toBe(2);
    expect(fixture.maxLive).toBe(1);
  });

  it('rejects a wrong sender before exposing onConnect, including immediate close', async () => {
    for (const immediateClose of [false, true]) {
      const fixture = await new Fixture().start();
      fixture.sender = TARGET;
      if (immediateClose) fixture.onConnection = socket => socket.close();
      let exposed = 0;
      const session = own(fixture, () => {
        exposed++;
      });
      await expect(session.run()).rejects.toMatchObject({
        code: 'sender_mismatch',
      });
      expect(exposed).toBe(0);
      await fixture.joinPeers();
      expect(fixture.live).toBe(0);
    }
  });

  it('keeps a confirmed result when a later seal wins other pending calls', async () => {
    const fixture = await new Fixture().start();
    const result = new Deferred<void>();
    fixture.onMessage = (socket, message) => {
      if (message.tag === 'CallReducer')
        send(
          socket,
          ServerMessage.ReducerResult({
            requestId: message.value.requestId,
            timestamp: new Timestamp(0n),
            result: { tag: 'OkEmpty' },
          })
        );
    };
    const pause = new AbortController();
    const session = own(fixture, connection => {
      void connection.reducers
        .createPlayer({ name: 'confirmed', location: { x: 0, y: 0 } })
        .then(
          () => {
            result.resolve();
            pause.abort();
          },
          error => result.reject(error)
        );
    });
    const run = session.run(pause.signal);
    await result.promise;
    await run;
    expect(
      fixture.calls.filter(call => call.message.tag === 'CallReducer')
    ).toHaveLength(1);
  });

  it('rejects a queued managed call as not sent before socket readiness', async () => {
    const fixture = await new Fixture().start();
    const ready = new Deferred<DbConnection>();
    const session = own(fixture, connection => ready.resolve(connection));
    const run = session.run();
    const connection = await ready.promise;
    connection.isActive = false;
    const queued = connection.reducers
      .createPlayer({ name: 'queued', location: { x: 0, y: 0 } })
      .catch(error => error);
    await session.shutdown();
    await run;
    expect(await queued).toMatchObject({ outcome: 'not_sent' });
    expect(
      fixture.calls.filter(call => call.message.tag === 'CallReducer')
    ).toHaveLength(0);
  });

  it('seals before subscription callbacks and joins even when a callback throws', async () => {
    const fixture = await new Fixture().start();
    const applied = new Deferred<void>();
    let oldCall: Promise<unknown> | undefined;
    let laterSubscriptionErrors = 0;
    const session = own(fixture, connection => {
      connection
        .subscriptionBuilder()
        .onApplied(() => applied.resolve())
        .onError(() => {
          oldCall = connection.reducers
            .createPlayer({ name: 'after-seal', location: { x: 0, y: 0 } })
            .catch(error => error);
          throw new Error('application-private-error');
        })
        .subscribe('SELECT * FROM player');
      connection
        .subscriptionBuilder()
        .onError(() => laterSubscriptionErrors++)
        .subscribe('SELECT * FROM player');
    });
    const run = session.run().catch(error => error);
    await applied.promise;
    await session.shutdown();
    expect(await run).toMatchObject({ code: 'callback_failed' });
    expect(await oldCall).toBeInstanceOf(ContainerSessionCallError);
    expect(await oldCall).toMatchObject({ outcome: 'not_sent' });
    expect(laterSubscriptionErrors).toBe(1);
    await session.shutdown();
    expect(laterSubscriptionErrors).toBe(1);
    await fixture.joinPeers();
    expect(fixture.live).toBe(0);
  });

  it('joins the socket before reporting an application callback failure', async () => {
    const fixture = await new Fixture().start();
    const session = own(fixture, () => {
      throw new Error('private app failure');
    });
    await expect(session.run()).rejects.toMatchObject({
      code: 'callback_failed',
    });
    await fixture.joinPeers();
    expect(fixture.live).toBe(0);
  });

  it('cancels an in-flight initial credential request and owns its socket until close', async () => {
    const fixture = await new Fixture().start();
    const received = new Deferred<void>();
    let brokerClosed: Promise<unknown> | undefined;
    fixture.onRequest = response => {
      brokerClosed = once(response.socket!, 'close');
      received.resolve();
    };
    const pause = new AbortController();
    const session = own(fixture, () => {
      throw new Error('must not connect');
    });
    const run = session.run(pause.signal);
    await received.promise;
    pause.abort();
    await run;
    await brokerClosed;
    expect(fixture.connections).toBe(0);
    expect(fixture.requests).toBe(1);
  });

  it('classifies v3 batched calls as unsent when shutdown seals before the flush', async () => {
    const fixture = new Fixture();
    fixture.protocol = V3_WS_PROTOCOL;
    await fixture.start();
    const pause = new AbortController();
    let reducer: Promise<unknown> | undefined;
    let procedure: Promise<unknown> | undefined;
    const session = own(fixture, connection => {
      reducer = connection.reducers
        .createPlayer({ name: 'queued-v3', location: { x: 0, y: 0 } })
        .catch(error => error);
      procedure = connection
        .callProcedure('queued_procedure', new Uint8Array())
        .catch(error => error);
      pause.abort();
    });
    await session.run(pause.signal);
    await fixture.joinPeers();
    expect(await reducer).toMatchObject({ outcome: 'not_sent' });
    expect(await procedure).toMatchObject({ outcome: 'not_sent' });
    expect(fixture.calls).toHaveLength(0);
  });

  it('classifies v3 reducer/procedure handoffs without results as unknown', async () => {
    const fixture = new Fixture();
    fixture.protocol = V3_WS_PROTOCOL;
    await fixture.start();
    const received = new Deferred<void>();
    fixture.onMessage = () => {
      if (fixture.calls.length === 2) received.resolve();
    };
    let reducer: Promise<unknown> | undefined;
    let procedure: Promise<unknown> | undefined;
    const session = own(fixture, connection => {
      reducer = connection.reducers
        .createPlayer({ name: 'sent-v3', location: { x: 0, y: 0 } })
        .catch(error => error);
      procedure = connection
        .callProcedure('sent_procedure', new Uint8Array())
        .catch(error => error);
    });
    const run = session.run();
    await received.promise;
    await session.shutdown();
    await run;
    await fixture.joinPeers();
    expect(await reducer).toMatchObject({ outcome: 'unknown' });
    expect(await procedure).toMatchObject({ outcome: 'unknown' });
    expect(fixture.calls).toHaveLength(2);
  });

  it('reports unsupported async setup after joining, without an unhandled rejection', async () => {
    const fixture = await new Fixture().start();
    const session = own(fixture, async () => {
      await Promise.resolve();
      throw new Error('private async callback');
    });
    await expect(session.run()).rejects.toMatchObject({
      code: 'callback_failed',
    });
    await fixture.joinPeers();
    expect(fixture.live).toBe(0);
  });

  it('keeps a confirmed reducer result when its row callback initiates shutdown', async () => {
    const fixture = await new Fixture().start();
    const confirmed = new Deferred<void>();
    let querySetId = 0;
    fixture.onMessage = (socket, message) => {
      if (message.tag === 'Subscribe') querySetId = message.value.querySetId.id;
      if (message.tag === 'CallReducer')
        send(
          socket,
          ServerMessage.ReducerResult({
            requestId: message.value.requestId,
            timestamp: new Timestamp(0n),
            result: {
              tag: 'Ok',
              value: {
                retValue: new Uint8Array(),
                transactionUpdate: {
                  querySets: [
                    makeQuerySetUpdate(
                      querySetId,
                      'player',
                      encodePlayer({
                        id: 99,
                        userId: new Identity(OWN),
                        name: 'confirmed',
                        location: { x: 0, y: 0 },
                      })
                    ),
                  ],
                },
              },
            },
          })
        );
    };
    const session = own(fixture, connection => {
      connection.db.player.onInsert(context => {
        if (context.event.tag === 'Reducer') void session.shutdown();
      });
      connection
        .subscriptionBuilder()
        .onApplied(() => {
          void connection.reducers
            .createPlayer({ name: 'confirmed', location: { x: 0, y: 0 } })
            .then(
              () => confirmed.resolve(),
              error => confirmed.reject(error)
            );
        })
        .subscribe('SELECT * FROM player');
    });
    const run = session.run();
    await confirmed.promise;
    await run;
    await fixture.joinPeers();
    expect(fixture.live).toBe(0);
  });

  it('rejects invalid server configuration before broker I/O and owns run state', async () => {
    const fixture = await new Fixture().start();
    for (const serverUri of [
      fixture.serverUri + '?token=secret',
      fixture.serverUri + '#fragment',
      fixture.serverUri + '\u0000',
      'http://user:secret@127.0.0.1/',
    ]) {
      expect(
        () =>
          new ContainerSession(DbConnection, {
            container: fixture.source,
            serverUri,
            onConnect: () => {},
          })
      ).toThrow(ContainerSessionError);
    }
    expect(fixture.requests).toBe(0);
    const session = own(fixture, () => {});
    const stopped = new AbortController();
    stopped.abort();
    await session.run(stopped.signal);
    expect(fixture.requests).toBe(0);
    const pause = new AbortController();
    const run = session.run(pause.signal);
    await expect(session.run()).rejects.toMatchObject({
      code: 'already_running',
    });
    pause.abort();
    await run;
    await session.shutdown();
    await expect(session.run()).rejects.toMatchObject({ code: 'terminated' });
  });

  it('rejects adopting a connection with subscriptions already queued', async () => {
    const adapter = new WebsocketTestAdapter();
    const closed = new Deferred<void>();
    const close = adapter.close.bind(adapter);
    adapter.close = () => {
      close();
      closed.resolve();
    };
    const connection = DbConnection.builder()
      .withUri('http://127.0.0.1:1')
      .withDatabaseName(TARGET)
      .withWSFn(adapter.openWebSocket)
      .build();
    try {
      connection.subscriptionBuilder().subscribe('SELECT * FROM player');
      expect(() => connection[INTERNAL_MANAGED_SESSION]().enable()).toThrow(
        'Managed boundary requires an unused connection'
      );
    } finally {
      connection.disconnect();
      await closed.promise;
      expect(adapter.closed).toBe(true);
    }
  });
});
