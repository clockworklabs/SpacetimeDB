import { Address } from './address.ts';
import {
  AlgebraicType,
  ProductType,
  ProductTypeElement,
  SumType,
  SumTypeVariant,
} from './algebraic_type.ts';
import {
  AlgebraicValue,
  BinaryAdapter,
  BinaryReducerArgsAdapter,
  parseValue,
  ProductValue,
  type ReducerArgsAdapter,
  type ValueAdapter,
} from './algebraic_value.ts';
import BinaryReader from './binary_reader.ts';
import * as ws from './client_api.ts';
import { ClientDB } from './client_db.ts';
import { DatabaseTable, type DatabaseTableClass } from './database_table.ts';
import { DbContext } from './db_context.ts';
import type { STDBEvent } from './event.ts';
import { EventEmitter } from './event_emitter.ts';
import type { Identity } from './identity.ts';
import { stdbLogger } from './logger.ts';
import {
  IdentityTokenMessage,
  SubscriptionUpdateMessage,
  TransactionUpdateEvent,
  TransactionUpdateMessage,
  type Message,
} from './message_types.ts';
import { Reducer } from './reducer.ts';
import { ReducerEvent } from './reducer_event.ts';
import { BinarySerializer, type Serializer } from './serializer.ts';
import { TableOperation, TableUpdate } from './table.ts';
import type { CallbackInit, EventType } from './types.ts';
import { toPascalCase } from './utils.ts';
import { WebsocketDecompressAdapter } from './websocket_decompress_adapter.ts';
import type { WebsocketTestAdapter } from './websocket_test_adapter.ts';

export {
  AlgebraicType,
  AlgebraicValue,
  BinarySerializer,
  DatabaseTable,
  DbContext,
  ProductType,
  ProductTypeElement,
  ProductValue,
  Reducer,
  ReducerEvent,
  SumType,
  SumTypeVariant,
  type CallbackInit,
  type DatabaseTableClass,
  type ReducerArgsAdapter,
  type Serializer,
  type STDBEvent,
  type ValueAdapter,
};

// TODO: This is temporary notes for how this should turn out. Will be removed by final PR
// CODEGEN:
// type RemoteTables = {
//   user: UserTable;
//   player: PlayerTable;
// };

// type RemoteReducers = {
//   createPlayer: CreatePlayerReducer;
//   onCreatePlayer: CreatePlayerReducerCB;
// };

// ctx.db.user;

// export interface DbContext<DBView = Remotetables, ReducerView> {
//   reducers: ReducerView;
//   db: DBView;
//   isActive: boolean;

//   // TODO: disconnect;
// }

// // These are autogeneratyed types by the codegen. DbContext is from npm package
// const ctx: DBContext<RemoteTables, RemoteReducers>;

// // Autoigenerated EventContext
// export interface EventContext extends DbContext<RemoteTables, RemoteReducers> {
//   event: Event<Reducer>; // ?
// }

// type Reducer =
//   | { tag: 'IdentityConnected'; value: IdentityConnected }
//   | { tag: 'IdentityDisconnected'; value: IdentityDisconnected }
//   | { tag: 'SendMessage'; value: SendMessage }
//   | { tag: 'SetName'; value: SetName };

// export class DbConnection extends DbContext<RemoteTables, RemoteReducers> {
//   static builder(): DbConnectionBuilder<DbConnection, EventContext> {}
// }

/**
 * The database client connection to a SpacetimeDB server.
 */
export class DBConnectionBuilder<DBView, ReducerView> {
  #base!: DBConnectionBase;

  #dbView: DBView;
  #reducerView: ReducerView;

  /**
   * Creates a new `SpacetimeDBClient` database client and set the initial parameters.
   *
   * @param host The host of the SpacetimeDB server.
   * @param name_or_address The name or address of the SpacetimeDB module.
   * @param auth_token The credentials to use to connect to authenticate with SpacetimeDB.
   *
   * @example
   *
   * ```ts
   * const host = "ws://localhost:3000";
   * const name_or_address = "database_name"
   * const auth_token = undefined;
   *
   * var spacetimeDBClient = new SpacetimeDBClient(host, name_or_address, auth_token, protocol);
   * ```
   */
  constructor(
    base: DBConnectionBase,
    dbView: DBView,
    reducerView: ReducerView
  ) {
    this.#dbView = dbView;
    this.#reducerView = reducerView;

    this.#base = base;

    // this.#db = new ClientDB();

    // if (DBConnectionBuilder.#tableClasses.size === 0) {
    //   stdbLogger(
    //     'warn',
    //     'No tables were automatically registered globally, if you want to automatically register tables, you need to register them with SpacetimeDBClient.registerTable() first'
    //   );
    // }

    // for (const [_name, table] of DBConnectionBuilder.#tableClasses) {
    //   this.#registerTable(table);
    // }

    // this.#createWSFn = WebsocketDecompressAdapter.createWebSocketFn;
  }

