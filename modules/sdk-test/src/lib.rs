// Some of our tests include reducers with large numbers of arguments.
// This is on purpose.
// Due to a clippy bug (as of 2023-08-31),
// we cannot locally disable the lint around the definitions of those reducers,
// because the definitions are macro-generated,
// and clippy misunderstands `#[allow]` attributes in macro-expansions.
#![allow(clippy::too_many_arguments)]

use anyhow::{Context, Result};
use spacetimedb::{spacetimedb, Address, Identity, ReducerContext, SpacetimeType};

#[derive(SpacetimeType)]
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
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    Bool(bool),
    F32(f32),
    F64(f64),
    Str(String),
    Identity(Identity),
    Address(Address),
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
    f: i8,
    g: i16,
    h: i32,
    i: i64,
    j: i128,
    k: bool,
    l: f32,
    m: f64,
    n: String,
    o: Identity,
    p: Address,
}

#[derive(SpacetimeType)]
pub struct EveryVecStruct {
    a: Vec<u8>,
    b: Vec<u16>,
    c: Vec<u32>,
    d: Vec<u64>,
    e: Vec<u128>,
    f: Vec<i8>,
    g: Vec<i16>,
    h: Vec<i32>,
    i: Vec<i64>,
    j: Vec<i128>,
    k: Vec<bool>,
    l: Vec<f32>,
    m: Vec<f64>,
    n: Vec<String>,
    o: Vec<Identity>,
    p: Vec<Address>,
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
/// } #[primarykey] name String,
///   #[autoinc] #[unique] id u32,
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
        #[spacetimedb(reducer)]
        pub fn $insert ($($field_name : $ty,)*) {
            $name::insert($name { $($field_name,)* });
        }

