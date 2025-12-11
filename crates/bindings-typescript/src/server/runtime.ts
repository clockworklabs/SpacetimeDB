import * as _syscalls1_0 from 'spacetime:sys@1.0';
import * as _syscalls1_2 from 'spacetime:sys@1.2';

import type { ModuleHooks, u16, u32 } from 'spacetime:sys@1.0';
import { AlgebraicType, ProductType } from '../lib/algebraic_type';
import RawModuleDef from '../lib/autogen/raw_module_def_type';
import type RawModuleDefV9 from '../lib/autogen/raw_module_def_v_9_type';
import type RawTableDefV9 from '../lib/autogen/raw_table_def_v_9_type';
import type Typespace from '../lib/autogen/typespace_type';
import BinaryReader from '../lib/binary_reader';
import BinaryWriter from '../lib/binary_writer';
import { ConnectionId } from '../lib/connection_id';
import { Identity } from '../lib/identity';
import {
  type Index,
  type IndexVal,
  type RangedIndex,
  type UniqueIndex,
} from '../lib/indexes';
import { callProcedure as callProcedure } from './procedures';
import {
  type AuthCtx,
  type JsonObject,
  type JwtClaims,
  type ReducerCtx,
} from '../lib/reducers';
import { type UntypedSchemaDef } from '../lib/schema';
import { type RowType, type Table, type TableMethods } from '../lib/table';
import { Timestamp } from '../lib/timestamp';
import type { Infer } from '../lib/type_builders';
import { bsatnBaseSize, toCamelCase } from '../lib/util';
import {
  ANON_VIEWS,
  VIEWS,
  type AnonymousViewCtx,
  type ViewCtx,
} from './views';
import { isRowTypedQuery, makeQueryBuilder, toSql } from './query';
import type { DbView } from './db_view';
import { SenderError, SpacetimeHostError } from './errors';
import { Range, type Bound } from './range';
import ViewResultHeader from '../lib/autogen/view_result_header_type';
import { REDUCERS } from './reducers';
import { getRegisteredSchema, GLOBAL_MODULE_CTX } from './schema';

const { freeze } = Object;

export const sys = freeze(wrapSyscalls(_syscalls1_0, _syscalls1_2));

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

export const makeReducerCtx = (
  sender: Identity,
  timestamp: Timestamp,
  connectionId: ConnectionId | null
): ReducerCtx<UntypedSchemaDef> => ({
  sender,
  get identity() {
    return new Identity(sys.identity().__identity__);
  },
  timestamp,
  connectionId,
  db: getDbView(),
  senderAuth: AuthCtxImpl.fromSystemTables(connectionId, sender),
});

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

export const hooks: ModuleHooks = {
  __describe_module__() {
    const writer = new BinaryWriter(128);
    AlgebraicType.serializeValue(
      writer,
      RawModuleDef.algebraicType,
      RawModuleDef.V9(GLOBAL_MODULE_CTX.moduleDef)
    );
    return writer.getBuffer();
  },
  __call_reducer__(reducerId, sender, connId, timestamp, argsBuf) {
    const argsType = AlgebraicType.Product(
      GLOBAL_MODULE_CTX.moduleDef.reducers[reducerId].params
    );
    const args = AlgebraicType.deserializeValue(
      new BinaryReader(argsBuf),
      argsType,
      GLOBAL_MODULE_CTX.typespace
    );
    const senderIdentity = new Identity(sender);
    const ctx: ReducerCtx<any> = freeze(
      makeReducerCtx(
        senderIdentity,
        new Timestamp(timestamp),
        ConnectionId.nullIfZero(new ConnectionId(connId))
      )
    );
    try {
      return callUserFunction(REDUCERS[reducerId], ctx, args) ?? { tag: 'ok' };
    } catch (e) {
      if (e instanceof SenderError) {
        return { tag: 'err', value: e.message };
      }
      throw e;
    }
  },
};

