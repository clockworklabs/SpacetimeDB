#[macro_use]
pub mod io;
mod impls;

use spacetimedb_lib::buffer::{BufReader, BufWriter, Cursor, DecodeError};
use spacetimedb_lib::type_def::TableDef;
use spacetimedb_lib::{PrimaryKey, TupleDef, TupleValue, TypeDef};
use std::cell::RefCell;
use std::ops::Range;
use std::panic;

#[cfg(feature = "macro")]
pub use spacetimedb_bindgen;
#[cfg(feature = "macro")]
pub use spacetimedb_bindgen::spacetimedb;

pub use spacetimedb_lib;
pub use spacetimedb_lib::hash;
pub use spacetimedb_lib::Hash;
pub use spacetimedb_lib::TypeValue;

pub use spacetimedb_sys as sys;

#[doc(hidden)]
pub mod __private {
    pub use once_cell::sync::OnceCell;
}

// #[cfg(target_arch = "wasm32")]
// #[global_allocator]
// static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

// this gets optimized away to a normal global since wasm32 doesn't have threads by default
thread_local! {
    static ROW_BUF: RefCell<Vec<u8>> = RefCell::new(Vec::with_capacity(8 * 1024));
}

fn with_row_buf<R>(f: impl FnOnce(&mut Vec<u8>) -> R) -> R {
    ROW_BUF.with(|r| {
        let mut buf = r.borrow_mut();
        buf.clear();
        f(&mut buf)
    })
}

pub fn encode_row(row: TupleValue, bytes: &mut impl BufWriter) {
    row.encode(bytes);
}

pub fn decode_row(schema: &TupleDef, bytes: &mut impl BufReader) -> Result<TupleValue, DecodeError> {
    TupleValue::decode(schema, bytes)
}

pub fn encode_schema(schema: TupleDef, bytes: &mut impl BufWriter) {
    schema.encode(bytes);
}

pub fn decode_schema(bytes: &mut impl BufReader) -> Result<TupleDef, DecodeError> {
    TupleDef::decode(bytes)
}

pub fn create_table(table_name: &str, schema: TupleDef) -> u32 {
    with_row_buf(|bytes| {
        let mut schema_bytes = Vec::new();
        schema.encode(&mut schema_bytes);

        let table_info = TupleValue {
            elements: vec![
                TypeValue::String(table_name.to_string()),
                TypeValue::Bytes(schema_bytes),
            ]
            .into(),
        };

        table_info.encode(bytes);

        sys::create_table(&bytes)
    })
}

pub fn get_table_id(table_name: &str) -> u32 {
    with_row_buf(|bytes| {
        let table_name = TypeValue::String(table_name.to_string());
        table_name.encode(bytes);

        sys::get_table_id(&bytes)
    })
}

pub fn insert(table_id: u32, row: TupleValue) {
    with_row_buf(|bytes| {
        row.encode(bytes);
        sys::insert(table_id, &bytes)
    })
}

// TODO: these return types should be fixed up, turned into Results

pub fn delete_pk(table_id: u32, primary_key: PrimaryKey) -> Option<usize> {
    with_row_buf(|bytes| {
        primary_key.encode(bytes);
        sys::delete_pk(table_id, &bytes).ok().map(|()| 1)
    })
}

pub fn delete_filter<F: Fn(&TupleValue) -> bool>(table_id: u32, f: F) -> Option<usize> {
    with_row_buf(|bytes| {
        let mut count = 0;
        for tuple_value in __iter__(table_id).unwrap() {
            if f(&tuple_value) {
                count += 1;
                bytes.clear();
                tuple_value.encode(bytes);
                if let Err(_) = sys::delete_value(table_id, &bytes) {
                    panic!("Something ain't right.");
                }
            }
        }
        Some(count)
    })
}

pub fn delete_eq(table_id: u32, col_id: u8, eq_value: TypeValue) -> Option<u32> {
    with_row_buf(|bytes| {
        eq_value.encode(bytes);
        sys::delete_eq(table_id, col_id.into(), &bytes).ok()
    })
}

pub fn delete_range(table_id: u32, col_id: u8, range: Range<TypeValue>) -> Option<u32> {
    with_row_buf(|bytes| {
        let start = TypeValue::from(range.start);
        let end = TypeValue::from(range.end);
        let tuple = TupleValue {
            elements: vec![start, end].into(),
        };
        tuple.encode(bytes);
        sys::delete_range(table_id, col_id.into(), &bytes).ok()
    })
}

