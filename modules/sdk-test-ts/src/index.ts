// ─────────────────────────────────────────────────────────────────────────────
// IMPORTS
// ─────────────────────────────────────────────────────────────────────────────
import { toCamelCase, Uuid } from 'spacetimedb';
import {
  type ModuleExport,
  type RowObj,
  schema,
  t,
  table,
} from 'spacetimedb/server';

const SimpleEnum = t.enum('SimpleEnum', ['Zero', 'One', 'Two']);

const EnumWithPayload = t.enum('EnumWithPayload', {
  U8: t.u8(),
  U16: t.u16(),
  U32: t.u32(),
  U64: t.u64(),
  U128: t.u128(),
  U256: t.u256(),
  I8: t.i8(),
  I16: t.i16(),
  I32: t.i32(),
  I64: t.i64(),
  I128: t.i128(),
  I256: t.i256(),
  Bool: t.bool(),
  F32: t.f32(),
  F64: t.f64(),
  Str: t.string(),
  Identity: t.identity(),
  ConnectionId: t.connectionId(),
  Timestamp: t.timestamp(),
  Uuid: t.uuid(),
  Bytes: t.array(t.u8()),
  Ints: t.array(t.i32()),
  Strings: t.array(t.string()),
  SimpleEnums: t.array(SimpleEnum),
  // SpacetimeDB doesn't yet support recursive types in modules
  // Recursive(Vec<EnumWithPayload>),
});

const UnitStruct = t.object('UnitStruct', {});

const ByteStruct = t.object('ByteStruct', {
  b: t.u8(),
});

const EveryPrimitiveStruct = t.object('EveryPrimitiveStruct', {
  a: t.u8(),
  b: t.u16(),
  c: t.u32(),
  d: t.u64(),
  e: t.u128(),
  f: t.u256(),
  g: t.i8(),
  h: t.i16(),
  i: t.i32(),
  j: t.i64(),
  k: t.i128(),
  l: t.i256(),
  m: t.bool(),
  n: t.f32(),
  o: t.f64(),
  p: t.string(),
  q: t.identity(),
  r: t.connectionId(),
  s: t.timestamp(),
  t: t.timeDuration(),
  u: t.uuid(),
});

const EveryVecStruct = t.object('EveryVecStruct', {
  a: t.array(t.u8()),
  b: t.array(t.u16()),
  c: t.array(t.u32()),
  d: t.array(t.u64()),
  e: t.array(t.u128()),
  f: t.array(t.u256()),
  g: t.array(t.i8()),
  h: t.array(t.i16()),
  i: t.array(t.i32()),
  j: t.array(t.i64()),
  k: t.array(t.i128()),
  l: t.array(t.i256()),
  m: t.array(t.bool()),
  n: t.array(t.f32()),
  o: t.array(t.f64()),
  p: t.array(t.string()),
  q: t.array(t.identity()),
  r: t.array(t.connectionId()),
  s: t.array(t.timestamp()),
  t: t.array(t.timeDuration()),
  u: t.array(t.uuid()),
});

type TableSchema = ReturnType<typeof table<any, any>>;

type TableWithReducers<Name extends string, Table extends TableSchema> = {
  table: Table;
  reducers(
    spacetimedb: ReturnType<typeof schema<{ [k in Name]: Table }>>
  ): ExportsObj;
};

type ExportsObj = Record<string, ModuleExport>;

/** Somewhat mimics the `define_tables!` macro in sdk-test/src/lib.rs */
function tbl<const Name extends string, Row extends RowObj>(
  name: Name,
  ops: {
    insert?: string;
    delete?: string;
    insert_or_panic?: string;
    update_by?: [string, keyof Row];
    update_non_pk_by?: [string, keyof Row];
    delete_by?: [string, keyof Row];
  },
  row: Row
): TableWithReducers<Name, ReturnType<typeof table<Row, { name: Name }>>> {
  const t = table({ public: true }, row);
  return {
    table: t,
    reducers(spacetimedb) {
      const exports: ExportsObj = {};
      if (ops.insert) {
        exports[ops.insert] = spacetimedb.reducer(row, (ctx, args) => {
          (ctx.db[toCamelCase(name)] as any).insert({ ...args });
        });
      }
      if (ops.delete) {
        exports[ops.delete] = spacetimedb.reducer(row, (ctx, args) => {
          (ctx.db[toCamelCase(name)] as any).delete({ ...args });
        });
      }
      if (ops.insert_or_panic) {
        exports[ops.insert_or_panic] = spacetimedb.reducer(row, (ctx, args) => {
          (ctx.db[toCamelCase(name)] as any).insert({ ...args });
        });
      }
      if (ops.update_by) {
        const [reducer, col] = ops.update_by;
        exports[reducer] = spacetimedb.reducer(row, (ctx, args) => {
          (ctx.db[toCamelCase(name)] as any)[col].update({ ...args });
        });
      }
      if (ops.update_non_pk_by) {
        const [reducer, col] = ops.update_non_pk_by;
        exports[reducer] = spacetimedb.reducer(row, (ctx, args) => {
          (ctx.db[toCamelCase(name)] as any)[col].delete(args[col as any]);
          (ctx.db[toCamelCase(name)] as any).insert({ ...args });
        });
      }
      if (ops.delete_by) {
        const [reducer, col] = ops.delete_by;
        exports[reducer] = spacetimedb.reducer(
          { [col]: row[col] },
          (ctx, args) => {
            (ctx.db[toCamelCase(name)] as any)[col].delete(args[col as any]);
          }
        );
      }
      return exports;
    },
  };
}

