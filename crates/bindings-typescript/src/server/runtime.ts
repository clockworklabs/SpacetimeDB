import * as _syscalls2_0 from 'spacetime:sys@2.0';

import type { ModuleHooks, u128, u16, u256, u32 } from 'spacetime:sys@2.0';
import {
  AlgebraicType,
  ProductType,
  type Deserializer,
} from '../lib/algebraic_type';
import RawModuleDef from '../lib/autogen/raw_module_def_type';
import type RawTableDefV10 from '../lib/autogen/raw_table_def_v_10_type';
import type Typespace from '../lib/autogen/typespace_type';
import { ConnectionId } from '../lib/connection_id';
import { Identity } from '../lib/identity';
import { Timestamp } from '../lib/timestamp';
import { Uuid } from '../lib/uuid';
import BinaryReader from '../lib/binary_reader';
import BinaryWriter, { ResizableBuffer } from '../lib/binary_writer';
import {
  type Index,
  type IndexVal,
  type PointIndex,
  type RangedIndex,
  type UniqueIndex,
} from '../lib/indexes';
import { callProcedure } from './procedures';
import {
  type AuthCtx,
  type JsonObject,
  type JwtClaims,
  type ReducerCtx as IReducerCtx,
} from '../lib/reducers';
import { type UntypedSchemaDef } from '../lib/schema';
import { type RowType, type Table, type TableMethods } from '../lib/table';
import type { Infer } from '../lib/type_builders';
import { hasOwn, toCamelCase } from '../lib/util';
import { type AnonymousViewCtx, type ViewCtx } from './views';
import { isRowTypedQuery, makeQueryBuilder, toSql } from './query';
import type { DbView } from './db_view';
import { getErrorConstructor, SenderError } from './errors';
import { Range, type Bound } from './range';
import ViewResultHeader from '../lib/autogen/view_result_header_type';
import { makeRandom, type Random } from './rng';
import type { SchemaInner } from './schema';

const { freeze } = Object;

export const sys = _syscalls2_0;

export function parseJsonObject(json: string): JsonObject {
  let value: unknown;

  try {
    value = JSON.parse(json);
  } catch {
    throw new Error('Invalid JSON: failed to parse string');
  }

  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error('Expected a JSON object at the top level');
  }

  // The runtime check above guarantees this cast is safe
  return value as JsonObject;
}

class JwtClaimsImpl implements JwtClaims {
  readonly fullPayload: JsonObject;
  private readonly _identity: Identity;
  /**
   * Creates a new JwtClaims instance.
   * @param rawPayload The JWT payload as a raw JSON string.
   * @param identity The identity for this JWT. We are only taking this because we don't have a blake3 implementation (which we need to compute it).
   */
  constructor(
    public readonly rawPayload: string,
    identity: Identity
  ) {
    this.fullPayload = parseJsonObject(rawPayload);
    this._identity = identity;
  }
  readonly [claim: string]: unknown;
  get identity(): Identity {
    return this._identity;
  }
  get subject() {
    return this.fullPayload['sub'] as string;
  }
  get issuer() {
    return this.fullPayload['iss'] as string;
  }
  get audience() {
    const aud = this.fullPayload['aud'];
    if (aud == null) {
      return [];
    }
    return typeof aud === 'string' ? [aud] : (aud as string[]);
  }
}

class AuthCtxImpl implements AuthCtx {
  public readonly isInternal: boolean;

  // Source of the JWT payload string, if there is one.
  private readonly _jwtSource: () => string | null;
  // Whether we have initialized the JWT claims.
  private _initializedJWT: boolean = false;
  private _jwtClaims?: JwtClaims | null;
  private _senderIdentity: Identity;

  private constructor(opts: {
    isInternal: boolean;
    jwtSource: () => string | null;
    senderIdentity: Identity;
  }) {
    this.isInternal = opts.isInternal;
    this._jwtSource = opts.jwtSource;
    this._senderIdentity = opts.senderIdentity;
  }

