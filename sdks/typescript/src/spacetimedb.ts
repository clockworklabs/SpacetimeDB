import { EventEmitter } from "events";

import WebSocket from "isomorphic-ws";

import type { WebsocketTestAdapter } from "./websocket_test_adapter";

import {
  ProductValue,
  AlgebraicValue,
  BinaryAdapter,
  JSONAdapter,
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
import {
  Message as ProtobufMessage,
  event_StatusToJSON,
  TableRowOperation_OperationType,
} from "./client_api";
import BinaryReader from "./binary_reader";
import OperationsMap from "./operations_map";

export {
  ProductValue,
  AlgebraicValue,
  AlgebraicType,
  ProductType,
  ProductTypeElement,
  SumType,
  SumTypeVariant,
  BuiltinType,
  ProtobufMessage,
  BinarySerializer,
};

export type { ValueAdapter, ReducerArgsAdapter, Serializer };

const g = (typeof window === "undefined" ? global : window)!;

type SpacetimeDBGlobals = {
  clientDB: ClientDB;
  spacetimeDBClient: SpacetimeDBClient | undefined;
  // TODO: it would be better to use a "family of classes" instead of any
  // in components and reducers, but I didn't have time to research
  // how to do it in TS
  reducers: Map<string, any>;
  components: Map<string, any>;

  registerReducer: (name: string, reducer: any) => void;
  registerComponent: (name: string, component: any) => void;
};

declare global {
  interface Window {
    __SPACETIMEDB__: SpacetimeDBGlobals;
  }
  var __SPACETIMEDB__: SpacetimeDBGlobals;
}

export class Reducer {}

export class IDatabaseTable {}

export class ReducerEvent {
  public callerIdentity: Identity;
  public callerAddress: Address | null;
  public reducerName: string;
  public status: string;
  public message: string;
  public args: any;

  constructor(
    callerIdentity: Identity,
    callerAddress: Address | null,
    reducerName: string,
    status: string,
    message: string,
    args: any
  ) {
    this.callerIdentity = callerIdentity;
    this.callerAddress = callerAddress;
    this.reducerName = reducerName;
    this.status = status;
    this.message = message;
    this.args = args;
  }
}

class DBOp {
  public type: "insert" | "delete";
  public instance: any;
  public rowPk: string;

  constructor(type: "insert" | "delete", rowPk: string, instance: any) {
    this.type = type;
    this.rowPk = rowPk;
    this.instance = instance;
  }
}

/**
 * Builder to generate calls to query a `table` in the database
 */
class Table {
  // TODO: most of this stuff should be probably private
  public name: string;
  public instances: Map<string, IDatabaseTable>;
  public emitter: EventEmitter;
  private entityClass: any;
  pkCol?: number;

  /**
   * @param name the table name
   * @param pkCol column designated as `#[primarykey]`
   * @param entityClass the entityClass
   */
  constructor(name: string, pkCol: number | undefined, entityClass: any) {
    this.name = name;
    this.instances = new Map();
    this.emitter = new EventEmitter();
    this.pkCol = pkCol;
    this.entityClass = entityClass;
  }

  /**
   * @returns number of entries in the table
   */
  public count(): number {
    return this.instances.size;
  }

  /**
   * @returns The values of the entries in the table
   */
  public getInstances(): IterableIterator<any> {
    return this.instances.values();
  }

  applyOperations = (
    protocol: "binary" | "json",
    operations: TableOperation[],
    reducerEvent: ReducerEvent | undefined
  ) => {
    let dbOps: DBOp[] = [];
    for (let operation of operations) {
      const pk: string = operation.rowPk;
      const adapter =
        protocol === "binary"
          ? new BinaryAdapter(new BinaryReader(operation.row))
          : new JSONAdapter(operation.row);
      const entry = AlgebraicValue.deserialize(
        this.entityClass.getAlgebraicType(),
        adapter
      );
      const instance = this.entityClass.fromValue(entry);

      dbOps.push(new DBOp(operation.type, pk, instance));
    }

    if (this.entityClass.primaryKey !== undefined) {
      const pkName = this.entityClass.primaryKey;
      const inserts: any[] = [];
      const deleteMap = new OperationsMap<any, DBOp>();
      for (const dbOp of dbOps) {
        if (dbOp.type === "insert") {
          inserts.push(dbOp);
        } else {
          deleteMap.set(dbOp.instance[pkName], dbOp);
        }
      }
      for (const dbOp of inserts) {
        const deleteOp = deleteMap.get(dbOp.instance[pkName]);
        if (deleteOp) {
          // the pk for updates will differ between insert/delete, so we have to
          // use the instance from delete
          this.update(dbOp, deleteOp, reducerEvent);
          deleteMap.delete(dbOp.instance[pkName]);
        } else {
          this.insert(dbOp, reducerEvent);
        }
      }
      for (const dbOp of deleteMap.values()) {
        this.delete(dbOp, reducerEvent);
      }
    } else {
      for (const dbOp of dbOps) {
        if (dbOp.type === "insert") {
          this.insert(dbOp, reducerEvent);
        } else {
          this.delete(dbOp, reducerEvent);
        }
      }
    }
  };

  update = (
    newDbOp: DBOp,
    oldDbOp: DBOp,
    reducerEvent: ReducerEvent | undefined
  ) => {
    const newInstance = newDbOp.instance;
    const oldInstance = oldDbOp.instance;
    this.instances.delete(oldDbOp.rowPk);
    this.instances.set(newDbOp.rowPk, newInstance);
    this.emitter.emit("update", oldInstance, newInstance, reducerEvent);
  };

  insert = (dbOp: DBOp, reducerEvent: ReducerEvent | undefined) => {
    this.instances.set(dbOp.rowPk, dbOp.instance);
    this.emitter.emit("insert", dbOp.instance, reducerEvent);
  };

  delete = (dbOp: DBOp, reducerEvent: ReducerEvent | undefined) => {
    this.instances.delete(dbOp.rowPk);
    this.emitter.emit("delete", dbOp.instance, reducerEvent);
  };

  /**
   * Register a callback for when a row is newly inserted into the database.
   *
   * ```ts
   * User.onInsert((user, reducerEvent) => {
   *   if (reducerEvent) {
   *      console.log("New user on reducer", reducerEvent, user);
   *   } else {
   *      console.log("New user received during subscription update on insert", user);
   *  }
   * });
   * ```
   *
   * @param cb Callback to be called when a new row is inserted
   */
  onInsert = (
    cb: (value: any, reducerEvent: ReducerEvent | undefined) => void
  ) => {
    this.emitter.on("insert", cb);
  };

  /**
   * Register a callback for when a row is deleted from the database.
   *
   * ```ts
   * User.onDelete((user, reducerEvent) => {
   *   if (reducerEvent) {
   *      console.log("Deleted user on reducer", reducerEvent, user);
   *   } else {
   *      console.log("Deleted user received during subscription update on update", user);
   *  }
   * });
   * ```
   *
   * @param cb Callback to be called when a new row is inserted
   */
  onDelete = (
    cb: (value: any, reducerEvent: ReducerEvent | undefined) => void
  ) => {
    this.emitter.on("delete", cb);
  };

  /**
   * Register a callback for when a row is updated into the database.
   *
   * ```ts
   * User.onInsert((user, reducerEvent) => {
   *   if (reducerEvent) {
   *      console.log("Updated user on reducer", reducerEvent, user);
   *   } else {
   *      console.log("Updated user received during subscription update on delete", user);
   *  }
   * });
   * ```
   *
   * @param cb Callback to be called when a new row is inserted
   */
  onUpdate = (
    cb: (
      value: any,
      oldValue: any,
      reducerEvent: ReducerEvent | undefined
    ) => void
  ) => {
    this.emitter.on("update", cb);
  };

  /**
   * Removes the event listener for when a new row is inserted
   * @param cb Callback to be called when the event listener is removed
   */
  removeOnInsert = (
    cb: (value: any, reducerEvent: ReducerEvent | undefined) => void
  ) => {
    this.emitter.off("insert", cb);
  };

  /**
   * Removes the event listener for when a row is deleted
   * @param cb Callback to be called when the event listener is removed
   */
  removeOnDelete = (
    cb: (value: any, reducerEvent: ReducerEvent | undefined) => void
  ) => {
    this.emitter.off("delete", cb);
  };

  /**
   * Removes the event listener for when a row is updated
   * @param cb Callback to be called when the event listener is removed
   */
  removeOnUpdate = (
    cb: (
      value: any,
      oldValue: any,
      reducerEvent: ReducerEvent | undefined
    ) => void
  ) => {
    this.emitter.off("update", cb);
  };
}

export class ClientDB {
  /**
   * The tables in the database.
   */
  tables: Map<string, Table>;

  constructor() {
    this.tables = new Map();
  }

  /**
   * Returns the table with the given name.
   * @param name The name of the table.
   * @returns The table
   */
  getTable(name: string): Table {
    const table = this.tables.get(name);

    // ! This should not happen as the table should be available but an exception is thrown just in case.
    if (!table) {
      throw new Error(`Table ${name} does not exist`);
    }

    return table;
  }

  getOrCreateTable = (
    tableName: string,
    pkCol: number | undefined,
    entityClass: any
  ) => {
    let table;
    if (!this.tables.has(tableName)) {
      table = new Table(tableName, pkCol, entityClass);
      this.tables.set(tableName, table);
    } else {
      table = this.tables.get(tableName)!;
    }
    return table;
  };
}

class TableOperation {
  /**
   * The type of CRUD operation.
   *
   * NOTE: An update is a `delete` followed by a 'insert' internally.
   */
  public type: "insert" | "delete";
  public rowPk: string;
  public row: Uint8Array | any;

  constructor(type: "insert" | "delete", rowPk: string, row: Uint8Array | any) {
    this.type = type;
    this.rowPk = rowPk;
    this.row = row;
  }
}

class TableUpdate {
  public tableName: string;
  public operations: TableOperation[];

  constructor(tableName: string, operations: TableOperation[]) {
    this.tableName = tableName;
    this.operations = operations;
  }
}

class SubscriptionUpdateMessage {
  public tableUpdates: TableUpdate[];

  constructor(tableUpdates: TableUpdate[]) {
    this.tableUpdates = tableUpdates;
  }
}

class TransactionUpdateEvent {
  public identity: Identity;
  public address: Address | null;
  public originalReducerName: string;
  public reducerName: string;
  public args: any[] | Uint8Array;
  public status: string;
  public message: string;

  constructor(
    identity: Identity,
    address: Address | null,
    originalReducerName: string,
    reducerName: string,
    args: any[] | Uint8Array,
    status: string,
    message: string
  ) {
    this.identity = identity;
    this.address = address;
    this.originalReducerName = originalReducerName;
    this.reducerName = reducerName;
    this.args = args;
    this.status = status;
    this.message = message;
  }
}

class TransactionUpdateMessage {
  public tableUpdates: TableUpdate[];
  public event: TransactionUpdateEvent;

  constructor(tableUpdates: TableUpdate[], event: TransactionUpdateEvent) {
    this.tableUpdates = tableUpdates;
    this.event = event;
  }
}

class IdentityTokenMessage {
  public identity: Identity;
  public token: string;
  public address: Address;

  constructor(identity: Identity, token: string, address: Address) {
    this.identity = identity;
    this.token = token;
    this.address = address;
  }
}
type Message =
  | SubscriptionUpdateMessage
  | TransactionUpdateMessage
  | IdentityTokenMessage;

type CreateWSFnType = (
  url: string,
  protocol: string
) => WebSocket | WebsocketTestAdapter;

let toPascalCase = function (s: string): string {
  const str = s.replace(/([-_][a-z])/gi, ($1) => {
    return $1.toUpperCase().replace("-", "").replace("_", "");
  });

  return str.charAt(0).toUpperCase() + str.slice(1);
};

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
  private reducers: Map<string, any>;
  private components: Map<string, any>;
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
    this.db = global.clientDB;
    // I don't really like it, but it seems like the only way to
    // make reducers work like they do in C#
    global.spacetimeDBClient = this;
    // register any reducers added before creating a client
    this.reducers = new Map();
    for (const [name, reducer] of global.reducers) {
      this.reducers.set(name, reducer);
    }
    this.components = new Map();
    for (const [name, component] of global.components) {
      this.registerComponent(name, component);
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
    console.error("Closed: ", event);
    this.emitter.emit("disconnected");
    this.emitter.emit("client_error", event);
  }

  /**
   * Handles WebSocket onError event.
   * @param event ErrorEvent object.
   */
  private handleOnError(event: ErrorEvent) {
    console.error("Error: ", event);
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
          const entityClass = this.runtime.global.components.get(tableName);
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
          ? this.reducers.get(reducerName)
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
          const entityClass = this.runtime.global.components.get(tableName);
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
            reducerArgs
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

    console.info("Connecting to SpacetimeDB WS...");

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
      let data = wsMessage.data;

      if (typeof data.arrayBuffer === "undefined") {
        data = new Blob([data]);
      }
      data.arrayBuffer().then((data: any) => {
        const message: ProtobufMessage = ProtobufMessage.decode(
          new Uint8Array(data)
        );
        if (message["subscriptionUpdate"]) {
          let subUpdate = message["subscriptionUpdate"] as any;
          const tableUpdates: TableUpdate[] = [];
          for (const rawTableUpdate of subUpdate["tableUpdates"]) {
            const tableName = rawTableUpdate["tableName"];
            const operations: TableOperation[] = [];
            for (const rawTableOperation of rawTableUpdate[
              "tableRowOperations"
            ]) {
              const type =
                rawTableOperation["op"] ===
                TableRowOperation_OperationType.INSERT
                  ? "insert"
                  : "delete";
              const rowPk = new TextDecoder().decode(
                rawTableOperation["rowPk"]
              );
              operations.push(
                new TableOperation(type, rowPk, rawTableOperation.row)
              );
            }
            const tableUpdate = new TableUpdate(tableName, operations);
            tableUpdates.push(tableUpdate);
          }

          const subscriptionUpdate = new SubscriptionUpdateMessage(
            tableUpdates
          );
          callback(subscriptionUpdate);
        } else if (message["transactionUpdate"]) {
          let txUpdate = (message["transactionUpdate"] as any)[
            "subscriptionUpdate"
          ];
          const tableUpdates: TableUpdate[] = [];
          for (const rawTableUpdate of txUpdate["tableUpdates"]) {
            const tableName = rawTableUpdate["tableName"];
            const operations: TableOperation[] = [];
            for (const rawTableOperation of rawTableUpdate[
              "tableRowOperations"
            ]) {
              const type =
                rawTableOperation["op"] ===
                TableRowOperation_OperationType.INSERT
                  ? "insert"
                  : "delete";
              const rowPk = new TextDecoder().decode(
                rawTableOperation["rowPk"]
              );
              operations.push(
                new TableOperation(type, rowPk, rawTableOperation.row)
              );
            }
            const tableUpdate = new TableUpdate(tableName, operations);
            tableUpdates.push(tableUpdate);
          }

          const event = message["transactionUpdate"]["event"] as any;
          const functionCall = event["functionCall"] as any;
          const identity: Identity = new Identity(event["callerIdentity"]);
          const address = Address.nullIfZero(event["callerAddress"]);
          const originalReducerName: string = functionCall["reducer"];
          const reducerName: string = toPascalCase(originalReducerName);
          const args = functionCall["argBytes"];
          const status: string = event_StatusToJSON(event["status"]);
          const messageStr = event["message"];

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
            tableUpdates,
            transactionUpdateEvent
          );
          callback(transactionUpdate);
        } else if (message["identityToken"]) {
          const identityToken = message["identityToken"] as any;
          const identity = new Identity(identityToken["identity"]);
          const token = identityToken["token"];
          const address = new Address(identityToken["address"]);
          const identityTokenMessage: IdentityTokenMessage =
            new IdentityTokenMessage(identity, token, address);
          callback(identityTokenMessage);
        }
      });
    } else {
      const data = JSON.parse(wsMessage.data);
      if (data["SubscriptionUpdate"]) {
        let subUpdate = data["SubscriptionUpdate"];
        const tableUpdates: TableUpdate[] = [];
        for (const rawTableUpdate of subUpdate["table_updates"]) {
          const tableName = rawTableUpdate["table_name"];
          const operations: TableOperation[] = [];
          for (const rawTableOperation of rawTableUpdate[
            "table_row_operations"
          ]) {
            const type = rawTableOperation["op"];
            const rowPk = rawTableOperation["rowPk"];
            operations.push(
              new TableOperation(type, rowPk, rawTableOperation.row)
            );
          }
          const tableUpdate = new TableUpdate(tableName, operations);
          tableUpdates.push(tableUpdate);
        }

        const subscriptionUpdate = new SubscriptionUpdateMessage(tableUpdates);
        callback(subscriptionUpdate);
      } else if (data["TransactionUpdate"]) {
        const txUpdate = data["TransactionUpdate"];
        const tableUpdates: TableUpdate[] = [];
        for (const rawTableUpdate of txUpdate["subscription_update"][
          "table_updates"
        ]) {
          const tableName = rawTableUpdate["table_name"];
          const operations: TableOperation[] = [];
          for (const rawTableOperation of rawTableUpdate[
            "table_row_operations"
          ]) {
            const type = rawTableOperation["op"];
            const rowPk = rawTableOperation["rowPk"];
            operations.push(
              new TableOperation(type, rowPk, rawTableOperation.row)
            );
          }
          const tableUpdate = new TableUpdate(tableName, operations);
          tableUpdates.push(tableUpdate);
        }

        const event = txUpdate["event"] as any;
        const functionCall = event["function_call"] as any;
        const identity: Identity = new Identity(event["caller_identity"]);
        const address = Address.fromStringOrNull(event["caller_address"]);
        const originalReducerName: string = functionCall["reducer"];
        const reducerName: string = toPascalCase(originalReducerName);
        const args = JSON.parse(functionCall["args"]);
        const status: string = event["status"];
        const message = event["message"];

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
          tableUpdates,
          transactionUpdateEvent
        );
        callback(transactionUpdate);
      } else if (data["IdentityToken"]) {
        const identityToken = data["IdentityToken"];
        const identity = new Identity(identityToken["identity"]);
        const token = identityToken["token"];
        const address = Address.fromString(identityToken["address"]);
        const identityTokenMessage: IdentityTokenMessage =
          new IdentityTokenMessage(identity, token, address);
        callback(identityTokenMessage);
      }
    }
  }

  /**
   * Register a reducer to be used with your SpacetimeDB module.
   *
   * @param name The name of the reducer to register
   * @param reducer The reducer to register
   */
  public registerReducer(name: string, reducer: any) {
    this.reducers.set(name, reducer);
  }

  /**
   * Register a component to be used with your SpacetimeDB module. If the websocket is already connected it will add it to the list of subscribed components
   *
   * @param name The name of the component to register
   * @param component The component to register
   */
  public registerComponent(name: string, component: any) {
    this.components.set(name, component);
    this.db.getOrCreateTable(name, undefined, component);
  }

  /**
   * @deprecated
   * Adds a component to the list of components to subscribe to in your websocket connection
   * @param element The component to subscribe to
   */
  public subscribeComponent(element: any) {
    if (element.tableName) {
      this.ws.send(
        JSON.stringify({ subscribe: { query_strings: [element.tableName] } })
      );
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
      const pmessage: ProtobufMessage = {
        functionCall: {
          reducer: reducerName,
          argBytes: serializer.args(),
        },
      };

      message = ProtobufMessage.encode(pmessage).finish();
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
   *  console.error("ERROR", args);
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
  components: new Map(),
  clientDB: new ClientDB(),
  reducers: new Map(),

  registerReducer: function (name: string, reducer: any) {
    let global = g.__SPACETIMEDB__;
    global.reducers.set(name, reducer);

    if (global.spacetimeDBClient) {
      global.spacetimeDBClient.registerReducer(name, reducer);
    }
  },

  registerComponent: function (name: string, component: any) {
    let global = g.__SPACETIMEDB__;
    global.components.set(name, component);

    if (global.spacetimeDBClient) {
      global.spacetimeDBClient.registerComponent(name, component);
    }
  },
  spacetimeDBClient: undefined,
};

export const __SPACETIMEDB__ = (
  typeof window === "undefined"
    ? global.__SPACETIMEDB__
    : window.__SPACETIMEDB__
)!;