// Tables holding a single value.
const singleValTables = {
  one_u8: tbl('one_u8', { insert: 'insert_one_u8' }, { n: t.u8() }),
  one_u16: tbl('one_u16', { insert: 'insert_one_u16' }, { n: t.u16() }),
  one_u32: tbl('one_u32', { insert: 'insert_one_u32' }, { n: t.u32() }),
  one_u64: tbl('one_u64', { insert: 'insert_one_u64' }, { n: t.u64() }),
  one_u128: tbl('one_u128', { insert: 'insert_one_u128' }, { n: t.u128() }),
  one_u256: tbl('one_u256', { insert: 'insert_one_u256' }, { n: t.u256() }),

  one_i8: tbl('one_i8', { insert: 'insert_one_i8' }, { n: t.i8() }),
  one_i16: tbl('one_i16', { insert: 'insert_one_i16' }, { n: t.i16() }),
  one_i32: tbl('one_i32', { insert: 'insert_one_i32' }, { n: t.i32() }),
  one_i64: tbl('one_i64', { insert: 'insert_one_i64' }, { n: t.i64() }),
  one_i128: tbl('one_i128', { insert: 'insert_one_i128' }, { n: t.i128() }),
  one_i256: tbl('one_i256', { insert: 'insert_one_i256' }, { n: t.i256() }),

  one_bool: tbl('one_bool', { insert: 'insert_one_bool' }, { b: t.bool() }),

  one_f32: tbl('one_f32', { insert: 'insert_one_f32' }, { f: t.f32() }),
  one_f64: tbl('one_f64', { insert: 'insert_one_f64' }, { f: t.f64() }),

  one_string: tbl(
    'one_string',
    { insert: 'insert_one_string' },
    { s: t.string() }
  ),

  one_identity: tbl(
    'one_identity',
    { insert: 'insert_one_identity' },
    { i: t.identity() }
  ),
  one_connection_id: tbl(
    'one_connection_id',
    { insert: 'insert_one_connection_id' },
    { a: t.connectionId() }
  ),

  one_uuid: tbl('one_uuid', { insert: 'insert_one_uuid' }, { u: t.uuid() }),

  one_timestamp: tbl(
    'one_timestamp',
    { insert: 'insert_one_timestamp' },
    { t: t.timestamp() }
  ),

  one_simple_enum: tbl(
    'one_simple_enum',
    { insert: 'insert_one_simple_enum' },
    { e: SimpleEnum }
  ),
  one_enum_with_payload: tbl(
    'one_enum_with_payload',
    { insert: 'insert_one_enum_with_payload' },
    { e: EnumWithPayload }
  ),

  one_unit_struct: tbl(
    'one_unit_struct',
    { insert: 'insert_one_unit_struct' },
    { s: UnitStruct }
  ),
  one_byte_struct: tbl(
    'one_byte_struct',
    { insert: 'insert_one_byte_struct' },
    { s: ByteStruct }
  ),
  one_every_primitive_struct: tbl(
    'one_every_primitive_struct',
    { insert: 'insert_one_every_primitive_struct' },
    { s: EveryPrimitiveStruct }
  ),
  one_every_vec_struct: tbl(
    'one_every_vec_struct',
    { insert: 'insert_one_every_vec_struct' },
    { s: EveryVecStruct }
  ),
} as const;