  private _initializeJWT() {
    if (this._initializedJWT) return;
    this._initializedJWT = true;

    const token = this._jwtSource();
    if (!token) {
      this._jwtClaims = null;
    } else {
      this._jwtClaims = new JwtClaimsImpl(token, this._senderIdentity);
    }
    // At this point we can safely freeze the object.
    Object.freeze(this);
  }

  /** Lazily compute whether a JWT exists and is parseable. */
  get hasJWT(): boolean {
    this._initializeJWT();
    return this._jwtClaims !== null;
  }

  /** Lazily parse the JwtClaims only when accessed. */
  get jwt(): JwtClaims | null {
    this._initializeJWT();
    return this._jwtClaims!;
  }

  /** Create a context representing internal (non-user) requests. */
  static internal(): AuthCtx {
    return new AuthCtxImpl({
      isInternal: true,
      jwtSource: () => null,
      senderIdentity: Identity.zero(),
    });
  }

  /** If there is a connection id, look up the JWT payload from the system tables. */
  static fromSystemTables(
    connectionId: ConnectionId | null,
    sender: Identity
  ): AuthCtx {
    if (connectionId === null) {
      return new AuthCtxImpl({
        isInternal: false,
        jwtSource: () => null,
        senderIdentity: sender,
      });
    }
    return new AuthCtxImpl({
      isInternal: false,
      jwtSource: () => {
        const payloadBuf = sys.get_jwt_payload(connectionId.__connection_id__);
        if (payloadBuf.length === 0) return null;
        const payloadStr = new TextDecoder().decode(payloadBuf);
        return payloadStr;
      },
      senderIdentity: sender,
    });
  }
}

// Using a class expression rather than declaration keeps the class out of the
// type namespace, so that `ReducerCtx` still refers to the interface.
export const ReducerCtxImpl = class ReducerCtx<
  SchemaDef extends UntypedSchemaDef,
