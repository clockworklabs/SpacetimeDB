// Some of our tests include reducers with large numbers of arguments.
// This is on purpose.
// Due to a clippy bug (as of 2023-08-31),
// we cannot locally disable the lint around the definitions of those reducers,
// because the definitions are macro-generated,
// and clippy misunderstands `#[allow]` attributes in macro-expansions.
#![allow(clippy::too_many_arguments)]

use anyhow::{anyhow, Context, Result};
use spacetimedb::{
    sats::{i256, u256},
    ConnectionId, Identity, ReducerContext, SpacetimeType, Table, TimeDuration, Timestamp,
};

#[derive(PartialEq, Eq, Hash, SpacetimeType)]
pub enum SimpleEnum {
    Zero,
    One,
    Two,
}

#[derive(SpacetimeType)]
pub enum EnumWithPayload {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    U256(u256),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    I256(i256),
    Bool(bool),
    F32(f32),
    F64(f64),
    Str(String),
    Identity(Identity),
    ConnectionId(ConnectionId),
    Timestamp(Timestamp),
    Bytes(Vec<u8>),
    Ints(Vec<i32>),
    Strings(Vec<String>),
    SimpleEnums(Vec<SimpleEnum>),
    // SpacetimeDB doesn't yet support recursive types in modules
    // Recursive(Vec<EnumWithPayload>),
}

#[derive(SpacetimeType)]
pub struct UnitStruct {}

#[derive(SpacetimeType)]
pub struct ByteStruct {
    b: u8,
}

#[derive(SpacetimeType)]
pub struct EveryPrimitiveStruct {
    a: u8,
    b: u16,
    c: u32,
    d: u64,
    e: u128,
    f: u256,
    g: i8,
    h: i16,
    i: i32,
    j: i64,
    k: i128,
    l: i256,
    m: bool,
    n: f32,
    o: f64,
    p: String,
    q: Identity,
    r: ConnectionId,
    s: Timestamp,
    t: TimeDuration,
}

#[derive(SpacetimeType)]
pub struct EveryVecStruct {
    a: Vec<u8>,
    b: Vec<u16>,
    c: Vec<u32>,
    d: Vec<u64>,
    e: Vec<u128>,
    f: Vec<u256>,
    g: Vec<i8>,
    h: Vec<i16>,
    i: Vec<i32>,
    j: Vec<i64>,
    k: Vec<i128>,
    l: Vec<i256>,
    m: Vec<bool>,
    n: Vec<f32>,
    o: Vec<f64>,
    p: Vec<String>,
    q: Vec<Identity>,
    r: Vec<ConnectionId>,
    s: Vec<Timestamp>,
    t: Vec<TimeDuration>,
}