// Tables holding a Vec of various types.
const vecTables = {
  vec_u8: tbl('vec_u8', { insert: 'insert_vec_u8' }, { n: t.array(t.u8()) }),
  vec_u16: tbl(
    'vec_u16',
    { insert: 'insert_vec_u16' },
    { n: t.array(t.u16()) }
  ),
  vec_u32: tbl(
    'vec_u32',
    { insert: 'insert_vec_u32' },
    { n: t.array(t.u32()) }
  ),
  vec_u64: tbl(
    'vec_u64',
    { insert: 'insert_vec_u64' },
    { n: t.array(t.u64()) }
  ),
  vec_u128: tbl(
    'vec_u128',
    { insert: 'insert_vec_u128' },
    { n: t.array(t.u128()) }
  ),
  vec_u256: tbl(
    'vec_u256',
    { insert: 'insert_vec_u256' },
    { n: t.array(t.u256()) }
  ),

  vec_i8: tbl('vec_i8', { insert: 'insert_vec_i8' }, { n: t.array(t.i8()) }),
  vec_i16: tbl(
    'vec_i16',
    { insert: 'insert_vec_i16' },
    { n: t.array(t.i16()) }
  ),
  vec_i32: tbl(
    'vec_i32',
    { insert: 'insert_vec_i32' },
    { n: t.array(t.i32()) }
  ),
  vec_i64: tbl(
    'vec_i64',
    { insert: 'insert_vec_i64' },
    { n: t.array(t.i64()) }
  ),
  vec_i128: tbl(
    'vec_i128',
    { insert: 'insert_vec_i128' },
    { n: t.array(t.i128()) }
  ),
  vec_i256: tbl(
    'vec_i256',
    { insert: 'insert_vec_i256' },
    { n: t.array(t.i256()) }
  ),

  vec_bool: tbl(
    'vec_bool',
    { insert: 'insert_vec_bool' },
    { b: t.array(t.bool()) }
  ),

  vec_f32: tbl(
    'vec_f32',
    { insert: 'insert_vec_f32' },
    { f: t.array(t.f32()) }
  ),
  vec_f64: tbl(
    'vec_f64',
    { insert: 'insert_vec_f64' },
    { f: t.array(t.f64()) }
  ),

  vec_string: tbl(
    'vec_string',
    { insert: 'insert_vec_string' },
    { s: t.array(t.string()) }
  ),

  vec_identity: tbl(
    'vec_identity',
    { insert: 'insert_vec_identity' },
    { i: t.array(t.identity()) }
  ),
  vec_connection_id: tbl(
    'vec_connection_id',
    { insert: 'insert_vec_connection_id' },
    { a: t.array(t.connectionId()) }
  ),

  vec_timestamp: tbl(
    'vec_timestamp',
    { insert: 'insert_vec_timestamp' },
    { t: t.array(t.timestamp()) }
  ),

  vec_uuid: tbl(
    'vec_uuid',
    { insert: 'insert_vec_uuid' },
    { u: t.array(t.uuid()) }
  ),

  vec_simple_enum: tbl(
    'vec_simple_enum',
    { insert: 'insert_vec_simple_enum' },
    { e: t.array(SimpleEnum) }
  ),
  vec_enum_with_payload: tbl(
    'vec_enum_with_payload',
    { insert: 'insert_vec_enum_with_payload' },
    { e: t.array(EnumWithPayload) }
  ),

  vec_unit_struct: tbl(
    'vec_unit_struct',
    { insert: 'insert_vec_unit_struct' },
    { s: t.array(UnitStruct) }
  ),
  vec_byte_struct: tbl(
    'vec_byte_struct',
    { insert: 'insert_vec_byte_struct' },
    { s: t.array(ByteStruct) }
  ),
  vec_every_primitive_struct: tbl(
    'vec_every_primitive_struct',
    { insert: 'insert_vec_every_primitive_struct' },
    { s: t.array(EveryPrimitiveStruct) }
  ),
  vec_every_vec_struct: tbl(
    'vec_every_vec_struct',
    { insert: 'insert_vec_every_vec_struct' },
    { s: t.array(EveryVecStruct) }
  ),
} as const;

// Tables holding an Option of various types.
const optionTables = {
  option_i32: tbl(
    'option_i32',
    { insert: 'insert_option_i32' },
    { n: t.option(t.i32()) }
  ),
  option_string: tbl(
    'option_string',
    { insert: 'insert_option_string' },
    { s: t.option(t.string()) }
  ),
  option_identity: tbl(
    'option_identity',
    { insert: 'insert_option_identity' },
    { i: t.option(t.identity()) }
  ),
  option_uuid: tbl(
    'option_uuid',
    { insert: 'insert_option_uuid' },
    { u: t.option(t.uuid()) }
  ),
  option_simple_enum: tbl(
    'option_simple_enum',
    { insert: 'insert_option_simple_enum' },
    { e: t.option(SimpleEnum) }
  ),
  option_every_primitive_struct: tbl(
    'option_every_primitive_struct',
    { insert: 'insert_option_every_primitive_struct' },
    { s: t.option(EveryPrimitiveStruct) }
  ),
  option_vec_option_i32: tbl(
    'option_vec_option_i32',
    { insert: 'insert_option_vec_option_i32' },
    { v: t.option(t.array(t.option(t.i32()))) }
  ),
} as const;

