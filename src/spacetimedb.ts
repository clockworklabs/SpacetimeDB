import { EventEmitter } from "events";

import WebSocket from "isomorphic-ws";

import type { WebsocketTestAdapter } from "./websocket_test_adapter";

import {
  ProductValue,
  AlgebraicValue,
  BinaryAdapter,
  ValueAdapter,
  ReducerArgsAdapter,
  JSONReducerArgsAdapter,
  BinaryReducerArgsAdapter,
} from "./algebraic_value";
import { Serializer, BinarySerializer, JSONSerializer } from "./serializer";
import {
  AlgebraicType,
  ProductType,
  ProductTypeElement,
  SumType,
  SumTypeVariant,
  BuiltinType,
} from "./algebraic_type";
import { EventType } from "./types";
import { Identity } from "./identity";
import { Address } from "./address";
import { ReducerEvent } from "./reducer_event";
import * as Proto from "./client_api";
import * as JsonApi from "./json_api";
import BinaryReader from "./binary_reader";
import { TableUpdate, TableOperation } from "./table";
import { _tableProxy, toPascalCase } from "./utils";
import { DatabaseTable, DatabaseTableClass } from "./database_table";
import { Reducer, ReducerClass } from "./reducer";
import { ClientDB } from "./client_db";
import {
  IdentityTokenMessage,
  Message,
  SubscriptionUpdateMessage,
  TransactionUpdateEvent,
  TransactionUpdateMessage,
} from "./message_types";
import { SpacetimeDBGlobals } from "./global";
import { stdbLogger } from "./logger";
import decompress from "brotli/decompress";
import { Buffer } from "buffer";

export {
  ProductValue,
  AlgebraicValue,
  AlgebraicType,
  ProductType,
  ProductTypeElement,
  SumType,
  SumTypeVariant,
  BuiltinType,
  BinarySerializer,
  ReducerEvent,
  Reducer,
  ReducerClass,
  DatabaseTable,
  DatabaseTableClass,
};

export type { ValueAdapter, ReducerArgsAdapter, Serializer };

const g = (typeof window === "undefined" ? global : window)!;

type CreateWSFnType = (
  url: string,
  protocol: string
) => WebSocket | WebsocketTestAdapter;

/**
 * The database client connection to a SpacetimeDB server.
 */
export class SpacetimeDBClient {
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
  public db: ClientDB;
  public emitter!: EventEmitter;

  /**
   * Whether the client is connected.
   */
  public live: boolean;

  private ws!: WebSocket | WebsocketTestAdapter;
  private manualTableSubscriptions: string[] = [];
  private queriesQueue: string[];
  private runtime: {
    host: string;
    name_or_address: string;
    auth_token?: string;
    global: SpacetimeDBGlobals;
  };
  private createWSFn: CreateWSFnType;
  private protocol: "binary" | "json";
  private ssl: boolean = false;
  private clientAddress: Address = Address.random();

  private static tableClasses: Map<string, DatabaseTableClass> = new Map();
  private static reducerClasses: Map<string, ReducerClass> = new Map();

  private static getTableClass(name: string): DatabaseTableClass {
    const tableClass = this.tableClasses.get(name);
    if (!tableClass) {
      throw `Could not find class \"${name}\", you need to register it with SpacetimeDBClient.registerTable() first`;
    }

    return tableClass;
  }

  private static getReducerClass(name: string): ReducerClass | undefined {
    const reducerName = `${name}Reducer`;
    const reducerClass = this.reducerClasses.get(reducerName);
    if (!reducerClass) {
      stdbLogger(
        "warn",
        `Could not find class \"${name}\", you need to register it with SpacetimeDBClient.registerReducer() first`
      );
      return;
    }

    return reducerClass;
  }

