import { EventEmitter } from "events";

import WS from "websocket";

import { ProductValue, AlgebraicValue } from "./algebraic_value.js";
import {
  AlgebraicType,
  ProductType,
  ProductTypeElement,
  SumType,
  SumTypeVariant,
  BuiltinType,
} from "./algebraic_type.js";
import { EventType } from "./types.js";

export {
  ProductValue,
  AlgebraicValue,
  AlgebraicType,
  ProductType,
  ProductTypeElement,
  SumType,
  SumTypeVariant,
  BuiltinType,
};

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

export type SpacetimeDBEvent = {
  timestamp: number;
  status: string;
  caller_identity: string;
  energy_quanta_used: number;
  function_call?: {
    reducer: string;
    arg_bytes: number[];
  };
};

class Table {
  // TODO: most of this stuff should be probably private
  public name: string;
  public entries: Map<string, AlgebraicValue>;
  public instances: Map<string, IDatabaseTable>;
  public emitter: EventEmitter;
  private entityClass: any;
  pkCol?: number;

  constructor(name: string, pkCol: number | undefined, entityClass: any) {
    this.name = name;
    // TODO: not sure if it's worth it to keep both rows and entries here 🤔
    this.entries = new Map();
    this.instances = new Map();
    this.emitter = new EventEmitter();
    this.pkCol = pkCol;
    this.entityClass = entityClass;
  }

  /**
   * @returns number of entries in the table
   */
  public count(): number {
    return this.entries.size;
  }

  /**
   * @returns The values of the entries in the table
   */
  public getEntries(): IterableIterator<AlgebraicValue> {
    return this.entries.values();
  }

  public getInstances(): IterableIterator<any> {
    return this.instances.values();
  }

  applyOperations = (
    operations: { op: string; row_pk: string; row: any[] }[]
  ) => {
    if (this.pkCol !== undefined) {
      const inserts: any[] = [];
      const deleteMap = new Map();
      for (const op of operations) {
        if (op.op === "insert") {
          inserts.push(op);
        } else {
          deleteMap.set(op.row[this.pkCol], op);
        }
      }
      for (const op of inserts) {
        const deleteOp = deleteMap.get(op.row[this.pkCol]);
        if (deleteOp) {
          this.update(deleteOp.row_pk, op.row_pk, op.row);
          deleteMap.delete(op.row[this.pkCol]);
        } else {
          this.insert(op.row_pk, op.row);
        }
      }
      for (const op of deleteMap.values()) {
        this.delete(op.row_pk);
      }
    } else {
      for (const op of operations) {
        if (op.op === "insert") {
          this.insert(op.row_pk, op.row);
        } else {
          this.delete(op.row_pk);
        }
      }
    }
  };

  update = (oldPk: string, pk: string, row: Array<any>) => {
    let entry = AlgebraicValue.deserialize(
      this.entityClass.getAlgebraicType(),
      row
    );
    const instance = this.entityClass.fromValue(entry);
    this.entries.set(pk, entry);
    this.instances.set(pk, instance);
    const oldInstance = this.instances.get(oldPk)!;
    this.entries.delete(oldPk);
    this.instances.delete(oldPk);
    this.emitter.emit("update", instance, oldInstance);
  };

  insert = (pk: string, row: Array<any>) => {
    let entry = AlgebraicValue.deserialize(
      this.entityClass.getAlgebraicType(),
      row
    );
    const instance = this.entityClass.fromValue(entry);
    this.instances.set(pk, instance);
    this.entries.set(pk, entry);
    this.emitter.emit("insert", instance);
  };

  delete = (pk: string) => {
    const instance = this.instances.get(pk);
    this.instances.delete(pk);
    this.entries.delete(pk);
    if (instance) {
      this.emitter.emit("delete", instance);
    }
  };

  /**
   * Called when a new row is inserted
   * @param cb Callback to be called when a new row is inserted
   */
  onInsert = (cb: (value: any) => void) => {
    this.emitter.on("insert", cb);
  };

  /**
   * Called when a row is deleted
   * @param cb Callback to be called when a row is deleted
   */
  onDelete = (cb: (value: any) => void) => {
    this.emitter.on("delete", cb);
  };

  /**
   * Called when a row is updated
   * @param cb Callback to be called when a row is updated
   */
  onUpdate = (cb: (value: any, oldValue: any) => void) => {
    this.emitter.on("update", cb);
  };

  /**
   * Removes the event listener for when a new row is inserted
   * @param cb Callback to be called when the event listener is removed
   */
  removeOnInsert = (cb: (value: any) => void) => {
    this.emitter.off("insert", cb);
  };

  /**
   * Removes the event listener for when a row is deleted
   * @param cb Callback to be called when the event listener is removed
   */
  removeOnDelete = (cb: (value: any) => void) => {
    this.emitter.off("delete", cb);
  };

