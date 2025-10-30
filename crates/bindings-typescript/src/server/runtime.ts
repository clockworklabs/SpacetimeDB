import { AlgebraicType } from '../lib/algebraic_type';
import RawModuleDef from '../lib/autogen/raw_module_def_type';
import type RawModuleDefV9 from '../lib/autogen/raw_module_def_v_9_type';
import type RawTableDefV9 from '../lib/autogen/raw_table_def_v_9_type';
import type Typespace from '../lib/autogen/typespace_type';
import { ConnectionId } from '../lib/connection_id';
import { Identity } from '../lib/identity';
import { Timestamp } from '../lib/timestamp';
import BinaryReader from '../lib/binary_reader';
import BinaryWriter from '../lib/binary_writer';
import { SenderError, SpacetimeHostError } from './errors';
import { Range, type Bound } from './range';
import {
  type Index,
  type IndexVal,
  type UniqueIndex,
  type RangedIndex,
} from './indexes';
import { type RowType, type Table, type TableMethods } from './table';
import {
  type DbView,
  type ReducerCtx,
  REDUCERS,
  type JwtClaims,
  type AuthCtx,
  type JsonObject,
} from './reducers';
import { MODULE_DEF } from './schema';

import * as _syscalls from 'spacetime:sys@1.0';
import type { u16, u32, ModuleHooks } from 'spacetime:sys@1.0';

const { freeze } = Object;

const sys: typeof _syscalls = freeze(
  Object.fromEntries(
    Object.entries(_syscalls).map(([name, syscall]) => [
      name,
      wrapSyscall(syscall),
    ])
  ) as typeof _syscalls
);

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

export const hooks: ModuleHooks = {
  __describe_module__() {
    const writer = new BinaryWriter(128);
    RawModuleDef.serialize(writer, RawModuleDef.V9(MODULE_DEF));
    return writer.getBuffer();
  },
  __call_reducer__(reducerId, sender, connId, timestamp, argsBuf) {
    const argsType = AlgebraicType.Product(
      MODULE_DEF.reducers[reducerId].params
    );
    const args = AlgebraicType.deserializeValue(
      new BinaryReader(argsBuf),
      argsType,
      MODULE_DEF.typespace
    );
    const senderIdentity = new Identity(sender);
    const ctx: ReducerCtx<any> = freeze({
      sender: senderIdentity,
      get identity() {
        return new Identity(sys.identity().__identity__);
      },
      timestamp: new Timestamp(timestamp),
      connectionId: ConnectionId.nullIfZero(new ConnectionId(connId)),
      db: getDbView(),
      senderAuth: AuthCtxImpl.fromSystemTables(
        ConnectionId.nullIfZero(new ConnectionId(connId)),
        senderIdentity
      ),
    });
    try {
      return REDUCERS[reducerId](ctx, args) ?? { tag: 'ok' };
    } catch (e) {
      if (e instanceof SenderError) {
        return { tag: 'err', value: e.message };
      }
      throw e;
    }
  },
};

let DB_VIEW: DbView<any> | null = null;
function getDbView() {
  DB_VIEW ??= makeDbView(MODULE_DEF);
  return DB_VIEW;
}

function makeDbView(module_def: RawModuleDefV9): DbView<any> {
  return freeze(
    Object.fromEntries(
      module_def.tables.map(table => [
        table.name,
        makeTableView(module_def.typespace, table),
      ])
    )
  );
}