  withUri(uri: string | URL): DBConnectionBuilder<DBView, ReducerView> {
    this.#base.runtime.uri = new URL(uri);
    return this;
  }

  withModuleName(
    nameOrAddress: string
  ): DBConnectionBuilder<DBView, ReducerView> {
    this.#base.runtime.nameOrAddress = nameOrAddress;
    return this;
  }

  withCredentials(
    creds: [identity: Identity, token: string]
  ): DBConnectionBuilder<DBView, ReducerView> {
    const [identity, token] = creds;
    this.#base.identity = identity;
    this.#base.token = token;

    return this;
  }

  /**
   * Connect to The SpacetimeDB Websocket For Your Module. By default, this will use a secure websocket connection. The parameters are optional, and if not provided, will use the values provided on construction of the client.
   *
   * @param host The hostname of the SpacetimeDB server. Defaults to the value passed to the `constructor`.
   * @param nameOrAddress The name or address of the SpacetimeDB module. Defaults to the value passed to the `constructor`.
   * @param authToken The credentials to use to authenticate with SpacetimeDB. Defaults to the value passed to the `constructor`.
   *
   * @example
   *
   * ```ts
   * const host = "ws://localhost:3000";
   * const name_or_address = "database_name"
   * const auth_token = undefined;
   *
   * var spacetimeDBClient = new SpacetimeDBClient(host, name_or_address, auth_token);
   * // Connect with the initial parameters
   * spacetimeDBClient.connect();
   * //Set the `auth_token`
   * spacetimeDBClient.connect(undefined, undefined, NEW_TOKEN);
   * ```
   */
  async build(): Promise<DBConnectionBuilder<DBView, ReducerView>> {
    if (this.#base.isActive) {
      return this;
    }

    stdbLogger('info', 'Connecting to SpacetimeDB WS...');

    let url = new URL(
      `database/subscribe/${this.#base.runtime.nameOrAddress}`,
      this.#base.runtime.uri
    );
    if (!/^wss?:/.test(this.#base.runtime.uri.protocol)) {
      url.protocol = 'ws:';
    }

    let clientAddress = this.#base.clientAddress.toHexString();
    url.searchParams.set('client_address', clientAddress);

    this.#base.ws = await this.#base.createWSFn({
      url,
      wsProtocol: 'v1.bin.spacetimedb',
      authToken: this.#base.runtime.authToken,
    });

    this.#base.ws.onclose = this.#base.handleOnClose.bind(this);
    this.#base.ws.onerror = this.#base.handleOnError.bind(this);
    this.#base.ws.onopen = this.#base.handleOnOpen.bind(this);
    this.#base.ws.onmessage = this.#base.handleOnMessage.bind(this);