> implements IReducerCtx<SchemaDef>
{
  #identity: Identity | undefined;
  #senderAuth: AuthCtx | undefined;
  #uuidCounter: { value: number } | undefined;
  #random: Random | undefined;
  sender: Identity;
  timestamp: Timestamp;
  connectionId: ConnectionId | null;
  db: DbView<SchemaDef>;

  constructor(
    sender: Identity,
    timestamp: Timestamp,
    connectionId: ConnectionId | null,
    dbView: DbView<any>
  ) {
    Object.seal(this);
    this.sender = sender;
    this.timestamp = timestamp;
    this.connectionId = connectionId;
    this.db = dbView;
  }

  /** Reset the `ReducerCtx` to be used for a new transaction */
  static reset(
    me: InstanceType<typeof this>,
    sender: Identity,
    timestamp: Timestamp,
    connectionId: ConnectionId | null
  ) {
    me.sender = sender;
    me.timestamp = timestamp;
    me.connectionId = connectionId;
    me.#uuidCounter = undefined;
    me.#senderAuth = undefined;
  }

  get identity() {
    return (this.#identity ??= new Identity(sys.identity()));
  }

  get senderAuth() {
    return (this.#senderAuth ??= AuthCtxImpl.fromSystemTables(
      this.connectionId,
      this.sender
    ));
  }

  get random() {
    return (this.#random ??= makeRandom(this.timestamp));
  }

  /**
   * Create a new random {@link Uuid} `v4` using this `ReducerCtx`'s RNG.
   */
  newUuidV4(): Uuid {
    const bytes = this.random.fill(new Uint8Array(16));
    return Uuid.fromRandomBytesV4(bytes);
  }

  /**
   * Create a new sortable {@link Uuid} `v7` using this `ReducerCtx`'s RNG, counter,
   * and timestamp.
   */
  newUuidV7(): Uuid {
    const bytes = this.random.fill(new Uint8Array(4));
    const counter = (this.#uuidCounter ??= { value: 0 });
    return Uuid.fromCounterV7(counter, this.timestamp, bytes);
  }
};

/**
 * Call into a user function `fn` - the backtrace from an exception thrown in
 * `fn` or one of its descendants in the callgraph will be stripped by host
 * code in `crates/core/src/host/v8/error.rs` such that `fn` will be shown to
 * be the root of the call stack.
 */
export const callUserFunction = function __spacetimedb_end_short_backtrace<
  Args extends any[],
  R,
>(fn: (...args: Args) => R, ...args: Args): R {
  return fn(...args);
};

export const makeHooks = (schema: SchemaInner): ModuleHooks =>
  new ModuleHooksImpl(schema);

class ModuleHooksImpl implements ModuleHooks {
  #schema: SchemaInner;
  #dbView_: DbView<any> | undefined;
  #reducerArgsDeserializers;
  /** Cache the `ReducerCtx` object to avoid allocating anew for ever reducer call. */
  #reducerCtx_: InstanceType<typeof ReducerCtxImpl> | undefined;

  constructor(schema: SchemaInner) {
    this.#schema = schema;
    this.#reducerArgsDeserializers = schema.moduleDef.reducers.map(
      ({ params }) => ProductType.makeDeserializer(params, schema.typespace)
    );
  }

  get #dbView() {
    return (this.#dbView_ ??= freeze(
      Object.fromEntries(
        this.#schema.moduleDef.tables.map(table => [
          toCamelCase(table.sourceName),
          makeTableView(this.#schema.typespace, table),
        ])
      )
    ));
  }

  get #reducerCtx() {
    return (this.#reducerCtx_ ??= new ReducerCtxImpl(
      Identity.zero(),
      Timestamp.UNIX_EPOCH,
      null,
      this.#dbView
    ));
  }

  __describe_module__() {
    const writer = new BinaryWriter(128);
    RawModuleDef.serialize(
      writer,
      RawModuleDef.V10(this.#schema.rawModuleDefV10())
    );
    return writer.getBuffer();
  }

  __get_error_constructor__(code: number): new (msg: string) => Error {
    return getErrorConstructor(code);
  }

  get __sender_error_class__() {
    return SenderError;
  }

  __call_reducer__(
    reducerId: u32,
    sender: u256,
    connId: u128,
    timestamp: bigint,
    argsBuf: DataView
  ): void {
    const moduleCtx = this.#schema;
    const deserializeArgs = this.#reducerArgsDeserializers[reducerId];
    BINARY_READER.reset(argsBuf);
    const args = deserializeArgs(BINARY_READER);
    const senderIdentity = new Identity(sender);
    const ctx = this.#reducerCtx;
    ReducerCtxImpl.reset(
      ctx,
      senderIdentity,
      new Timestamp(timestamp),
      ConnectionId.nullIfZero(new ConnectionId(connId))
    );
    callUserFunction(moduleCtx.reducers[reducerId], ctx, args);
  }

  __call_view__(
    id: u32,
    sender: u256,
    argsBuf: Uint8Array
  ): { data: Uint8Array } {
    const moduleCtx = this.#schema;
    const { fn, deserializeParams, serializeReturn, returnTypeBaseSize } =
      moduleCtx.views[id];
    const ctx: ViewCtx<any> = freeze({
      sender: new Identity(sender),
      // this is the non-readonly DbView, but the typing for the user will be
      // the readonly one, and if they do call mutating functions it will fail
      // at runtime
      db: this.#dbView,
      from: makeQueryBuilder(moduleCtx.schemaType),
    });
    const args = deserializeParams(new BinaryReader(argsBuf));
    const ret = callUserFunction(fn, ctx, args);
    const retBuf = new BinaryWriter(returnTypeBaseSize);
    if (isRowTypedQuery(ret)) {
      const query = toSql(ret);
      ViewResultHeader.serialize(retBuf, ViewResultHeader.RawSql(query));
    } else {
      ViewResultHeader.serialize(retBuf, ViewResultHeader.RowData);
      serializeReturn(retBuf, ret);
    }
    return { data: retBuf.getBuffer() };
  }

  __call_view_anon__(id: u32, argsBuf: Uint8Array): { data: Uint8Array } {
    const moduleCtx = this.#schema;
    const { fn, deserializeParams, serializeReturn, returnTypeBaseSize } =
      moduleCtx.anonViews[id];
    const ctx: AnonymousViewCtx<any> = freeze({
      // this is the non-readonly DbView, but the typing for the user will be
      // the readonly one, and if they do call mutating functions it will fail
      // at runtime
      db: this.#dbView,
      from: makeQueryBuilder(moduleCtx.schemaType),
    });
    const args = deserializeParams(new BinaryReader(argsBuf));
    const ret = callUserFunction(fn, ctx, args);
    const retBuf = new BinaryWriter(returnTypeBaseSize);
    if (isRowTypedQuery(ret)) {
      const query = toSql(ret);
      ViewResultHeader.serialize(retBuf, ViewResultHeader.RawSql(query));
    } else {
      ViewResultHeader.serialize(retBuf, ViewResultHeader.RowData);
      serializeReturn(retBuf, ret);
    }
    return { data: retBuf.getBuffer() };
  }

  __call_procedure__(
    id: u32,
    sender: u256,
    connection_id: u128,
    timestamp: bigint,
    args: Uint8Array
  ): Uint8Array {
    return callProcedure(
      this.#schema,
      id,
      new Identity(sender),
      ConnectionId.nullIfZero(new ConnectionId(connection_id)),
      new Timestamp(timestamp),
      args,
      () => this.#dbView
    );
  }
}

