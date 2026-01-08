import * as _syscalls2_0 from 'spacetime:sys@2.0';

import type { ModuleHooks, u16, u32 } from 'spacetime:sys@2.0';
import {
  AlgebraicType,
  ProductType,
  type Deserializer,
} from '../lib/algebraic_type';
import RawModuleDef from '../lib/autogen/raw_module_def_type';
import type RawModuleDefV9 from '../lib/autogen/raw_module_def_v_9_type';
import type RawTableDefV9 from '../lib/autogen/raw_table_def_v_9_type';
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
  type RangedIndex,
  type UniqueIndex,
} from '../lib/indexes';
import { callProcedure as callProcedure } from './procedures';
import {
  REDUCERS,
  type AuthCtx,
  type JsonObject,
  type JwtClaims,
  type ReducerCtx as IReducerCtx,
} from '../lib/reducers';
import {
  MODULE_DEF,
  getRegisteredSchema,
  type UntypedSchemaDef,
} from '../lib/schema';
import { type RowType, type Table, type TableMethods } from '../lib/table';
import type { Infer } from '../lib/type_builders';
import { hasOwn, toCamelCase } from '../lib/util';
import {
  ANON_VIEWS,
  VIEWS,
  type AnonymousViewCtx,
  type ViewCtx,
} from '../lib/views';
import { isRowTypedQuery, makeQueryBuilder, toSql } from './query';
import type { DbView } from './db_view';
import { SenderError, SpacetimeHostError } from './errors';
import { Range, type Bound } from './range';
import ViewResultHeader from '../lib/autogen/view_result_header_type';

const { freeze } = Object;

export const sys = freeze(wrapSyscalls(_syscalls2_0));

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

let REDUCER_CTX: InstanceType<typeof ReducerCtxImpl> | undefined;

// Using a class expression rather than declaration keeps the class out of the
// type namespace, so that `ReducerCtx` still refers to the interface.
export const ReducerCtxImpl = class ReducerCtx<
  SchemaDef extends UntypedSchemaDef,
