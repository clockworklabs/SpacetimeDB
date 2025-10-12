// ─────────────────────────────────────────────────────────────────────────────
// IMPORTS
// ─────────────────────────────────────────────────────────────────────────────
import { type RowObj, schema, t, table } from 'spacetimedb/server';

// TODO: Remove
export { __call_reducer__, __describe_module__ } from 'spacetimedb/server';

const SimpleEnum = t.enum('SimpleEnum', {
  Zero: t.unit(),
  One: t.unit(),
  Two: t.unit(),
});

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
});

// /// Defines one or more tables, and optionally reducers alongside them.
// ///
// /// Each table specifier is:
// ///
// /// TableName { reducers... } fields...;
// ///
// /// where:
// ///
// /// - TableName is an identifier for the new table.
// ///
// /// - reducers... is a comma-separated list of reducer specifiers, which may be:
// ///   - insert reducer_name
// ///     Defines a reducer which takes an argument for each of the table's columns, and inserts a new row.
// ///     Not suitable for tables with unique constraints.
// ///     e.g. insert insert_my_table
// ///   - insert_or_panic reducer_name
// ///     Like insert, but for tables with unique constraints. Unwraps the output of `insert`.
// ///     e.g. insert_or_panic insert_my_table
// ///   - update_by reducer_name = update_method(field_name)
// ///     Defines a reducer which takes an argument for each of the table's columns,
// ///     and calls the update_method with the value of field_name as a first argument
// ///     to update an existing row.
// ///     e.g. update_by update_my_table = update_by_name(name)
// ///   - delete_by reducer_name = delete_method(field_name: field_type)
// ///     Defines a reducer which takes a single argument, and passes it to the delete_method
// ///     to delete a row.
// ///     e.g. delete_by delete_my_table = delete_by_name(name: String)
// ///
// /// - fields is a comma-separated list of field specifiers, which are optional attributes,
// ///   followed by a field name identifier and a type.
// ///   e.g. #[unique] name String
// ///
// /// A full table definition might be:
// ///
// /// MyTable {
// ///     insert_or_panic insert_my_table,
// ///     update_by update_my_table = update_by_name(name),
// ///     delete_by delete_my_table = delete_by_name(name: String),
// /// } #[primary_key] name String,
// ///   #[auto_inc] #[unique] id u32,
// ///   count i64;
// //
// // Internal rules are prefixed with @.
// macro_rules! define_tables {
//     // Base case for `@impl_ops` recursion: no more ops to define.
//     (@impl_ops $name:ident { $(,)? } $($more:tt)*) => {};

//     // Define a reducer for tables without unique constraints,
//     // which inserts a row.
//     (@impl_ops $name:ident
//      { insert $insert:ident
//        $(, $($ops:tt)* )? }
//      $($field_name:ident $ty:ty),* $(,)*) => {
//         paste::paste! {
//             #[spacetimedb::reducer]
//             pub fn $insert (ctx: &ReducerContext, $($field_name : $ty,)*) {
//                 ctx.db.[<$name:snake>]().insert($name { $($field_name,)* });
//             }
//         }

//         define_tables!(@impl_ops $name { $($($ops)*)? } $($field_name $ty,)*);
//     };

//     // Define a reducer for tables without unique constraints,
//     // which deletes a row.
//     (@impl_ops $name:ident
//      { delete $delete:ident
//        $(, $($ops:tt)* )? }
//      $($field_name:ident $ty:ty),* $(,)*) => {
//         paste::paste! {
//             #[spacetimedb::reducer]
//             pub fn $delete (ctx: &ReducerContext, $($field_name : $ty,)*) {
//                 ctx.db.[<$name:snake>]().delete($name { $($field_name,)* });
//             }
//         }

//         define_tables!(@impl_ops $name { $($($ops)*)? } $($field_name $ty,)*);
//     };

//     // Define a reducer for tables with unique constraints,
//     // which inserts a row, or panics with `expect` if the row violates a unique constraint.
//     (@impl_ops $name:ident
//      { insert_or_panic $insert:ident
//        $(, $($ops:tt)* )? }
//      $($field_name:ident $ty:ty),* $(,)*) => {
//         paste::paste! {
//             #[spacetimedb::reducer]
//             pub fn $insert (ctx: &ReducerContext, $($field_name : $ty,)*) {
//                 ctx.db.[<$name:snake>]().insert($name { $($field_name,)* });
//             }
//         }