const BINARY_WRITER = new BinaryWriter(0);
const BINARY_READER = new BinaryReader(new Uint8Array());

function makeTableView(
  typespace: Infer<typeof Typespace>,
  table: Infer<typeof RawTableDefV10>
): Table<any> {
  const table_id = sys.table_id_from_name(table.sourceName);
  const rowType = typespace.types[table.productTypeRef];
  if (rowType.tag !== 'Product') {
    throw 'impossible';
  }

  const serializeRow = AlgebraicType.makeSerializer(rowType, typespace);
  const deserializeRow = AlgebraicType.makeDeserializer(rowType, typespace);

  const sequences = table.sequences.map(seq => {
    const col = rowType.value.elements[seq.column];
    const colType = col.algebraicType;

    // Determine the sentinel value which users will pass to as a placeholder
    // to cause the sequence to advance.
    // For small integer SATS types which fit in V8 `number`s, this is `0: number`,
    // and for larger integer SATS types it's `0n: BigInt`.
    let sequenceTrigger: bigint | number;
    switch (colType.tag) {
      case 'U8':
      case 'I8':
      case 'U16':
      case 'I16':
      case 'U32':
      case 'I32':
        sequenceTrigger = 0;
        break;
      case 'U64':
      case 'I64':
      case 'U128':
      case 'I128':
      case 'U256':
      case 'I256':
        sequenceTrigger = 0n;
        break;
      default:
        throw new TypeError('invalid sequence type');
    }
    return {
      colName: col.name!,
      sequenceTrigger,
      deserialize: AlgebraicType.makeDeserializer(colType, typespace),
    };
  });
  const hasAutoIncrement = sequences.length > 0;

  const iter = () =>
    tableIterator(sys.datastore_table_scan_bsatn(table_id), deserializeRow);

  const integrateGeneratedColumns = hasAutoIncrement
    ? (row: RowType<any>, ret_buf: DataView) => {
        BINARY_READER.reset(ret_buf);
        for (const { colName, deserialize, sequenceTrigger } of sequences) {
          if (row[colName] === sequenceTrigger) {
            row[colName] = deserialize(BINARY_READER);
          }
        }
      }
    : null;

  const tableMethods: TableMethods<any> = {
    count: () => sys.datastore_table_row_count(table_id),
    iter,
    [Symbol.iterator]: () => iter(),
    insert: row => {
      const buf = LEAF_BUF;
      BINARY_WRITER.reset(buf);
      serializeRow(BINARY_WRITER, row);
      sys.datastore_insert_bsatn(table_id, buf.buffer, BINARY_WRITER.offset);
      const ret = { ...row };
      integrateGeneratedColumns?.(ret, buf.view);

      return ret;
    },
    delete: (row: RowType<any>): boolean => {
      const buf = LEAF_BUF;
      BINARY_WRITER.reset(buf);
      BINARY_WRITER.writeU32(1);
      serializeRow(BINARY_WRITER, row);
      const count = sys.datastore_delete_all_by_eq_bsatn(
        table_id,
        buf.buffer,
        BINARY_WRITER.offset
      );
      return count > 0;
    },
  };

  const tableView = Object.assign(
    Object.create(null),
    tableMethods
  ) as Table<any>;

  for (const indexDef of table.indexes) {
    const index_id = sys.index_id_from_name(indexDef.sourceName!);

    let column_ids: number[];
    let isHashIndex = false;
    switch (indexDef.algorithm.tag) {
      case 'Hash':
        isHashIndex = true;
        column_ids = indexDef.algorithm.value;
        break;
      case 'BTree':
        column_ids = indexDef.algorithm.value;
        break;
      case 'Direct':
        column_ids = [indexDef.algorithm.value];
        break;
    }
    const numColumns = column_ids.length;

    const columnSet = new Set(column_ids);
    const isUnique = table.constraints
      .filter(x => x.data.tag === 'Unique')
      .some(x => columnSet.isSubsetOf(new Set(x.data.value.columns)));

    const indexSerializers = column_ids.map(id =>
      AlgebraicType.makeSerializer(
        rowType.value.elements[id].algebraicType,
        typespace
      )
    );

    const serializePoint = (buffer: ResizableBuffer, colVal: any[]): number => {
      BINARY_WRITER.reset(buffer);
      for (let i = 0; i < numColumns; i++) {
        indexSerializers[i](BINARY_WRITER, colVal[i]);
      }
      return BINARY_WRITER.offset;
    };

    const serializeSingleElement =
      numColumns === 1 ? indexSerializers[0] : null;

    const serializeSinglePoint =
      serializeSingleElement &&
      ((buffer: ResizableBuffer, colVal: any): number => {
        BINARY_WRITER.reset(buffer);
        serializeSingleElement(BINARY_WRITER, colVal);
        return BINARY_WRITER.offset;
      });

    type IndexScanArgs = [
      prefix_len: u32,
      prefix_elems: u16,
      rstart_len: u32,
      rend_len: u32,
    ];

    let index: Index<any, any>;
    if (isUnique && serializeSinglePoint) {
      // numColumns == 1, unique index
      index = {
        find: (colVal: IndexVal<any, any>): RowType<any> | null => {
          const buf = LEAF_BUF;
          const point_len = serializeSinglePoint(buf, colVal);
          const iter_id = sys.datastore_index_scan_point_bsatn(
            index_id,
            buf.buffer,
            point_len
          );
          return tableIterateOne(iter_id, deserializeRow);
        },
        delete: (colVal: IndexVal<any, any>): boolean => {
          const buf = LEAF_BUF;
          const point_len = serializeSinglePoint(buf, colVal);
          const num = sys.datastore_delete_by_index_scan_point_bsatn(
            index_id,
            buf.buffer,
            point_len
          );
          return num > 0;
        },
        update: (row: RowType<any>): RowType<any> => {
          const buf = LEAF_BUF;
          BINARY_WRITER.reset(buf);
          serializeRow(BINARY_WRITER, row);
          sys.datastore_update_bsatn(
            table_id,
            index_id,
            buf.buffer,
            BINARY_WRITER.offset
          );
          integrateGeneratedColumns?.(row, buf.view);
          return row;
        },
      } as UniqueIndex<any, any>;
    } else if (isUnique) {
      // numColumns != 1, unique index
      index = {
        find: (colVal: IndexVal<any, any>): RowType<any> | null => {
          if (colVal.length !== numColumns) {
            throw new TypeError('wrong number of elements');
          }
          const buf = LEAF_BUF;
          const point_len = serializePoint(buf, colVal);
          const iter_id = sys.datastore_index_scan_point_bsatn(
            index_id,
            buf.buffer,
            point_len
          );
          return tableIterateOne(iter_id, deserializeRow);
        },
        delete: (colVal: IndexVal<any, any>): boolean => {
          if (colVal.length !== numColumns)
            throw new TypeError('wrong number of elements');

          const buf = LEAF_BUF;
          const point_len = serializePoint(buf, colVal);
          const num = sys.datastore_delete_by_index_scan_point_bsatn(
            index_id,
            buf.buffer,
            point_len
          );
          return num > 0;
        },
        update: (row: RowType<any>): RowType<any> => {
          const buf = LEAF_BUF;
          BINARY_WRITER.reset(buf);
          serializeRow(BINARY_WRITER, row);
          sys.datastore_update_bsatn(
            table_id,
            index_id,
            buf.buffer,
            BINARY_WRITER.offset
          );
          integrateGeneratedColumns?.(row, buf.view);
          return row;
        },
      } as UniqueIndex<any, any>;
    } else if (serializeSinglePoint) {
      // numColumns == 1
      const rawIndex = {
        filter: (range: any): IteratorObject<RowType<any>> => {
          const buf = LEAF_BUF;
          const point_len = serializeSinglePoint(buf, range);
          const iter_id = sys.datastore_index_scan_point_bsatn(
            index_id,
            buf.buffer,
            point_len
          );
          return tableIterator(iter_id, deserializeRow);
        },
        delete: (range: any): u32 => {
          const buf = LEAF_BUF;
          const point_len = serializeSinglePoint(buf, range);
          return sys.datastore_delete_by_index_scan_point_bsatn(
            index_id,
            buf.buffer,
            point_len
          );
        },
      };
      if (isHashIndex) {
        index = rawIndex as PointIndex<any, any>;
      } else {
        index = rawIndex as RangedIndex<any, any>;
      }
    } else if (isHashIndex) {
      // numColumns != 1
      index = {
        filter: (range: any[]): IteratorObject<RowType<any>> => {
          const buf = LEAF_BUF;
          const point_len = serializePoint(buf, range);
          const iter_id = sys.datastore_index_scan_point_bsatn(
            index_id,
            buf.buffer,
            point_len
          );
          return tableIterator(iter_id, deserializeRow);
        },
        delete: (range: any[]): u32 => {
          const buf = LEAF_BUF;
          const point_len = serializePoint(buf, range);
          return sys.datastore_delete_by_index_scan_point_bsatn(
            index_id,
            buf.buffer,
            point_len
          );
        },
      } as PointIndex<any, any>;
    } else {
      // numColumns != 1
      const serializeRange = (
        buffer: ResizableBuffer,
        range: any[]
      ): IndexScanArgs => {
        if (range.length > numColumns) throw new TypeError('too many elements');

        BINARY_WRITER.reset(buffer);
        const writer = BINARY_WRITER;
        const prefix_elems = range.length - 1;
        for (let i = 0; i < prefix_elems; i++) {
          indexSerializers[i](writer, range[i]);
        }
        const rstartOffset = writer.offset;
        const term = range[range.length - 1];
        const serializeTerm = indexSerializers[range.length - 1];
        if (term instanceof Range) {
          const writeBound = (bound: Bound<any>) => {
            const tags = { included: 0, excluded: 1, unbounded: 2 };
            writer.writeU8(tags[bound.tag]);
            if (bound.tag !== 'unbounded') serializeTerm(writer, bound.value);
          };
          writeBound(term.from);
          const rstartLen = writer.offset - rstartOffset;
          writeBound(term.to);
          const rendLen = writer.offset - rstartLen;
          return [rstartOffset, prefix_elems, rstartLen, rendLen];
        } else {
          writer.writeU8(0);
          serializeTerm(writer, term);
          const rstartLen = writer.offset;
          const rendLen = 0;
          return [rstartOffset, prefix_elems, rstartLen, rendLen];
        }
      };
      index = {
        filter: (range: any[]): IteratorObject<RowType<any>> => {
          if (range.length === numColumns) {
            const buf = LEAF_BUF;
            const point_len = serializePoint(buf, range);
            const iter_id = sys.datastore_index_scan_point_bsatn(
              index_id,
              buf.buffer,
              point_len
            );
            return tableIterator(iter_id, deserializeRow);
          } else {
            const buf = LEAF_BUF;
            const args = serializeRange(buf, range);
            const iter_id = sys.datastore_index_scan_range_bsatn(
              index_id,
              buf.buffer,
              ...args
            );
            return tableIterator(iter_id, deserializeRow);
          }
        },
        delete: (range: any[]): u32 => {
          if (range.length === numColumns) {
            const buf = LEAF_BUF;
            const point_len = serializePoint(buf, range);
            return sys.datastore_delete_by_index_scan_point_bsatn(
              index_id,
              buf.buffer,
              point_len
            );
          } else {
            const buf = LEAF_BUF;
            const args = serializeRange(buf, range);
            return sys.datastore_delete_by_index_scan_range_bsatn(
              index_id,
              buf.buffer,
              ...args
            );
          }
        },
      } as RangedIndex<any, any>;
    }

    //TODO: use accessor name
    if (Object.hasOwn(tableView, indexDef.sourceName!)) {
      freeze(Object.assign(tableView[indexDef.sourceName!], index));
    } else {
      tableView[indexDef.sourceName!] = freeze(index) as any;
    }
  }

  return freeze(tableView);
}