  /**
   * Removes the event listener for when a row is updated
   * @param cb Callback to be called when the event listener is removed
   */
  removeOnUpdate = (cb: (value: any, oldRow: any) => void) => {
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

export class SpacetimeDBClient {
  /**
   * The identity of the user.
   */
  identity?: string = undefined;
  /**
   * The token of the user.
   */
  token?: string = undefined;
  /**
   * Reference to the database of the client.
   */
  public db: ClientDB;
  public emitter!: EventEmitter;

  /**
   * Whether the client is connected via websocket.
   */
  public live: boolean;

  private ws!: WS.w3cwebsocket;
  private reducers: Map<string, any>;
  private components: Map<string, any>;
  private runtime: {
    host: string;
    name_or_address: string;
    credentials?: { identity: string; token: string };
    global: SpacetimeDBGlobals;
  };

  constructor(
    host: string,
    name_or_address: string,
    credentials?: { identity: string; token: string }
  ) {
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

    this.runtime = {
      host,
      name_or_address,
      credentials,
      global,
    };
  }

  /**
   * Disconnect from The SpacetimeDB Websocket For Your Module.
   */
  public disconnect() {
    this.ws.close();
  }

  /**
   * Connect to The SpacetimeDB Websocket For Your Module. By default, this will use a secure websocket connection. The parameters are optional, and if not provided, will use the values provided on construction of the client.
   * @param host The host of the spacetimeDB server
   * @param name_or_address The name or address of the spacetimeDB module
   * @param credentials The credentials to use to connect to the spacetimeDB module
   */
  public connect(
    host?: string,
    name_or_address?: string,
    credentials?: { identity: string; token: string }
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

    if (credentials) {
      this.runtime.credentials = credentials;
    }

    let headers: any = undefined;
    if (this.runtime.credentials) {
      this.identity = this.runtime.credentials.identity;
      this.token = this.runtime.credentials.token;
      headers = {
        Authorization: `Basic ${btoa("token:" + this.token)}`,
      };
    }
    let url = `${this.runtime.host}/database/subscribe?name_or_address=${this.runtime.name_or_address}`;
    if (
      !this.runtime.host.startsWith("ws://") &&
      !this.runtime.host.startsWith("wss://")
    ) {
      url = "ws://" + url;
    }
    this.emitter = new EventEmitter();

    this.ws = new WS.w3cwebsocket(
      url,
      "v1.text.spacetimedb",
      undefined,
      headers,
      undefined,
      {
        maxReceivedFrameSize: 100000000,
        maxReceivedMessageSize: 100000000,
      }
    );

    this.ws.onclose = (event) => {
      console.error("Closed: ", event);
      this.emitter.emit("disconnected");
      this.emitter.emit("client_error", event);
    };

    this.ws.onerror = (event) => {
      console.error("Error: ", event);
      this.emitter.emit("disconnected");
      this.emitter.emit("client_error", event);
    };

    this.ws.onopen = () => {
      this.live = true;
      this.components.forEach((component) => {
        this.subscribeComponent(component);
      });
    };

    this.ws.onmessage = (message: any) => {
      const data = JSON.parse(message.data);

      if (data) {
        if (data["SubscriptionUpdate"]) {
          let subUpdate = data["SubscriptionUpdate"];
          const tableUpdates = subUpdate["table_updates"];
          for (const tableUpdate of tableUpdates) {
            const tableName = tableUpdate["table_name"];
            const entityClass = this.runtime.global.components.get(tableName);
            const table = this.db.getOrCreateTable(
              tableName,
              undefined,
              entityClass
            );
            table.applyOperations(tableUpdate["table_row_operations"]);
          }

          if (this.emitter) {
            this.emitter.emit("initialStateSync");
          }
        } else if (data["TransactionUpdate"]) {
          const txUpdate = data["TransactionUpdate"];
          const subUpdate = txUpdate["subscription_update"];
          const tableUpdates = subUpdate["table_updates"];
          for (const tableUpdate of tableUpdates) {
            const tableName = tableUpdate["table_name"];
            const entityClass = this.runtime.global.components.get(tableName);
            const table = this.db.getOrCreateTable(
              tableName,
              undefined,
              entityClass
            );
            table.applyOperations(tableUpdate["table_row_operations"]);
          }

          const event = txUpdate["event"];
          if (event) {
            const functionCall = event["function_call"];
            const identity = event["caller_identity"];
            const reducerName: string | undefined = functionCall?.["reducer"];
            const args: number[] | undefined = functionCall?.["arg_bytes"];
            const status: string | undefined = event["status"];
            const reducer: any | undefined = reducerName
              ? this.reducers.get(reducerName)
              : undefined;

            if (reducerName && args && identity && status && reducer) {
              const jsonArray = JSON.parse(String.fromCharCode(...args));
              const reducerArgs = reducer.deserializeArgs(jsonArray);
              this.emitter.emit(
                "reducer:" + reducerName,
                status,
                identity,
                reducerArgs
              );
            }
          }

          // this.emitter.emit("event", txUpdate['event']);
        } else if (data["IdentityToken"]) {
          const identityToken = data["IdentityToken"];
          const identity = identityToken["identity"];
          const token = identityToken["token"];
          this.identity = identity;
          this.token = token;
          this.emitter.emit("connected", identity);
        }
      }
    };
  }

  /**
   * Register a reducer to be used with your SpacetimeDB module
   * @param name The name of the reducer to register
   * @param reducer The reducer to register
   */
  public registerReducer(name: string, reducer: any) {
    this.reducers.set(name, reducer);
  }

  /**
   * Register a component to be used with your SpacetimeDB module. If the websocket is already connected it will add it to the list of subscribed components
   * @param name The name of the component to register
   * @param component The component to register
   */
  public registerComponent(name: string, component: any) {
    this.components.set(name, component);
    this.db.getOrCreateTable(name, undefined, component);
    if (this.live) {
      this.subscribeComponent(component);
    }
  }

  /**
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
   * Call a reducer on your SpacetimeDB module
   * @param reducerName The name of the reducer to call
   * @param args The arguments to pass to the reducer
   */
  public call(reducerName: String, args: Array<any>) {
    const msg = `{
    "call": {
      "fn": "${reducerName}",
      "args": ${JSON.stringify(args)}
    }
}`;
    this.ws.send(msg);
  }

  on(eventName: EventType | string, callback: (...args: any[]) => void) {
    this.emitter.on(eventName, callback);
  }

  off(eventName: EventType | string, callback: (...args: any[]) => void) {
    this.emitter.off(eventName, callback);
  }

  onConnect(callback: (...args: any[]) => void) {
    this.on("connected", callback);
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