> implements IReducerCtx<SchemaDef>
{
  #identity: Identity | undefined;
  #senderAuth: AuthCtx | undefined;
  #uuidCounter: { value: number } | undefined;
  sender: Identity;
  timestamp: Timestamp;
  connectionId: ConnectionId | null;
  db: DbView<SchemaDef>;

  constructor(
    sender: Identity,
    timestamp: Timestamp,
    connectionId: ConnectionId | null
  ) {
    Object.seal(this);
    this.sender = sender;
    this.timestamp = timestamp;
    this.connectionId = connectionId;
    this.db = getDbView();
  }

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
    return (this.#identity ??= new Identity(sys.identity().__identity__));
  }

  get senderAuth() {
    return (this.#senderAuth ??= AuthCtxImpl.fromSystemTables(
      this.connectionId,
      this.sender
    ));
  }

  /**
   * Create a new random {@link Uuid} `v4` using the {@link crypto} RNG.
   *
   * WARN: Until we use a spacetime RNG this make calls non-deterministic.
   */
  newUuidV4(): Uuid {
    // TODO: Use a spacetime RNG when available
    const bytes = crypto.getRandomValues(new Uint8Array(16));
    return Uuid.fromRandomBytesV4(bytes);
  }

  /**
   * Create a new sortable {@link Uuid} `v7` using the {@link crypto} RNG, counter,
   * and the timestamp.
   *
   * WARN: Until we use a spacetime RNG this make calls non-deterministic.
   */
  newUuidV7(): Uuid {
    // TODO: Use a spacetime RNG when available
    const bytes = crypto.getRandomValues(new Uint8Array(4));
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

let reducerArgsDeserializers: Deserializer<any>[];

export const hooks: ModuleHooks = {
  __describe_module__() {
    const writer = new BinaryWriter(128);
    RawModuleDef.serialize(writer, RawModuleDef.V9(MODULE_DEF));
    return writer.getBuffer();
  },
  __call_reducer__(reducerId, sender, connId, timestamp, argsBuf) {
    if (reducerArgsDeserializers === undefined) {
      reducerArgsDeserializers = MODULE_DEF.reducers.map(({ params }) =>
        ProductType.makeDeserializer(params, MODULE_DEF.typespace)
      );
    }
    const deserializeArgs = reducerArgsDeserializers[reducerId];
    BINARY_READER.reset(argsBuf);
    const args = deserializeArgs(BINARY_READER);
    const senderIdentity = new Identity(sender);
    let ctx;
    if (REDUCER_CTX === undefined) {
      ctx = REDUCER_CTX = new ReducerCtxImpl(
        senderIdentity,
        new Timestamp(timestamp),
        ConnectionId.nullIfZero(new ConnectionId(connId))
      );
    } else {
      ctx = REDUCER_CTX;
      ReducerCtxImpl.reset(
        REDUCER_CTX,
        senderIdentity,
        new Timestamp(timestamp),
        ConnectionId.nullIfZero(new ConnectionId(connId))
      );
    }
    try {
      callUserFunction(REDUCERS[reducerId], ctx, args);
    } catch (e) {
      if (e instanceof SenderError) {
        return { tag: 'err', value: e.message };
      }
      throw e;
    }
  },
  __call_view__(id, sender, argsBuf) {
    const { fn, deserializeParams, serializeReturn, returnTypeBaseSize } =
      VIEWS[id];
    const ctx: ViewCtx<any> = freeze({
      sender: new Identity(sender),
      // this is the non-readonly DbView, but the typing for the user will be
      // the readonly one, and if they do call mutating functions it will fail
      // at runtime
      db: getDbView(),
      from: makeQueryBuilder(getRegisteredSchema()),
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
  },
  __call_view_anon__(id, argsBuf) {
    const { fn, deserializeParams, serializeReturn, returnTypeBaseSize } =
      ANON_VIEWS[id];
    const ctx: AnonymousViewCtx<any> = freeze({
      // this is the non-readonly DbView, but the typing for the user will be
      // the readonly one, and if they do call mutating functions it will fail
      // at runtime
      db: getDbView(),
      from: makeQueryBuilder(getRegisteredSchema()),
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
  },
  __call_procedure__(id, sender, connection_id, timestamp, args) {
    return callProcedure(
      id,
      new Identity(sender),
      ConnectionId.nullIfZero(new ConnectionId(connection_id)),
      new Timestamp(timestamp),
      args
    );
  },
};

let DB_VIEW: DbView<any> | null = null;
function getDbView() {
  DB_VIEW ??= makeDbView(MODULE_DEF);
  return DB_VIEW;
}

function makeDbView(moduleDef: Infer<typeof RawModuleDefV9>): DbView<any> {
  return freeze(
    Object.fromEntries(
      moduleDef.tables.map(table => [
        toCamelCase(table.name),
        makeTableView(moduleDef.typespace, table),
      ])
    )
  );
}

const BINARY_WRITER = new BinaryWriter(0);
const BINARY_READER = new BinaryReader(new Uint8Array());

function makeTableView(
  typespace: Infer<typeof Typespace>,
  table: Infer<typeof RawTableDefV9>
): Table<any> {
  const table_id = sys.table_id_from_name(table.name);
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
      const buf = takeBuf();
      try {
        BINARY_WRITER.reset(buf);
        serializeRow(BINARY_WRITER, row);
        sys.datastore_insert_bsatn(table_id, buf.buffer, BINARY_WRITER.offset);
        const ret = { ...row };
        integrateGeneratedColumns?.(ret, buf.view);

        return ret;
      } finally {
        returnBuf(buf);
      }
    },
    delete: (row: RowType<any>): boolean => {
      const buf = takeBuf();
      try {
        BINARY_WRITER.reset(buf);
        BINARY_WRITER.writeU32(1);
        serializeRow(BINARY_WRITER, row);
        const count = sys.datastore_delete_all_by_eq_bsatn(
          table_id,
          buf.buffer,
          BINARY_WRITER.offset
        );
        return count > 0;
      } finally {
        returnBuf(buf);
      }
    },
  };

  const tableView = Object.assign(
    Object.create(null),
    tableMethods
  ) as Table<any>;

  for (const indexDef of table.indexes) {
    const index_id = sys.index_id_from_name(indexDef.name!);

    let column_ids: number[];
    switch (indexDef.algorithm.tag) {
      case 'BTree':
        column_ids = indexDef.algorithm.value;
        break;
      case 'Hash':
        throw new Error('impossible');
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
          let iter_id;
          {
            const buf = takeBuf();
            try {
              const point_len = serializeSinglePoint(buf, colVal);
              iter_id = sys.datastore_index_scan_point_bsatn(
                index_id,
                buf.buffer,
                point_len
              );
            } finally {
              returnBuf(buf);
            }
          }
          return tableIterateOne(iter_id, deserializeRow);
        },
        delete: (colVal: IndexVal<any, any>): boolean => {
          const buf = takeBuf();
          try {
            const point_len = serializeSinglePoint(buf, colVal);
            const num = sys.datastore_delete_by_index_scan_point_bsatn(
              index_id,
              buf.buffer,
              point_len
            );
            return num > 0;
          } finally {
            returnBuf(buf);
          }
        },
        update: (row: RowType<any>): RowType<any> => {
          const buf = takeBuf();
          try {
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
          } finally {
            returnBuf(buf);
          }
        },
      } as UniqueIndex<any, any>;
    } else if (isUnique) {
      // numColumns != 1, unique index
      index = {
        find: (colVal: IndexVal<any, any>): RowType<any> | null => {
          if (colVal.length !== numColumns) {
            throw new TypeError('wrong number of elements');
          }
          let iter_id;
          {
            const buf = takeBuf();
            try {
              const point_len = serializePoint(buf, colVal);
              iter_id = sys.datastore_index_scan_point_bsatn(
                index_id,
                buf.buffer,
                point_len
              );
            } finally {
              returnBuf(buf);
            }
          }
          return tableIterateOne(iter_id, deserializeRow);
        },
        delete: (colVal: IndexVal<any, any>): boolean => {
          if (colVal.length !== numColumns)
            throw new TypeError('wrong number of elements');

          const buf = takeBuf();
          try {
            const point_len = serializePoint(buf, colVal);
            const num = sys.datastore_delete_by_index_scan_point_bsatn(
              index_id,
              buf.buffer,
              point_len
            );
            return num > 0;
          } finally {
            returnBuf(buf);
          }
        },
        update: (row: RowType<any>): RowType<any> => {
          const buf = takeBuf();
          try {
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
          } finally {
            returnBuf(buf);
          }
        },
      } as UniqueIndex<any, any>;
    } else if (serializeSinglePoint) {
      // numColumns == 1
      index = {
        filter: (range: any): IteratorObject<RowType<any>> => {
          let iter_id;
          {
            const buf = takeBuf();
            try {
              const point_len = serializeSinglePoint(buf, range);
              iter_id = sys.datastore_index_scan_point_bsatn(
                index_id,
                buf.buffer,
                point_len
              );
            } finally {
              returnBuf(buf);
            }
          }
          return tableIterator(iter_id, deserializeRow);
        },
        delete: (range: any): u32 => {
          const buf = takeBuf();
          try {
            const point_len = serializeSinglePoint(buf, range);
            return sys.datastore_delete_by_index_scan_point_bsatn(
              index_id,
              buf.buffer,
              point_len
            );
          } finally {
            returnBuf(buf);
          }
        },
      } as RangedIndex<any, any>;
    } else {
      // numColumns != 1
      const serializeRange = (
        buffer: ResizableBuffer,
        range: any[]
      ): IndexScanArgs => {
        if (range.length > numColumns) throw new TypeError('too many elements');

        const writer = new BinaryWriter(buffer);
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
            let iter_id;
            {
              const buf = takeBuf();
              try {
                const point_len = serializePoint(buf, range);
                iter_id = sys.datastore_index_scan_point_bsatn(
                  index_id,
                  buf.buffer,
                  point_len
                );
              } finally {
                returnBuf(buf);
              }
            }
            return tableIterator(iter_id, deserializeRow);
          } else {
            let iter_id;
            {
              const buf = takeBuf();
              try {
                const args = serializeRange(buf, range);
                iter_id = sys.datastore_index_scan_range_bsatn(
                  index_id,
                  buf.buffer,
                  ...args
                );
              } finally {
                returnBuf(buf);
              }
            }
            return tableIterator(iter_id, deserializeRow);
          }
        },
        delete: (range: any[]): u32 => {
          if (range.length === numColumns) {
            const buf = takeBuf();
            try {
              const point_len = serializePoint(buf, range);
              return sys.datastore_delete_by_index_scan_point_bsatn(
                index_id,
                buf.buffer,
                point_len
              );
            } finally {
              returnBuf(buf);
            }
          } else {
            const buf = takeBuf();
            try {
              const args = serializeRange(buf, range);
              return sys.datastore_delete_by_index_scan_range_bsatn(
                index_id,
                buf.buffer,
                ...args
              );
            } finally {
              returnBuf(buf);
            }
          }
        },
      } as RangedIndex<any, any>;
    }

    if (Object.hasOwn(tableView, indexDef.accessorName!)) {
      freeze(Object.assign(tableView[indexDef.accessorName!], index));
    } else {
      tableView[indexDef.accessorName!] = freeze(index) as any;
    }
  }

  return freeze(tableView);
}