function* tableIterator<T>(
  id: u32,
  deserialize: Deserializer<T>
): Generator<T, undefined> {
  using iter = new IteratorHandle(id);

  const iterBuf = takeBuf();
  try {
    let amt;
    while ((amt = iter.advance(iterBuf))) {
      const reader = new BinaryReader(iterBuf.view);
      while (reader.offset < amt) {
        yield deserialize(reader);
      }
    }
  } finally {
    returnBuf(iterBuf);
  }
}

function tableIterateOne<T>(id: u32, deserialize: Deserializer<T>): T | null {
  const buf = LEAF_BUF;
  // we only need to check for the `<= 0` case, since this function is only used
  // with iterators that should only have zero or one element.
  const ret = advanceIterRaw(id, buf);
  if (ret !== 0) {
    BINARY_READER.reset(buf.view);
    return deserialize(BINARY_READER);
  }
  return null;
}

/**
 * `ret < 0` means the iterator yielded elements but is now exhausted and has been destroyed.
 * `ret === 0` means the iterator was empty and has been destroyed.
 * `ret > 0` means the iterator yielded elements and has more to give.
 */
function advanceIterRaw(id: u32, buf: ResizableBuffer): number {
  while (true) {
    try {
      return 0 | sys.row_iter_bsatn_advance(id, buf.buffer);
    } catch (e) {
      if (e && typeof e === 'object' && hasOwn(e, '__buffer_too_small__')) {
        buf.grow(e.__buffer_too_small__ as number);
        continue;
      }
      throw e;
    }
  }
}

