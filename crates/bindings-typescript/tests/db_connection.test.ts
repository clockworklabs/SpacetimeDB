import { DbConnection } from '../test-app/src/module_bindings';
import User from '../test-app/src/module_bindings/user_table';
import { beforeEach, describe, expect, test } from 'vitest';
import {
  BinaryWriter,
  ConnectionId,
  InternalError,
  SenderError,
  Timestamp,
  type Infer,
} from '../src';
import ServerMessage from '../src/sdk/client_api/server_message_type';
import { Identity } from '../src';
import WebsocketTestAdapter from '../src/sdk/websocket_test_adapter';
import {
  anIdentity,
  bobIdentity,
  encodePlayer,
  encodeUser,
  makeQuerySetUpdate,
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

function getLastCallReducerRequestId(wsAdapter: WebsocketTestAdapter): number {
  for (let i = wsAdapter.outgoingMessages.length - 1; i >= 0; i--) {
    const message = wsAdapter.outgoingMessages[i];
    if (message.tag === 'CallReducer') {
      return message.value.requestId;
    }

    console.log('Message: ', JSON.stringify(message));
  }
  console.log('Outgoing messages length: ', wsAdapter.outgoingMessages.length);
  throw new Error('No CallReducer message found in messageQueue.');
}

function makeReducerResult(
  requestId: number,
  reducerQuerySetUpdate: ReturnType<typeof makeQuerySetUpdate>
) {
  return ServerMessage.ReducerResult({
    requestId,
    timestamp: new Timestamp(0n),
    result: {
      tag: 'Ok',
      value: {
        retValue: new Uint8Array(),
        transactionUpdate: {
          querySets: [reducerQuerySetUpdate],
        },
      },
    },
  });
}

function makeReducerErrorResult(requestId: number, error: string) {
  const errorWriter = new BinaryWriter(64);
  errorWriter.writeString(error);
  const errorPayload = errorWriter.getBuffer();
  return ServerMessage.ReducerResult({
    requestId,
    timestamp: new Timestamp(0n),
    result: {
      tag: 'Err',
      value: errorPayload,
    },
  });
}

function makeReducerInternalErrorResult(requestId: number, error: string) {
  return ServerMessage.ReducerResult({
    requestId,
    timestamp: new Timestamp(0n),
    result: {
      tag: 'InternalError',
      value: error,
    },
  });
}

describe('DbConnection', () => {
  test('call onConnectError callback after websocket connection failed to be established', async () => {
    const onConnectErrorPromise = new Deferred<void>();

    let errorCalled = false;
    let connectCalled = false;
    const client = DbConnection.builder()
      .withUri('ws://127.0.0.1:1234')
      .withDatabaseName('db')
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
      .withDatabaseName('db')
      .withWSFn(wsAdapter.createWebSocketFn.bind(wsAdapter) as any)
      .onConnect(() => {
        called = true;
        onConnectPromise.resolve();
      })
      .build();

    await client['wsPromise'];
    wsAdapter.acceptConnection();

    const tokenMessage = ServerMessage.InitialConnection({
      identity: anIdentity,
      token: 'a-token',
      connectionId: ConnectionId.random(),
    });
    wsAdapter.sendToClient(tokenMessage);

    await onConnectPromise.promise;

    expect(called).toBeTruthy();
  });

  test('fires row callbacks after reducer resolution in ReducerResult', async () => {
    const wsAdapter = new WebsocketTestAdapter();
    const onConnectPromise = new Deferred<void>();
    const client = DbConnection.builder()
      .withUri('ws://127.0.0.1:1234')
      .withDatabaseName('db')
      .withWSFn(wsAdapter.createWebSocketFn.bind(wsAdapter) as any)
      .onConnect(() => {
        onConnectPromise.resolve();
      })
      .build();

    await client['wsPromise'];
    wsAdapter.acceptConnection();
    wsAdapter.sendToClient(
      ServerMessage.InitialConnection({
        identity: anIdentity,
        token: 'a-token',
        connectionId: ConnectionId.random(),
      })
    );
    await onConnectPromise.promise;

    let reducerResolved = false;

    const rowCallbackPromise = new Deferred<void>();
    client.db.player.onInsert(ctx => {
      expect(reducerResolved).toBeFalsy();
      expect(ctx.event.tag).toEqual('Reducer');
      if (ctx.event.tag === 'Reducer') {
        expect(ctx.event.value.reducer.name).toEqual('create_player');
        expect(ctx.event.value.reducer.args).toEqual({
          name: 'A Player',
          location: { x: 1, y: 2 },
        });
      }
      rowCallbackPromise.resolve();
    });

    const reducerPromise = client.reducers.createPlayer({
      name: 'A Player',
      location: { x: 1, y: 2 },
    });
    reducerPromise.then(() => {
      reducerResolved = true;
    });
    // Hack to get the request sent from the client.
    await Promise.resolve();
    const requestId = getLastCallReducerRequestId(wsAdapter);
    const reducerQuerySetUpdate = makeQuerySetUpdate(
      0,
      'player',
      encodePlayer({
        id: 1,
        userId: anIdentity,
        name: 'A Player',
        location: { x: 1, y: 2 },
      })
    );
    wsAdapter.sendToClient(makeReducerResult(requestId, reducerQuerySetUpdate));

    await rowCallbackPromise.promise;
    await reducerPromise;
    expect(reducerResolved).toBeTruthy();
  });

  test('reducer error rejects and does not fire row callbacks', async () => {
    const wsAdapter = new WebsocketTestAdapter();
    const onConnectPromise = new Deferred<void>();
    const client = DbConnection.builder()
      .withUri('ws://127.0.0.1:1234')
      .withDatabaseName('db')
      .withWSFn(wsAdapter.createWebSocketFn.bind(wsAdapter) as any)
      .onConnect(() => {
        onConnectPromise.resolve();
      })
      .build();

    await client['wsPromise'];
    wsAdapter.acceptConnection();
    wsAdapter.sendToClient(
      ServerMessage.InitialConnection({
        identity: anIdentity,
        token: 'a-token',
        connectionId: ConnectionId.random(),
      })
    );
    await onConnectPromise.promise;

    let insertCalled = false;
    client.db.player.onInsert(() => {
      insertCalled = true;
    });

    const reducerPromise = client.reducers.createPlayer({
      name: 'A Player',
      location: { x: 1, y: 2 },
    });

    await Promise.resolve();
    const requestId = getLastCallReducerRequestId(wsAdapter);
    wsAdapter.sendToClient(makeReducerErrorResult(requestId, 'test error'));

    await expect(reducerPromise).rejects.toBeInstanceOf(SenderError);
    await expect(reducerPromise).rejects.toHaveProperty(
      'message',
      'test error'
    );
    expect(insertCalled).toBeFalsy();
  });

  test('reducer internal error rejects with InternalError', async () => {
    const wsAdapter = new WebsocketTestAdapter();
    const onConnectPromise = new Deferred<void>();
    const client = DbConnection.builder()
      .withUri('ws://127.0.0.1:1234')
      .withDatabaseName('db')
      .withWSFn(wsAdapter.createWebSocketFn.bind(wsAdapter) as any)
      .onConnect(() => {
        onConnectPromise.resolve();
      })
      .build();

    await client['wsPromise'];
    wsAdapter.acceptConnection();
    wsAdapter.sendToClient(
      ServerMessage.InitialConnection({
        identity: anIdentity,
        token: 'a-token',
        connectionId: ConnectionId.random(),
      })
    );
    await onConnectPromise.promise;

    const reducerPromise = client.reducers.createPlayer({
      name: 'A Player',
      location: { x: 1, y: 2 },
    });

    await Promise.resolve();
    const requestId = getLastCallReducerRequestId(wsAdapter);
    wsAdapter.sendToClient(
      makeReducerInternalErrorResult(requestId, 'internal test error')
    );

    await expect(reducerPromise).rejects.toBeInstanceOf(InternalError);
    await expect(reducerPromise).rejects.toHaveProperty(
      'message',
      'internal test error'
    );
  });

  /*
  test('it calls onInsert callback when a record is added with a subscription update and then with a transaction update', async () => {
    const wsAdapter = new WebsocketTestAdapter();
    const client = DbConnection.builder()
      .withUri('ws://127.0.0.1:1234')
      .withDatabaseName('db')
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

    const tokenMessage = ServerMessage.InitialConnection({
      identity: anIdentity,
      token: 'a-token',
      connectionId: ConnectionId.random(),
    });
    wsAdapter.sendToClient(tokenMessage);

    const inserts: {
      reducerEvent:
        | ReducerEvent<{
            name: 'create_player';
            args: Infer<typeof CreatePlayerReducer>;
          }>
        | undefined;
      player: Infer<typeof Player>;
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
        name: 'create_player';
        args: Infer<typeof CreatePlayerReducer>;
      }>;
      reducerArgs: any[];
    }[] = [];
    client.reducers.onCreatePlayer(
      (ctx, { name, location }: Infer<typeof CreatePlayerReducer>) => {
        const reducerEvent = ctx.event;
        reducerCallbackLog.push({
          reducerEvent,
          reducerArgs: [name, location],
        });
      }
    );

    const initialQuerySetUpdate = makeQuerySetUpdate(
      0,
      'player',
      encodePlayer({
        id: 1,
        userId: anIdentity,
        name: 'drogus',
        location: { x: 0, y: 0 },
      })
    );
    wsAdapter.sendToClient(
      ServerMessage.TransactionUpdate({
        querySets: [initialQuerySetUpdate],
      })
    );

    await Promise.race([
      insert1Promise.promise,
      new Promise((_, reject) =>
        setTimeout(() => reject(new Error('Timeout')), 1000)
      ),
    ]);

    expect(inserts).toHaveLength(1);
    expect(inserts[0].player.id).toEqual(1);
    expect(inserts[0].reducerEvent).toEqual(undefined);

    client.reducers.createPlayer({
      name: 'A Player',
      location: { x: 2, y: 3 },
    });
    const requestId = await getLastCallReducerRequestId(wsAdapter);
    const reducerQuerySetUpdate = makeQuerySetUpdate(
      0,
      'player',
      encodePlayer({
        id: 2,
        userId: anIdentity,
        name: 'drogus',
        location: { x: 2, y: 3 },
      })
    );
    wsAdapter.sendToClient(makeReducerResult(requestId, reducerQuerySetUpdate));

    await Promise.race([
      insert2Promise.promise,
      new Promise((_, reject) =>
        setTimeout(() => reject(new Error('Timeout')), 1000)
      ),
    ]);

    expect(inserts).toHaveLength(2);
    expect(inserts[1].player.id).toEqual(2);
    expect(inserts[1].reducerEvent).toEqual(undefined);

    expect(reducerCallbackLog).toHaveLength(1);
    expect(reducerCallbackLog[0].reducerEvent.reducer.name).toEqual(
      'create_player'
    );
    expect(reducerCallbackLog[0].reducerEvent.outcome.tag).toEqual('Ok');
    expect(reducerCallbackLog[0].reducerEvent.reducer.args).toEqual({
      name: 'A Player',
      location: { x: 2, y: 3 },
    });
  });
  */

  /*
  test('tables should be updated before the reducer callback', async () => {
    const wsAdapter = new WebsocketTestAdapter();
    const client = DbConnection.builder()
      .withUri('ws://127.0.0.1:1234')
      .withDatabaseName('db')
      .withWSFn(wsAdapter.createWebSocketFn.bind(wsAdapter) as any)
      .onConnect(() => {})
      .build();

    await client['wsPromise'];
    wsAdapter.acceptConnection();

    const updatePromise = new Deferred<void>();

    expect(client.db.player.count()).toEqual(0n);

    client.reducers.onCreatePlayer(() => {
      expect(client.db.player.count()).toEqual(1n);
      updatePromise.resolve();
    });

    client.reducers.createPlayer({
      name: 'A Player',
      location: { x: 2, y: 3 },
    });
    const requestId = await getLastCallReducerRequestId(wsAdapter);
    const reducerQuerySetUpdate = makeQuerySetUpdate(
      0,
      'player',
      new Uint8Array([
        ...encodePlayer({
          id: 1,
          userId: anIdentity,
          name: 'foo',
          location: { x: 0, y: 0 },
        }),
      ])
    );
    wsAdapter.sendToClient(makeReducerResult(requestId, reducerQuerySetUpdate));

    await Promise.all([updatePromise.promise]);
  });
  */

  /*
  test('a reducer callback should be called after the database callbacks', async () => {
    const wsAdapter = new WebsocketTestAdapter();
    const client = DbConnection.builder()
      .withUri('ws://127.0.0.1:1234')
      .withDatabaseName('db')
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

    client.reducers.createPlayer({
      name: 'A Player',
      location: { x: 2, y: 3 },
    });
    const requestId = await getLastCallReducerRequestId(wsAdapter);
    const reducerQuerySetUpdate = makeQuerySetUpdate(
      0,
      'player',
      new Uint8Array([
        ...encodePlayer({
          id: 2,
          userId: anIdentity,
          name: 'foo',
          location: { x: 0, y: 0 },
        }),
      ])
    );
    wsAdapter.sendToClient(makeReducerResult(requestId, reducerQuerySetUpdate));

    await Promise.all([insertPromise.promise, updatePromise.promise]);

    expect(callbackLog).toEqual(['Player', 'CreatePlayerReducer']);
  });
  */

  test('it calls onUpdate callback when a record is added with a subscription update and then with a transaction update when the PK is of type Identity', async () => {
    const wsAdapter = new WebsocketTestAdapter();
    const client = DbConnection.builder()
      .withUri('ws://127.0.0.1:1234')
      .withDatabaseName('db')
      .withWSFn(wsAdapter.createWebSocketFn.bind(wsAdapter) as any)
      .onConnect(() => {})
      .build();

    await client['wsPromise'];
    wsAdapter.acceptConnection();

    const tokenMessage = ServerMessage.InitialConnection({
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

    const initialUser: Infer<typeof User> = {
      identity: userIdentity,
      username: 'originalName',
    };
    const updatedUser: Infer<typeof User> = {
      identity: userIdentity,
      username: 'newName',
    };

    const updates: {
      oldUser: Infer<typeof User>;
      newUser: Infer<typeof User>;
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

    const initialQuerySetUpdate = makeQuerySetUpdate(
      0,
      'user',
      new Uint8Array([...encodeUser(initialUser)])
    );
    wsAdapter.sendToClient(
      ServerMessage.TransactionUpdate({
        querySets: [initialQuerySetUpdate],
      })
    );

    // await update1Promise.promise;
    await initialInsertPromise.promise;
    console.log('First insert is done');

    const transactionUpdate = ServerMessage.TransactionUpdate({
      querySets: [
        makeQuerySetUpdate(
          0,
          'user',
          new Uint8Array([...encodeUser(updatedUser)]),
          new Uint8Array([...encodeUser(initialUser)])
        ),
      ],
    });

    console.log('Sending transaction update');
    wsAdapter.sendToClient(transactionUpdate);

    await update1Promise.promise;

    expect(updates).toHaveLength(1);
    expect(updates[0]['oldUser'].username).toEqual(initialUser.username);
    expect(updates[0]['newUser'].username).toEqual(updatedUser.username);

    console.log('Users: ', [...client.db.user.iter()]);
    expect(client.db.user.count()).toEqual(1n);
  });

  test('Filtering works', async () => {
    const wsAdapter = new WebsocketTestAdapter();
    const client = DbConnection.builder()
      .withUri('ws://127.0.0.1:1234')
      .withDatabaseName('db')
      .withWSFn(wsAdapter.createWebSocketFn.bind(wsAdapter) as any)
      .build();
    await client['wsPromise'];
    const user1 = { identity: bobIdentity, username: 'bob' };
    const user2 = {
      identity: sallyIdentity,
      username: 'sally',
    };
    const binary = [...encodeUser(user1)].concat([...encodeUser(user2)]);
    const transactionUpdate = ServerMessage.TransactionUpdate({
      querySets: [makeQuerySetUpdate(0, 'user', new Uint8Array(binary))],
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

    const foundUser = client.db.user.identity.find(sallyIdentity);
    expect(foundUser).not.toBeUndefined();
    expect(foundUser!.username).toEqual('sally');
    expect(client.db.user.count()).toEqual(2n);
  });
});