function makeTableView(typespace: Typespace, table: RawTableDefV9): Table<any> {
  const table_id = sys.table_id_from_name(table.name);
  const rowType = typespace.types[table.productTypeRef];
  if (rowType.tag !== 'Product') throw 'impossible';

  const baseSize = bsatnBaseSize(typespace, rowType);

  const sequences = table.sequences.map(seq => {
    const col = rowType.value.elements[seq.column];
    const colType = col.algebraicType;
    return {
      colName: col.name!,
      read: (reader: BinaryReader) =>
        AlgebraicType.deserializeValue(reader, colType, typespace),
    };
  });
  const hasAutoIncrement = sequences.length > 0;

  const iter = () =>
    new TableIterator(sys.datastore_table_scan_bsatn(table_id), rowType);

  const integrate_generated_columns = hasAutoIncrement
    ? (row: RowType<any>, ret_buf: Uint8Array) => {
        const reader = new BinaryReader(ret_buf);
        for (const { colName, read } of sequences) {
          row[colName] = read(reader);
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
      integrate_generated_columns?.(ret, ret_buf);

      return { ok: true, val: ret };
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
      const serializeBound = (col_val: any[]): IndexScanArgs => {
        if (col_val.length !== numColumns)
          throw new TypeError('wrong number of elements');

        const writer = new BinaryWriter(baseSize + 1);
        const prefix_elems = numColumns - 1;
        serializePrefix(writer, col_val, prefix_elems);
        const rstartOffset = writer.offset;
        writer.writeU8(0);
        AlgebraicType.serializeValue(
          writer,
          indexType.value.elements[numColumns - 1].algebraicType,
          col_val[numColumns - 1],
          typespace
        );
        const buffer = writer.getBuffer();
        const prefix = buffer.slice(0, rstartOffset);
        const rstart = buffer.slice(rstartOffset);
        return [prefix, prefix_elems, rstart, rstart];
      };
      index = {
        find: (col_val: IndexVal<any, any>): RowType<any> | null => {
          if (numColumns === 1) col_val = [col_val];
          const args = serializeBound(col_val);
          const iter = new TableIterator(
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
        delete: (col_val: IndexVal<any, any>): boolean => {
          if (numColumns === 1) col_val = [col_val];
          const args = serializeBound(col_val);
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
          integrate_generated_columns?.(row, ret_buf);
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
        filter: (range: any): IterableIterator<RowType<any>> => {
          if (numColumns === 1) range = [range];
          const args = serializeRange(range);
          return new TableIterator(
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

function bsatnBaseSize(typespace: Typespace, ty: AlgebraicType): number {
  const assumedArrayLength = 4;
  while (ty.tag === 'Ref') ty = typespace.types[ty.value];
  if (ty.tag === 'Product') {
    let sum = 0;
    for (const { algebraicType: elem } of ty.value.elements) {
      sum += bsatnBaseSize(typespace, elem);
    }
    return sum;
  } else if (ty.tag === 'Sum') {
    let min = Infinity;
    for (const { algebraicType: vari } of ty.value.variants) {
      const vSize = bsatnBaseSize(typespace, vari);
      if (vSize < min) min = vSize;
    }
    if (min === Infinity) min = 0;
    return 4 + min;
  } else if (ty.tag == 'Array') {
    return 4 + assumedArrayLength * bsatnBaseSize(typespace, ty.value);
  }
  return {
    String: 4 + assumedArrayLength,
    Sum: 1,
    Bool: 1,
    I8: 1,
    U8: 1,
    I16: 2,
    U16: 2,
    I32: 4,
    U32: 4,
    F32: 4,
    I64: 8,
    U64: 8,
    F64: 8,
    I128: 16,
    U128: 16,
    I256: 32,
    U256: 32,
  }[ty.tag];
}

function hasOwn<K extends PropertyKey>(
  o: object,
  k: K
): o is K extends PropertyKey ? { [k in K]: unknown } : never {
  return Object.hasOwn(o, k);
}

class TableIterator implements IterableIterator<any, undefined> {
  #id: u32 | -1;
  #reader: BinaryReader;
  #ty: AlgebraicType;
  constructor(id: u32, ty: AlgebraicType) {
    this.#id = id;
    this.#reader = new BinaryReader(new Uint8Array());
    this.#ty = ty;
  }
  [Symbol.iterator](): typeof this {
    return this;
  }
  next(): IteratorResult<any, undefined> {
    while (true) {
      if (this.#reader.remaining > 0) {
        const value = AlgebraicType.deserializeValue(
          this.#reader,
          this.#ty,
          MODULE_DEF.typespace
        );
        return { value };
      }
      if (this.#id === -1) {
        return { value: undefined, done: true };
      }
      this.#advance_iter();
    }
  }

  #advance_iter() {
    let buf_max_len = 0x10000;
    while (true) {
      try {
        const { 0: done, 1: buf } = sys.row_iter_bsatn_advance(
          this.#id,
          buf_max_len
        );
        if (done) this.#id = -1;
        this.#reader = new BinaryReader(buf);
        return;
      } catch (e) {
        if (e && typeof e === 'object' && hasOwn(e, '__buffer_too_small__')) {
          buf_max_len = e.__buffer_too_small__ as number;
          continue;
        }
        throw e;
      }
    }
  }

  [Symbol.dispose]() {
    if (this.#id >= 0) {
      this.#id = -1;
      sys.row_iter_bsatn_close(this.#id);
    }
  }
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
          throw new SpacetimeHostError(e.__code_error__);
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
