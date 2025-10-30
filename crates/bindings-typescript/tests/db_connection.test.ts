import {
  CreatePlayer,
  DbConnection,
  Player,
  Point,
  User,
} from '../test-app/src/module_bindings';
import { beforeEach, describe, expect, test } from 'vitest';
import { ConnectionId } from '../src';
import { Timestamp } from '../src';
import { TimeDuration } from '../src';
import * as ws from '../src/sdk/client_api';
import type { ReducerEvent } from '../src/sdk/db_connection_impl';
import { Identity } from '../src';
import WebsocketTestAdapter from '../src/sdk/websocket_test_adapter';
import {
  anIdentity,
  bobIdentity,
  encodeCreatePlayerArgs,
  encodePlayer,
  encodeUser,
  sallyIdentity,
} from './utils';

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

describe('DbConnection', () => {
  test('call onConnectError callback after websocket connection failed to be established', async () => {
    const onConnectErrorPromise = new Deferred<void>();

    let errorCalled = false;
    let connectCalled = false;
    const client = DbConnection.builder()
      .withUri('ws://127.0.0.1:1234')
      .withModuleName('db')
      .withWSFn(() => {
        return Promise.reject(new Error('Failed to connect'));
      })
      .onConnect(() => {
        connectCalled = true;
      })
      .onConnectError(() => {
        errorCalled = true;
        onConnectErrorPromise.resolve();
      })
      .build();

    await client['wsPromise'];
    await onConnectErrorPromise.promise;
    expect(errorCalled).toBeTruthy();
    expect(connectCalled).toBeFalsy();
  });

  test('call onConnect callback after getting an identity', async () => {
    const onConnectPromise = new Deferred<void>();

    const wsAdapter = new WebsocketTestAdapter();
    let called = false;
    const client = DbConnection.builder()
      .withUri('ws://127.0.0.1:1234')
      .withModuleName('db')
      .withWSFn(wsAdapter.createWebSocketFn.bind(wsAdapter) as any)
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
    const client = DbConnection.builder()
      .withUri('ws://127.0.0.1:1234')
      .withModuleName('db')
      .withWSFn(wsAdapter.createWebSocketFn.bind(wsAdapter) as any)
      .onConnect(() => {})
      .build();

    await Promise.race([
      client['wsPromise'],
      new Promise((_, reject) =>
        setTimeout(() => reject(new Error('Timeout')), 1000)
      ),
    ]);
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
      console.log('onInsert called');
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

    const reducerCallbackLog: {
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

    await Promise.race([
      insert1Promise.promise,
      new Promise((_, reject) =>
        setTimeout(() => reject(new Error('Timeout')), 1000)
      ),
    ]);

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

    await Promise.race([
      insert2Promise.promise,
      new Promise((_, reject) =>
        setTimeout(() => reject(new Error('Timeout')), 1000)
      ),
    ]);

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

  test('tables should be updated before the reducer callback', async () => {
    const wsAdapter = new WebsocketTestAdapter();
    const client = DbConnection.builder()
      .withUri('ws://127.0.0.1:1234')
      .withModuleName('db')
      .withWSFn(wsAdapter.createWebSocketFn.bind(wsAdapter) as any)
      .onConnect(() => {})
      .build();

    await client['wsPromise'];
    wsAdapter.acceptConnection();

    const updatePromise = new Deferred<void>();

    expect(client.db.player.count()).toBe(0);

    client.reducers.onCreatePlayer(() => {
      expect(client.db.player.count()).toBe(1);
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

    await Promise.all([updatePromise.promise]);
  });

  test('a reducer callback should be called before the database callbacks', async () => {
    const wsAdapter = new WebsocketTestAdapter();
    const client = DbConnection.builder()
      .withUri('ws://127.0.0.1:1234')
      .withModuleName('db')
      .withWSFn(wsAdapter.createWebSocketFn.bind(wsAdapter) as any)
      .onConnect(() => {})
      .build();

    await client['wsPromise'];
    wsAdapter.acceptConnection();

    const callbackLog: string[] = [];

    const insertPromise = new Deferred<void>();
    const updatePromise = new Deferred<void>();

    client.db.player.onInsert(() => {
      callbackLog.push('Player');

      insertPromise.resolve();
    });

    client.reducers.onCreatePlayer(() => {
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

    expect(callbackLog).toEqual(['CreatePlayerReducer', 'Player']);
  });

  test('it calls onUpdate callback when a record is added with a subscription update and then with a transaction update when the PK is of type Identity', async () => {
    const wsAdapter = new WebsocketTestAdapter();
    const client = DbConnection.builder()
      .withUri('ws://127.0.0.1:1234')
      .withModuleName('db')
      .withWSFn(wsAdapter.createWebSocketFn.bind(wsAdapter) as any)
      .onConnect(() => {})
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
    const initialInsertPromise = new Deferred<void>();
    const userIdentity = Identity.fromString(
      '41db74c20cdda916dd2637e5a11b9f31eb1672249aa7172f7e22b4043a6a9008'
    );

    const initialUser: User = {
      identity: userIdentity,
      username: 'originalName',
    };
    const updatedUser: User = {
      identity: userIdentity,
      username: 'newName',
    };

    const updates: {
      oldUser: User;
      newUser: User;
    }[] = [];
    client.db.user.onInsert(() => {
      initialInsertPromise.resolve();
      console.log('got insert');
    });
    client.db.user.onUpdate((_ctx, oldUser, newUser) => {
      updates.push({
        oldUser,
        newUser,
      });
      update1Promise.resolve();
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
                  rowsData: new Uint8Array([]),
                },
                inserts: {
                  sizeHint: ws.RowSizeHint.FixedSize(0), // not used
                  rowsData: new Uint8Array([...encodeUser(initialUser)]),
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

    // await update1Promise.promise;
    await initialInsertPromise.promise;
    console.log('First insert is done');

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
                  rowsData: new Uint8Array([...encodeUser(initialUser)]),
                },
                // FIXME: this test is evil: an initial subscription can never contain deletes or updates.
                inserts: {
                  sizeHint: ws.RowSizeHint.FixedSize(0), // not used
                  rowsData: new Uint8Array([...encodeUser(updatedUser)]),
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

    console.log('Sending transaction update');
    wsAdapter.sendToClient(transactionUpdate);

    await update1Promise.promise;

    expect(updates).toHaveLength(1);
    expect(updates[0]['oldUser'].username).toBe(initialUser.username);
    expect(updates[0]['newUser'].username).toBe(updatedUser.username);

    console.log('Users: ', [...client.db.user.iter()]);
    expect(client.db.user.count()).toBe(1);
  });

  test('Filtering works', async () => {
    const wsAdapter = new WebsocketTestAdapter();
    const client = DbConnection.builder()
      .withUri('ws://127.0.0.1:1234')
      .withModuleName('db')
      .withWSFn(wsAdapter.createWebSocketFn.bind(wsAdapter) as any)
      .build();
    await client['wsPromise'];
    const user1 = { identity: bobIdentity, username: 'bob' };
    const user2 = {
      identity: sallyIdentity,
      username: 'sally',
    };
    const binary = [...encodeUser(user1)].concat([...encodeUser(user2)]);
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
                  rowsData: new Uint8Array([]),
                },
                // FIXME: this test is evil: an initial subscription can never contain deletes or updates.
                inserts: {
                  sizeHint: ws.RowSizeHint.FixedSize(0), // not used
                  rowsData: new Uint8Array(binary),
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
    const gotAllInserts = new Deferred<void>();
    let inserts = 0;
    client.db.user.onInsert(() => {
      inserts++;
      if (inserts == 2) {
        gotAllInserts.resolve();
      }
    });
    wsAdapter.sendToClient(transactionUpdate);
    await gotAllInserts.promise;

    const filteredUser = client.db.user.identity.find(sallyIdentity);
    expect(filteredUser).not.toBeUndefined();
    expect(filteredUser!.username).toBe('sally');
    expect(client.db.user.count()).toBe(2);
  });
});
