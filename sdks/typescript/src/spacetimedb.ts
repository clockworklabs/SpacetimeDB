import { EventEmitter } from "events";
import { ICloseEvent, w3cwebsocket as WSClient } from 'websocket';

export class SumValue {
  public tag: number;
  public value: AlgebraicValue;

  constructor(tag: number, value: AlgebraicValue) {
    this.tag = tag;
    this.value = value;
  }

  public static deserialize(type: SumType | undefined, value: object): SumValue {
    if (type === undefined) {
      // TODO: get rid of undefined here
      throw "sum type is undefined";
    }

    // TODO: this will likely change, but I'm using whatever we return from the server now
    let tag = parseInt(Object.keys(value)[0]);
    let sumValue = AlgebraicValue.deserialize(type.variants[tag].algebraicType, tag);
    return new SumValue(tag, sumValue);
  }
}

export class ProductValue {
  elements: AlgebraicValue[];

  constructor(elements: AlgebraicValue[]) {
    this.elements = elements;
  }

  public static deserialize(type: ProductType | undefined, value: any): ProductValue {
    if (type === undefined) {
      throw "type is undefined"
    }

    let elements: AlgebraicValue[] = [];

    for (let i in type.elements) {
      let element = type.elements[i];
      elements.push(AlgebraicValue.deserialize(element.algebraicType, value[i]));
    }
    return new ProductValue(elements);
  }
}

type BuiltinValueType = boolean | string | number;

export class BuiltinValue {
  value: BuiltinValueType;

  constructor(value: BuiltinValueType) {
    this.value = value
  }

  public static deserialize(type: BuiltinType | undefined, value: any): BuiltinValue {
    if (type === undefined) {
      // TODO: what to do here? I guess I would prefer to remove this case alltogether
      return new BuiltinValue(false);
    }

    return new this(value);
  }

  public asString(): string {
    return this.value as string;
  }
}

type AnyValue = SumValue | ProductValue | BuiltinValue;

export class AlgebraicValue {
  sum: SumValue | undefined;
  product: ProductValue | undefined;
  builtin: BuiltinValue | undefined;

  constructor(value: AnyValue | undefined) {
    if (value === undefined) {
      // TODO: possibly get rid of it
      throw "value is undefined"
    }
    switch (value.constructor) {
      case SumValue:
        this.sum = value as SumValue;
        break;
      case ProductValue:
        this.product = value as ProductValue;
        break;
      case BuiltinValue:
        this.builtin = value as BuiltinValue;
        break;
    }
  }

  public static deserialize(type: AlgebraicType, value: any) {
    switch (type.type) {
      case Type.ProductType:
        return new this(ProductValue.deserialize(type.product, value));
      case Type.SumType:
        return new this(SumValue.deserialize(type.sum, value));
      case Type.BuiltinType:
        return new this(BuiltinValue.deserialize(type.builtin, value));
      default:
        throw new Error("not implemented exception");
    }
  }
  
  public asProductValue(): ProductValue {
    return this.product as ProductValue;
  }

  public asBuiltinValue(): BuiltinValue {
    return this.builtin as BuiltinValue;
  }

  public asSumValue(): SumValue {
    return this.sum as SumValue;
  }

  public asString(): string {
    return (this.builtin as BuiltinValue).asString();
  }
}


export class SumTypeVariant {
  public name: string;
  public algebraicType: AlgebraicType;

  constructor(name: string, algebraicType: AlgebraicType) {
    this.name = name;
    this.algebraicType = algebraicType;
  }
}

export class SumType {
  public variants: SumTypeVariant[];

  constructor(variants: SumTypeVariant[]) {
    this.variants = variants;
  }
}

export class ProductTypeElement {
  public name: string;
  public algebraicType: AlgebraicType;

  constructor(name: string, algebraicType: AlgebraicType) {
    this.name = name;
    this.algebraicType = algebraicType;
  }
}

class ProductType {
  public elements: ProductTypeElement[];

  constructor(elements: ProductTypeElement[]) {
    this.elements = elements;
  }
}

class MapType {
  public keyType: AlgebraicType;
  public valueType: AlgebraicType;

  constructor(keyType: AlgebraicType, valueType: AlgebraicType) {
    this.keyType = keyType;
    this.valueType = valueType;
  }
}

export enum BuiltinTypeType {
  Bool,
  I8,
  U8,
  I16,
  U16,
  I32,
  U32,
  I64,
  U64,
  I128,
  U128,
  F32,
  F64,
  String,
  Array,
  Map

}

class BuiltinType {
  public type: BuiltinTypeType;
  public arrayType: AlgebraicType | undefined;
  public mapType: MapType | undefined;

  constructor(type: BuiltinTypeType) {
    this.type = type;
  }
}

enum Type {
  SumType,
  ProductType,
  BuiltinType,
  None
}

type TypeRef = null;
type None = null;

type AnyType = ProductType | SumType | BuiltinType | TypeRef | None;

export class AlgebraicType {
  type!: Type;
  type_?: AnyType;

  public get product(): ProductType | undefined {
    return this.type == Type.ProductType ? this.type_ as ProductType : undefined;
  }
  public set product(value: ProductType | undefined) {
    this.type_ = value;
    this.type = value == undefined ? Type.None : Type.ProductType;
  }

  public get sum(): SumType | undefined {
    return this.type == Type.SumType ? this.type_ as SumType : undefined;
  }
  public set sum(value: SumType | undefined) {
    this.type_ = value;
    this.type = value == undefined ? Type.None : Type.SumType;
  }

  public get builtin(): BuiltinType | undefined {
    return this.type == Type.BuiltinType ? this.type_ as BuiltinType : undefined;
  }
  public set builtin(value: BuiltinType | undefined) {
    this.type_ = value;
    this.type = value == undefined ? Type.None : Type.BuiltinType;
  }

  public static createProductType(elements: ProductTypeElement[]): AlgebraicType {
    let type = new AlgebraicType();
    type.product = new ProductType(elements);
    return type;
  }

  public static createSumType(variants: SumTypeVariant[]): AlgebraicType {
    let type = new AlgebraicType();
    type.sum = new SumType(variants);
    return type;
  }

  public static createPrimitiveType(type: BuiltinTypeType) {
    let algebraicType = new AlgebraicType();
    algebraicType.builtin = new BuiltinType(type);
    return algebraicType;
  }
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