function* tableIterator<T>(
  id: u32,
  deserialize: Deserializer<T>
): Generator<T, undefined> {
  const iter = new GCedIteratorHandle(id);
  try {
    const buf = takeBuf();
    try {
      const reader = new BinaryReader(buf.view);
      let amt;
      while ((amt = advanceIter(iter, buf))) {
        reader.reset(buf.view);
        while (reader.offset < amt) {
          yield deserialize(reader);
        }
      }
    } finally {
      returnBuf(buf);
    }
  } finally {
    iter[Symbol.dispose]();
  }
}

function tableIterateOne<T>(id: u32, deserialize: Deserializer<T>): T | null {
  const buf = takeBuf();
  try {
    // coerce to int32
    const ret = advanceIterRaw(id, buf);
    // ret <= 0 means the iterator is exhausted
    if (ret > 0) throw new Error('iter should only have one');
    const amt = -ret;
    if (amt) {
      BINARY_READER.reset(buf.view);
      return deserialize(BINARY_READER);
    }
    return null;
  } finally {
    returnBuf(buf);
  }
}

function advanceIter(iter: IteratorHandle, buf: ResizableBuffer): number {
  while (true) {
    try {
      return iter.advance(buf.buffer);
    } catch (e) {
      if (e && typeof e === 'object' && hasOwn(e, '__buffer_too_small__')) {
        buf.grow(e.__buffer_too_small__ as number);
        continue;
      }
      throw e;
    }
  }
}

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

