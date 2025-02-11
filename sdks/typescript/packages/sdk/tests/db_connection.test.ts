import {
  CreatePlayer,
  DbConnection,
  Player,
  Point,
  User,
} from '@clockworklabs/test-app/src/module_bindings';
import { beforeEach, describe, expect, test } from 'vitest';
import { ConnectionId } from '../src/connection_id';
import { Timestamp } from '../src/timestamp';
import { TimeDuration } from '../src/time_duration';
import { AlgebraicType } from '../src/algebraic_type';
import { parseValue } from '../src/algebraic_value';
import BinaryWriter from '../src/binary_writer';
import * as ws from '../src/client_api';
import { ReducerEvent } from '../src/db_connection_impl';
import { Identity } from '../src/identity';
import WebsocketTestAdapter from '../src/websocket_test_adapter';

const anIdentity = Identity.fromString(
  '0000000000000000000000000000000000000000000000000000000000000069'
);
const bobIdentity = Identity.fromString(
  '0000000000000000000000000000000000000000000000000000000000000b0b'
);
const sallyIdentity = Identity.fromString(
  '000000000000000000000000000000000000000000000000000000000006a111'
);

class Deferred<T> {
  #isResolved: boolean = false;
  #isRejected: boolean = false;
  #resolve: (value: T | PromiseLike<T>) => void;
  #reject: (reason?: any) => void;
  promise: Promise<T>;

  constructor() {
    this.promise = new Promise<T>((resolve, reject) => {
      this.#resolve = resolve;
      this.#reject = reject;
    });
  }

  // Getter for isResolved
  get isResolved(): boolean {
    return this.#isResolved;
  }

  // Getter for isRejected
  get isRejected(): boolean {
    return this.#isRejected;
  }

  // Resolve method
  resolve(value: T): void {
    if (!this.#isResolved && !this.#isRejected) {
      this.#isResolved = true;
      this.#resolve(value);
    }
  }

  // Reject method
  reject(reason?: any): void {
    if (!this.#isResolved && !this.#isRejected) {
      this.#isRejected = true;
      this.#reject(reason);
    }
  }
}

beforeEach(() => {});

function encodePlayer(value: Player): Uint8Array {
  const writer = new BinaryWriter(1024);
  Player.serialize(writer, value);
  return writer.getBuffer();
}

function encodeUser(value: User): Uint8Array {
  const writer = new BinaryWriter(1024);
  User.serialize(writer, value);
  return writer.getBuffer();
}

function encodeCreatePlayerArgs(name: string, location: Point): Uint8Array {
  const writer = new BinaryWriter(1024);
  AlgebraicType.createStringType().serialize(writer, name);
  Point.serialize(writer, location);
  return writer.getBuffer();
}