        define_tables!(@impl_ops $name { $($($ops)*)? } $($field_name $ty,)*);
    };

    // Define a reducer for tables with unique constraints,
    // which inserts a row, or panics with `expect` if the row violates a unique constraint.
    (@impl_ops $name:ident
     { insert_or_panic $insert:ident
       $(, $($ops:tt)* )? }
     $($field_name:ident $ty:ty),* $(,)*) => {
        #[spacetimedb(reducer)]
        pub fn $insert ($($field_name : $ty,)*) {
            $name::insert($name { $($field_name,)* }).expect(concat!("Failed to insert row for table: ", stringify!($name)));
        }

        define_tables!(@impl_ops $name { $($($ops)*)? } $($field_name $ty,)*);
    };

    // Define a reducer for tables with a unique field,
    // which uses `$update_method` to update by that unique field.
    (@impl_ops $name:ident
     { update_by $update:ident = $update_method:ident($unique_field:ident)
       $(, $($ops:tt)* )? }
     $($field_name:ident $ty:ty),* $(,)*) => {
        #[spacetimedb(reducer)]
        pub fn $update ($($field_name : $ty,)*) {
            let key = $unique_field.clone();
            $name::$update_method(&key, $name { $($field_name,)* });
        }

        define_tables!(@impl_ops $name { $($($ops)*)? } $($field_name $ty,)*);
    };

    // Define a reducer for tables with a unique field,
    // which uses `$delete_method` to delete by that unique field.
    (@impl_ops $name:ident
     { delete_by $delete:ident = $delete_method:ident($unique_field:ident : $unique_ty:ty)
       $(, $($ops:tt)*)? }
     $($other_fields:tt)* ) => {
        #[spacetimedb(reducer)]
        pub fn $delete ($unique_field : $unique_ty) {
            $name::$delete_method(&$unique_field);
        }

        define_tables!(@impl_ops $name { $($($ops)*)? } $($other_fields)*);
    };

    // Define a table.
    (@one $name:ident { $($ops:tt)* } $($(#[$attr:meta])* $field_name:ident $ty:ty),* $(,)*) => {
        #[spacetimedb(table)]
        pub struct $name {
            $($(#[$attr])* pub $field_name : $ty,)*
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

    OneI8 { insert insert_one_i8 } n i8;
    OneI16 { insert insert_one_i16 } n i16;
    OneI32 { insert insert_one_i32 } n i32;
    OneI64 { insert insert_one_i64 } n i64;
    OneI128 { insert insert_one_i128 } n i128;

    OneBool { insert insert_one_bool } b bool;

    OneF32 { insert insert_one_f32 } f f32;
    OneF64 { insert insert_one_f64 } f f64;

    OneString { insert insert_one_string } s String;

    OneIdentity { insert insert_one_identity } i Identity;
    OneAddress { insert insert_one_address } a Address;

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

    VecI8 { insert insert_vec_i8 } n Vec<i8>;
    VecI16 { insert insert_vec_i16 } n Vec<i16>;
    VecI32 { insert insert_vec_i32 } n Vec<i32>;
    VecI64 { insert insert_vec_i64 } n Vec<i64>;
    VecI128 { insert insert_vec_i128 } n Vec<i128>;

    VecBool { insert insert_vec_bool } b Vec<bool>;

    VecF32 { insert insert_vec_f32 } f Vec<f32>;
    VecF64 { insert insert_vec_f64 } f Vec<f64>;

    VecString { insert insert_vec_string } s Vec<String>;

    VecIdentity { insert insert_vec_identity } i Vec<Identity>;
    VecAddress { insert insert_vec_address } a Vec<Address>;

    VecSimpleEnum { insert insert_vec_simple_enum } e Vec<SimpleEnum>;
    VecEnumWithPayload { insert insert_vec_enum_with_payload } e Vec<EnumWithPayload>;

    VecUnitStruct { insert insert_vec_unit_struct } s Vec<UnitStruct>;
    VecByteStruct { insert insert_vec_byte_struct } s Vec<ByteStruct>;
    VecEveryPrimitiveStruct { insert insert_vec_every_primitive_struct } s Vec<EveryPrimitiveStruct>;
    VecEveryVecStruct { insert insert_vec_every_vec_struct } s Vec<EveryVecStruct>;
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

    UniqueAddress {
        insert_or_panic insert_unique_address,
        update_by update_unique_address = update_by_a(a),
        delete_by delete_unique_address = delete_by_a(a: Address),
    } #[unique] a Address, data i32;
}

// Tables mapping a primary key to a boring i32 payload.
// This allows us to test update and delete events.
define_tables! {
    PkU8 {
        insert_or_panic insert_pk_u8,
        update_by update_pk_u8 = update_by_n(n),
        delete_by delete_pk_u8 = delete_by_n(n: u8),
    } #[primarykey] n u8, data i32;

    PkU16 {
        insert_or_panic insert_pk_u16,
        update_by update_pk_u16 = update_by_n(n),
        delete_by delete_pk_u16 = delete_by_n(n: u16),
    } #[primarykey] n u16, data i32;

    PkU32 {
        insert_or_panic insert_pk_u32,
        update_by update_pk_u32 = update_by_n(n),
        delete_by delete_pk_u32 = delete_by_n(n: u32),
    } #[primarykey] n u32, data i32;

    PkU64 {
        insert_or_panic insert_pk_u64,
        update_by update_pk_u64 = update_by_n(n),
        delete_by delete_pk_u64 = delete_by_n(n: u64),
    } #[primarykey] n u64, data i32;

    PkU128 {
        insert_or_panic insert_pk_u128,
        update_by update_pk_u128 = update_by_n(n),
        delete_by delete_pk_u128 = delete_by_n(n: u128),
    } #[primarykey] n u128, data i32;

    PkI8 {
        insert_or_panic insert_pk_i8,
        update_by update_pk_i8 = update_by_n(n),
        delete_by delete_pk_i8 = delete_by_n(n: i8),
    } #[primarykey] n i8, data i32;

    PkI16 {
        insert_or_panic insert_pk_i16,
        update_by update_pk_i16 = update_by_n(n),
        delete_by delete_pk_i16 = delete_by_n(n: i16),
    } #[primarykey] n i16, data i32;

    PkI32 {
        insert_or_panic insert_pk_i32,
        update_by update_pk_i32 = update_by_n(n),
        delete_by delete_pk_i32 = delete_by_n(n: i32),
    } #[primarykey] n i32, data i32;

    PkI64 {
        insert_or_panic insert_pk_i64,
        update_by update_pk_i64 = update_by_n(n),
        delete_by delete_pk_i64 = delete_by_n(n: i64),
    } #[primarykey] n i64, data i32;

    PkI128 {
        insert_or_panic insert_pk_i128,
        update_by update_pk_i128 = update_by_n(n),
        delete_by delete_pk_i128 = delete_by_n(n: i128),
    } #[primarykey] n i128, data i32;

    PkBool {
        insert_or_panic insert_pk_bool,
        update_by update_pk_bool = update_by_b(b),
        delete_by delete_pk_bool = delete_by_b(b: bool),
    } #[primarykey] b bool, data i32;

    PkString {
        insert_or_panic insert_pk_string,
        update_by update_pk_string = update_by_s(s),
        delete_by delete_pk_string = delete_by_s(s: String),
    } #[primarykey] s String, data i32;

    PkIdentity {
        insert_or_panic insert_pk_identity,
        update_by update_pk_identity = update_by_i(i),
        delete_by delete_pk_identity = delete_by_i(i: Identity),
    } #[primarykey] i Identity, data i32;

    PkAddress {
        insert_or_panic insert_pk_address,
        update_by update_pk_address = update_by_a(a),
        delete_by delete_pk_address = delete_by_a(a: Address),
    } #[primarykey] a Address, data i32;
}

#[spacetimedb(reducer)]
fn insert_caller_one_identity(ctx: ReducerContext) -> anyhow::Result<()> {
    OneIdentity::insert(OneIdentity { i: ctx.sender });
    Ok(())
}

#[spacetimedb(reducer)]
fn insert_caller_vec_identity(ctx: ReducerContext) -> anyhow::Result<()> {
    VecIdentity::insert(VecIdentity { i: vec![ctx.sender] });
    Ok(())
}

#[spacetimedb(reducer)]
fn insert_caller_unique_identity(ctx: ReducerContext, data: i32) -> anyhow::Result<()> {
    UniqueIdentity::insert(UniqueIdentity { i: ctx.sender, data })?;
    Ok(())
}

#[spacetimedb(reducer)]
fn insert_caller_pk_identity(ctx: ReducerContext, data: i32) -> anyhow::Result<()> {
    PkIdentity::insert(PkIdentity { i: ctx.sender, data })?;
    Ok(())
}

#[spacetimedb(reducer)]
fn insert_caller_one_address(ctx: ReducerContext) -> anyhow::Result<()> {
    OneAddress::insert(OneAddress {
        a: ctx.address.context("No address in reducer context")?,
    });
    Ok(())
}

#[spacetimedb(reducer)]
fn insert_caller_vec_address(ctx: ReducerContext) -> anyhow::Result<()> {
    VecAddress::insert(VecAddress {
        a: vec![ctx.address.context("No address in reducer context")?],
    });
    Ok(())
}

#[spacetimedb(reducer)]
fn insert_caller_unique_address(ctx: ReducerContext, data: i32) -> anyhow::Result<()> {
    UniqueAddress::insert(UniqueAddress {
        a: ctx.address.context("No address in reducer context")?,
        data,
    })?;
    Ok(())
}

#[spacetimedb(reducer)]
fn insert_caller_pk_address(ctx: ReducerContext, data: i32) -> anyhow::Result<()> {
    PkAddress::insert(PkAddress {
        a: ctx.address.context("No address in reducer context")?,
        data,
    })?;
    Ok(())
}

// Some weird-looking tables.
define_tables! {
    // A table with many fields, of many different types.
    LargeTable {
        insert insert_large_table,
    }
    a u8,
    b u16,
    c u32,
    d u64,
    e u128,
    f i8,
    g i16,
    h i32,
    i i64,
    j i128,
    k bool,
    l f32,
    m f64,
    n String,
    o SimpleEnum,
    p EnumWithPayload,
    q UnitStruct,
    r ByteStruct,
    s EveryPrimitiveStruct,
    t EveryVecStruct,
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