  /**
   * Creates a new `SpacetimeDBClient` database client and set the initial parameters.
   *
   * @param host The host of the SpacetimeDB server.
   * @param name_or_address The name or address of the SpacetimeDB module.
   * @param auth_token The credentials to use to connect to authenticate with SpacetimeDB.
   * @param protocol Define how encode the messages: `"binary" | "json"`. Binary is more efficient and compact, but JSON provides human-readable debug information.
   *
   * @example
   *
   * ```ts
   * const host = "ws://localhost:3000";
   * const name_or_address = "database_name"
   * const auth_token = undefined;
   * const protocol = "binary"
   *
   * var spacetimeDBClient = new SpacetimeDBClient(host, name_or_address, auth_token, protocol);
   * ```
   */
  constructor(
    host: string,
    name_or_address: string,
    auth_token?: string,
    protocol?: "binary" | "json"
  ) {
    this.protocol = protocol || "binary";
    const global = g.__SPACETIMEDB__;

    if (global.spacetimeDBClient) {
      // If a client has been already created earlier it means the developer
      // wants to create multiple clients and thus let's create a new ClientDB.
      // The global ClientDB will be onl shared with the first created client
      this.db = new ClientDB();
    } else {
      // if this is the first client let's use the global ClientDB and set this instance
      // as the global instance
      this.db = global.clientDB;
      global.spacetimeDBClient = this;
    }

    // for (const [_name, reducer] of SpacetimeDBClient.reducerClasses) {
    //   this.registerReducer(reducer);
    // }

    if (SpacetimeDBClient.tableClasses.size === 0) {
      stdbLogger(
        "warn",
        "No tables were automatically registered globally, if you want to automatically register tables, you need to register them with SpacetimeDBClient.registerTable() first"
      );
    }

    for (const [_name, table] of SpacetimeDBClient.tableClasses) {
      this.registerTable(table);
    }

    this.live = false;
    this.emitter = new EventEmitter();
    this.queriesQueue = [];

    this.runtime = {
      host,
      name_or_address,
      auth_token,
      global,
    };

    this.createWSFn = this.defaultCreateWebSocketFn;
  }

  private async defaultCreateWebSocketFn(
    url: string,
    protocol: string
  ): Promise<WebSocket | WebsocketTestAdapter> {
    const headers: { [key: string]: string } = {};
    if (this.runtime.auth_token) {
      headers["Authorization"] = `Basic ${btoa(
        "token:" + this.runtime.auth_token
      )}`;
    }

    if (typeof window === "undefined" || !this.runtime.auth_token) {
      // NodeJS environment
      const ws = new WebSocket(url, protocol, {
        maxReceivedFrameSize: 100000000,
        maxReceivedMessageSize: 100000000,
        headers,
      });
      return ws;
    } else {
      // In the browser we first have to get a short lived token and only then connect to the websocket
      let httpProtocol = this.ssl ? "https://" : "http://";
      let tokenUrl = `${httpProtocol}${this.runtime.host}/identity/websocket_token`;

      const response = await fetch(tokenUrl, { method: "POST", headers });
      if (response.ok) {
        const { token } = await response.json();
        url += "&token=" + btoa("token:" + token);
      }
      return new WebSocket(url, protocol);
    }
  }

  /**
   * Handles WebSocket onClose event.
   * @param event CloseEvent object.
   */
  private handleOnClose(event: CloseEvent) {
    stdbLogger("warn", "Closed: " + event);
    this.emitter.emit("disconnected");
    this.emitter.emit("client_error", event);
  }

  /**
   * Handles WebSocket onError event.
   * @param event ErrorEvent object.
   */
  private handleOnError(event: ErrorEvent) {
    stdbLogger("warn", "WS Error: " + event);
    this.emitter.emit("disconnected");
    this.emitter.emit("client_error", event);
  }

  /**
   * Handles WebSocket onOpen event.
   */
  private handleOnOpen() {
    this.live = true;

    if (this.queriesQueue.length > 0) {
      this.subscribe(this.queriesQueue);
      this.queriesQueue = [];
    }
  }