//         define_tables!(@impl_ops $name { $($($ops)*)? } $($field_name $ty,)*);
//     };

//     // Define a reducer for tables with a unique field,
//     // which uses `$update_method` to update by that unique field.
//     (@impl_ops $name:ident
//      { update_by $update:ident = $update_method:ident($unique_field:ident)
//        $(, $($ops:tt)* )? }
//      $($field_name:ident $ty:ty),* $(,)*) => {
//         paste::paste! {
//             #[spacetimedb::reducer]
//             pub fn $update (ctx: &ReducerContext, $($field_name : $ty,)*) {
//                 ctx.db.[<$name:snake>]().$unique_field().update($name { $($field_name,)* });
//             }
//         }

//         define_tables!(@impl_ops $name { $($($ops)*)? } $($field_name $ty,)*);
//     };

//     // Define a reducer for tables with a unique field,
//     // which uses `$delete_method` to delete by that unique field.
//     (@impl_ops $name:ident
//      { delete_by $delete:ident = $delete_method:ident($unique_field:ident : $unique_ty:ty)
//        $(, $($ops:tt)*)? }
//      $($other_fields:tt)* ) => {
//         paste::paste! {
//             #[spacetimedb::reducer]
//             pub fn $delete (ctx: &ReducerContext, $unique_field : $unique_ty) {
//                 ctx.db.[<$name:snake>]().$unique_field().delete(&$unique_field);
//             }
//         }

//         define_tables!(@impl_ops $name { $($($ops)*)? } $($other_fields)*);
//     };

//     // Define a table.
//     (@one $name:ident { $($ops:tt)* } $($(#[$attr:meta])* $field_name:ident $ty:ty),* $(,)*) => {
//         paste::paste! {
//             #[spacetimedb::table(name = [<$name:snake>], public)]
//             pub struct $name {
//                 $($(#[$attr])* pub $field_name : $ty,)*
//             }
//         }

//         // Recursively implement reducers based on the `ops`.
//         define_tables!(@impl_ops $name { $($ops)* } $($field_name $ty,)*);
//     };

//     // "Public" interface: Define many tables.
//     ($($name:ident { $($ops:tt)* } $($(#[$attr:meta])* $field_name:ident $ty:ty),* $(,)*;)*) => {
//         // Define each table one-by-one, iteratively.
//         $(define_tables!(@one $name { $($ops)* } $($(#[$attr])* $field_name $ty,)*);)*
//     };
// }

type TableSchema = ReturnType<typeof table<any, any>>;

type TableWithReducers<Table extends TableSchema> = {
  table: Table;
  reducers(spacetimedb: ReturnType<typeof schema<[Table]>>): void;
};

function tbl<const Name extends string, Row extends RowObj>(
  name: Name,
  ops: {
    insert?: string;
    delete?: string;
    insert_or_panic?: string;
    update_by?: [string, keyof Row];
    delete_by?: [string, keyof Row];
  },
  row: Row
): TableWithReducers<ReturnType<typeof table<Row, { name: Name }>>> {
  const t = table({ name, public: true }, row);
  return {
    table: t,
    reducers(spacetimedb) {
      if (ops.insert) {
        spacetimedb.reducer(ops.insert, row, (ctx, args) => {
          (ctx.db[name] as any).insert({ ...args });
        });
      }
      if (ops.delete) {
        spacetimedb.reducer(ops.delete, row, (ctx, args) => {
          (ctx.db[name] as any).delete({ ...args });
        });
      }
      if (ops.insert_or_panic) {
        spacetimedb.reducer(ops.insert_or_panic, row, (ctx, args) => {
          (ctx.db[name] as any).insert({ ...args });
        });
      }
      if (ops.update_by) {
        const [reducer, col] = ops.update_by;
        spacetimedb.reducer(reducer, row, (ctx, args) => {
          (ctx.db[name] as any)[col].update({ ...args });
        });
      }
      if (ops.delete_by) {
        const [reducer, col] = ops.delete_by;
        spacetimedb.reducer(reducer, { [col]: row[col] }, (ctx, args) => {
          (ctx.db[name] as any)[col].delete(args[col as any]);
        });
      }
    },
  };
}