// Tables for Result<Ok, Err> values.
const resultTables = {
  result_i32_string: tbl(
    'result_i32_string',
    { insert: 'insert_result_i32_string' },
    { r: t.result(t.i32(), t.string()) }
  ),
  result_string_i32: tbl(
    'result_string_i32',
    { insert: 'insert_result_string_i32' },
    { r: t.result(t.string(), t.i32()) }
  ),
  result_identity_string: tbl(
    'result_identity_string',
    { insert: 'insert_result_identity_string' },
    { r: t.result(t.identity(), t.string()) }
  ),
  result_simple_enum_i32: tbl(
    'result_simple_enum_i32',
    { insert: 'insert_result_simple_enum_i32' },
    { r: t.result(SimpleEnum, t.i32()) }
  ),
  result_every_primitive_struct_string: tbl(
    'result_every_primitive_struct_string',
    { insert: 'insert_result_every_primitive_struct_string' },
    { r: t.result(EveryPrimitiveStruct, t.string()) }
  ),
  result_vec_i32_string: tbl(
    'result_vec_i32_string',
    { insert: 'insert_result_vec_i32_string' },
    { r: t.result(t.array(t.i32()), t.string()) }
  ),
} as const;

// Tables mapping a unique, but non-pk, key to a boring i32 payload.
// This allows us to test delete events, and the semantically correct absence of update events.
const uniqueTables = {
  unique_u8: tbl(
    'unique_u8',
    {
      insert_or_panic: 'insert_unique_u8',
      update_non_pk_by: ['update_unique_u8', 'n'],
      delete_by: ['delete_unique_u8', 'n'],
    },
    { n: t.u8().unique(), data: t.i32() }
  ),

  unique_u16: tbl(
    'unique_u16',
    {
      insert_or_panic: 'insert_unique_u16',
      update_non_pk_by: ['update_unique_u16', 'n'],
      delete_by: ['delete_unique_u16', 'n'],
    },
    { n: t.u16().unique(), data: t.i32() }
  ),

  unique_u32: tbl(
    'unique_u32',
    {
      insert_or_panic: 'insert_unique_u32',
      update_non_pk_by: ['update_unique_u32', 'n'],
      delete_by: ['delete_unique_u32', 'n'],
    },
    { n: t.u32().unique(), data: t.i32() }
  ),

  unique_u64: tbl(
    'unique_u64',
    {
      insert_or_panic: 'insert_unique_u64',
      update_non_pk_by: ['update_unique_u64', 'n'],
      delete_by: ['delete_unique_u64', 'n'],
    },
    { n: t.u64().unique(), data: t.i32() }
  ),

  unique_u128: tbl(
    'unique_u128',
    {
      insert_or_panic: 'insert_unique_u128',
      update_non_pk_by: ['update_unique_u128', 'n'],
      delete_by: ['delete_unique_u128', 'n'],
    },
    { n: t.u128().unique(), data: t.i32() }
  ),

  unique_u256: tbl(
    'unique_u256',
    {
      insert_or_panic: 'insert_unique_u256',
      update_non_pk_by: ['update_unique_u256', 'n'],
      delete_by: ['delete_unique_u256', 'n'],
    },
    { n: t.u256().unique(), data: t.i32() }
  ),

  unique_i8: tbl(
    'unique_i8',
    {
      insert_or_panic: 'insert_unique_i8',
      update_non_pk_by: ['update_unique_i8', 'n'],
      delete_by: ['delete_unique_i8', 'n'],
    },
    { n: t.i8().unique(), data: t.i32() }
  ),

  unique_i16: tbl(
    'unique_i16',
    {
      insert_or_panic: 'insert_unique_i16',
      update_non_pk_by: ['update_unique_i16', 'n'],
      delete_by: ['delete_unique_i16', 'n'],
    },
    { n: t.i16().unique(), data: t.i32() }
  ),

  unique_i32: tbl(
    'unique_i32',
    {
      insert_or_panic: 'insert_unique_i32',
      update_non_pk_by: ['update_unique_i32', 'n'],
      delete_by: ['delete_unique_i32', 'n'],
    },
    { n: t.i32().unique(), data: t.i32() }
  ),

  unique_i64: tbl(
    'unique_i64',
    {
      insert_or_panic: 'insert_unique_i64',
      update_non_pk_by: ['update_unique_i64', 'n'],
      delete_by: ['delete_unique_i64', 'n'],
    },
    { n: t.i64().unique(), data: t.i32() }
  ),

  unique_i128: tbl(
    'unique_i128',
    {
      insert_or_panic: 'insert_unique_i128',
      update_non_pk_by: ['update_unique_i128', 'n'],
      delete_by: ['delete_unique_i128', 'n'],
    },
    { n: t.i128().unique(), data: t.i32() }
  ),

  unique_i256: tbl(
    'unique_i256',
    {
      insert_or_panic: 'insert_unique_i256',
      update_non_pk_by: ['update_unique_i256', 'n'],
      delete_by: ['delete_unique_i256', 'n'],
    },
    { n: t.i256().unique(), data: t.i32() }
  ),

  unique_bool: tbl(
    'unique_bool',
    {
      insert_or_panic: 'insert_unique_bool',
      update_non_pk_by: ['update_unique_bool', 'b'],
      delete_by: ['delete_unique_bool', 'b'],
    },
    { b: t.bool().unique(), data: t.i32() }
  ),

  unique_string: tbl(
    'unique_string',
    {
      insert_or_panic: 'insert_unique_string',
      update_non_pk_by: ['update_unique_string', 's'],
      delete_by: ['delete_unique_string', 's'],
    },
    { s: t.string().unique(), data: t.i32() }
  ),

  unique_identity: tbl(
    'unique_identity',
    {
      insert_or_panic: 'insert_unique_identity',
      update_non_pk_by: ['update_unique_identity', 'i'],
      delete_by: ['delete_unique_identity', 'i'],
    },
    { i: t.identity().unique(), data: t.i32() }
  ),

  unique_connection_id: tbl(
    'unique_connection_id',
    {
      insert_or_panic: 'insert_unique_connection_id',
      update_non_pk_by: ['update_unique_connection_id', 'a'],
      delete_by: ['delete_unique_connection_id', 'a'],
    },
    { a: t.connectionId().unique(), data: t.i32() }
  ),

  unique_uuid: tbl(
    'unique_uuid',
    {
      insert_or_panic: 'insert_unique_uuid',
      update_non_pk_by: ['update_unique_uuid', 'u'],
      delete_by: ['delete_unique_uuid', 'u'],
    },
    { u: t.uuid().unique(), data: t.i32() }
  ),
} as const;