  /**
   * Handles WebSocket onMessage event.
   * @param wsMessage MessageEvent object.
   */
  private handleOnMessage(wsMessage: any) {
    this.emitter.emit("receiveWSMessage", wsMessage);

    this.processMessage(wsMessage, (message: Message) => {
      if (message instanceof SubscriptionUpdateMessage) {
        for (let tableUpdate of message.tableUpdates) {
          const tableName = tableUpdate.tableName;
          const entityClass = SpacetimeDBClient.getTableClass(tableName);
          const table = this.db.getOrCreateTable(
            tableUpdate.tableName,
            undefined,
            entityClass
          );

          table.applyOperations(
            this.protocol,
            tableUpdate.operations,
            undefined
          );
        }

        if (this.emitter) {
          this.emitter.emit("initialStateSync");
        }
      } else if (message instanceof TransactionUpdateMessage) {
        const reducerName = message.event.reducerName;
        const reducer: any | undefined = reducerName
          ? SpacetimeDBClient.getReducerClass(reducerName)
          : undefined;

        let reducerEvent: ReducerEvent | undefined;
        let reducerArgs: any;
        if (reducer && message.event.status === "committed") {
          let adapter: ReducerArgsAdapter;
          if (this.protocol === "binary") {
            adapter = new BinaryReducerArgsAdapter(
              new BinaryAdapter(
                new BinaryReader(message.event.args as Uint8Array)
              )
            );
          } else {
            adapter = new JSONReducerArgsAdapter(message.event.args as any[]);
          }

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
          const entityClass = SpacetimeDBClient.getTableClass(tableName);
          const table = this.db.getOrCreateTable(
            tableUpdate.tableName,
            undefined,
            entityClass
          );

          table.applyOperations(
            this.protocol,
            tableUpdate.operations,
            reducerEvent
          );
        }

        if (reducer) {
          this.emitter.emit(
            "reducer:" + reducerName,
            reducerEvent,
            ...(reducerArgs || [])
          );
        }
      } else if (message instanceof IdentityTokenMessage) {
        this.identity = message.identity;
        if (this.runtime.auth_token) {
          this.token = this.runtime.auth_token;
        } else {
          this.token = message.token;
        }
        this.clientAddress = message.address;
        this.emitter.emit(
          "connected",
          this.token,
          this.identity,
          this.clientAddress
        );
      }
    });
  }

  /**
   * Subscribes to a table without registering it as a component.
   *
   * @param table The table to subscribe to
   * @param query The query to subscribe to. If not provided, the default is `SELECT * FROM {table}`
   */
  public registerManualTable(table: string, query?: string) {
    this.manualTableSubscriptions.push(
      query ? query : `SELECT * FROM ${table}`
    );

    this.ws.send(
      JSON.stringify({
        subscribe: {
          query_strings: [...this.manualTableSubscriptions],
        },
      })
    );
  }