// Tables holding a single value.
const singleValTables = [
  tbl('one_u8', { insert: 'insert_one_u8' }, { n: t.u8() }),
  tbl('one_u16', { insert: 'insert_one_u16' }, { n: t.u16() }),
  tbl('one_u32', { insert: 'insert_one_u32' }, { n: t.u32() }),
  tbl('one_u64', { insert: 'insert_one_u64' }, { n: t.u64() }),
  tbl('one_u128', { insert: 'insert_one_u128' }, { n: t.u128() }),
  tbl('one_u256', { insert: 'insert_one_u256' }, { n: t.u256() }),

  tbl('one_i8', { insert: 'insert_one_i8' }, { n: t.i8() }),
  tbl('one_i16', { insert: 'insert_one_i16' }, { n: t.i16() }),
  tbl('one_i32', { insert: 'insert_one_i32' }, { n: t.i32() }),
  tbl('one_i64', { insert: 'insert_one_i64' }, { n: t.i64() }),
  tbl('one_i128', { insert: 'insert_one_i128' }, { n: t.i128() }),
  tbl('one_i256', { insert: 'insert_one_i256' }, { n: t.i256() }),

  tbl('one_bool', { insert: 'insert_one_bool' }, { b: t.bool() }),

  tbl('one_f32', { insert: 'insert_one_f32' }, { f: t.f32() }),
  tbl('one_f64', { insert: 'insert_one_f64' }, { f: t.f64() }),

  tbl('one_string', { insert: 'insert_one_string' }, { s: t.string() }),

  tbl('one_identity', { insert: 'insert_one_identity' }, { i: t.identity() }),
  tbl(
    'one_connection_id',
    { insert: 'insert_one_connection_id' },
    { a: t.connectionId() }
  ),

  tbl(
    'one_timestamp',
    { insert: 'insert_one_timestamp' },
    { t: t.timestamp() }
  ),

  tbl(
    'one_simple_enum',
    { insert: 'insert_one_simple_enum' },
    { e: SimpleEnum }
  ),
  tbl(
    'one_enum_with_payload',
    { insert: 'insert_one_enum_with_payload' },
    { e: EnumWithPayload }
  ),

  tbl(
    'one_unit_struct',
    { insert: 'insert_one_unit_struct' },
    { s: UnitStruct }
  ),
  tbl(
    'one_byte_struct',
    { insert: 'insert_one_byte_struct' },
    { s: ByteStruct }
  ),
  tbl(
    'one_every_primitive_struct',
    { insert: 'insert_one_every_primitive_struct' },
    { s: EveryPrimitiveStruct }
  ),
  tbl(
    'one_every_vec_struct',
    { insert: 'insert_one_every_vec_struct' },
    { s: EveryVecStruct }
  ),
] as const;

