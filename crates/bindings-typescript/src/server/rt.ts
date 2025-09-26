import { AlgebraicType } from '../lib/algebraic_type';
import type RawModuleDefV9 from '../lib/autogen/raw_module_def_v_9_type';
import type RawTableDefV9 from '../lib/autogen/raw_table_def_v_9_type';
import type Typespace from '../lib/autogen/typespace_type';
import { BinaryReader, BinaryWriter } from '../sdk';
import { MODULE_DEF, REDUCERS, type DbView, type Table } from './schema';

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
  function datastore_delete_all_by_eq_bsatn(table_id: u32, relation: u8[]): u32;
  function volatile_nonatomic_schedule_immediate(
    reducer_name: string,
    args: u8[]
  ): void;
  function console_log(level: u8, message: string): void;
  function console_timer_start(name: string): u32;
  function console_timer_end(span_id: u32): void;
  function identity(): { __identity__: u256 };

  // function A
  function __call_reducer__(
    reducer_id: u32,
    sender: u256,
    conn_id: u128,
    timestamp: bigint,
    args: Uint8Array
  ): void;
}

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
  REDUCERS[reducer_id]({ db: getDbView() }, args);
};

let DB_VIEW: DbView<any> | null = null;
function getDbView() {
  DB_VIEW ??= makeDbView(MODULE_DEF);
  return DB_VIEW;
}

function makeDbView(module_def: RawModuleDefV9): DbView<any> {
  const dbView: DbView<any> = {};
  for (const table of module_def.tables) {
    dbView[table.name] = makeTableView(module_def.typespace, table);
  }
  return dbView;
}

function makeTableView(typespace: Typespace, table: RawTableDefV9): Table<any> {
  const table_id = table_id_from_name(table.name);
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
    new TableIterator(datastore_table_scan_bsatn(table_id), rowType);

  const try_insert: Table<any>['try_insert'] = row => {
    const writer = new BinaryWriter(baseSize);
    AlgebraicType.serializeValue(writer, rowType, row);
    const ret = datastore_insert_bsatn(table_id, writer.getBuffer());
    if (hasAutoIncrement) {
      const reader = new BinaryReader(ret);
      for (const { colName, read } of sequences) {
        row[colName] = read(reader);
      }
    }

    return { ok: true, val: row };
  };

  return {
    count: () => datastore_table_row_count(table_id),
    iter,
    [Symbol.iterator]: iter,
    insert: row => {
      const res = try_insert(row);
      if (res.ok) return res.val;
      throw res.err;
    },
    try_insert,
    delete: row => {
      const writer = new BinaryWriter(baseSize);
      AlgebraicType.serializeValue(writer, rowType, row);
      return false;
    },
  };
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
        const [done, buf] = row_iter_bsatn_advance(this.#id, buf_max_len);
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
      row_iter_bsatn_close(this.#id);
    }
  }
}