export const hooks_v1_1: import('spacetime:sys@1.1').ModuleHooks = {
  __call_view__(id, sender, argsBuf) {
    const { fn, params, returnType, returnTypeBaseSize } = VIEWS[id];
    const ctx: ViewCtx<any> = freeze({
      sender: new Identity(sender),
      // this is the non-readonly DbView, but the typing for the user will be
      // the readonly one, and if they do call mutating functions it will fail
      // at runtime
      db: getDbView(),
      from: makeQueryBuilder(getRegisteredSchema()),
    });
    // ViewResultHeader.RawSql
    const args = ProductType.deserializeValue(
      new BinaryReader(argsBuf),
      params,
      GLOBAL_MODULE_CTX.typespace
    );
    const ret = callUserFunction(fn, ctx, args);
    const retBuf = new BinaryWriter(returnTypeBaseSize);
    if (isRowTypedQuery(ret)) {
      const query = toSql(ret);
      const v = ViewResultHeader.RawSql(query);
      AlgebraicType.serializeValue(
        retBuf,
        ViewResultHeader.algebraicType,
        v,
        GLOBAL_MODULE_CTX.typespace
      );
      return {
        data: retBuf.getBuffer(),
      };
    } else {
      AlgebraicType.serializeValue(
        retBuf,
        ViewResultHeader.algebraicType,
        ViewResultHeader.RowData,
        GLOBAL_MODULE_CTX.typespace
      );
      AlgebraicType.serializeValue(
        retBuf,
        returnType,
        ret,
        GLOBAL_MODULE_CTX.typespace
      );
      return {
        data: retBuf.getBuffer(),
      };
    }
  },
  __call_view_anon__(id, argsBuf) {
    const { fn, params, returnType, returnTypeBaseSize } = ANON_VIEWS[id];
    const ctx: AnonymousViewCtx<any> = freeze({
      // this is the non-readonly DbView, but the typing for the user will be
      // the readonly one, and if they do call mutating functions it will fail
      // at runtime
      db: getDbView(),
      from: makeQueryBuilder(getRegisteredSchema()),
    });
    const args = ProductType.deserializeValue(
      new BinaryReader(argsBuf),
      params,
      GLOBAL_MODULE_CTX.typespace
    );
    const ret = callUserFunction(fn, ctx, args);
    const retBuf = new BinaryWriter(returnTypeBaseSize);
    if (isRowTypedQuery(ret)) {
      const query = toSql(ret);
      const v = ViewResultHeader.RawSql(query);
      AlgebraicType.serializeValue(
        retBuf,
        ViewResultHeader.algebraicType,
        v,
        GLOBAL_MODULE_CTX.typespace
      );
      return {
        data: retBuf.getBuffer(),
      };
    } else {
      AlgebraicType.serializeValue(
        retBuf,
        ViewResultHeader.algebraicType,
        ViewResultHeader.RowData,
        GLOBAL_MODULE_CTX.typespace
      );
      AlgebraicType.serializeValue(
        retBuf,
        returnType,
        ret,
        GLOBAL_MODULE_CTX.typespace
      );
      return {
        data: retBuf.getBuffer(),
      };
    }
  },
};