pub fn create_index(_table_id: u32, _index_type: u8, _col_ids: Vec<u8>) {}

// TODO: going to have to somehow ensure TypeValue is equatable
// pub fn filter_eq(_table_id: u32, _col_id: u8, _eq_value: TypeValue) -> Option<TupleValue> {
//     return None;
// }

//
// fn page_table(table_id : u32, pager_token : u32, read_entries : u32) {
//
// }

pub fn __iter__(table_id: u32) -> Option<TableIter> {
    let bytes = sys::iter(table_id);

    let mut buffer = Cursor::new(bytes);
    let schema = decode_schema(&mut buffer);
    let schema = schema.unwrap_or_else(|e| {
        panic!("__iter__: Could not decode schema. Err: {}", e);
    });

    Some(TableIter { buffer, schema })
}

pub struct TableIter {
    buffer: Cursor<Box<[u8]>>,
    schema: TupleDef,
}

impl Iterator for TableIter {
    type Item = TupleValue;

    fn next(&mut self) -> Option<Self::Item> {
        if self.buffer.remaining() == 0 {
            return None;
        }
        let row = decode_row(&self.schema, &mut self.buffer).unwrap_or_else(|e| {
            panic!("TableIter::next: Failed to decode row! Err: {}", e);
        });
        Some(row)
    }
}

pub trait SchemaType: Sized + 'static {
    fn get_schema() -> TypeDef;
}

pub trait FromValue: SchemaType {
    fn from_value(v: TypeValue) -> Option<Self>;
}

pub trait IntoValue: SchemaType {
    fn into_value(self) -> TypeValue;
}

pub trait TupleType: Sized + 'static {
    fn get_tupledef() -> TupleDef;

    #[doc(hidden)]
    fn describe_tuple() -> u64 {
        // const _: () = assert!(std::mem::size_of::<usize>() == std::mem::size_of::<u32>());
        let tuple_def = Self::get_tupledef();
        let mut bytes = vec![];
        tuple_def.encode(&mut bytes);
        let offset = bytes.as_ptr() as u64;
        let length = bytes.len() as u64;
        std::mem::forget(bytes);
        offset << 32 | length
    }
}

impl<T: TupleType> SchemaType for T {
    fn get_schema() -> TypeDef {
        TypeDef::Tuple(T::get_tupledef())
    }
}

pub trait FromTuple: TupleType {
    fn from_tuple(v: TupleValue) -> Option<Self>;
}

pub trait IntoTuple: TupleType {
    fn into_tuple(self) -> TupleValue;
}

impl<T: FromTuple> FromValue for T {
    fn from_value(v: TypeValue) -> Option<Self> {
        match v {
            TypeValue::Tuple(v) => Self::from_tuple(v),
            _ => None,
        }
    }
}
impl<T: IntoTuple> IntoValue for T {
    fn into_value(self) -> TypeValue {
        TypeValue::Tuple(self.into_tuple())
    }
}

pub trait TableType: TupleType + FromTuple + IntoTuple {
    const TABLE_NAME: &'static str;
    const UNIQUE_COLUMNS: &'static [u8];
    const FILTERABLE_BY_COLUMNS: &'static [u8];

    fn create_table() -> u32 {
        let tuple_def = Self::get_tupledef();
        create_table(Self::TABLE_NAME, tuple_def)
    }

    fn get_tabledef() -> TableDef {
        TableDef {
            tuple: Self::get_tupledef(),
            unique_columns: Self::UNIQUE_COLUMNS.to_owned(),
            filterable_by_columns: Self::FILTERABLE_BY_COLUMNS.to_owned(),
        }
    }

    fn describe_table() -> u64 {
        let table_def = Self::get_tabledef();
        let mut bytes = vec![];
        table_def.encode(&mut bytes);
        sys::pack_slice(bytes.into())
    }
}

pub trait FilterableValue: FromValue + IntoValue {
    fn equals(&self, other: &TypeValue) -> bool;
}

pub trait UniqueValue: FilterableValue {
    fn into_primarykey(self) -> PrimaryKey;
}
