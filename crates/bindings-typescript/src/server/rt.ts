import { AlgebraicType, ProductType } from '../lib/algebraic_type';
import RawModuleDef from '../lib/autogen/raw_module_def_type';
import type RawModuleDefV9 from '../lib/autogen/raw_module_def_v_9_type';
import type RawTableDefV9 from '../lib/autogen/raw_table_def_v_9_type';
import type Typespace from '../lib/autogen/typespace_type';
import { ConnectionId } from '../lib/connection_id';
import { Identity } from '../lib/identity';
import { Timestamp } from '../lib/timestamp';
import { BinaryReader, BinaryWriter } from '../sdk';
import {
  MODULE_DEF,
  REDUCERS,
  type DbView,
  type ReducerCtx,
  type Table,
  type UntypedTableDef,
  type RowType,
  type TryInsertError,
  type Result,
  type TableMethods,
  type Index,
  type UniqueIndex,
  type RangedIndex,
  type IndexVal,
  type IndexScanRangeBounds,
  Range,
  type Bound,
} from './schema';
import type { InferTypeOfTypeBuilder, ColumnBuilder } from './type_builders';

type u8 = number;
type u16 = number;
type u32 = number;
type u64 = bigint;
type u128 = bigint;
type u256 = bigint;

declare global {
  function table_id_from_name(name: string): u32;
  function index_id_from_name(name: string): u32;
  function datastore_table_row_count(table_id: u32): u64;
  function datastore_table_scan_bsatn(table_id: u32): u32;
  function datastore_index_scan_range_bsatn(
    index_id: u32,
    prefix: Uint8Array,
    prefix_elems: u16,
    rstart: Uint8Array,
    rend: Uint8Array
  ): u32;
  function row_iter_bsatn_advance(
    iter: u32,
    buffer_max_len: u32
  ): [boolean, Uint8Array];
  function row_iter_bsatn_close(iter: u32): void;
  function datastore_insert_bsatn(table_id: u32, row: Uint8Array): Uint8Array;
  function datastore_update_bsatn(
    table_id: u32,
    index_id: u32,
    row: Uint8Array
  ): Uint8Array;
  function datastore_delete_by_index_scan_range_bsatn(
    index_id: u32,
    prefix: Uint8Array,
    prefix_elems: u16,
    rstart: Uint8Array,
    rend: Uint8Array
  ): u32;
  function datastore_delete_all_by_eq_bsatn(
    table_id: u32,
    relation: Uint8Array
  ): u32;
  function volatile_nonatomic_schedule_immediate(
    reducer_name: string,
    args: Uint8Array
  ): void;
  function console_log(level: u8, message: string): void;
  function console_timer_start(name: string): u32;
  function console_timer_end(span_id: u32): void;
  function identity(): { __identity__: u256 };

  function __call_reducer__(
    reducer_id: u32,
    sender: u256,
    conn_id: u128,
    timestamp: bigint,
    args: Uint8Array
  ): void;
  function __describe_module__(): RawModuleDef;
}

const { freeze } = Object;

const _syscalls = {
  table_id_from_name,
  index_id_from_name,
  datastore_table_row_count,
  datastore_table_scan_bsatn,
  datastore_index_scan_range_bsatn,
  row_iter_bsatn_advance,
  row_iter_bsatn_close,
  datastore_insert_bsatn,
  datastore_update_bsatn,
  datastore_delete_by_index_scan_range_bsatn,
  datastore_delete_all_by_eq_bsatn,
  volatile_nonatomic_schedule_immediate,
  console_log,
  console_timer_start,
  console_timer_end,
  identity,
};

const sys = freeze(
  Object.fromEntries(
    Object.entries(_syscalls).map(([name, syscall]) => {
      return [name, wrapSyscall(syscall)];
    })
  ) as typeof _syscalls
);