// This should guarantee in most cases that we don't have to reallocate an iterator
// buffer, unless there's a single row that serializes to >1 MiB.
const DEFAULT_BUFFER_CAPACITY = 32 * 1024 * 2;

const ITER_BUFS: ResizableBuffer[] = [
  new ResizableBuffer(DEFAULT_BUFFER_CAPACITY),
];
let ITER_BUF_COUNT = 1;

function takeBuf(): ResizableBuffer {
  return ITER_BUF_COUNT
    ? ITER_BUFS[--ITER_BUF_COUNT]
    : new ResizableBuffer(DEFAULT_BUFFER_CAPACITY);
}

function returnBuf(buf: ResizableBuffer) {
  ITER_BUFS[ITER_BUF_COUNT++] = buf;
}

/**
 * This should only be used from functions that don't need persistent ownership
 * over the buffer. While using this value, one should not call a function that
 * also uses this value.
 */
const LEAF_BUF = new ResizableBuffer(DEFAULT_BUFFER_CAPACITY);

/** A class to manage the lifecycle of an iterator handle. */
class IteratorHandle implements Disposable {
  #id: u32 | -1;

  static #finalizationRegistry = new FinalizationRegistry<u32>(
    sys.row_iter_bsatn_close
  );

  constructor(id: u32) {
    this.#id = id;
    IteratorHandle.#finalizationRegistry.register(this, id, this);
  }

  /** Unregister this object with the finalization registry and return the id */
  #detach() {
    const id = this.#id;
    this.#id = -1;
    IteratorHandle.#finalizationRegistry.unregister(this);
    return id;
  }

  /** Call `row_iter_bsatn_advance`, returning 0 if this iterator has been exhausted. */
  advance(buf: ResizableBuffer): number {
    if (this.#id === -1) return 0;
    const ret = advanceIterRaw(this.#id, buf);
    if (ret <= 0) this.#detach();
    return ret < 0 ? -ret : ret;
  }

  [Symbol.dispose]() {
    if (this.#id >= 0) {
      const id = this.#detach();
      sys.row_iter_bsatn_close(id);
    }
  }
}

function fmtLog(...data: any[]) {
  return data.join(' ');
}

const console_level_error = 0;
const console_level_warn = 1;
const console_level_info = 2;
const console_level_debug = 3;
const console_level_trace = 4;
const _console_level_panic = 101;

const timerMap = new Map<string, u32>();

const console: Console = {
  // @ts-expect-error we want a blank prototype, but typescript complains
  __proto__: {},
  [Symbol.toStringTag]: 'console',
  assert: (condition = false, ...data: any[]) => {
    if (!condition) {
      sys.console_log(console_level_error, fmtLog(...data));
    }
  },
  clear: () => {},
  debug: (...data: any[]) => {
    sys.console_log(console_level_debug, fmtLog(...data));
  },
  error: (...data: any[]) => {
    sys.console_log(console_level_error, fmtLog(...data));
  },
  info: (...data: any[]) => {
    sys.console_log(console_level_info, fmtLog(...data));
  },
  log: (...data: any[]) => {
    sys.console_log(console_level_info, fmtLog(...data));
  },
  table: (tabularData: any, _properties: any) => {
    sys.console_log(console_level_info, fmtLog(tabularData));
  },
  trace: (...data: any[]) => {
    sys.console_log(console_level_trace, fmtLog(...data));
  },
  warn: (...data: any[]) => {
    sys.console_log(console_level_warn, fmtLog(...data));
  },
  dir: (_item: any, _options: any) => {},
  dirxml: (..._data: any[]) => {},
  // Counting
  count: (_label = 'default') => {},
  countReset: (_label = 'default') => {},
  // Grouping
  group: (..._data: any[]) => {},
  groupCollapsed: (..._data: any[]) => {},
  groupEnd: () => {},
  // Timing
  time: (label = 'default') => {
    if (timerMap.has(label)) {
      sys.console_log(console_level_warn, `Timer '${label}' already exists.`);
      return;
    }
    timerMap.set(label, sys.console_timer_start(label));
  },
  timeLog: (label = 'default', ...data: any[]) => {
    sys.console_log(console_level_info, fmtLog(label, ...data));
  },
  timeEnd: (label = 'default') => {
    const spanId = timerMap.get(label);
    if (spanId === undefined) {
      sys.console_log(console_level_warn, `Timer '${label}' does not exist.`);
      return;
    }
    sys.console_timer_end(spanId);
    timerMap.delete(label);
  },
  // Additional console methods to satisfy the Console interface
  timeStamp: () => {},
  profile: () => {},
  profileEnd: () => {},
};

(console as any).Console = console;

globalThis.console = console;