/// Defines one or more tables, and optionally reducers alongside them.
///
/// Each table specifier is:
///
/// TableName { reducers... } fields...;
///
/// where:
///
/// - TableName is an identifier for the new table.
///
/// - reducers... is a comma-separated list of reducer specifiers, which may be:
///   - insert reducer_name
///     Defines a reducer which takes an argument for each of the table's columns, and inserts a new row.
///     Not suitable for tables with unique constraints.
///     e.g. insert insert_my_table
///   - insert_or_panic reducer_name
///     Like insert, but for tables with unique constraints. Unwraps the output of `insert`.
///     e.g. insert_or_panic insert_my_table
///   - update_by reducer_name = update_method(field_name)
///     Defines a reducer which takes an argument for each of the table's columns,
///     and calls the update_method with the value of field_name as a first argument
///     to update an existing row.
///     e.g. update_by update_my_table = update_by_name(name)
///   - delete_by reducer_name = delete_method(field_name: field_type)
///     Defines a reducer which takes a single argument, and passes it to the delete_method
///     to delete a row.
///     e.g. delete_by delete_my_table = delete_by_name(name: String)
///
/// - fields is a comma-separated list of field specifiers, which are optional attribues,
///   followed by a field name identifier and a type.
///   e.g. #[unique] name String
///
/// A full table definition might be:
///
/// MyTable {
///     insert_or_panic insert_my_table,
///     update_by update_my_table = update_by_name(name),
///     delete_by delete_my_table = delete_by_name(name: String),
/// } #[primary_key] name String,
///   #[auto_inc] #[unique] id u32,
///   count i64;
//
// Internal rules are prefixed with @.
macro_rules! define_tables {
    // Base case for `@impl_ops` recursion: no more ops to define.
    (@impl_ops $name:ident { $(,)? } $($more:tt)*) => {};

    // Define a reducer for tables without unique constraints,
    // which inserts a row.
    (@impl_ops $name:ident
     { insert $insert:ident
       $(, $($ops:tt)* )? }
     $($field_name:ident $ty:ty),* $(,)*) => {
        paste::paste! {
            #[spacetimedb::reducer]
            pub fn $insert (ctx: &ReducerContext, $($field_name : $ty,)*) {
                ctx.db.[<$name:snake>]().insert($name { $($field_name,)* });
            }
        }

        define_tables!(@impl_ops $name { $($($ops)*)? } $($field_name $ty,)*);
    };

    // Define a reducer for tables without unique constraints,
    // which deletes a row.
    (@impl_ops $name:ident
     { delete $delete:ident
       $(, $($ops:tt)* )? }
     $($field_name:ident $ty:ty),* $(,)*) => {
        paste::paste! {
            #[spacetimedb::reducer]
            pub fn $delete (ctx: &ReducerContext, $($field_name : $ty,)*) {
                ctx.db.[<$name:snake>]().delete($name { $($field_name,)* });
            }
        }

        define_tables!(@impl_ops $name { $($($ops)*)? } $($field_name $ty,)*);
    };

    // Define a reducer for tables with unique constraints,
    // which inserts a row, or panics with `expect` if the row violates a unique constraint.
    (@impl_ops $name:ident
     { insert_or_panic $insert:ident
       $(, $($ops:tt)* )? }
     $($field_name:ident $ty:ty),* $(,)*) => {
        paste::paste! {
            #[spacetimedb::reducer]
            pub fn $insert (ctx: &ReducerContext, $($field_name : $ty,)*) {
                ctx.db.[<$name:snake>]().insert($name { $($field_name,)* });
            }
        }

        define_tables!(@impl_ops $name { $($($ops)*)? } $($field_name $ty,)*);
    };

    // Define a reducer for tables with a unique field,
    // which uses `$update_method` to update by that unique field.
    (@impl_ops $name:ident
     { update_by $update:ident = $update_method:ident($unique_field:ident)
       $(, $($ops:tt)* )? }
     $($field_name:ident $ty:ty),* $(,)*) => {
        paste::paste! {
            #[spacetimedb::reducer]
            pub fn $update (ctx: &ReducerContext, $($field_name : $ty,)*) {
                ctx.db.[<$name:snake>]().$unique_field().update($name { $($field_name,)* });
            }
        }

        define_tables!(@impl_ops $name { $($($ops)*)? } $($field_name $ty,)*);
    };

    // Define a reducer for tables with a unique field,
    // which uses `$delete_method` to delete by that unique field.
    (@impl_ops $name:ident
     { delete_by $delete:ident = $delete_method:ident($unique_field:ident : $unique_ty:ty)
       $(, $($ops:tt)*)? }
     $($other_fields:tt)* ) => {
        paste::paste! {
            #[spacetimedb::reducer]
            pub fn $delete (ctx: &ReducerContext, $unique_field : $unique_ty) {
                ctx.db.[<$name:snake>]().$unique_field().delete(&$unique_field);
            }
        }

        define_tables!(@impl_ops $name { $($($ops)*)? } $($other_fields)*);
    };

    // Define a table.
    (@one $name:ident { $($ops:tt)* } $($(#[$attr:meta])* $field_name:ident $ty:ty),* $(,)*) => {
        paste::paste! {
            #[spacetimedb::table(name = [<$name:snake>], public)]
            pub struct $name {
                $($(#[$attr])* pub $field_name : $ty,)*
            }
        }

        // Recursively implement reducers based on the `ops`.
        define_tables!(@impl_ops $name { $($ops)* } $($field_name $ty,)*);
    };

    // "Public" interface: Define many tables.
    ($($name:ident { $($ops:tt)* } $($(#[$attr:meta])* $field_name:ident $ty:ty),* $(,)*;)*) => {
        // Define each table one-by-one, iteratively.
        $(define_tables!(@one $name { $($ops)* } $($(#[$attr])* $field_name $ty,)*);)*
    };
}

// Tables holding a single value.
define_tables! {
    OneU8 { insert insert_one_u8 } n u8;
    OneU16 { insert insert_one_u16 } n u16;
    OneU32 { insert insert_one_u32 } n u32;
    OneU64 { insert insert_one_u64 } n u64;
    OneU128 { insert insert_one_u128 } n u128;
    OneU256 { insert insert_one_u256 } n u256;

    OneI8 { insert insert_one_i8 } n i8;
    OneI16 { insert insert_one_i16 } n i16;
    OneI32 { insert insert_one_i32 } n i32;
    OneI64 { insert insert_one_i64 } n i64;
    OneI128 { insert insert_one_i128 } n i128;
    OneI256 { insert insert_one_i256 } n i256;

    OneBool { insert insert_one_bool } b bool;

    OneF32 { insert insert_one_f32 } f f32;
    OneF64 { insert insert_one_f64 } f f64;

    OneString { insert insert_one_string } s String;

    OneIdentity { insert insert_one_identity } i Identity;
    OneConnectionId { insert insert_one_connection_id} a ConnectionId;

    OneTimestamp { insert insert_one_timestamp } t Timestamp;

    OneSimpleEnum { insert insert_one_simple_enum } e SimpleEnum;
    OneEnumWithPayload { insert insert_one_enum_with_payload } e EnumWithPayload;

    OneUnitStruct { insert insert_one_unit_struct } s UnitStruct;
    OneByteStruct { insert insert_one_byte_struct } s ByteStruct;
    OneEveryPrimitiveStruct { insert insert_one_every_primitive_struct } s EveryPrimitiveStruct;
    OneEveryVecStruct { insert insert_one_every_vec_struct } s EveryVecStruct;
}

// Tables holding a Vec of various types.
define_tables! {
    VecU8 { insert insert_vec_u8 } n Vec<u8>;
    VecU16 { insert insert_vec_u16 } n Vec<u16>;
    VecU32 { insert insert_vec_u32 } n Vec<u32>;
    VecU64 { insert insert_vec_u64 } n Vec<u64>;
    VecU128 { insert insert_vec_u128 } n Vec<u128>;
    VecU256 { insert insert_vec_u256 } n Vec<u256>;

    VecI8 { insert insert_vec_i8 } n Vec<i8>;
    VecI16 { insert insert_vec_i16 } n Vec<i16>;
    VecI32 { insert insert_vec_i32 } n Vec<i32>;
    VecI64 { insert insert_vec_i64 } n Vec<i64>;
    VecI128 { insert insert_vec_i128 } n Vec<i128>;
    VecI256 { insert insert_vec_i256 } n Vec<i256>;

    VecBool { insert insert_vec_bool } b Vec<bool>;

    VecF32 { insert insert_vec_f32 } f Vec<f32>;
    VecF64 { insert insert_vec_f64 } f Vec<f64>;

    VecString { insert insert_vec_string } s Vec<String>;

    VecIdentity { insert insert_vec_identity } i Vec<Identity>;
    VecConnectionId { insert insert_vec_connection_id} a Vec<ConnectionId>;

    VecTimestamp { insert insert_vec_timestamp } t Vec<Timestamp>;

    VecSimpleEnum { insert insert_vec_simple_enum } e Vec<SimpleEnum>;
    VecEnumWithPayload { insert insert_vec_enum_with_payload } e Vec<EnumWithPayload>;

    VecUnitStruct { insert insert_vec_unit_struct } s Vec<UnitStruct>;
    VecByteStruct { insert insert_vec_byte_struct } s Vec<ByteStruct>;
    VecEveryPrimitiveStruct { insert insert_vec_every_primitive_struct } s Vec<EveryPrimitiveStruct>;
    VecEveryVecStruct { insert insert_vec_every_vec_struct } s Vec<EveryVecStruct>;
}

// Tables holding an Option of various types.
define_tables! {
    OptionI32 { insert insert_option_i32 } n Option<i32>;
    OptionString { insert insert_option_string } s Option<String>;
    OptionIdentity { insert insert_option_identity } i Option<Identity>;
    OptionSimpleEnum { insert insert_option_simple_enum } e Option<SimpleEnum>;
    OptionEveryPrimitiveStruct { insert insert_option_every_primitive_struct } s Option<EveryPrimitiveStruct>;
    OptionVecOptionI32 { insert insert_option_vec_option_i32 } v Option<Vec<Option<i32>>>;
}

// Tables mapping a unique, but non-pk, key to a boring i32 payload.
// This allows us to test delete events, and the semantically correct absence of update events.
define_tables! {
    UniqueU8 {
        insert_or_panic insert_unique_u8,
        update_by update_unique_u8 = update_by_n(n),
        delete_by delete_unique_u8 = delete_by_n(n: u8),
    } #[unique] n u8, data i32;

    UniqueU16 {
        insert_or_panic insert_unique_u16,
        update_by update_unique_u16 = update_by_n(n),
        delete_by delete_unique_u16 = delete_by_n(n: u16),
    } #[unique] n u16, data i32;

    UniqueU32 {
        insert_or_panic insert_unique_u32,
        update_by update_unique_u32 = update_by_n(n),
        delete_by delete_unique_u32 = delete_by_n(n: u32),
    } #[unique] n u32, data i32;

    UniqueU64 {
        insert_or_panic insert_unique_u64,
        update_by update_unique_u64 = update_by_n(n),
        delete_by delete_unique_u64 = delete_by_n(n: u64),
    } #[unique] n u64, data i32;

    UniqueU128 {
        insert_or_panic insert_unique_u128,
        update_by update_unique_u128 = update_by_n(n),
        delete_by delete_unique_u128 = delete_by_n(n: u128),
    } #[unique] n u128, data i32;

    UniqueU256 {
        insert_or_panic insert_unique_u256,
        update_by update_unique_u256 = update_by_n(n),
        delete_by delete_unique_u256 = delete_by_n(n: u256),
    } #[unique] n u256, data i32;


    UniqueI8 {
        insert_or_panic insert_unique_i8,
        update_by update_unique_i8 = update_by_n(n),
        delete_by delete_unique_i8 = delete_by_n(n: i8),
    } #[unique] n i8, data i32;


    UniqueI16 {
        insert_or_panic insert_unique_i16,
        update_by update_unique_i16 = update_by_n(n),
        delete_by delete_unique_i16 = delete_by_n(n: i16),
    } #[unique] n i16, data i32;

    UniqueI32 {
        insert_or_panic insert_unique_i32,
        update_by update_unique_i32 = update_by_n(n),
        delete_by delete_unique_i32 = delete_by_n(n: i32),
    } #[unique] n i32, data i32;

    UniqueI64 {
        insert_or_panic insert_unique_i64,
        update_by update_unique_i64 = update_by_n(n),
        delete_by delete_unique_i64 = delete_by_n(n: i64),
    } #[unique] n i64, data i32;

    UniqueI128 {
        insert_or_panic insert_unique_i128,
        update_by update_unique_i128 = update_by_n(n),
        delete_by delete_unique_i128 = delete_by_n(n: i128),
    } #[unique] n i128, data i32;

    UniqueI256 {
        insert_or_panic insert_unique_i256,
        update_by update_unique_i256 = update_by_n(n),
        delete_by delete_unique_i256 = delete_by_n(n: i256),
    } #[unique] n i256, data i32;


    UniqueBool {
        insert_or_panic insert_unique_bool,
        update_by update_unique_bool = update_by_b(b),
        delete_by delete_unique_bool = delete_by_b(b: bool),
    } #[unique] b bool, data i32;

    UniqueString {
        insert_or_panic insert_unique_string,
        update_by update_unique_string = update_by_s(s),
        delete_by delete_unique_string = delete_by_s(s: String),
    } #[unique] s String, data i32;

    UniqueIdentity {
        insert_or_panic insert_unique_identity,
        update_by update_unique_identity = update_by_i(i),
        delete_by delete_unique_identity = delete_by_i(i: Identity),
    } #[unique] i Identity, data i32;

    UniqueConnectionId {
        insert_or_panic insert_unique_connection_id,
        update_by update_unique_connection_id = update_by_a(a),
        delete_by delete_unique_connection_id = delete_by_a(a: ConnectionId),
    } #[unique] a ConnectionId, data i32;
}

// Tables mapping a primary key to a boring i32 payload.
// This allows us to test update and delete events.
define_tables! {
    PkU8 {
        insert_or_panic insert_pk_u8,
        update_by update_pk_u8 = update_by_n(n),
        delete_by delete_pk_u8 = delete_by_n(n: u8),
    } #[primary_key] n u8, data i32;

    PkU16 {
        insert_or_panic insert_pk_u16,
        update_by update_pk_u16 = update_by_n(n),
        delete_by delete_pk_u16 = delete_by_n(n: u16),
    } #[primary_key] n u16, data i32;

    PkU32 {
        insert_or_panic insert_pk_u32,
        update_by update_pk_u32 = update_by_n(n),
        delete_by delete_pk_u32 = delete_by_n(n: u32),
    } #[primary_key] n u32, data i32;

    PkU32Two {
        insert_or_panic insert_pk_u32_two,
        update_by update_pk_u32_two = update_by_n(n),
        delete_by delete_pk_u32_two = delete_by_n(n: u32),
    } #[primary_key] n u32, data i32;

    PkU64 {
        insert_or_panic insert_pk_u64,
        update_by update_pk_u64 = update_by_n(n),
        delete_by delete_pk_u64 = delete_by_n(n: u64),
    } #[primary_key] n u64, data i32;

    PkU128 {
        insert_or_panic insert_pk_u128,
        update_by update_pk_u128 = update_by_n(n),
        delete_by delete_pk_u128 = delete_by_n(n: u128),
    } #[primary_key] n u128, data i32;

    PkU256 {
        insert_or_panic insert_pk_u256,
        update_by update_pk_u256 = update_by_n(n),
        delete_by delete_pk_u256 = delete_by_n(n: u256),
    } #[primary_key] n u256, data i32;

    PkI8 {
        insert_or_panic insert_pk_i8,
        update_by update_pk_i8 = update_by_n(n),
        delete_by delete_pk_i8 = delete_by_n(n: i8),
    } #[primary_key] n i8, data i32;

    PkI16 {
        insert_or_panic insert_pk_i16,
        update_by update_pk_i16 = update_by_n(n),
        delete_by delete_pk_i16 = delete_by_n(n: i16),
    } #[primary_key] n i16, data i32;

    PkI32 {
        insert_or_panic insert_pk_i32,
        update_by update_pk_i32 = update_by_n(n),
        delete_by delete_pk_i32 = delete_by_n(n: i32),
    } #[primary_key] n i32, data i32;

    PkI64 {
        insert_or_panic insert_pk_i64,
        update_by update_pk_i64 = update_by_n(n),
        delete_by delete_pk_i64 = delete_by_n(n: i64),
    } #[primary_key] n i64, data i32;

    PkI128 {
        insert_or_panic insert_pk_i128,
        update_by update_pk_i128 = update_by_n(n),
        delete_by delete_pk_i128 = delete_by_n(n: i128),
    } #[primary_key] n i128, data i32;

    PkI256 {
        insert_or_panic insert_pk_i256,
        update_by update_pk_i256 = update_by_n(n),
        delete_by delete_pk_i256 = delete_by_n(n: i256),
    } #[primary_key] n i256, data i32;

    PkBool {
        insert_or_panic insert_pk_bool,
        update_by update_pk_bool = update_by_b(b),
        delete_by delete_pk_bool = delete_by_b(b: bool),
    } #[primary_key] b bool, data i32;

    PkString {
        insert_or_panic insert_pk_string,
        update_by update_pk_string = update_by_s(s),
        delete_by delete_pk_string = delete_by_s(s: String),
    } #[primary_key] s String, data i32;

    PkIdentity {
        insert_or_panic insert_pk_identity,
        update_by update_pk_identity = update_by_i(i),
        delete_by delete_pk_identity = delete_by_i(i: Identity),
    } #[primary_key] i Identity, data i32;

    PkConnectionId {
        insert_or_panic insert_pk_connection_id,
        update_by update_pk_connection_id = update_by_a(a),
        delete_by delete_pk_connection_id = delete_by_a(a: ConnectionId),
    } #[primary_key] a ConnectionId, data i32;

    PkSimpleEnum {
        insert_or_panic insert_pk_simple_enum,
    } #[primary_key] a SimpleEnum, data i32;
}

#[spacetimedb::reducer]
fn update_pk_simple_enum(ctx: &ReducerContext, a: SimpleEnum, data: i32) -> anyhow::Result<()> {
    let Some(mut o) = ctx.db.pk_simple_enum().a().find(&a) else {
        return Err(anyhow!("row not found"));
    };
    o.data = data;
    ctx.db.pk_simple_enum().a().update(o);
    Ok(())
}

#[spacetimedb::reducer]
fn insert_into_btree_u32(ctx: &ReducerContext, rows: Vec<BTreeU32>) -> anyhow::Result<()> {
    for row in rows {
        ctx.db.btree_u32().insert(row);
    }
    Ok(())
}

#[spacetimedb::reducer]
fn delete_from_btree_u32(ctx: &ReducerContext, rows: Vec<BTreeU32>) -> anyhow::Result<()> {
    for row in rows {
        ctx.db.btree_u32().delete(row);
    }
    Ok(())
}

#[spacetimedb::reducer]
fn insert_into_pk_btree_u32(ctx: &ReducerContext, pk_u32: Vec<PkU32>, bt_u32: Vec<BTreeU32>) -> anyhow::Result<()> {
    for row in pk_u32 {
        ctx.db.pk_u32().insert(row);
    }
    for row in bt_u32 {
        ctx.db.btree_u32().insert(row);
    }
    Ok(())
}

/// The purpose of this reducer is for a test which
/// left-semijoins `UniqueU32` to `PkU32`
/// for the purposes of behavior testing row-deduplication.
#[spacetimedb::reducer]
fn insert_unique_u32_update_pk_u32(ctx: &ReducerContext, n: u32, d_unique: i32, d_pk: i32) -> anyhow::Result<()> {
    ctx.db.unique_u32().insert(UniqueU32 { n, data: d_unique });
    ctx.db.pk_u32().n().update(PkU32 { n, data: d_pk });
    Ok(())
}

/// The purpose of this reducer is for a test with two separate semijoin queries
/// - `UniqueU32` to `PkU32`
/// - `UniqueU32` to `PkU32Two`
///
/// for the purposes of behavior testing row-deduplication.
#[spacetimedb::reducer]
fn delete_pk_u32_insert_pk_u32_two(ctx: &ReducerContext, n: u32, data: i32) -> anyhow::Result<()> {
    ctx.db.pk_u32_two().insert(PkU32Two { n, data });
    ctx.db.pk_u32().delete(PkU32 { n, data });
    Ok(())
}

#[spacetimedb::reducer]
fn insert_caller_one_identity(ctx: &ReducerContext) -> anyhow::Result<()> {
    ctx.db.one_identity().insert(OneIdentity { i: ctx.sender });
    Ok(())
}

#[spacetimedb::reducer]
fn insert_caller_vec_identity(ctx: &ReducerContext) -> anyhow::Result<()> {
    ctx.db.vec_identity().insert(VecIdentity { i: vec![ctx.sender] });
    Ok(())
}

#[spacetimedb::reducer]
fn insert_caller_unique_identity(ctx: &ReducerContext, data: i32) -> anyhow::Result<()> {
    ctx.db.unique_identity().insert(UniqueIdentity { i: ctx.sender, data });
    Ok(())
}

#[spacetimedb::reducer]
fn insert_caller_pk_identity(ctx: &ReducerContext, data: i32) -> anyhow::Result<()> {
    ctx.db.pk_identity().insert(PkIdentity { i: ctx.sender, data });
    Ok(())
}

#[spacetimedb::reducer]
fn insert_caller_one_connection_id(ctx: &ReducerContext) -> anyhow::Result<()> {
    ctx.db.one_connection_id().insert(OneConnectionId {
        a: ctx.connection_id.context("No connection id in reducer context")?,
    });
    Ok(())
}

#[spacetimedb::reducer]
fn insert_caller_vec_connection_id(ctx: &ReducerContext) -> anyhow::Result<()> {
    ctx.db.vec_connection_id().insert(VecConnectionId {
        a: vec![ctx.connection_id.context("No connection id in reducer context")?],
    });
    Ok(())
}

#[spacetimedb::reducer]
fn insert_caller_unique_connection_id(ctx: &ReducerContext, data: i32) -> anyhow::Result<()> {
    ctx.db.unique_connection_id().insert(UniqueConnectionId {
        a: ctx.connection_id.context("No connection id in reducer context")?,
        data,
    });
    Ok(())
}

#[spacetimedb::reducer]
fn insert_caller_pk_connection_id(ctx: &ReducerContext, data: i32) -> anyhow::Result<()> {
    ctx.db.pk_connection_id().insert(PkConnectionId {
        a: ctx.connection_id.context("No connection id in reducer context")?,
        data,
    });
    Ok(())
}

#[spacetimedb::reducer]
fn insert_call_timestamp(ctx: &ReducerContext) {
    ctx.db.one_timestamp().insert(OneTimestamp { t: ctx.timestamp });
}

#[spacetimedb::reducer]
fn insert_primitives_as_strings(ctx: &ReducerContext, s: EveryPrimitiveStruct) {
    ctx.db.vec_string().insert(VecString {
        s: vec![
            s.a.to_string(),
            s.b.to_string(),
            s.c.to_string(),
            s.d.to_string(),
            s.e.to_string(),
            s.f.to_string(),
            s.g.to_string(),
            s.h.to_string(),
            s.i.to_string(),
            s.j.to_string(),
            s.k.to_string(),
            s.l.to_string(),
            s.m.to_string(),
            s.n.to_string(),
            s.o.to_string(),
            s.p.to_string(),
            s.q.to_string(),
            s.r.to_string(),
            s.s.to_string(),
            s.t.to_string(),
        ],
    });
}

// Some weird-looking tables.
define_tables! {
    // A table with many fields, of many different types.
    LargeTable {
        insert insert_large_table,
        delete delete_large_table,
    }
    a u8,
    b u16,
    c u32,
    d u64,
    e u128,
    f u256,
    g i8,
    h i16,
    i i32,
    j i64,
    k i128,
    l i256,
    m bool,
    n f32,
    o f64,
    p String,
    q SimpleEnum,
    r EnumWithPayload,
    s UnitStruct,
    t ByteStruct,
    u EveryPrimitiveStruct,
    v EveryVecStruct,
    ;

    // A table which holds instances of other table structs.
    // This tests that we can use tables as types.
    TableHoldsTable {
        insert insert_table_holds_table,
    }
    a OneU8,
    b VecU8,
    ;
}

#[spacetimedb::reducer]
fn no_op_succeeds(_ctx: &ReducerContext) {}

#[spacetimedb::client_visibility_filter]
const ONE_U8_VISIBLE: spacetimedb::Filter = spacetimedb::Filter::Sql("SELECT * FROM one_u8");

#[spacetimedb::table(name = scheduled_table, scheduled(send_scheduled_message), public)]
pub struct ScheduledTable {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: spacetimedb::ScheduleAt,
    text: String,
}

#[spacetimedb::reducer]
fn send_scheduled_message(_ctx: &ReducerContext, arg: ScheduledTable) {
    let _ = arg.text;
    let _ = arg.scheduled_at;
    let _ = arg.scheduled_id;
}

#[spacetimedb::table(name = indexed_table)]
struct IndexedTable {
    #[index(btree)]
    player_id: u32,
}

#[spacetimedb::table(name = indexed_table_2, index(name=player_id_snazz_index, btree(columns = [player_id, player_snazz])))]
struct IndexedTable2 {
    player_id: u32,
    player_snazz: f32,
}

#[spacetimedb::table(name = btree_u32, public)]
struct BTreeU32 {
    #[index(btree)]
    n: u32,
    data: i32,
}

#[spacetimedb::table(name = indexed_simple_enum, public)]
struct IndexedSimpleEnum {
    #[index(btree)]
    n: SimpleEnum,
}

#[spacetimedb::reducer]
fn insert_into_indexed_simple_enum(ctx: &ReducerContext, n: SimpleEnum) -> anyhow::Result<()> {
    ctx.db.indexed_simple_enum().insert(IndexedSimpleEnum { n });
    Ok(())
}

#[spacetimedb::reducer]
fn update_indexed_simple_enum(ctx: &ReducerContext, a: SimpleEnum, b: SimpleEnum) -> anyhow::Result<()> {
    if ctx.db.indexed_simple_enum().n().filter(&a).next().is_some() {
        ctx.db.indexed_simple_enum().n().delete(&a);
        ctx.db.indexed_simple_enum().insert(IndexedSimpleEnum { n: b });
    }
    Ok(())
}