// Tables mapping a primary key to a boring i32 payload.
// This allows us to test update and delete events.
const pkTables = {
  pk_u8: tbl(
    'pk_u8',
    {
      insert_or_panic: 'insert_pk_u8',
      update_by: ['update_pk_u8', 'n'],
      delete_by: ['delete_pk_u8', 'n'],
    },
    { n: t.u8().primaryKey(), data: t.i32() }
  ),

  pk_u16: tbl(
    'pk_u16',
    {
      insert_or_panic: 'insert_pk_u16',
      update_by: ['update_pk_u16', 'n'],
      delete_by: ['delete_pk_u16', 'n'],
    },
    { n: t.u16().primaryKey(), data: t.i32() }
  ),

  pk_u32: tbl(
    'pk_u32',
    {
      insert_or_panic: 'insert_pk_u32',
      update_by: ['update_pk_u32', 'n'],
      delete_by: ['delete_pk_u32', 'n'],
    },
    { n: t.u32().primaryKey(), data: t.i32() }
  ),

  pk_u32_two: tbl(
    'pk_u32_two',
    {
      insert_or_panic: 'insert_pk_u32_two',
      update_by: ['update_pk_u32_two', 'n'],
      delete_by: ['delete_pk_u32_two', 'n'],
    },
    { n: t.u32().primaryKey(), data: t.i32() }
  ),

  pk_u64: tbl(
    'pk_u64',
    {
      insert_or_panic: 'insert_pk_u64',
      update_by: ['update_pk_u64', 'n'],
      delete_by: ['delete_pk_u64', 'n'],
    },
    { n: t.u64().primaryKey(), data: t.i32() }
  ),

  pk_u128: tbl(
    'pk_u128',
    {
      insert_or_panic: 'insert_pk_u128',
      update_by: ['update_pk_u128', 'n'],
      delete_by: ['delete_pk_u128', 'n'],
    },
    { n: t.u128().primaryKey(), data: t.i32() }
  ),

  pk_u256: tbl(
    'pk_u256',
    {
      insert_or_panic: 'insert_pk_u256',
      update_by: ['update_pk_u256', 'n'],
      delete_by: ['delete_pk_u256', 'n'],
    },
    { n: t.u256().primaryKey(), data: t.i32() }
  ),

  pk_i8: tbl(
    'pk_i8',
    {
      insert_or_panic: 'insert_pk_i8',
      update_by: ['update_pk_i8', 'n'],
      delete_by: ['delete_pk_i8', 'n'],
    },
    { n: t.i8().primaryKey(), data: t.i32() }
  ),

  pk_i16: tbl(
    'pk_i16',
    {
      insert_or_panic: 'insert_pk_i16',
      update_by: ['update_pk_i16', 'n'],
      delete_by: ['delete_pk_i16', 'n'],
    },
    { n: t.i16().primaryKey(), data: t.i32() }
  ),

  pk_i32: tbl(
    'pk_i32',
    {
      insert_or_panic: 'insert_pk_i32',
      update_by: ['update_pk_i32', 'n'],
      delete_by: ['delete_pk_i32', 'n'],
    },
    { n: t.i32().primaryKey(), data: t.i32() }
  ),

  pk_i64: tbl(
    'pk_i64',
    {
      insert_or_panic: 'insert_pk_i64',
      update_by: ['update_pk_i64', 'n'],
      delete_by: ['delete_pk_i64', 'n'],
    },
    { n: t.i64().primaryKey(), data: t.i32() }
  ),

  pk_i128: tbl(
    'pk_i128',
    {
      insert_or_panic: 'insert_pk_i128',
      update_by: ['update_pk_i128', 'n'],
      delete_by: ['delete_pk_i128', 'n'],
    },
    { n: t.i128().primaryKey(), data: t.i32() }
  ),

  pk_i256: tbl(
    'pk_i256',
    {
      insert_or_panic: 'insert_pk_i256',
      update_by: ['update_pk_i256', 'n'],
      delete_by: ['delete_pk_i256', 'n'],
    },
    { n: t.i256().primaryKey(), data: t.i32() }
  ),

  pk_bool: tbl(
    'pk_bool',
    {
      insert_or_panic: 'insert_pk_bool',
      update_by: ['update_pk_bool', 'b'],
      delete_by: ['delete_pk_bool', 'b'],
    },
    { b: t.bool().primaryKey(), data: t.i32() }
  ),

  pk_string: tbl(
    'pk_string',
    {
      insert_or_panic: 'insert_pk_string',
      update_by: ['update_pk_string', 's'],
      delete_by: ['delete_pk_string', 's'],
    },
    { s: t.string().primaryKey(), data: t.i32() }
  ),

  pk_identity: tbl(
    'pk_identity',
    {
      insert_or_panic: 'insert_pk_identity',
      update_by: ['update_pk_identity', 'i'],
      delete_by: ['delete_pk_identity', 'i'],
    },
    { i: t.identity().primaryKey(), data: t.i32() }
  ),

  pk_connection_id: tbl(
    'pk_connection_id',
    {
      insert_or_panic: 'insert_pk_connection_id',
      update_by: ['update_pk_connection_id', 'a'],
      delete_by: ['delete_pk_connection_id', 'a'],
    },
    { a: t.connectionId().primaryKey(), data: t.i32() }
  ),

  pk_uuid: tbl(
    'pk_uuid',
    {
      insert_or_panic: 'insert_pk_uuid',
      update_by: ['update_pk_uuid', 'u'],
      delete_by: ['delete_pk_uuid', 'u'],
    },
    { u: t.uuid().primaryKey(), data: t.i32() }
  ),

  pk_simple_enum: tbl(
    'pk_simple_enum',
    {
      insert_or_panic: 'insert_pk_simple_enum',
    },
    { a: SimpleEnum.primaryKey(), data: t.i32() }
  ),
} as const;