describe('DbConnection', () => {
  test('call onConnect callback after getting an identity', async () => {
    const onConnectPromise = new Deferred<void>();

    const wsAdapter = new WebsocketTestAdapter();
    let called = false;
    const client = DbConnection.builder()
      .withUri('ws://127.0.0.1:1234')
      .withModuleName('db')
      .withWSFn(wsAdapter.createWebSocketFn.bind(wsAdapter))
      .onConnect(() => {
        called = true;
        onConnectPromise.resolve();
      })
      .build();

    await client['wsPromise'];
    wsAdapter.acceptConnection();

    const tokenMessage = ws.ServerMessage.IdentityToken({
      identity: anIdentity,
      token: 'a-token',
      connectionId: ConnectionId.random(),
    });
    wsAdapter.sendToClient(tokenMessage);

    await onConnectPromise.promise;

    expect(called).toBeTruthy();
  });

  test('it calls onInsert callback when a record is added with a subscription update and then with a transaction update', async () => {
    const wsAdapter = new WebsocketTestAdapter();
    let called = false;
    const client = DbConnection.builder()
      .withUri('ws://127.0.0.1:1234')
      .withModuleName('db')
      .withWSFn(wsAdapter.createWebSocketFn.bind(wsAdapter))
      .onConnect(() => {
        called = true;
      })
      .build();

    await client['wsPromise'];
    wsAdapter.acceptConnection();

    const tokenMessage = ws.ServerMessage.IdentityToken({
      identity: anIdentity,
      token: 'a-token',
      connectionId: ConnectionId.random(),
    });
    wsAdapter.sendToClient(tokenMessage);

    const inserts: {
      reducerEvent:
        | ReducerEvent<{
            name: 'CreatePlayer';
            args: CreatePlayer;
          }>
        | undefined;
      player: Player;
    }[] = [];

    const insert1Promise = new Deferred<void>();
    const insert2Promise = new Deferred<void>();

    client.db.player.onInsert((ctx, player) => {
      if (ctx.event.tag === 'Reducer') {
        inserts.push({ reducerEvent: ctx.event.value, player });
      } else {
        inserts.push({ reducerEvent: undefined, player });
      }

      if (!insert1Promise.isResolved) {
        insert1Promise.resolve();
      } else {
        insert2Promise.resolve();
      }
    });

    let reducerCallbackLog: {
      reducerEvent: ReducerEvent<{
        name: 'CreatePlayer';
        args: CreatePlayer;
      }>;
      reducerArgs: any[];
    }[] = [];
    client.reducers.onCreatePlayer((ctx, name: string, location: Point) => {
      const reducerEvent = ctx.event;
      reducerCallbackLog.push({
        reducerEvent,
        reducerArgs: [name, location],
      });
    });

    const subscriptionMessage: ws.ServerMessage =
      ws.ServerMessage.InitialSubscription({
        databaseUpdate: {
          tables: [
            {
              tableId: 35,
              tableName: 'player',
              numRows: BigInt(1),
              updates: [
                ws.CompressableQueryUpdate.Uncompressed({
                  deletes: {
                    sizeHint: ws.RowSizeHint.FixedSize(0), // not used
                    rowsData: new Uint8Array(),
                  },
                  inserts: {
                    sizeHint: ws.RowSizeHint.FixedSize(0), // not used
                    rowsData: encodePlayer({
                      ownerId: 'player-1',
                      name: 'drogus',
                      location: { x: 0, y: 0 },
                    }),
                  },
                }),
              ],
            },
          ],
        },
        requestId: 0,
        totalHostExecutionDuration: new TimeDuration(BigInt(0)),
      });

    wsAdapter.sendToClient(subscriptionMessage);

    await insert1Promise.promise;

    expect(inserts).toHaveLength(1);
    expect(inserts[0].player.ownerId).toBe('player-1');
    expect(inserts[0].reducerEvent).toBe(undefined);

    const transactionUpdate = ws.ServerMessage.TransactionUpdate({
      status: ws.UpdateStatus.Committed({
        tables: [
          {
            tableId: 35,
            tableName: 'player',
            numRows: BigInt(2),
            updates: [
              ws.CompressableQueryUpdate.Uncompressed({
                deletes: {
                  sizeHint: ws.RowSizeHint.FixedSize(0), // not used
                  rowsData: new Uint8Array(),
                },
                inserts: {
                  sizeHint: ws.RowSizeHint.FixedSize(0), // not used
                  rowsData: encodePlayer({
                    ownerId: 'player-2',
                    name: 'drogus',
                    location: { x: 2, y: 3 },
                  }),
                },
              }),
            ],
          },
        ],
      }),
      timestamp: new Timestamp(1681391805281203n),
      callerIdentity: anIdentity,
      callerConnectionId: ConnectionId.random(),
      reducerCall: {
        reducerName: 'create_player',
        reducerId: 0,
        args: encodeCreatePlayerArgs('A Player', { x: 2, y: 3 }),
        requestId: 0,
      },
      energyQuantaUsed: { quanta: BigInt(33841000) },
      totalHostExecutionDuration: new TimeDuration(BigInt(1234567890)),
    });
    wsAdapter.sendToClient(transactionUpdate);

    await insert2Promise.promise;

    expect(inserts).toHaveLength(2);
    expect(inserts[1].player.ownerId).toBe('player-2');
    expect(inserts[1].reducerEvent?.reducer.name).toBe('create_player');
    expect(inserts[1].reducerEvent?.status.tag).toBe('Committed');
    expect(inserts[1].reducerEvent?.callerIdentity).toEqual(anIdentity);
    expect(inserts[1].reducerEvent?.reducer.args).toEqual({
      name: 'A Player',
      location: { x: 2, y: 3 },
    });

    expect(reducerCallbackLog).toHaveLength(1);

    expect(reducerCallbackLog[0].reducerEvent.callerIdentity).toEqual(
      anIdentity
    );
  });

  test('it calls onUpdate callback when a record is added with a subscription update and then with a transaction update', async () => {
    const wsAdapter = new WebsocketTestAdapter();
    let called = false;
    const client = DbConnection.builder()
      .withUri('ws://127.0.0.1:1234')
      .withModuleName('db')
      .withWSFn(wsAdapter.createWebSocketFn.bind(wsAdapter))
      .onConnect(() => {
        called = true;
      })
      .build();

    await client['wsPromise'];
    wsAdapter.acceptConnection();

    const tokenMessage = ws.ServerMessage.IdentityToken({
      identity: anIdentity,
      token: 'a-token',
      connectionId: ConnectionId.random(),
    });
    wsAdapter.sendToClient(tokenMessage);

    const update1Promise = new Deferred<void>();
    const update2Promise = new Deferred<void>();

    const updates: {
      oldPlayer: Player;
      newPlayer: Player;
    }[] = [];
    client.db.player.onUpdate((_ctx, oldPlayer, newPlayer) => {
      updates.push({
        oldPlayer,
        newPlayer,
      });

      if (!update1Promise.isResolved) {
        update1Promise.resolve();
      } else {
        update2Promise.resolve();
      }
    });

    const subscriptionMessage = ws.ServerMessage.InitialSubscription({
      databaseUpdate: {
        tables: [
          {
            tableId: 35,
            tableName: 'player',
            numRows: BigInt(2),
            updates: [
              ws.CompressableQueryUpdate.Uncompressed({
                deletes: {
                  sizeHint: ws.RowSizeHint.FixedSize(0), // not used
                  rowsData: new Uint8Array([
                    ...encodePlayer({
                      ownerId: 'player-1',
                      name: 'drogus',
                      location: { x: 0, y: 0 },
                    }),
                  ]),
                },
                // FIXME: this test is evil: an initial subscription can never contain deletes or updates.
                inserts: {
                  sizeHint: ws.RowSizeHint.FixedSize(0), // not used
                  rowsData: new Uint8Array([
                    ...encodePlayer({
                      ownerId: 'player-1',
                      name: 'mr.drogus',
                      location: { x: 0, y: 0 },
                    }),
                  ]),
                },
              }),
            ],
          },
        ],
      },
      requestId: 0,
      totalHostExecutionDuration: new TimeDuration(BigInt(1234567890)),
    });
    wsAdapter.sendToClient(subscriptionMessage);

    await update1Promise.promise;

    expect(updates).toHaveLength(1);
    expect(updates[0]['oldPlayer'].name).toBe('drogus');
    expect(updates[0]['newPlayer'].name).toBe('mr.drogus');

    const transactionUpdate = ws.ServerMessage.TransactionUpdate({
      status: ws.UpdateStatus.Committed({
        tables: [
          {
            tableId: 35,
            tableName: 'player',
            numRows: BigInt(2),
            updates: [
              ws.CompressableQueryUpdate.Uncompressed({
                deletes: {
                  sizeHint: ws.RowSizeHint.FixedSize(0), // not used
                  rowsData: encodePlayer({
                    ownerId: 'player-2',
                    name: 'Jaime',
                    location: { x: 0, y: 0 },
                  }),
                },
                inserts: {
                  sizeHint: ws.RowSizeHint.FixedSize(0), // not used
                  rowsData: encodePlayer({
                    ownerId: 'player-2',
                    name: 'Kingslayer',
                    location: { x: 0, y: 0 },
                  }),
                },
              }),
            ],
          },
        ],
      }),
      timestamp: new Timestamp(1681391805281203n),
      callerIdentity: anIdentity,
      callerConnectionId: ConnectionId.random(),
      reducerCall: {
        reducerName: 'create_player',
        reducerId: 0,
        args: encodeCreatePlayerArgs('A Player', { x: 2, y: 3 }),
        requestId: 0,
      },
      energyQuantaUsed: { quanta: BigInt(33841000) },
      totalHostExecutionDuration: new TimeDuration(BigInt(1234567890)),
    });
    wsAdapter.sendToClient(transactionUpdate);

    await update2Promise.promise;

    expect(updates).toHaveLength(2);
    expect(updates[1]['oldPlayer'].name).toBe('Jaime');
    expect(updates[1]['newPlayer'].name).toBe('Kingslayer');
  });

  test('a reducer callback should be called after the database callbacks', async () => {
    const wsAdapter = new WebsocketTestAdapter();
    let called = false;
    const client = DbConnection.builder()
      .withUri('ws://127.0.0.1:1234')
      .withModuleName('db')
      .withWSFn(wsAdapter.createWebSocketFn.bind(wsAdapter))
      .onConnect(() => {
        called = true;
      })
      .build();

    await client['wsPromise'];
    wsAdapter.acceptConnection();

    let callbackLog: string[] = [];

    const insertPromise = new Deferred<void>();
    const updatePromise = new Deferred<void>();

    client.db.player.onInsert((ctx, player) => {
      callbackLog.push('Player');

      insertPromise.resolve();
    });

    client.reducers.onCreatePlayer((ctx, name, location) => {
      callbackLog.push('CreatePlayerReducer');

      updatePromise.resolve();
    });

    const transactionUpdate = ws.ServerMessage.TransactionUpdate({
      status: ws.UpdateStatus.Committed({
        tables: [
          {
            tableId: 35,
            tableName: 'player',
            numRows: BigInt(1),
            updates: [
              ws.CompressableQueryUpdate.Uncompressed({
                deletes: {
                  sizeHint: ws.RowSizeHint.FixedSize(0), // not used
                  rowsData: new Uint8Array(),
                },
                // FIXME: this test is evil: an initial subscription can never contain deletes or updates.
                inserts: {
                  sizeHint: ws.RowSizeHint.FixedSize(0), // not used
                  rowsData: new Uint8Array([
                    ...encodePlayer({
                      ownerId: 'player-2',
                      name: 'foo',
                      location: { x: 0, y: 0 },
                    }),
                  ]),
                },
              }),
            ],
          },
        ],
      }),
      timestamp: new Timestamp(1681391805281203n),
      callerIdentity: anIdentity,
      callerConnectionId: ConnectionId.random(),
      reducerCall: {
        reducerName: 'create_player',
        reducerId: 0,
        args: encodeCreatePlayerArgs('A Player', { x: 2, y: 3 }),
        requestId: 0,
      },
      energyQuantaUsed: { quanta: BigInt(33841000) },
      totalHostExecutionDuration: new TimeDuration(BigInt(1234567890)),
    });
    wsAdapter.sendToClient(transactionUpdate);

    await Promise.all([insertPromise.promise, updatePromise.promise]);

    expect(callbackLog).toEqual(['Player', 'CreatePlayerReducer']);
  });

  test('it calls onUpdate callback when a record is added with a subscription update and then with a transaction update when the PK is of type Identity', async () => {
    const wsAdapter = new WebsocketTestAdapter();
    let called = false;
    const client = DbConnection.builder()
      .withUri('ws://127.0.0.1:1234')
      .withModuleName('db')
      .withWSFn(wsAdapter.createWebSocketFn.bind(wsAdapter))
      .onConnect(() => {
        called = true;
      })
      .build();

    await client['wsPromise'];
    wsAdapter.acceptConnection();

    const tokenMessage = ws.ServerMessage.IdentityToken({
      identity: Identity.fromString(
        '0000000000000000000000000000000000000000000000000000000000000069'
      ),
      token: 'a-token',
      connectionId: ConnectionId.random(),
    });
    wsAdapter.sendToClient(tokenMessage);

    const update1Promise = new Deferred<void>();
    const update2Promise = new Deferred<void>();

    const updates: {
      oldUser: User;
      newUser: User;
    }[] = [];
    client.db.user.onUpdate((_ctx, oldUser, newUser) => {
      updates.push({
        oldUser,
        newUser,
      });

      if (!update1Promise.isResolved) {
        update1Promise.resolve();
      } else {
        update2Promise.resolve();
      }
    });

    const subscriptionMessage = ws.ServerMessage.InitialSubscription({
      databaseUpdate: {
        tables: [
          {
            tableId: 35,
            tableName: 'user',
            numRows: BigInt(1),
            updates: [
              // pgoldman 2024-06-25: This is weird, `InitialSubscription`s aren't supposed to contain deletes or updates.
              ws.CompressableQueryUpdate.Uncompressed({
                deletes: {
                  sizeHint: ws.RowSizeHint.FixedSize(0), // not used
                  rowsData: new Uint8Array([
                    ...encodeUser({
                      identity: Identity.fromString(
                        '41db74c20cdda916dd2637e5a11b9f31eb1672249aa7172f7e22b4043a6a9008'
                      ),
                      username: 'drogus',
                    }),
                  ]),
                },
                // FIXME: this test is evil: an initial subscription can never contain deletes or updates.
                inserts: {
                  sizeHint: ws.RowSizeHint.FixedSize(0), // not used
                  rowsData: new Uint8Array([
                    ...encodeUser({
                      identity: Identity.fromString(
                        '41db74c20cdda916dd2637e5a11b9f31eb1672249aa7172f7e22b4043a6a9008'
                      ),
                      username: 'mr.drogus',
                    }),
                  ]),
                },
              }),
            ],
          },
        ],
      },
      requestId: 0,
      totalHostExecutionDuration: new TimeDuration(BigInt(1234567890)),
    });

    wsAdapter.sendToClient(subscriptionMessage);

    await update1Promise.promise;

    expect(updates).toHaveLength(1);
    expect(updates[0]['oldUser'].username).toBe('drogus');
    expect(updates[0]['newUser'].username).toBe('mr.drogus');

    const transactionUpdate = ws.ServerMessage.TransactionUpdate({
      status: ws.UpdateStatus.Committed({
        tables: [
          {
            tableId: 35,
            tableName: 'user',
            numRows: BigInt(1),
            updates: [
              ws.CompressableQueryUpdate.Uncompressed({
                deletes: {
                  sizeHint: ws.RowSizeHint.FixedSize(0), // not used
                  rowsData: new Uint8Array([
                    ...encodeUser({
                      identity: Identity.fromString(
                        '11db74c20cdda916dd2637e5a11b9f31eb1672249aa7172f7e22b4043a6a9008'
                      ),
                      username: 'jaime',
                    }),
                  ]),
                },
                // FIXME: this test is evil: an initial subscription can never contain deletes or updates.
                inserts: {
                  sizeHint: ws.RowSizeHint.FixedSize(0), // not used
                  rowsData: new Uint8Array([
                    ...encodeUser({
                      identity: Identity.fromString(
                        '11db74c20cdda916dd2637e5a11b9f31eb1672249aa7172f7e22b4043a6a9008'
                      ),
                      username: 'kingslayer',
                    }),
                  ]),
                },
              }),
            ],
          },
        ],
      }),
      timestamp: new Timestamp(1681391805281203n),
      callerIdentity: anIdentity,
      callerConnectionId: ConnectionId.random(),
      reducerCall: {
        reducerName: 'create_player',
        reducerId: 0,
        args: encodeCreatePlayerArgs('A Player', { x: 2, y: 3 }),
        requestId: 0,
      },
      energyQuantaUsed: { quanta: BigInt(33841000) },
      totalHostExecutionDuration: new TimeDuration(BigInt(1234567890)),
    });

    wsAdapter.sendToClient(transactionUpdate);

    await update2Promise.promise;

    expect(updates).toHaveLength(2);
    expect(updates[1]['oldUser'].username).toBe('jaime');
    expect(updates[1]['newUser'].username).toBe('kingslayer');
  });

  test('Filtering works', async () => {
    const wsAdapter = new WebsocketTestAdapter();
    const client = DbConnection.builder()
      .withUri('ws://127.0.0.1:1234')
      .withModuleName('db')
      .withWSFn(wsAdapter.createWebSocketFn.bind(wsAdapter))
      .build();
    await client['wsPromise'];
    const db = client.db;
    const user1 = { identity: bobIdentity, username: 'bob' };
    const user2 = {
      identity: sallyIdentity,
      username: 'sally',
    };
    const users: Map<string, User> = (db.user.tableCache as any).rows;
    users.set('abc123', user1);
    users.set('def456', user2);

    const filteredUser = client.db.user.identity.find(sallyIdentity);
    expect(filteredUser).not.toBeUndefined;
    expect(filteredUser!.username).toBe('sally');
  });
});