// Tables holding a Vec of various types.
const vecTables = [
  tbl('vec_u8', { insert: 'insert_vec_u8' }, { n: t.array(t.u8()) }),
  tbl('vec_u16', { insert: 'insert_vec_u16' }, { n: t.array(t.u16()) }),
  tbl('vec_u32', { insert: 'insert_vec_u32' }, { n: t.array(t.u32()) }),
  tbl('vec_u64', { insert: 'insert_vec_u64' }, { n: t.array(t.u64()) }),
  tbl('vec_u128', { insert: 'insert_vec_u128' }, { n: t.array(t.u128()) }),
  tbl('vec_u256', { insert: 'insert_vec_u256' }, { n: t.array(t.u256()) }),

  tbl('vec_i8', { insert: 'insert_vec_i8' }, { n: t.array(t.i8()) }),
  tbl('vec_i16', { insert: 'insert_vec_i16' }, { n: t.array(t.i16()) }),
  tbl('vec_i32', { insert: 'insert_vec_i32' }, { n: t.array(t.i32()) }),
  tbl('vec_i64', { insert: 'insert_vec_i64' }, { n: t.array(t.i64()) }),
  tbl('vec_i128', { insert: 'insert_vec_i128' }, { n: t.array(t.i128()) }),
  tbl('vec_i256', { insert: 'insert_vec_i256' }, { n: t.array(t.i256()) }),

  tbl('vec_bool', { insert: 'insert_vec_bool' }, { b: t.array(t.bool()) }),

  tbl('vec_f32', { insert: 'insert_vec_f32' }, { f: t.array(t.f32()) }),
  tbl('vec_f64', { insert: 'insert_vec_f64' }, { f: t.array(t.f64()) }),

  tbl(
    'vec_string',
    { insert: 'insert_vec_string' },
    { s: t.array(t.string()) }
  ),

  tbl(
    'vec_identity',
    { insert: 'insert_vec_identity' },
    { i: t.array(t.identity()) }
  ),
  tbl(
    'vec_connection_id',
    { insert: 'insert_vec_connection_id' },
    { a: t.array(t.connectionId()) }
  ),

  tbl(
    'vec_timestamp',
    { insert: 'insert_vec_timestamp' },
    { t: t.array(t.timestamp()) }
  ),

  tbl(
    'vec_simple_enum',
    { insert: 'insert_vec_simple_enum' },
    { e: t.array(SimpleEnum) }
  ),
  tbl(
    'vec_enum_with_payload',
    { insert: 'insert_vec_enum_with_payload' },
    { e: t.array(EnumWithPayload) }
  ),

  tbl(
    'vec_unit_struct',
    { insert: 'insert_vec_unit_struct' },
    { s: t.array(UnitStruct) }
  ),
  tbl(
    'vec_byte_struct',
    { insert: 'insert_vec_byte_struct' },
    { s: t.array(ByteStruct) }
  ),
  tbl(
    'vec_every_primitive_struct',
    { insert: 'insert_vec_every_primitive_struct' },
    { s: t.array(EveryPrimitiveStruct) }
  ),
  tbl(
    'vec_every_vec_struct',
    { insert: 'insert_vec_every_vec_struct' },
    { s: t.array(EveryVecStruct) }
  ),
] as const;

// Tables holding an Option of various types.
const optionTables = [
  tbl('option_i32', { insert: 'insert_option_i32' }, { n: t.option(t.i32()) }),
  tbl(
    'option_string',
    { insert: 'insert_option_string' },
    { s: t.option(t.string()) }
  ),
  tbl(
    'option_identity',
    { insert: 'insert_option_identity' },
    { i: t.option(t.identity()) }
  ),
  tbl(
    'option_simple_enum',
    { insert: 'insert_option_simple_enum' },
    { e: t.option(SimpleEnum) }
  ),
  tbl(
    'option_every_primitive_struct',
    { insert: 'insert_option_every_primitive_struct' },
    { s: t.option(EveryPrimitiveStruct) }
  ),
  tbl(
    'option_vec_option_i32',
    { insert: 'insert_option_vec_option_i32' },
    { v: t.option(t.array(t.option(t.i32()))) }
  ),
] as const;