    return this;
  }

  /**
   * Register a callback to be invoked upon authentication with the database.
   *
   * @param token The credentials to use to authenticate with SpacetimeDB.
   * @param identity A unique identifier for a client connected to a database.
   *
   * The callback will be invoked with the `Identity` and private authentication `token` provided by the database to identify this connection.
   *
   * If credentials were supplied to connect, those passed to the callback will be equivalent to the ones used to connect.
   *
   * If the initial connection was anonymous, a new set of credentials will be generated by the database to identify this user.
   *
   * The credentials passed to the callback can be saved and used to authenticate the same user in future connections.
   *
   * @example
   *
   * ```ts
   * spacetimeDBClient.onConnect((token, identity) => {
   *  console.log("Connected to SpacetimeDB");
   *  console.log("Token", token);
   *  console.log("Identity", identity);
   * });
   * ```
   */
  onConnect(
    callback: (identity: Identity, token: string) => void,
    init: CallbackInit = {}
  ): void {
    this.#base.on('connected', callback);

    if (init.signal) {
      init.signal.addEventListener('abort', () => {
        this.#base.off('connected', callback);
      });
    }
  }

  /**
   * Register a callback to be invoked upon an error.
   *
   * @example
   *
   * ```ts
   * spacetimeDBClient.onError((...args: any[]) => {
   *  stdbLogger("warn","ERROR", args);
   * });
   * ```
   */
  onConnectError(
    callback: (...args: any[]) => void,
    init: CallbackInit = {}
  ): void {
    this.#base.on('client_error', callback);

    if (init.signal) {
      init.signal.addEventListener('abort', () => {
        this.#base.off('client_error', callback);
      });
    }
  }

  /**
   * Registers a callback to run when a {@link DbConnection} whose connection initially succeeded
   * is disconnected, either after a {@link DbConnection.disconnect} call or due to an error.
   *
   * If the connection ended because of an error, the error is passed to the callback.
   *
   * The `callback` will be installed on the `DbConnection` created by `build`
   * before initiating the connection, ensuring there's no opportunity for the disconnect to happen
   * before the callback is installed.
   *
   * Note that this does not trigger if `build` fails
   * or in cases where {@link DbConnectionBuilder.onConnectError} would trigger.
   * This callback only triggers if the connection closes after `build` returns successfully
   * and {@link DbConnectionBuilder.onConnect} is invoked, i.e., after the `IdentityToken` is received.
   *
   * To simplify SDK implementation, at most one such callback can be registered.
   * Calling `onDisconnect` on the same `DbConnectionBuilder` multiple times throws an error.
   *
   * Unlike callbacks registered via {@link DbConnection},
   * no mechanism is provided to unregister the provided callback.
   * This is a concession to ergonomics; there's no clean place to return a `CallbackId` from this method
   * or from `build`.
   *
   * @param {function(error?: Error): void} callback - The callback to invoke upon disconnection.
   * @throws {Error} Throws an error if called multiple times on the same `DbConnectionBuilder`.
   */
  onDisconnect(
    callback: (...args: any[]) => void,
    init: CallbackInit = {}
  ): void {
    this.#base.on('disconnected', callback);

    if (init.signal) {
      init.signal.addEventListener('abort', () => {
        this.#base.off('disconnected', callback);
      });
    }
  }
}

// class DBConnection extends DbConnectionBase {}

// const ctxz = new DbConnection()

// ctxz.

export class DBConnectionBase {
  isActive = false;
  /**
   * The user's public identity.
   */
  identity?: Identity = undefined;
  /**
   * The user's private authentication token.
   */
  token?: string = undefined;

  /**
   * Reference to the database of the client.
   */
  #db: ClientDB;
  #emitter: EventEmitter = new EventEmitter();

  #manualTableSubscriptions: string[] = [];
  queriesQueue: string[] = [];

  ws!: WebsocketDecompressAdapter | WebsocketTestAdapter;
  createWSFn: typeof WebsocketDecompressAdapter.createWebSocketFn;

  runtime!: {
    uri: URL;
    nameOrAddress: string;
    authToken?: string;
  };
  clientAddress: Address = Address.random();

  static #tableClasses: Map<string, DatabaseTableClass> = new Map();
  static #reducerClasses: Map<string, Reducer> = new Map();

  constructor() {
    this.#db = new ClientDB();
    this.createWSFn = WebsocketDecompressAdapter.createWebSocketFn;
  }

  static #getTableClass(name: string): DatabaseTableClass {
    const tableClass = this.#tableClasses.get(name);
    if (!tableClass) {
      throw `Could not find class \"${name}\", you need to register it with SpacetimeDBClient.registerTable() first`;
    }

