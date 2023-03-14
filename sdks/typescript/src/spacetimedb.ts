import { EventEmitter } from "events";
import { ICloseEvent, w3cwebsocket as WSClient } from 'websocket';

import { ProductValue, AlgebraicValue } from "./algebraic_value";
import { AlgebraicType, ProductTypeElement, SumType, SumTypeVariant, BuiltinType } from "./algebraic_type";

export { ProductValue, AlgebraicValue, AlgebraicType, ProductTypeElement, SumType, SumTypeVariant, BuiltinType };

declare global {
  var entityClasses: Map<string, any>;

}

global.entityClasses = new Map();

export class DatabaseTable {
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
  public name: string;
  public rows: Map<string, DatabaseTable>;
  public emitter: EventEmitter;
  private entityClass: any;
  pkCol?: number;

  constructor(name: string, pkCol: number | undefined, entityClass: any) {
    this.name = name;
    this.rows = new Map();
    this.emitter = new EventEmitter();
    this.pkCol = pkCol;
    this.entityClass = entityClass;
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

  instanceFromRow(row: Array<any>): DatabaseTable {
    console.log('deserialize row', row);
    let value = AlgebraicValue.deserialize(this.entityClass.getAlgebraicType(), row);
    return this.entityClass.fromValue(value);
  }

  update = (oldPk: string, pk: string, row: Array<any>) => {
    const instance = this.instanceFromRow(row);
    this.rows.set(pk, instance);
    const oldInstance = this.rows.get(oldPk)!;
    this.rows.delete(oldPk);
    this.emitter.emit("update", instance, oldInstance);
  }

  insert = (pk: string, row: Array<any>) => {
    const instance = this.instanceFromRow(row);
    this.rows.set(pk, instance);
    this.emitter.emit("insert", instance);
  }

  delete = (pk: string) => {
    const instance = this.rows.get(pk);
    this.rows.delete(pk);
    if (instance) {
      this.emitter.emit("delete", instance);
    }
  }

  onInsert = (cb: (row: DatabaseTable) => void) => {
    this.emitter.on("insert", cb);
  }

  onDelete = (cb: (row: DatabaseTable) => void) => {
    this.emitter.on("delete", cb);
  }

  onUpdate = (cb: (row: DatabaseTable, oldRow: DatabaseTable) => void) => {
    this.emitter.on("update", cb);
  }
}

class Database {
  tables: Map<string, Table>

  constructor() {
    this.tables = new Map();
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
  public db: Database;
  public emitter: EventEmitter;
  private ws: WSClient;
  // this should get populated with codegen

  constructor(host: string, name_or_address: string, credentials?: { identity: string, token: string }) {
    // TODO: not sure if we should connect right away. Maybe it's better
    // to decouple this and first just create the client to then
    // allow calling sth like `client.connect()`
    let headers = undefined;
    if (credentials) {
      this.identity = credentials.identity;
      this.token = credentials.token;
      headers = {
        "Authorization": `Basic ${Buffer.from("token:" + this.token).toString('base64')}`
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

    this.db = new Database();
    this.ws.onclose = (event) => {
      console.error("Closed: ", event);
    };

    this.ws.onopen = () => {
      global.entityClasses.forEach(element => {
        this.ws.send(JSON.stringify({ "subscribe": { "query_strings": [element.tableName] } }));
      });
    }
    this.ws.onmessage = (message: any) => {
      console.log('message', message);
      const data = JSON.parse(message.data);
      if (data) {
        if (data['SubscriptionUpdate']) {
          let subUpdate = data['SubscriptionUpdate'];
          const tableUpdates = subUpdate["table_updates"];
          for (const tableUpdate of tableUpdates) {
            const tableName = tableUpdate["table_name"];
            const entityClass = global.entityClasses.get(tableName);
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
            const entityClass = global.entityClasses.get(tableName);
            const table = this.db.getOrCreateTable(tableName, undefined, entityClass);
            table.applyOperations(tableUpdate["table_row_operations"]);
          }
          this.emitter.emit("event", txUpdate['event']);
        } 
        // else if (data['IdentityToken']) {
        //   const identityToken = data['IdentityToken'];
        //   const identity = identityToken['identity'];
        //   const token = identityToken['token'];
        //   this.identity = identity;
        //   this.token = token;
        // }
      }
    };
  }

  call = (reducerName: String, args: Array<any>) => {
    const msg = `{
    "fn": "${reducerName}",
    "args": ${JSON.stringify(args)}
}`;
    this.ws.send(msg);
  }
}