globalThis.__call_reducer__ = function __call_reducer__(
  reducer_id,
  sender,
  conn_id,
  timestamp,
  args_buf
) {
  const args_type = AlgebraicType.Product(
    MODULE_DEF.reducers[reducer_id].params
  );
  const args = AlgebraicType.deserializeValue(
    new BinaryReader(args_buf),
    args_type
  );
  const ctx: ReducerCtx<any> = freeze({
    sender: new Identity(sender),
    timestamp: new Timestamp(timestamp),
    connection_id: ConnectionId.nullIfZero(new ConnectionId(conn_id)),
    db: getDbView(),
  });
  REDUCERS[reducer_id](ctx, args);
};

globalThis.__describe_module__ = function __describe_module__() {
  return RawModuleDef.V9(MODULE_DEF);
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
        AlgebraicType.deserializeValue(reader, colType),
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

  const tryInsert: Table<any>['tryInsert'] = row => {
    const writer = new BinaryWriter(baseSize);
    AlgebraicType.serializeValue(writer, rowType, row);
    let ret_buf;
    try {
      ret_buf = sys.datastore_insert_bsatn(table_id, writer.getBuffer());
    } catch (e) {
      if (e instanceof UniqueAlreadyExists || e instanceof AutoIncOverflow)
        return { ok: false, err: e };
      throw e;
    }
    integrate_generated_columns?.(row, ret_buf);

    return { ok: true, val: row };
  };

  const tableMethods: TableMethods<any> = {
    count: () => sys.datastore_table_row_count(table_id),
    iter,
    [Symbol.iterator]: () => iter(),
    insert: (row: RowType<any>): RowType<any> => {
      const res = tryInsert(row);
      if (res.ok) return res.val;
      throw res.err;
    },
    tryInsert,
    delete: (row: RowType<any>): boolean => {
      const writer = new BinaryWriter(4 + baseSize);
      writer.writeU32(1);
      AlgebraicType.serializeValue(writer, rowType, row);
      const count = sys.datastore_delete_all_by_eq_bsatn(
        table_id,
        writer.getBuffer()
      );
      return count > 0;
    },
  };

  const tableView = tableMethods as Table<any>;

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
      .map(x => columnSet.isSubsetOf(new Set(x.data.value.columns)));

    const indexType = AlgebraicType.Product({
      elements: column_ids.map(id => rowType.value.elements[id]),
    });

    const baseSize = bsatnBaseSize(typespace, indexType);

    const serializePrefix = (
      writer: BinaryWriter,
      prefix: any[],
      prefix_elems: number
    ) => {
      if (prefix.length > numColumns - 1)
        throw new TypeError('too many elements in prefix');
      for (let i = 0; i < prefix_elems; i++) {
        const elemType = indexType.value.elements[i].algebraicType;
        AlgebraicType.serializeValue(writer, elemType, prefix[i]);
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
          col_val[numColumns - 1]
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
          AlgebraicType.serializeValue(writer, rowType, row);
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
              AlgebraicType.serializeValue(writer, termType, bound.value);
          };
          writeBound(term.from);
          const rendOffset = writer.offset;
          writeBound(term.to);
          rstart = writer.getBuffer().slice(rstartOffset, rendOffset);
          rend = writer.getBuffer().slice(rendOffset);
        } else {
          writer.writeU8(0);
          AlgebraicType.serializeValue(writer, termType, term);
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

    if (Object.hasOwn(tableView, indexDef.name!)) {
      freeze(Object.assign(tableView[indexDef.name!], index));
    } else {
      tableView[indexDef.name!] = freeze(index) as any;
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
    return 4 + assumedArrayLength * bsatnBaseSize(typespace, ty);
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
      if (this.#reader.remaining) {
        const value = AlgebraicType.deserializeValue(this.#reader, this.#ty);
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
        const [done, buf] = sys.row_iter_bsatn_advance(this.#id, buf_max_len);
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
          throw new SpacetimeError(e.__code_error__);
        }
        throw e;
      }
    },
  }[name];
}

export class SpacetimeError {
  public readonly code: u16;
  public readonly message: string;
  constructor(code: u16) {
    const proto = Object.getPrototypeOf(this);
    let cls;
    if (error_protoypes.has(proto)) {
      cls = proto.constructor;
      if (code !== cls.CODE)
        throw new TypeError(`invalid error code for ${cls.name}`);
    } else if (proto === SpacetimeError.prototype) {
      cls = errno_to_class.get(code);
      if (!cls) throw new RangeError(`unknown error code ${code}`);
    } else {
      throw new TypeError('cannot subclass SpacetimeError');
    }
    Object.setPrototypeOf(this, cls.prototype);
    this.code = cls.CODE;
    this.message = cls.MESSAGE;
  }
}

export class HostCallFailure extends SpacetimeError {
  static CODE = 1;
  static MESSAGE = 'ABI called by host returned an error';
  constructor() {
    super(HostCallFailure.CODE);
  }
}
export class NotInTransaction extends SpacetimeError {
  static CODE = 2;
  static MESSAGE = 'ABI call can only be made while in a transaction';
  constructor() {
    super(NotInTransaction.CODE);
  }
}
export class BsatnDecodeError extends SpacetimeError {
  static CODE = 3;
  static MESSAGE = "Couldn't decode the BSATN to the expected type";
  constructor() {
    super(BsatnDecodeError.CODE);
  }
}
export class NoSuchTable extends SpacetimeError {
  static CODE = 4;
  static MESSAGE = 'No such table';
  constructor() {
    super(NoSuchTable.CODE);
  }
}
export class NoSuchIndex extends SpacetimeError {
  static CODE = 5;
  static MESSAGE = 'No such index';
  constructor() {
    super(NoSuchIndex.CODE);
  }
}
export class NoSuchIter extends SpacetimeError {
  static CODE = 6;
  static MESSAGE = 'The provided row iterator is not valid';
  constructor() {
    super(NoSuchIter.CODE);
  }
}
export class NoSuchConsoleTimer extends SpacetimeError {
  static CODE = 7;
  static MESSAGE = 'The provided console timer does not exist';
  constructor() {
    super(NoSuchConsoleTimer.CODE);
  }
}
export class NoSuchBytes extends SpacetimeError {
  static CODE = 8;
  static MESSAGE = 'The provided bytes source or sink is not valid';
  constructor() {
    super(NoSuchBytes.CODE);
  }
}
export class NoSpace extends SpacetimeError {
  static CODE = 9;
  static MESSAGE = 'The provided sink has no more space left';
  constructor() {
    super(NoSpace.CODE);
  }
}
export class BufferTooSmall extends SpacetimeError {
  static CODE = 11;
  static MESSAGE = 'The provided buffer is not large enough to store the data';
  constructor() {
    super(BufferTooSmall.CODE);
  }
}
export class UniqueAlreadyExists extends SpacetimeError {
  static CODE = 12;
  static MESSAGE = 'Value with given unique identifier already exists';
  constructor() {
    super(UniqueAlreadyExists.CODE);
  }
}
export class ScheduleAtDelayTooLong extends SpacetimeError {
  static CODE = 13;
  static MESSAGE = 'Specified delay in scheduling row was too long';
  constructor() {
    super(ScheduleAtDelayTooLong.CODE);
  }
}
export class IndexNotUnique extends SpacetimeError {
  static CODE = 14;
  static MESSAGE = 'The index was not unique';
  constructor() {
    super(IndexNotUnique.CODE);
  }
}
export class NoSuchRow extends SpacetimeError {
  static CODE = 15;
  static MESSAGE = 'The row was not found, e.g., in an update call';
  constructor() {
    super(NoSuchRow.CODE);
  }
}
export class AutoIncOverflow extends SpacetimeError {
  static CODE = 16;
  static MESSAGE = 'The auto-increment sequence overflowed';
  constructor() {
    super(AutoIncOverflow.CODE);
  }
}

const error_subclasses = [
  HostCallFailure,
  NotInTransaction,
  BsatnDecodeError,
  NoSuchTable,
  NoSuchIndex,
  NoSuchIter,
  NoSuchConsoleTimer,
  NoSuchBytes,
  NoSpace,
  BufferTooSmall,
  UniqueAlreadyExists,
  ScheduleAtDelayTooLong,
  IndexNotUnique,
  NoSuchRow,
];

const error_protoypes = new Set(error_subclasses.map(cls => cls.prototype));

const errno_to_class = new Map(
  error_subclasses.map(cls => [cls.CODE as number, cls])
);