/** A class to manage the lifecycle of an iterator handle. */
class IteratorHandle implements Disposable {
  #id: u32 | -1;

  constructor(id: u32) {
    this.#id = id;
  }

  /** Unregister this object with the finalization registry and return the id */
  protected detach() {
    const id = this.#id;
    this.#id = -1;
    return id;
  }

  /** Call `row_iter_bsatn_advance`, returning 0 if this iterator has been exhausted. */
  advance(buf: ArrayBuffer): number {
    if (this.#id === -1) return 0;
    // coerce to int32
    const ret = 0 | sys.row_iter_bsatn_advance(this.#id, buf);
    // ret <= 0 means the iterator is exhausted
    if (ret <= 0) this.detach();
    return ret < 0 ? -ret : ret;
  }

  [Symbol.dispose]() {
    if (this.#id >= 0) {
      const id = this.detach();
      sys.row_iter_bsatn_close(id);
    }
  }
}

class GCedIteratorHandle extends IteratorHandle {
  static #finalizationRegistry = new FinalizationRegistry<u32>(
    sys.row_iter_bsatn_close
  );

  constructor(id: u32) {
    super(id);
    GCedIteratorHandle.#finalizationRegistry.register(this, id, this);
  }

  protected override detach() {
    const id = super.detach();
    GCedIteratorHandle.#finalizationRegistry.unregister(this);
    return id;
  }
}

type Intersections<Ts extends readonly any[]> = Ts extends [
  infer T,
  ...infer Rest,
]
  ? T & Intersections<Rest>
  : unknown;

function wrapSyscalls<
  Modules extends Record<string, (...args: any[]) => any>[],
>(...modules: Modules): Intersections<Modules> {
  return Object.fromEntries(
    modules.flatMap(Object.entries).map(([k, v]) => [k, wrapSyscall(v)])
  ) as Intersections<Modules>;
}

function wrapSyscall<F extends (...args: any[]) => any>(
  func: F
): (...args: Parameters<F>) => ReturnType<F> {
  const name = func.name;
  return {
    [name](...args: Parameters<F>) {
      try {
        return func(...args);
      } catch (e) {
        if (
          e !== null &&
          typeof e === 'object' &&
          hasOwn(e, '__code_error__') &&
          typeof e.__code_error__ == 'number'
        ) {
          const message =
            hasOwn(e, '__error_message__') &&
            typeof e.__error_message__ === 'string'
              ? e.__error_message__
              : undefined;
          throw new SpacetimeHostError(e.__code_error__, message);
        }
        throw e;
      }
    },
  }[name];
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
