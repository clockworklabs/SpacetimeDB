import { EventEmitter } from "events";
import { ICloseEvent, w3cwebsocket as WSClient } from 'websocket';

import { ProductValue, AlgebraicValue } from "./algebraic_value";
import { AlgebraicType, ProductType, ProductTypeElement, SumType, SumTypeVariant, BuiltinType } from "./algebraic_type";

export { ProductValue, AlgebraicValue, AlgebraicType, ProductType, ProductTypeElement, SumType, SumTypeVariant, BuiltinType };

type SpacetimeDBGlobals = {
  clientDB: ClientDB,
  spacetimeDBClient: SpacetimeDBClient | undefined,
  // TODO: it would be better to use a "family of classes" instead of any
  // in components and reducers, but I didn't have time to research
  // how to do it in TS
  reducers: Map<string, any>,
  components: Map<string, any>,

  registerReducer: (name: string, reducer: any) => void,
  registerComponent: (name: string, component: any) => void,
}

declare global {
  interface Window {
    __SPACETIMEDB__: SpacetimeDBGlobals;
  }
}

export class Reducer {
}

export class IDatabaseTable {
}

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

  public count(): number {
    return this.entries.size;
  }

  public getEntries(): IterableIterator<AlgebraicValue> {
    return this.entries.values();
  }

  public getInstances(): IterableIterator<IDatabaseTable> {
    return this.instances.values();
  }

  applyOperations = (operations: { op: string, row_pk: string, row: any[] }[]) => {
    if (this.pkCol !== undefined) {
      const inserts = [];
      const deleteMap = new Map();
      for (const op of operations) {
        if (op.op === "insert") {
          inserts.push(op);
        } else {
          deleteMap.set(op.row[this.pkCol], op);
        }
      }
      for (const op of inserts) {
        const deleteOp = deleteMap.get(op.row[this.pkCol])
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
          this.insert(op.row_pk, op.row)
        } else {
          this.delete(op.row_pk)
        }
      }
    }

  }

  update = (oldPk: string, pk: string, row: Array<any>) => {
    let entry = AlgebraicValue.deserialize(this.entityClass.getAlgebraicType(), row);
    const instance = this.entityClass.fromValue(entry);
    this.entries.set(pk, entry);
    this.instances.set(pk, instance);
    const oldInstance = this.instances.get(oldPk)!;
    this.entries.delete(oldPk);
    this.instances.delete(oldPk);
    this.emitter.emit("update", instance, oldInstance);
  }

  insert = (pk: string, row: Array<any>) => {
    let entry = AlgebraicValue.deserialize(this.entityClass.getAlgebraicType(), row);
    const instance = this.entityClass.fromValue(entry);
    this.instances.set(pk, instance);
    this.entries.set(pk, entry);
    this.emitter.emit("insert", instance);
  }

  delete = (pk: string) => {
    const instance = this.instances.get(pk);
    this.instances.delete(pk);
    this.entries.delete(pk);
    if (instance) {
      this.emitter.emit("delete", instance);
    }
  }

  onInsert = (cb: (value: any) => void) => {
    this.emitter.on("insert", cb);
  }

  onDelete = (cb: (value: any) => void) => {
    this.emitter.on("delete", cb);
  }

  onUpdate = (cb: (value: any, oldValue: any) => void) => {
    this.emitter.on("update", cb);
  }

  removeOnInsert = (cb: (value: any) => void) => {
    this.emitter.off("insert", cb);
  }

  removeOnDelete = (cb: (value: any) => void) => {
    this.emitter.off("delete", cb);
  }

  removeOnUpdate = (cb: (value: any, oldRow: any) => void) => {
    this.emitter.off("update", cb);
  }
}

export class ClientDB {
  tables: Map<string, Table>

  constructor() {
    this.tables = new Map();
  }

  getTable(name: string): Table {
    // I cast to Table, because this will assume that the table is
    // already there
    // TODO: might be better to throw an exception here?
    return this.tables.get(name) as Table;
  }

  getOrCreateTable = (tableName: string, pkCol: number | undefined, entityClass: any) => {
    let table;
    if (!this.tables.has(tableName)) {
      table = new Table(tableName, pkCol, entityClass);
      this.tables.set(tableName, table);
    } else {
      table = this.tables.get(tableName)!;
    }
    return table;
  }
}

export class SpacetimeDBClient {
  // ws: WoperationsSClient;
  identity?: string = undefined;
  token?: string = undefined;
  public db: ClientDB;
  public emitter: EventEmitter;
  private ws: WSClient;
  private reducers: Map<string, any>;
  private components: Map<string, any>;
  private live: boolean;