// Some weird-looking tables.
const weirdTables = {
  // A table with many fields, of many different types.
  large_table: tbl(
    'large_table',
    {
      insert: 'insert_large_table',
      delete: 'delete_large_table',
    },
    {
      a: t.u8(),
      b: t.u16(),
      c: t.u32(),
      d: t.u64(),
      e: t.u128(),
      f: t.u256(),
      g: t.i8(),
      h: t.i16(),
      i: t.i32(),
      j: t.i64(),
      k: t.i128(),
      l: t.i256(),
      m: t.bool(),
      n: t.f32(),
      o: t.f64(),
      p: t.string(),
      q: SimpleEnum,
      r: EnumWithPayload,
      s: UnitStruct,
      t: ByteStruct,
      u: EveryPrimitiveStruct,
      v: EveryVecStruct,
    }
  ),

  // A table which holds instances of other table structs.
  // This tests that we can use tables as types.
  table_holds_table: tbl(
    'table_holds_table',
    {
      insert: 'insert_table_holds_table',
    },
    {
      a: singleValTables.one_u8.table.rowType,
      b: vecTables.vec_u8.table.rowType,
    }
  ),
};

const PkU32 = pkTables.pk_u32.table.rowType;

const allTables = {
  ...singleValTables,
  ...vecTables,
  ...optionTables,
  ...resultTables,
  ...uniqueTables,
  ...pkTables,
  ...weirdTables,
} as const;

