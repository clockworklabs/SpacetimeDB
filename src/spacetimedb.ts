import { EventEmitter } from "events";
import { ICloseEvent, w3cwebsocket as WSClient } from 'websocket';

import { ProductValue, AlgebraicValue } from "./algebraic_value";
import { AlgebraicType, ProductTypeElement, SumType, SumTypeVariant, BuiltinType } from "./algebraic_type";

export { ProductValue, AlgebraicValue, AlgebraicType, ProductTypeElement, SumType, SumTypeVariant, BuiltinType };

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
  public rows: Map<string, Array<any>>;
  public emitter: EventEmitter;
  pkCol?: number;

  constructor(name: string, pkCol?: number) {
    this.name = name;
    this.rows = new Map();
    this.emitter = new EventEmitter();
    this.pkCol = pkCol;
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
    this.rows.set(pk, row);
    const oldRow = this.rows.get(oldPk)!;
    this.rows.delete(oldPk);
    this.emitter.emit("update", row, oldRow);
  }

  insert = (pk: string, row: Array<any>) => {
    this.rows.set(pk, row);
    this.emitter.emit("insert", row);
  }

  delete = (pk: string) => {
    const row = this.rows.get(pk);
    this.rows.delete(pk);
    if (row) {
      this.emitter.emit("delete", row);
    }
  }

  onInsert = (cb: (row: Array<any>) => void) => {
    this.emitter.on("insert", cb);
  }

  onDelete = (cb: (row: Array<any>) => void) => {
    this.emitter.on("delete", cb);
  }

  onUpdate = (cb: (row: Array<any>, oldRow: Array<any>) => void) => {
    this.emitter.on("update", cb);
  }
}

class Database {
  tables: Map<string, Table>

  constructor() {
    this.tables = new Map();
  }

  getOrCreateTable = (tableName: string, pkCol?: number) => {
    let table;
    if (!this.tables.has(tableName)) {
      table = new Table(tableName, pkCol);
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

  constructor(host: string, name_or_address: string, credentials?: { identity: string, token: string }) {
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
      this.ws.send(JSON.stringify({ "subscribe": { "query_strings": ["Person"] } }));
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
            const table = this.db.getOrCreateTable(tableName);
            table.applyOperations(tableUpdate["table_row_operations"]);
          }
          this.emitter.emit("initialStateSync");
        } else if (data['TransactionUpdate']) {
          const txUpdate = data['TransactionUpdate'];
          const subUpdate = txUpdate["subscription_update"];
          const tableUpdates = subUpdate["table_updates"];
          for (const tableUpdate of tableUpdates) {
            const tableName = tableUpdate["table_name"];
            const table = this.db.getOrCreateTable(tableName);
            table.applyOperations(tableUpdate["table_row_operations"]);
          }
          this.emitter.emit("event", txUpdate['event']);
        } else if (data['IdentityToken']) {
          const identityToken = data['IdentityToken'];
          const identity = identityToken['identity'];
          const token = identityToken['token'];
          this.identity = identity;
          this.token = token;
        }
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