// Tables mapping a unique, but non-pk, key to a boring i32 payload.
// This allows us to test delete events, and the semantically correct absence of update events.
const uniqueTables = [
  tbl(
    'unique_u8',
    {
      insert_or_panic: 'insert_unique_u8',
      update_by: ['update_unique_u8', 'n'],
      delete_by: ['delete_unique_u8', 'n'],
    },
    { n: t.u8().unique(), data: t.i32() }
  ),

  tbl(
    'unique_u16',
    {
      insert_or_panic: 'insert_unique_u16',
      update_by: ['update_unique_u16', 'n'],
      delete_by: ['delete_unique_u16', 'n'],
    },
    { n: t.u16().unique(), data: t.i32() }
  ),

  tbl(
    'unique_u32',
    {
      insert_or_panic: 'insert_unique_u32',
      update_by: ['update_unique_u32', 'n'],
      delete_by: ['delete_unique_u32', 'n'],
    },
    { n: t.u32().unique(), data: t.i32() }
  ),

  tbl(
    'unique_u64',
    {
      insert_or_panic: 'insert_unique_u64',
      update_by: ['update_unique_u64', 'n'],
      delete_by: ['delete_unique_u64', 'n'],
    },
    { n: t.u64().unique(), data: t.i32() }
  ),

  tbl(
    'unique_u128',
    {
      insert_or_panic: 'insert_unique_u128',
      update_by: ['update_unique_u128', 'n'],
      delete_by: ['delete_unique_u128', 'n'],
    },
    { n: t.u128().unique(), data: t.i32() }
  ),

  tbl(
    'unique_u256',
    {
      insert_or_panic: 'insert_unique_u256',
      update_by: ['update_unique_u256', 'n'],
      delete_by: ['delete_unique_u256', 'n'],
    },
    { n: t.u256().unique(), data: t.i32() }
  ),

  tbl(
    'unique_i8',
    {
      insert_or_panic: 'insert_unique_i8',
      update_by: ['update_unique_i8', 'n'],
      delete_by: ['delete_unique_i8', 'n'],
    },
    { n: t.i8().unique(), data: t.i32() }
  ),

  tbl(
    'unique_i16',
    {
      insert_or_panic: 'insert_unique_i16',
      update_by: ['update_unique_i16', 'n'],
      delete_by: ['delete_unique_i16', 'n'],
    },
    { n: t.i16().unique(), data: t.i32() }
  ),

  tbl(
    'unique_i32',
    {
      insert_or_panic: 'insert_unique_i32',
      update_by: ['update_unique_i32', 'n'],
      delete_by: ['delete_unique_i32', 'n'],
    },
    { n: t.i32().unique(), data: t.i32() }
  ),

  tbl(
    'unique_i64',
    {
      insert_or_panic: 'insert_unique_i64',
      update_by: ['update_unique_i64', 'n'],
      delete_by: ['delete_unique_i64', 'n'],
    },
    { n: t.i64().unique(), data: t.i32() }
  ),

  tbl(
    'unique_i128',
    {
      insert_or_panic: 'insert_unique_i128',
      update_by: ['update_unique_i128', 'n'],
      delete_by: ['delete_unique_i128', 'n'],
    },
    { n: t.i128().unique(), data: t.i32() }
  ),

  tbl(
    'unique_i256',
    {
      insert_or_panic: 'insert_unique_i256',
      update_by: ['update_unique_i256', 'n'],
      delete_by: ['delete_unique_i256', 'n'],
    },
    { n: t.i256().unique(), data: t.i32() }
  ),

  tbl(
    'unique_bool',
    {
      insert_or_panic: 'insert_unique_bool',
      update_by: ['update_unique_bool', 'b'],
      delete_by: ['delete_unique_bool', 'b'],
    },
    { b: t.bool().unique(), data: t.i32() }
  ),

  tbl(
    'unique_string',
    {
      insert_or_panic: 'insert_unique_string',
      update_by: ['update_unique_string', 's'],
      delete_by: ['delete_unique_string', 's'],
    },
    { s: t.string().unique(), data: t.i32() }
  ),

  tbl(
    'unique_identity',
    {
      insert_or_panic: 'insert_unique_identity',
      update_by: ['update_unique_identity', 'i'],
      delete_by: ['delete_unique_identity', 'i'],
    },
    { i: t.identity().unique(), data: t.i32() }
  ),

  tbl(
    'unique_connection_id',
    {
      insert_or_panic: 'insert_unique_connection_id',
      update_by: ['update_unique_connection_id', 'a'],
      delete_by: ['delete_unique_connection_id', 'a'],
    },
    { a: t.connectionId().unique(), data: t.i32() }
  ),
] as const;