export const hooks_v1_2: import('spacetime:sys@1.2').ModuleHooks = {
  __call_procedure__(id, sender, connection_id, timestamp, args) {
    return callProcedure(
      GLOBAL_MODULE_CTX.typespace,
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
  DB_VIEW ??= makeDbView(GLOBAL_MODULE_CTX.moduleDef);
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

function makeTableView(
  typespace: Infer<typeof Typespace>,
  table: Infer<typeof RawTableDefV9>
): Table<any> {
  const table_id = sys.table_id_from_name(table.name);
  const rowType = typespace.types[table.productTypeRef];
  if (rowType.tag !== 'Product') {
    throw 'impossible';
  }

  const baseSize = bsatnBaseSize(typespace, rowType);

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
      read: (reader: BinaryReader) =>
        AlgebraicType.deserializeValue(reader, colType, typespace),
    };
  });
  const hasAutoIncrement = sequences.length > 0;

  const iter = () =>
    tableIterator(sys.datastore_table_scan_bsatn(table_id), rowType);

  const integrateGeneratedColumns = hasAutoIncrement
    ? (row: RowType<any>, ret_buf: Uint8Array) => {
        const reader = new BinaryReader(ret_buf);
        for (const { colName, read, sequenceTrigger } of sequences) {
          if (row[colName] === sequenceTrigger) {
            row[colName] = read(reader);
          }
        }
      }
    : null;

  const tableMethods: TableMethods<any> = {
    count: () => sys.datastore_table_row_count(table_id),
    iter,
    [Symbol.iterator]: () => iter(),
    insert: row => {
      const writer = new BinaryWriter(baseSize);
      AlgebraicType.serializeValue(writer, rowType, row, typespace);
      const ret_buf = sys.datastore_insert_bsatn(table_id, writer.getBuffer());
      const ret = { ...row };
      integrateGeneratedColumns?.(ret, ret_buf);

      return ret;
    },
    delete: (row: RowType<any>): boolean => {
      const writer = new BinaryWriter(4 + baseSize);
      writer.writeU32(1);
      AlgebraicType.serializeValue(writer, rowType, row, typespace);
      const count = sys.datastore_delete_all_by_eq_bsatn(
        table_id,
        writer.getBuffer()
      );
      return count > 0;
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

    const indexType = AlgebraicType.Product({
      elements: column_ids.map(id => rowType.value.elements[id]),
    });

    const baseSize = bsatnBaseSize(typespace, indexType);

    const serializePrefix = (
      writer: BinaryWriter,
      prefix: any[],
      prefix_elems: number
    ) => {
      if (prefix_elems > numColumns - 1)
        throw new TypeError('too many elements in prefix');
      for (let i = 0; i < prefix_elems; i++) {
        const elemType = indexType.value.elements[i].algebraicType;
        AlgebraicType.serializeValue(writer, elemType, prefix[i], typespace);
      }
      return writer;
    };

    type IndexScanArgs = [
      prefix: Uint8Array,
      prefix_elems: u16,
      rstart: Uint8Array,
      rend: Uint8Array,
    ];

    let index: Index<any, any>;
    if (isUnique) {
      const serializeBound = (colVal: any[]): IndexScanArgs => {
        if (colVal.length !== numColumns)
          throw new TypeError('wrong number of elements');

        const writer = new BinaryWriter(baseSize + 1);
        const prefix_elems = numColumns - 1;
        serializePrefix(writer, colVal, prefix_elems);
        const rstartOffset = writer.offset;
        writer.writeU8(0);
        AlgebraicType.serializeValue(
          writer,
          indexType.value.elements[numColumns - 1].algebraicType,
          colVal[numColumns - 1],
          typespace
        );
        const buffer = writer.getBuffer();
        const prefix = buffer.slice(0, rstartOffset);
        const rstart = buffer.slice(rstartOffset);
        return [prefix, prefix_elems, rstart, rstart];
      };
      index = {
        find: (colVal: IndexVal<any, any>): RowType<any> | null => {
          if (numColumns === 1) colVal = [colVal];
          const args = serializeBound(colVal);
          const iter = tableIterator(
            sys.datastore_index_scan_range_bsatn(index_id, ...args),
            rowType
          );
          const { value, done } = iter.next();
          if (done) return null;
          if (!iter.next().done)
            throw new Error(
              '`datastore_index_scan_range_bsatn` on unique field cannot return >1 rows'
            );
          return value;
        },
        delete: (colVal: IndexVal<any, any>): boolean => {
          if (numColumns === 1) colVal = [colVal];
          const args = serializeBound(colVal);
          const num = sys.datastore_delete_by_index_scan_range_bsatn(
            index_id,
            ...args
          );
          return num > 0;
        },
        update: (row: RowType<any>): RowType<any> => {
          const writer = new BinaryWriter(baseSize);
          AlgebraicType.serializeValue(writer, rowType, row, typespace);
          const ret_buf = sys.datastore_update_bsatn(
            table_id,
            index_id,
            writer.getBuffer()
          );
          integrateGeneratedColumns?.(row, ret_buf);
          return row;
        },
      } as UniqueIndex<any, any>;
    } else {
      const serializeRange = (range: any[]): IndexScanArgs => {
        if (range.length > numColumns) throw new TypeError('too many elements');

        const writer = new BinaryWriter(baseSize + 1);
        const prefix_elems = range.length - 1;
        serializePrefix(writer, range, prefix_elems);
        const rstartOffset = writer.offset;
        const term = range[range.length - 1];
        const termType =
          indexType.value.elements[range.length - 1].algebraicType;
        let rstart: Uint8Array, rend: Uint8Array;
        if (term instanceof Range) {
          const writeBound = (bound: Bound<any>) => {
            const tags = { included: 0, excluded: 1, unbounded: 2 };
            writer.writeU8(tags[bound.tag]);
            if (bound.tag !== 'unbounded')
              AlgebraicType.serializeValue(
                writer,
                termType,
                bound.value,
                typespace
              );
          };
          writeBound(term.from);
          const rendOffset = writer.offset;
          writeBound(term.to);
          rstart = writer.getBuffer().slice(rstartOffset, rendOffset);
          rend = writer.getBuffer().slice(rendOffset);
        } else {
          writer.writeU8(0);
          AlgebraicType.serializeValue(writer, termType, term, typespace);
          rstart = rend = writer.getBuffer().slice(rstartOffset);
        }
        const buffer = writer.getBuffer();
        const prefix = buffer.slice(0, rstartOffset);
        return [prefix, prefix_elems, rstart, rend];
      };
      index = {
        filter: (range: any): IteratorObject<RowType<any>> => {
          if (numColumns === 1) range = [range];
          const args = serializeRange(range);
          return tableIterator(
            sys.datastore_index_scan_range_bsatn(index_id, ...args),
            rowType
          );
        },
        delete: (range: any): u32 => {
          if (numColumns === 1) range = [range];
          const args = serializeRange(range);
          return sys.datastore_delete_by_index_scan_range_bsatn(
            index_id,
            ...args
          );
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

function hasOwn<K extends PropertyKey>(
  o: object,
  k: K
): o is K extends PropertyKey ? { [k in K]: unknown } : never {
  return Object.hasOwn(o, k);
}

function* tableIterator(id: u32, ty: AlgebraicType): Generator<any, undefined> {
  using iter = new IteratorHandle(id);
  const { typespace } = GLOBAL_MODULE_CTX.moduleDef;

  let buf;
  while ((buf = advanceIter(iter)) != null) {
    const reader = new BinaryReader(buf);
    while (reader.remaining > 0) {
      yield AlgebraicType.deserializeValue(reader, ty, typespace);
    }
  }
}

function advanceIter(iter: IteratorHandle): Uint8Array | null {
  let buf_max_len = 0x10000;
  while (true) {
    try {
      return iter.advance(buf_max_len);
    } catch (e) {
      if (e && typeof e === 'object' && hasOwn(e, '__buffer_too_small__')) {
        buf_max_len = e.__buffer_too_small__ as number;
        continue;
      }
      throw e;
    }
  }
}

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

  /** Call `row_iter_bsatn_advance`, returning null if this iterator was already exhausted. */
  advance(buf_max_len: u32): Uint8Array | null {
    if (this.#id === -1) return null;
    const { 0: done, 1: buf } = sys.row_iter_bsatn_advance(
      this.#id,
      buf_max_len
    );
    if (done) this.#detach();
    return buf;
  }

  [Symbol.dispose]() {
    if (this.#id >= 0) {
      const id = this.#detach();
      sys.row_iter_bsatn_close(id);
    }
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