  /**
   * Unsubscribes from a table without unregistering it as a component.
   *
   * @param table The table to unsubscribe from
   */
  public removeManualTable(table: string) {
    this.manualTableSubscriptions = this.manualTableSubscriptions.filter(
      (val) => val !== table
    );

    this.ws.send(
      JSON.stringify({
        subscribe: {
          query_strings: this.manualTableSubscriptions.map(
            (val) => `SELECT * FROM ${val}`
          ),
        },
      })
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
  public disconnect() {
    this.ws.close();
  }

  /**
   * Connect to The SpacetimeDB Websocket For Your Module. By default, this will use a secure websocket connection. The parameters are optional, and if not provided, will use the values provided on construction of the client.
   *
   * @param host The hostname of the SpacetimeDB server. Defaults to the value passed to the `constructor`.
   * @param name_or_address The name or address of the SpacetimeDB module. Defaults to the value passed to the `constructor`.
   * @param auth_token The credentials to use to authenticate with SpacetimeDB. Defaults to the value passed to the `constructor`.
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
  public async connect(
    host?: string,
    name_or_address?: string,
    auth_token?: string
  ) {
    if (this.live) {
      return;
    }

    stdbLogger("info", "Connecting to SpacetimeDB WS...");

    if (host) {
      this.runtime.host = host;
    }

    if (name_or_address) {
      this.runtime.name_or_address = name_or_address;
    }

    if (auth_token) {
      // TODO: do we need both of these
      this.runtime.auth_token = auth_token;
      this.token = auth_token;
    }

    // TODO: we should probably just accept a host and an ssl boolean flag in stead of this
    // whole dance
    let url = `${this.runtime.host}/database/subscribe/${this.runtime.name_or_address}`;
    if (
      !this.runtime.host.startsWith("ws://") &&
      !this.runtime.host.startsWith("wss://")
    ) {
      url = "ws://" + url;
    }

    let clientAddress = this.clientAddress.toHexString();
    url += `?client_address=${clientAddress}`;

    this.ssl = url.startsWith("wss");
    this.runtime.host = this.runtime.host
      .replace("ws://", "")
      .replace("wss://", "");

    const stdbProtocol = this.protocol === "binary" ? "bin" : "text";
    this.ws = await this.createWSFn(url, `v1.${stdbProtocol}.spacetimedb`);

    this.ws.onclose = this.handleOnClose.bind(this);
    this.ws.onerror = this.handleOnError.bind(this);
    this.ws.onopen = this.handleOnOpen.bind(this);
    this.ws.onmessage = this.handleOnMessage.bind(this);
  }

  private processMessage(wsMessage: any, callback: (message: Message) => void) {
    if (this.protocol === "binary") {
      // Helpers for parsing message components which appear in multiple messages.
      const parseTableRowOperation = (
        rawTableOperation: Proto.TableRowOperation
      ): TableOperation => {
        const type =
          rawTableOperation.op === Proto.TableRowOperation_OperationType.INSERT
            ? "insert"
            : "delete";
        // Our SDKs are architected around having a hashable, equality-comparable key
        // which uniquely identifies every row.
        // This used to be a strong content-addressed hash computed by the DB,
        // but the DB no longer computes those hashes,
        // so now we just use the serialized row as the identifier.
        const rowPk = new TextDecoder().decode(rawTableOperation.row);
        return new TableOperation(type, rowPk, rawTableOperation.row);
      };
      const parseTableUpdate = (
        rawTableUpdate: Proto.TableUpdate
      ): TableUpdate => {
        const tableName = rawTableUpdate.tableName;
        const operations: TableOperation[] = [];
        for (const rawTableOperation of rawTableUpdate.tableRowOperations) {
          operations.push(parseTableRowOperation(rawTableOperation));
        }
        return new TableUpdate(tableName, operations);
      };
      const parseSubscriptionUpdate = (
        subUpdate: Proto.SubscriptionUpdate
      ): SubscriptionUpdateMessage => {
        const tableUpdates: TableUpdate[] = [];
        for (const rawTableUpdate of subUpdate.tableUpdates) {
          tableUpdates.push(parseTableUpdate(rawTableUpdate));
        }
        return new SubscriptionUpdateMessage(tableUpdates);
      };

      let data = wsMessage.data;
      if (typeof data.arrayBuffer === "undefined") {
        data = new Blob([data]);
      }
      data.arrayBuffer().then((data: Uint8Array) => {
        // From https://github.com/foliojs/brotli.js/issues/31 :
        // use a `Buffer` rather than a `Uint8Array` because for some reason brotli requires that.
        let decompressed = decompress(new Buffer(data));
        const message: Proto.Message = Proto.Message.decode(
          new Uint8Array(decompressed)
        );
        if (message["subscriptionUpdate"]) {
          const rawSubscriptionUpdate = message.subscriptionUpdate;
          const subscriptionUpdate = parseSubscriptionUpdate(
            rawSubscriptionUpdate
          );
          callback(subscriptionUpdate);
        } else if (message["transactionUpdate"]) {
          const txUpdate = message.transactionUpdate;
          const rawSubscriptionUpdate = txUpdate.subscriptionUpdate;
          if (!rawSubscriptionUpdate) {
            throw new Error(
              "Received TransactionUpdate without SubscriptionUpdate"
            );
          }
          const subscriptionUpdate = parseSubscriptionUpdate(
            rawSubscriptionUpdate
          );

          const event = txUpdate.event;
          if (!event) {
            throw new Error("Received TransactionUpdate without Event");
          }
          const functionCall = event.functionCall;
          if (!functionCall) {
            throw new Error(
              "Received TransactionUpdate with Event but no FunctionCall"
            );
          }
          const identity: Identity = new Identity(event.callerIdentity);
          const address = Address.nullIfZero(event.callerAddress);
          const originalReducerName: string = functionCall.reducer;
          const reducerName: string = toPascalCase(originalReducerName);
          const args = functionCall.argBytes;
          const status: string = Proto.event_StatusToJSON(event.status);
          const messageStr = event.message;

          const transactionUpdateEvent: TransactionUpdateEvent =
            new TransactionUpdateEvent(
              identity,
              address,
              originalReducerName,
              reducerName,
              args,
              status,
              messageStr
            );

          const transactionUpdate = new TransactionUpdateMessage(
            subscriptionUpdate.tableUpdates,
            transactionUpdateEvent
          );
          callback(transactionUpdate);
        } else if (message["identityToken"]) {
          const identityToken = message.identityToken;
          const identity = new Identity(identityToken.identity);
          const token = identityToken.token;
          const address = new Address(identityToken.address);
          const identityTokenMessage: IdentityTokenMessage =
            new IdentityTokenMessage(identity, token, address);
          callback(identityTokenMessage);
        }
      });
    } else {
      const parseTableRowOperation = (
        rawTableOperation: JsonApi.TableRowOperation
      ): TableOperation => {
        const type = rawTableOperation["op"];
        // Our SDKs are architected around having a hashable, equality-comparable key
        // which uniquely identifies every row.
        // This used to be a strong content-addressed hash computed by the DB,
        // but the DB no longer computes those hashes,
        // so now we just use the serialized row as the identifier.
        //
        // JSON.stringify may be expensive here, but if the client cared about performance
        // they'd be using the binary format anyway, so we don't care.
        const rowPk = JSON.stringify(rawTableOperation.row);
        return new TableOperation(type, rowPk, rawTableOperation.row);
      };
      const parseTableUpdate = (
        rawTableUpdate: JsonApi.TableUpdate
      ): TableUpdate => {
        const tableName = rawTableUpdate.table_name;
        const operations: TableOperation[] = [];
        for (const rawTableOperation of rawTableUpdate.table_row_operations) {
          operations.push(parseTableRowOperation(rawTableOperation));
        }
        return new TableUpdate(tableName, operations);
      };
      const parseSubscriptionUpdate = (
        rawSubscriptionUpdate: JsonApi.SubscriptionUpdate
      ): SubscriptionUpdateMessage => {
        const tableUpdates: TableUpdate[] = [];
        for (const rawTableUpdate of rawSubscriptionUpdate.table_updates) {
          tableUpdates.push(parseTableUpdate(rawTableUpdate));
        }
        return new SubscriptionUpdateMessage(tableUpdates);
      };

      const data = JSON.parse(wsMessage.data) as JsonApi.Message;
      if (data["SubscriptionUpdate"]) {
        const subscriptionUpdate = parseSubscriptionUpdate(
          data.SubscriptionUpdate
        );
        callback(subscriptionUpdate);
      } else if (data["TransactionUpdate"]) {
        const txUpdate = data.TransactionUpdate;
        const subscriptionUpdate = parseSubscriptionUpdate(
          txUpdate.subscription_update
        );

        const event = txUpdate.event;
        const functionCall = event.function_call;
        const identity: Identity = new Identity(event.caller_identity);
        const address = Address.fromStringOrNull(event.caller_address);
        const originalReducerName: string = functionCall.reducer;
        const reducerName: string = toPascalCase(originalReducerName);
        const args = JSON.parse(functionCall.args);
        const status: string = event.status;
        const message = event.message;

        const transactionUpdateEvent: TransactionUpdateEvent =
          new TransactionUpdateEvent(
            identity,
            address,
            originalReducerName,
            reducerName,
            args,
            status,
            message
          );

        const transactionUpdate = new TransactionUpdateMessage(
          subscriptionUpdate.tableUpdates,
          transactionUpdateEvent
        );
        callback(transactionUpdate);
      } else if (data["IdentityToken"]) {
        const identityToken = data.IdentityToken;
        const identity = new Identity(identityToken.identity);
        const token = identityToken.token;
        const address = Address.fromString(identityToken.address);
        const identityTokenMessage: IdentityTokenMessage =
          new IdentityTokenMessage(identity, token, address);
        callback(identityTokenMessage);
      }
    }
  }

  /**
   * Register a component to be used with your SpacetimeDB module. If the websocket is already connected it will add it to the list of subscribed components
   *
   * @param name The name of the component to register
   * @param component The component to register
   */
  private registerTable(tableClass: DatabaseTableClass) {
    this.db.getOrCreateTable(tableClass.tableName, undefined, tableClass);
    // only set a default ClientDB on a table class if it's not set yet. This means
    // that only the first created client will be usable without the `with` method
    if (!tableClass.db) {
      tableClass.db = this.db;
    }
  }

  /**
   * Register a component to be used with any SpacetimeDB client. The component will be automatically registered to any
   * new clients
   * @param table Component to be registered
   */
  public static registerTable(table: DatabaseTableClass) {
    this.tableClasses.set(table.tableName, table);
  }

  /**
   *  Register a list of components to be used with any SpacetimeDB client. The components will be automatically registered to any new clients
   * @param tables A list of tables to register globally with SpacetimeDBClient
   */
  public static registerTables(...tables: DatabaseTableClass[]) {
    for (const table of tables) {
      this.registerTable(table);
    }
  }

  /**
   * Register a reducer to be used with any SpacetimeDB client. The reducer will be automatically registered to any
   * new clients
   * @param reducer Reducer to be registered
   */
  public static registerReducer(reducer: ReducerClass) {
    this.reducerClasses.set(reducer.reducerName + "Reducer", reducer);
  }

  /**
   * Register a list of reducers to be used with any SpacetimeDB client. The reducers will be automatically registered to any new clients
   * @param reducers A list of reducers to register globally with SpacetimeDBClient
   */
  public static registerReducers(...reducers: ReducerClass[]) {
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
  public subscribe(queryOrQueries: string | string[]) {
    const queries =
      typeof queryOrQueries === "string" ? [queryOrQueries] : queryOrQueries;

    if (this.live) {
      const message = { subscribe: { query_strings: queries } };
      this.emitter.emit("sendWSMessage", message);
      this.ws.send(JSON.stringify(message));
    } else {
      this.queriesQueue = this.queriesQueue.concat(queries);
    }
  }

  /**
   * Call a reducer on your SpacetimeDB module.
   *
   * @param reducerName The name of the reducer to call
   * @param args The arguments to pass to the reducer
   */
  public call(reducerName: string, serializer: Serializer) {
    let message: any;
    if (this.protocol === "binary") {
      const pmessage: Proto.Message = {
        functionCall: {
          reducer: reducerName,
          argBytes: serializer.args(),
        },
      };

      message = Proto.Message.encode(pmessage).finish();
    } else {
      message = JSON.stringify({
        call: {
          fn: reducerName,
          args: serializer.args(),
        },
      });
    }

    this.emitter.emit("sendWSMessage", message);

    this.ws.send(message);
  }

  on(eventName: EventType | string, callback: (...args: any[]) => void) {
    this.emitter.on(eventName, callback);
  }

  off(eventName: EventType | string, callback: (...args: any[]) => void) {
    this.emitter.off(eventName, callback);
  }

  /**
   * Register a callback to be invoked upon authentication with the database.
   *
   * @param token The credentials to use to authenticate with SpacetimeDB.
   * @param identity A unique public identifier for a client connected to a database.
   *
   * The callback will be invoked with the public `Identity` and private authentication `token` provided by the database to identify this connection.
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
    callback: (token: string, identity: Identity, address: Address) => void
  ) {
    this.on("connected", callback);
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
  onError(callback: (...args: any[]) => void) {
    this.on("client_error", callback);
  }

  _setCreateWSFn(fn: CreateWSFnType) {
    this.createWSFn = fn;
  }

  getSerializer(): Serializer {
    if (this.protocol === "binary") {
      return new BinarySerializer();
    } else {
      return new JSONSerializer();
    }
  }
}

g.__SPACETIMEDB__ = {
  clientDB: new ClientDB(),
  spacetimeDBClient: undefined,
};

export const __SPACETIMEDB__ = (
  typeof window === "undefined"
    ? global.__SPACETIMEDB__
    : window.__SPACETIMEDB__
)!;