// Tables mapping a primary key to a boring i32 payload.
// This allows us to test update and delete events.
const pkTables = [
  tbl(
    'pk_u8',
    {
      insert_or_panic: 'insert_pk_u8',
      update_by: ['update_pk_u8', 'n'],
      delete_by: ['delete_pk_u8', 'n'],
    },
    { n: t.u8().primaryKey(), data: t.i32() }
  ),

  tbl(
    'pk_u16',
    {
      insert_or_panic: 'insert_pk_u16',
      update_by: ['update_pk_u16', 'n'],
      delete_by: ['delete_pk_u16', 'n'],
    },
    { n: t.u16().primaryKey(), data: t.i32() }
  ),

  tbl(
    'pk_u32',
    {
      insert_or_panic: 'insert_pk_u32',
      update_by: ['update_pk_u32', 'n'],
      delete_by: ['delete_pk_u32', 'n'],
    },
    { n: t.u32().primaryKey(), data: t.i32() }
  ),

  tbl(
    'pk_u32_two',
    {
      insert_or_panic: 'insert_pk_u32_two',
      update_by: ['update_pk_u32_two', 'n'],
      delete_by: ['delete_pk_u32_two', 'n'],
    },
    { n: t.u32().primaryKey(), data: t.i32() }
  ),

  tbl(
    'pk_u64',
    {
      insert_or_panic: 'insert_pk_u64',
      update_by: ['update_pk_u64', 'n'],
      delete_by: ['delete_pk_u64', 'n'],
    },
    { n: t.u64().primaryKey(), data: t.i32() }
  ),

  tbl(
    'pk_u128',
    {
      insert_or_panic: 'insert_pk_u128',
      update_by: ['update_pk_u128', 'n'],
      delete_by: ['delete_pk_u128', 'n'],
    },
    { n: t.u128().primaryKey(), data: t.i32() }
  ),

  tbl(
    'pk_u256',
    {
      insert_or_panic: 'insert_pk_u256',
      update_by: ['update_pk_u256', 'n'],
      delete_by: ['delete_pk_u256', 'n'],
    },
    { n: t.u256().primaryKey(), data: t.i32() }
  ),

  tbl(
    'pk_i8',
    {
      insert_or_panic: 'insert_pk_i8',
      update_by: ['update_pk_i8', 'n'],
      delete_by: ['delete_pk_i8', 'n'],
    },
    { n: t.i8().primaryKey(), data: t.i32() }
  ),

  tbl(
    'pk_i16',
    {
      insert_or_panic: 'insert_pk_i16',
      update_by: ['update_pk_i16', 'n'],
      delete_by: ['delete_pk_i16', 'n'],
    },
    { n: t.i16().primaryKey(), data: t.i32() }
  ),

  tbl(
    'pk_i32',
    {
      insert_or_panic: 'insert_pk_i32',
      update_by: ['update_pk_i32', 'n'],
      delete_by: ['delete_pk_i32', 'n'],
    },
    { n: t.i32().primaryKey(), data: t.i32() }
  ),

  tbl(
    'pk_i64',
    {
      insert_or_panic: 'insert_pk_i64',
      update_by: ['update_pk_i64', 'n'],
      delete_by: ['delete_pk_i64', 'n'],
    },
    { n: t.i64().primaryKey(), data: t.i32() }
  ),

  tbl(
    'pk_i128',
    {
      insert_or_panic: 'insert_pk_i128',
      update_by: ['update_pk_i128', 'n'],
      delete_by: ['delete_pk_i128', 'n'],
    },
    { n: t.i128().primaryKey(), data: t.i32() }
  ),

  tbl(
    'pk_i256',
    {
      insert_or_panic: 'insert_pk_i256',
      update_by: ['update_pk_i256', 'n'],
      delete_by: ['delete_pk_i256', 'n'],
    },
    { n: t.i256().primaryKey(), data: t.i32() }
  ),

  tbl(
    'pk_bool',
    {
      insert_or_panic: 'insert_pk_bool',
      update_by: ['update_pk_bool', 'b'],
      delete_by: ['delete_pk_bool', 'b'],
    },
    { b: t.bool().primaryKey(), data: t.i32() }
  ),

  tbl(
    'pk_string',
    {
      insert_or_panic: 'insert_pk_string',
      update_by: ['update_pk_string', 's'],
      delete_by: ['delete_pk_string', 's'],
    },
    { s: t.string().primaryKey(), data: t.i32() }
  ),

  tbl(
    'pk_identity',
    {
      insert_or_panic: 'insert_pk_identity',
      update_by: ['update_pk_identity', 'i'],
      delete_by: ['delete_pk_identity', 'i'],
    },
    { i: t.identity().primaryKey(), data: t.i32() }
  ),

  tbl(
    'pk_connection_id',
    {
      insert_or_panic: 'insert_pk_connection_id',
      update_by: ['update_pk_connection_id', 'a'],
      delete_by: ['delete_pk_connection_id', 'a'],
    },
    { a: t.connectionId().primaryKey(), data: t.i32() }
  ),

  tbl(
    'pk_simple_enum',
    {
      insert_or_panic: 'insert_pk_simple_enum',
    },
    { a: SimpleEnum.primaryKey(), data: t.i32() }
  ),
] as const;