const allTableDefs: {
  [k in keyof typeof allTables]: (typeof allTables)[k]['table'];
} = Object.fromEntries(
  Object.entries(allTables).map(([k, v]) => [k, v.table])
) as any;

const ScheduledTable = table(
  {
    name: 'scheduled_table',
    scheduled: (): any => send_scheduled_message,
    public: true,
  },
  {
    scheduled_id: t.u64().primaryKey().autoInc(),
    scheduled_at: t.scheduleAt(),
    text: t.string(),
  }
);

const IndexedTable = table(
  { name: 'indexed_table' },
  { player_id: t.u32().index('btree') }
);

const IndexedTable2 = table(
  {
    indexes: [
      {
        name: 'player_id_snazz_index',
        algorithm: 'btree',
        columns: ['player_id', 'player_snazz'],
      },
    ],
  },
  {
    player_id: t.u32(),
    player_snazz: t.f32(),
  }
);

const BTreeU32 = table(
  { public: true },
  t.row('BTreeU32', {
    n: t.u32().index('btree'),
    data: t.i32(),
  })
);

const Users = table(
  { name: 'users', public: true },
  {
    identity: t.identity().primaryKey(),
    name: t.string(),
  }
);

const IndexedSimpleEnum = table(
  { name: 'indexed_simple_enum', public: true },
  { n: SimpleEnum.index('btree') }
);

const spacetimedb = schema({
  ...allTableDefs,
  scheduledTable: ScheduledTable,
  indexedTable: IndexedTable,
  indexedTable2: IndexedTable2,
  btreeU32: BTreeU32,
  users: Users,
  indexedSimpleEnum: IndexedSimpleEnum,
});
export default spacetimedb;

export const reducers = spacetimedb.exportGroup(
  Object.assign(
    {},
    ...Object.values(allTables).map(({ reducers }) =>
      reducers(spacetimedb as any)
    )
  )
);

export const userFilter = spacetimedb.clientVisibilityFilter.sql(
  'SELECT * FROM users WHERE identity = :sender'
);

export const update_pk_simple_enum = spacetimedb.reducer(
  { a: SimpleEnum, data: t.i32() },
  (ctx, { a, data }) => {
    const o = ctx.db.pkSimpleEnum.a.find(a);
    if (o == null) throw new Error('row not found');
    o.data = data;
    ctx.db.pkSimpleEnum.a.update(o);
  }
);

export const insert_into_btree_u32 = spacetimedb.reducer(
  { rows: t.array(BTreeU32.rowType) },
  (ctx, { rows }) => {
    for (const row of rows) {
      ctx.db.btreeU32.insert(row);
    }
  }
);

export const delete_from_btree_u32 = spacetimedb.reducer(
  { rows: t.array(BTreeU32.rowType) },
  (ctx, { rows }) => {
    for (const row of rows) {
      ctx.db.btreeU32.delete(row);
    }
  }
);

export const insert_into_pk_btree_u32 = spacetimedb.reducer(
  { pk_u32: t.array(PkU32), bt_u32: t.array(BTreeU32.rowType) },
  (ctx, { pk_u32, bt_u32 }) => {
    for (const row of pk_u32) {
      ctx.db.pkU32.insert(row);
    }
    for (const row of bt_u32) {
      ctx.db.btreeU32.insert(row);
    }
  }
);

/// The purpose of this reducer is for a test which
/// left-semijoins `UniqueU32` to `PkU32`
/// for the purposes of behavior testing row-deduplication.
export const insert_unique_u32_update_pk_u32 = spacetimedb.reducer(
  { n: t.u32(), d_unique: t.i32(), d_pk: t.i32() },
  (ctx, { n, d_unique, d_pk }) => {
    ctx.db.uniqueU32.insert({ n, data: d_unique });
    ctx.db.pkU32.n.update({ n, data: d_pk });
  }
);

/// The purpose of this reducer is for a test with two separate semijoin queries
/// - `UniqueU32` to `PkU32`
/// - `UniqueU32` to `PkU32Two`
///
/// for the purposes of behavior testing row-deduplication.
export const delete_pk_u32_insert_pk_u32_two = spacetimedb.reducer(
  { n: t.u32(), data: t.i32() },
  (ctx, { n, data }) => {
    ctx.db.pkU32Two.insert({ n, data });
    ctx.db.pkU32.delete({ n, data });
  }
);

export const insert_caller_one_identity = spacetimedb.reducer(ctx => {
  ctx.db.oneIdentity.insert({ i: ctx.sender });
});

export const insert_caller_vec_identity = spacetimedb.reducer(ctx => {
  ctx.db.vecIdentity.insert({ i: [ctx.sender] });
});