  constructor(host: string, name_or_address: string, credentials?: { identity: string, token: string }) {
    const global = window.__SPACETIMEDB__;
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

    // TODO: not sure if we should connect right away. Maybe it's better
    // to decouple this and first just create the client to then
    // allow calling sth like `client.connect()`
    let headers = undefined;
    if (credentials) {
      this.identity = credentials.identity;
      this.token = credentials.token;
      headers = {
        "Authorization": `Basic ${btoa("token:" + this.token)}`
      };
    }
    this.emitter = new EventEmitter();
    this.ws = new WSClient(
      `ws://${host}/database/subscribe?name_or_address=${name_or_address}`,
      'v1.text.spacetimedb',
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
    };

    this.ws.onopen = () => {
      this.live = true;
      this.components.forEach(component => {
        this.subscribeComponent(component);
      });
    }
    this.ws.onmessage = (message: any) => {
      console.log(message);
      const data = JSON.parse(message.data);
      if (data) {
        if (data['SubscriptionUpdate']) {
          let subUpdate = data['SubscriptionUpdate'];
          const tableUpdates = subUpdate["table_updates"];
          for (const tableUpdate of tableUpdates) {
            const tableName = tableUpdate["table_name"];
            const entityClass = global.components.get(tableName);
            const table = this.db.getOrCreateTable(tableName, undefined, entityClass);
            table.applyOperations(tableUpdate["table_row_operations"]);
          }
          this.emitter.emit("initialStateSync");
        } else if (data['TransactionUpdate']) {
          const txUpdate = data['TransactionUpdate'];
          const subUpdate = txUpdate["subscription_update"];
          const tableUpdates = subUpdate["table_updates"];
          for (const tableUpdate of tableUpdates) {
            const tableName = tableUpdate["table_name"];
            const entityClass = global.components.get(tableName);
            const table = this.db.getOrCreateTable(tableName, undefined, entityClass);
            table.applyOperations(tableUpdate["table_row_operations"]);
          }

          const event = txUpdate['event'];
          if (event) {
            const functionCall = event['function_call'];
            const identity = event['caller_identity']
            const reducerName: string | undefined = functionCall?.['reducer'];
            const args: number[] | undefined = functionCall?.['arg_bytes'];
            const status: string | undefined = event['status'];
            const reducer: any | undefined = reducerName ? this.reducers.get(reducerName) : undefined;

            if (reducerName && args && identity && status && reducer) {
              const jsonArray = JSON.parse(String.fromCharCode(...args));
              const reducerArgs = reducer.deserializeArgs(jsonArray);
              this.emitter.emit("reducer:" + reducerName, status, identity, reducerArgs);
            }
          }

          // this.emitter.emit("event", txUpdate['event']);
        }
        else if (data['IdentityToken']) {
          const identityToken = data['IdentityToken'];
          const identity = identityToken['identity'];
          const token = identityToken['token'];
          this.identity = identity;
          this.token = token;
        }
      }
    };
  }

  public registerReducer(name: string, reducer: any) {
    this.reducers.set(name, reducer);
  }

  public registerComponent(name: string, component: any) {
    this.components.set(name, component);
    this.db.getOrCreateTable(name, undefined, component);
    if (this.live) {
      this.subscribeComponent(component);
    }
  }

  public subscribeComponent(element: any) {
    if (element.tableName) {
      this.ws.send(JSON.stringify({ "subscribe": { "query_strings": [element.tableName] } }));
    }
  }

  public call(reducerName: String, args: Array<any>) {
    const msg = `{
    "call": {
      "fn": "${reducerName}",
      "args": ${JSON.stringify(args)}
    }
}`;
    this.ws.send(msg);
  }

  onEvent(eventName: string, callback: (...args: any[]) => void) {
    this.emitter.on(eventName, callback);
  }

  offEvent(eventName: string, callback: (...args: any[]) => void) {
    this.emitter.off(eventName, callback);
  }
}

window.__SPACETIMEDB__ = {
  components: new Map(),
  clientDB: new ClientDB(),
  reducers: new Map(),

  registerReducer: function(name: string, reducer: any) {
    let global = window.__SPACETIMEDB__;
    global.reducers.set(name, reducer);

    if (global.spacetimeDBClient) {
      global.spacetimeDBClient.registerReducer(name, reducer);
    }
  },

  registerComponent: function(name: string, component: any) {
    let global = window.__SPACETIMEDB__;
    global.components.set(name, component);

    if (global.spacetimeDBClient) {
      global.spacetimeDBClient.registerComponent(name, component);
    }
  },
  spacetimeDBClient: undefined
};