// Some weird-looking tables.
const weirdTables = [
  // A table with many fields, of many different types.
  tbl(
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
  tbl(
    'table_holds_table',
    {
      insert: 'insert_table_holds_table',
    },
    {
      a: singleValTables[0].table.rowType, // OneU8
      b: vecTables[0].table.rowType, // VecU8
    }
  ),
];

const PkU32 = pkTables[2].table.rowType;

const allTables = [
  ...singleValTables,
  ...vecTables,
  ...optionTables,
  ...uniqueTables,
  ...pkTables,
  ...weirdTables,
] as const;

type ExtractTables<T extends readonly TableWithReducers<any>[]> = {
  [i in keyof T]: T[i]['table'];
};
const allTableDefs: ExtractTables<typeof allTables> = allTables.map(
  x => x.table
) as any;

const ScheduledTable = table(
  {
    name: 'scheduled_table',
    scheduled: 'send_scheduled_message',
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
    name: 'indexed_table_2',
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
  { name: 'btree_u32', public: true },
  {
    n: t.u32().index('btree'),
    data: t.i32(),
  }
);

// #[spacetimedb::client_visibility_filter]
// const USERS_FILTER: spacetimedb::Filter = spacetimedb::Filter::Sql("SELECT * FROM users WHERE identity = :sender");

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

const spacetimedb = schema(
  ...allTableDefs,
  ScheduledTable,
  IndexedTable,
  IndexedTable2,
  BTreeU32,
  Users,
  IndexedSimpleEnum
);

for (const { reducers } of allTables) {
  reducers(spacetimedb as any);
}

spacetimedb.reducer(
  'update_pk_simple_enum',
  { a: SimpleEnum, data: t.i32() },
  (ctx, { a, data }) => {
    const o = ctx.db.pk_simple_enum.a.find(a);
    if (o == null) throw new Error('row not found');
    o.data = data;
    ctx.db.pk_simple_enum.a.update(o);
  }
);

spacetimedb.reducer(
  'insert_into_btree_u32',
  { rows: t.array(BTreeU32.rowType) },
  (ctx, { rows }) => {
    for (const row of rows) {
      ctx.db.btree_u32.insert(row);
    }
  }
);

spacetimedb.reducer(
  'delete_from_btree_u32',
  { rows: t.array(BTreeU32.rowType) },
  (ctx, { rows }) => {
    for (const row of rows) {
      ctx.db.btree_u32.delete(row);
    }
  }
);

spacetimedb.reducer(
  'insert_into_pk_btree_u32',
  { pk_u32: t.array(PkU32), bt_u32: t.array(BTreeU32.rowType) },
  (ctx, { pk_u32, bt_u32 }) => {
    for (const row of pk_u32) {
      ctx.db.pk_u32.insert(row);
    }
    for (const row of bt_u32) {
      ctx.db.btree_u32.insert(row);
    }
  }
);

/// The purpose of this reducer is for a test which
/// left-semijoins `UniqueU32` to `PkU32`
/// for the purposes of behavior testing row-deduplication.
spacetimedb.reducer(
  'insert_unique_u32_update_pk_u32',
  { n: t.u32(), d_unique: t.i32(), d_pk: t.i32() },
  (ctx, { n, d_unique, d_pk }) => {
    ctx.db.unique_u32.insert({ n, data: d_unique });
    ctx.db.pk_u32.n.update({ n, data: d_pk });
  }
);

/// The purpose of this reducer is for a test with two separate semijoin queries
/// - `UniqueU32` to `PkU32`
/// - `UniqueU32` to `PkU32Two`
///
/// for the purposes of behavior testing row-deduplication.
spacetimedb.reducer(
  'delete_pk_u32_insert_pk_u32_two',
  { n: t.u32(), data: t.i32() },
  (ctx, { n, data }) => {
    ctx.db.pk_u32_two.insert({ n, data });
    ctx.db.pk_u32.delete({ n, data });
  }
);

spacetimedb.reducer('insert_caller_one_identity', ctx => {
  ctx.db.one_identity.insert({ i: ctx.sender });
});

spacetimedb.reducer('insert_caller_vec_identity', ctx => {
  ctx.db.vec_identity.insert({ i: [ctx.sender] });
});

spacetimedb.reducer(
  'insert_caller_unique_identity',
  { data: t.i32() },
  (ctx, { data }) => {
    ctx.db.unique_identity.insert({ i: ctx.sender, data });
  }
);

spacetimedb.reducer(
  'insert_caller_pk_identity',
  { data: t.i32() },
  (ctx, { data }) => {
    ctx.db.pk_identity.insert({ i: ctx.sender, data });
  }
);

spacetimedb.reducer('insert_caller_one_connection_id', ctx => {
  if (!ctx.connectionId) throw new Error('No connection id in reducer context');
  ctx.db.one_connection_id.insert({
    a: ctx.connectionId,
  });
});

spacetimedb.reducer('insert_caller_vec_connection_id', ctx => {
  if (!ctx.connectionId) throw new Error('No connection id in reducer context');
  ctx.db.vec_connection_id.insert({
    a: [ctx.connectionId],
  });
});

spacetimedb.reducer(
  'insert_caller_unique_connection_id',
  { data: t.i32() },
  (ctx, { data }) => {
    if (!ctx.connectionId)
      throw new Error('No connection id in reducer context');
    ctx.db.unique_connection_id.insert({
      a: ctx.connectionId,
      data,
    });
  }
);

spacetimedb.reducer(
  'insert_caller_pk_connection_id',
  { data: t.i32() },
  (ctx, { data }) => {
    if (!ctx.connectionId)
      throw new Error('No connection id in reducer context');
    ctx.db.pk_connection_id.insert({
      a: ctx.connectionId,
      data,
    });
  }
);

spacetimedb.reducer('insert_call_timestamp', ctx => {
  ctx.db.one_timestamp.insert({ t: ctx.timestamp });
});

spacetimedb.reducer(
  'insert_primitives_as_strings',
  { s: EveryPrimitiveStruct },
  (ctx, { s }) => {
    ctx.db.vec_string.insert({
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
        s.s.toDate().toISOString(),
        s.t.toString(),
      ],
    });
  }
);

spacetimedb.reducer('no_op_succeeds', _ctx => {});

// #[spacetimedb::client_visibility_filter]
// const ONE_U8_VISIBLE: spacetimedb::Filter = spacetimedb::Filter::Sql("SELECT * FROM one_u8");

spacetimedb.reducer(
  'send_scheduled_message',
  { arg: ScheduledTable.rowType },
  (_ctx, { arg }) => {
    const _ = [arg.text, arg.scheduled_at, arg.scheduled_id];
  }
);

spacetimedb.reducer(
  'insert_user',
  { name: t.string(), identity: t.identity() },
  (ctx, { name, identity }) => {
    ctx.db.users.insert({ name, identity });
  }
);

spacetimedb.reducer(
  'insert_into_indexed_simple_enum',
  { n: SimpleEnum },
  (ctx, { n }) => {
    ctx.db.indexed_simple_enum.insert({ n });
  }
);

spacetimedb.reducer(
  'update_indexed_simple_enum',
  { a: SimpleEnum, b: SimpleEnum },
  (ctx, { a, b }) => {
    if (!ctx.db.indexed_simple_enum.n.filter(a).next().done) {
      ctx.db.indexed_simple_enum.n.delete(a);
      ctx.db.indexed_simple_enum.insert({ n: b });
    }
  }
);