export const insert_caller_unique_identity = spacetimedb.reducer(
  { data: t.i32() },
  (ctx, { data }) => {
    ctx.db.uniqueIdentity.insert({ i: ctx.sender, data });
  }
);

export const insert_caller_pk_identity = spacetimedb.reducer(
  { data: t.i32() },
  (ctx, { data }) => {
    ctx.db.pkIdentity.insert({ i: ctx.sender, data });
  }
);

export const insert_caller_one_connection_id = spacetimedb.reducer(ctx => {
  if (!ctx.connectionId) throw new Error('No connection id in reducer context');
  ctx.db.oneConnectionId.insert({
    a: ctx.connectionId,
  });
});

export const insert_caller_vec_connection_id = spacetimedb.reducer(ctx => {
  if (!ctx.connectionId) throw new Error('No connection id in reducer context');
  ctx.db.vecConnectionId.insert({
    a: [ctx.connectionId],
  });
});

export const insert_caller_unique_connection_id = spacetimedb.reducer(
  { data: t.i32() },
  (ctx, { data }) => {
    if (!ctx.connectionId)
      throw new Error('No connection id in reducer context');
    ctx.db.uniqueConnectionId.insert({
      a: ctx.connectionId,
      data,
    });
  }
);

export const insert_caller_pk_connection_id = spacetimedb.reducer(
  { data: t.i32() },
  (ctx, { data }) => {
    if (!ctx.connectionId)
      throw new Error('No connection id in reducer context');
    ctx.db.pkConnectionId.insert({
      a: ctx.connectionId,
      data,
    });
  }
);

export const insert_call_timestamp = spacetimedb.reducer(ctx => {
  ctx.db.oneTimestamp.insert({ t: ctx.timestamp });
});

export const insert_call_uuid_v4 = spacetimedb.reducer(ctx => {
  ctx.db.oneUuid.insert({ u: ctx.newUuidV4() });
});

export const insert_call_uuid_v7 = spacetimedb.reducer(ctx => {
  ctx.db.oneUuid.insert({ u: ctx.newUuidV7() });
});

export const insert_primitives_as_strings = spacetimedb.reducer(
  { s: EveryPrimitiveStruct },
  (ctx, { s }) => {
    ctx.db.vecString.insert({
      s: [
        s.a.toString(),
        s.b.toString(),
        s.c.toString(),
        s.d.toString(),
        s.e.toString(),
        s.f.toString(),
        s.g.toString(),
        s.h.toString(),
        s.i.toString(),
        s.j.toString(),
        s.k.toString(),
        s.l.toString(),
        s.m.toString(),
        s.n.toString(),
        s.o.toString(),
        s.p.toString(),
        s.q.toHexString(),
        s.r.toHexString(),
        // FIXME: precise ISO string match between JS and Rust
        // s.s.toDate().toISOString(),
        '1970-01-01T02:44:36.543210+00:00',
        s.t.toString(),
        s.u.toString(),
      ],
    });
  }
);

export const no_op_succeeds = spacetimedb.reducer(_ctx => {});

export const oneu8Filter = spacetimedb.clientVisibilityFilter.sql(
  'SELECT * FROM one_u8'
);

export const send_scheduled_message = spacetimedb.reducer(
  { arg: ScheduledTable.rowType },
  (_ctx, { arg }) => {
    const _ = [arg.text, arg.scheduled_at, arg.scheduled_id];
  }
);

export const insert_user = spacetimedb.reducer(
  { name: t.string(), identity: t.identity() },
  (ctx, { name, identity }) => {
    ctx.db.users.insert({ name, identity });
  }
);

export const insert_into_indexed_simple_enum = spacetimedb.reducer(
  { n: SimpleEnum },
  (ctx, { n }) => {
    ctx.db.indexedSimpleEnum.insert({ n });
  }
);

export const update_indexed_simple_enum = spacetimedb.reducer(
  { a: SimpleEnum, b: SimpleEnum },
  (ctx, { a, b }) => {
    if (!ctx.db.indexedSimpleEnum.n.filter(a).next().done) {
      ctx.db.indexedSimpleEnum.n.delete(a);
      ctx.db.indexedSimpleEnum.insert({ n: b });
    }
  }
);

export const sorted_uuids_insert = spacetimedb.reducer(ctx => {
  for (let i = 0; i < 1000; i++) {
    const uuid = ctx.newUuidV7();
    ctx.db.pkUuid.insert({ u: uuid, data: 0 });
  }

  // Verify UUIDs are sorted
  let lastUuid: Uuid | null = null;

  for (const row of ctx.db.pkUuid.iter()) {
    if (lastUuid !== null && lastUuid >= row.u) {
      throw new Error('UUIDs are not sorted correctly');
    }
    lastUuid = row.u;
  }
});