    return tableClass;
  }

  static #getReducerClass(name: string): Reducer | undefined {
    const reducerName = `${name}Reducer`;
    const reducerClass = this.#reducerClasses.get(reducerName);
    if (!reducerClass) {
      stdbLogger(
        'warn',
        `Could not find class \"${name}\", you need to register it with SpacetimeDBClient.registerReducer() first`
      );
      return;
    }

    return reducerClass;
  }

  /**
   * Subscribes to a table without registering it as a component.
   *
   * @param table The table to subscribe to
   * @param query The query to subscribe to. If not provided, the default is `SELECT * FROM {table}`
   */
  #registerManualTable(table: string, query?: string): void {
    this.#manualTableSubscriptions.push(
      query ? query : `SELECT * FROM ${table}`
    );

    this.subscribe([...this.#manualTableSubscriptions]);
  }

  /**
   * Unsubscribes from a table without unregistering it as a component.
   *
   * @param table The table to unsubscribe from
   */
  #removeManualTable(table: string): void {
    // pgoldman 2024-06-25: Is this broken? `registerManualTable` treats `manualTableSubscriptions`
    // as containing SQL strings,
    // but this code treats it as containing table name strings.
    this.#manualTableSubscriptions = this.#manualTableSubscriptions.filter(
      val => val !== table
    );

    this.subscribe(
      this.#manualTableSubscriptions.map(val => `SELECT * FROM ${val}`)
    );
  }

  /**
   * Close the current connection.
   *
   * @example
   *
   * ```ts
   * var spacetimeDBClient = new SpacetimeDBClient("ws://localhost:3000", "database_name");
   *
   * spacetimeDBClient.disconnect()
   * ```
   */
  disconnect(): void {
    this.ws.close();
  }

  #processParsedMessage(
    message: ws.ServerMessage,
    callback: (message: Message) => void
  ) {
    // Helpers for parsing message components which appear in multiple messages.
    const parseTableOperation = (
      rawRow: ws.EncodedValue,
      type: 'insert' | 'delete'
    ): TableOperation => {
      // Our SDKs are architected around having a hashable, equality-comparable key
      // which uniquely identifies every row.
      // This used to be a strong content-addressed hash computed by the DB,
      // but the DB no longer computes those hashes,
      // so now we just use the serialized row as the identifier.
      // That's the second argument to the `TableRowOperation` constructor.

      switch (rawRow.tag) {
        case 'Binary':
          return new TableOperation(
            type,
            new TextDecoder().decode(rawRow.value),
            rawRow.value
          );
        case 'Text':
          return new TableOperation(type, rawRow.value, rawRow.value);
      }
    };
    const parseTableUpdate = (rawTableUpdate: ws.TableUpdate): TableUpdate => {
      const tableName = rawTableUpdate.tableName;
      const operations: TableOperation[] = [];
      for (const insert of rawTableUpdate.inserts) {
        operations.push(parseTableOperation(insert, 'insert'));
      }
      for (const del of rawTableUpdate.deletes) {
        operations.push(parseTableOperation(del, 'delete'));
      }
      return new TableUpdate(tableName, operations);
    };
    const parseDatabaseUpdate = (
      dbUpdate: ws.DatabaseUpdate
    ): SubscriptionUpdateMessage => {
      const tableUpdates: TableUpdate[] = [];
      for (const rawTableUpdate of dbUpdate.tables) {
        tableUpdates.push(parseTableUpdate(rawTableUpdate));
      }
      return new SubscriptionUpdateMessage(tableUpdates);
    };

    switch (message.tag) {
      case 'InitialSubscription': {
        const dbUpdate = message.value.databaseUpdate;
        const subscriptionUpdate = parseDatabaseUpdate(dbUpdate);
        callback(subscriptionUpdate);
        break;
      }

      case 'TransactionUpdate': {
        const txUpdate = message.value;
        const identity = txUpdate.callerIdentity;
        const address = Address.nullIfZero(txUpdate.callerAddress);
        const originalReducerName = txUpdate.reducerCall.reducerName;
        const reducerName: string = toPascalCase(originalReducerName);
        const rawArgs = txUpdate.reducerCall.args;
        if (rawArgs.tag !== 'Binary') {
          throw new Error(
            `Expected a binary EncodedValue but found ${rawArgs.tag} ${rawArgs.value}`
          );
        }
        const args = rawArgs.value;
        let subscriptionUpdate: SubscriptionUpdateMessage;
        let errMessage = '';
        switch (txUpdate.status.tag) {
          case 'Committed':
            subscriptionUpdate = parseDatabaseUpdate(txUpdate.status.value);
            break;
          case 'Failed':
            subscriptionUpdate = new SubscriptionUpdateMessage([]);
            errMessage = txUpdate.status.value;
            break;
          case 'OutOfEnergy':
            subscriptionUpdate = new SubscriptionUpdateMessage([]);
            break;
        }
        const transactionUpdateEvent: TransactionUpdateEvent =
          new TransactionUpdateEvent(
            identity,
            address,
            originalReducerName,
            reducerName,
            args,
            txUpdate.status.tag.toLowerCase(),
            errMessage
          );

        const transactionUpdate = new TransactionUpdateMessage(
          subscriptionUpdate.tableUpdates,
          transactionUpdateEvent
        );
        callback(transactionUpdate);
        break;
      }

      case 'IdentityToken': {
        const identityTokenMessage: IdentityTokenMessage =
          new IdentityTokenMessage(
            message.value.identity,
            message.value.token,
            message.value.address
          );
        callback(identityTokenMessage);
        break;
      }

      case 'OneOffQueryResponse': {
        throw new Error(
          `TypeScript SDK never sends one-off queries, but got OneOffQueryResponse ${message}`
        );
      }
    }
  }

  processMessage(data: Uint8Array, callback: (message: Message) => void): void {
    const message: ws.ServerMessage = parseValue(ws.ServerMessage, data);
    this.#processParsedMessage(message, callback);
  }

  /**
   * Register a component to be used with your SpacetimeDB module. If the websocket is already connected it will add it to the list of subscribed components
   *
   * @param name The name of the component to register
   * @param component The component to register
   */
  #registerTable(tableClass: DatabaseTableClass) {
    this.#db.getOrCreateTable(tableClass.tableName, undefined, tableClass);
    // only set a default ClientDB on a table class if it's not set yet. This means
    // that only the first created client will be usable without the `with` method
    if (!tableClass.db) {
      tableClass.db = this.#db;
    }
  }

  /**
   * Register a component to be used with any SpacetimeDB client. The component will be automatically registered to any
   * new clients
   * @param table Component to be registered
   * @private
   */
  static registerTable(table: DatabaseTableClass): void {
    this.#tableClasses.set(table.tableName, table);
  }

  /**
   *  Register a list of components to be used with any SpacetimeDB client. The components will be automatically registered to any new clients
   * @param tables A list of tables to register globally with SpacetimeDBClient
   */
  static registerTables(...tables: DatabaseTableClass[]): void {
    for (const table of tables) {
      this.registerTable(table);
    }
  }

  /**
   * Register a reducer to be used with any SpacetimeDB client. The reducer will be automatically registered to any
   * new clients
   * @param reducer Reducer to be registered
   * @private
   */
  static registerReducer(reducer: Reducer): void {
    this.#reducerClasses.set(reducer.reducerName + 'Reducer', reducer);
  }

  /**
   * Register a list of reducers to be used with any SpacetimeDB client. The reducers will be automatically registered to any new clients
   * @param reducers A list of reducers to register globally with SpacetimeDBClient
   * @private
   */
  static registerReducers(...reducers: Reducer[]): void {
    for (const reducer of reducers) {
      this.registerReducer(reducer);
    }
  }

  /**
   * Subscribe to a set of queries, to be notified when rows which match those queries are altered.
   *
   * NOTE: A new call to `subscribe` will remove all previous subscriptions and replace them with the new `queries`.
   *
   * If any rows matched the previous subscribed queries but do not match the new queries,
   * those rows will be removed from the client cache, and `{Table}.on_delete` callbacks will be invoked for them.
   *
   * @param queries A `SQL` query or list of queries.
   *
   * @example
   *
   * ```ts
   * spacetimeDBClient.subscribe(["SELECT * FROM User","SELECT * FROM Message"]);
   * ```
   */
  subscribe(queryOrQueries: string | string[]): void {
    const queries =
      typeof queryOrQueries === 'string' ? [queryOrQueries] : queryOrQueries;
    if (this.isActive) {
      const message = ws.ClientMessage.Subscribe(
        new ws.Subscribe(
          queries,
          // The TypeScript SDK doesn't currently track `request_id`s,
          // so always use 0.
          0
        )
      );
      this.#sendMessage(message);
    } else {
      this.queriesQueue = this.queriesQueue.concat(queries);
    }
  }

  #sendMessage(message: ws.ClientMessage) {
    const serializer = new BinarySerializer();
    serializer.write(ws.ClientMessage.getAlgebraicType(), message);
    const encoded = serializer.args();
    this.#emitter.emit('sendWSMessage', encoded);
    this.ws.send(encoded);
  }

  /**
   * Call a reducer on your SpacetimeDB module.
   *
   * @param reducerName The name of the reducer to call
   * @param argsSerializer The arguments to pass to the reducer
   */
  call(reducerName: string, argsSerializer: Serializer): void {
    const message = ws.ClientMessage.CallReducer(
      new ws.CallReducer(
        reducerName,
        ws.EncodedValue.Binary(argsSerializer.args()),
        // The TypeScript SDK doesn't currently track `request_id`s,
        // so always use 0.
        0
      )
    );
    this.#sendMessage(message);
  }

  /**
   * Handles WebSocket onClose event.
   * @param event CloseEvent object.
   */
  handleOnClose(event: CloseEvent): void {
    stdbLogger('warn', 'Closed: ' + event);
    this.#emitter.emit('disconnected');
    this.#emitter.emit('client_error', event);
  }

  /**
   * Handles WebSocket onError event.
   * @param event ErrorEvent object.
   */
  handleOnError(event: ErrorEvent): void {
    stdbLogger('warn', 'WS Error: ' + event);
    this.#emitter.emit('disconnected');
    this.#emitter.emit('client_error', event);
  }

  /**
   * Handles WebSocket onOpen event.
   */
  handleOnOpen(): void {
    this.isActive = true;

    if (this.queriesQueue.length > 0) {
      this.subscribe(this.queriesQueue);
      this.queriesQueue = [];
    }
  }

  /**
   * Handles WebSocket onMessage event.
   * @param wsMessage MessageEvent object.
   */
  handleOnMessage(wsMessage: { data: Uint8Array }): void {
    this.#emitter.emit('receiveWSMessage', wsMessage);

    this.processMessage(wsMessage.data, (message: Message) => {
      if (message instanceof SubscriptionUpdateMessage) {
        for (let tableUpdate of message.tableUpdates) {
          const tableName = tableUpdate.tableName;
          const entityClass = DBConnectionBase.#getTableClass(tableName);
          const table = this.#db.getOrCreateTable(
            tableUpdate.tableName,
            undefined,
            entityClass
          );

          table.applyOperations(tableUpdate.operations, undefined);
        }

        if (this.#emitter) {
          this.#emitter.emit('initialStateSync');
        }
      } else if (message instanceof TransactionUpdateMessage) {
        const reducerName = message.event.reducerName;

        if (reducerName == '<none>') {
          let errorMessage = message.event.message;
          console.error(`Received an error from the database: ${errorMessage}`);
        } else {
          const reducer: any | undefined = reducerName
            ? DBConnectionBase.#getReducerClass(reducerName)
            : undefined;

          let reducerEvent: ReducerEvent | undefined;
          let reducerArgs: any;
          if (reducer && message.event.status === 'committed') {
            let adapter: ReducerArgsAdapter = new BinaryReducerArgsAdapter(
              new BinaryAdapter(
                new BinaryReader(message.event.args as Uint8Array)
              )
            );

            reducerArgs = reducer.deserializeArgs(adapter);
          }

          reducerEvent = new ReducerEvent(
            message.event.identity,
            message.event.address,
            message.event.originalReducerName,
            message.event.status,
            message.event.message,
            reducerArgs
          );

          for (let tableUpdate of message.tableUpdates) {
            const tableName = tableUpdate.tableName;
            const entityClass = DBConnectionBase.#getTableClass(tableName);
            const table = this.#db.getOrCreateTable(
              tableUpdate.tableName,
              undefined,
              entityClass
            );

            table.applyOperations(tableUpdate.operations, reducerEvent);
          }

          if (reducer) {
            this.#emitter.emit(
              'reducer:' + reducerName,
              reducerEvent,
              ...(reducerArgs || [])
            );
          }
        }
      } else if (message instanceof IdentityTokenMessage) {
        this.identity = message.identity;
        if (this.runtime.authToken) {
          this.token = this.runtime.authToken;
        } else {
          this.token = message.token;
        }
        this.clientAddress = message.address;
        this.#emitter.emit(
          'connected',
          this.token,
          this.identity,
          this.clientAddress
        );
      }
    });
  }

  /**
   * @private
   */
  getSerializer(): Serializer {
    return new BinarySerializer();
  }

  on(
    eventName: EventType | (string & {}),
    callback: (...args: any[]) => void
  ): void {
    this.#emitter.on(eventName, callback);
  }

  off(
    eventName: EventType | (string & {}),
    callback: (...args: any[]) => void
  ): void {
    this.#emitter.off(eventName, callback);
  }
}
