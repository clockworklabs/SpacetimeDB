#[macro_use]
pub mod io;
mod impls;

use spacetimedb_lib::buffer::{BufReader, BufWriter, Cursor, DecodeError};
use spacetimedb_lib::type_def::TableDef;
use spacetimedb_lib::{PrimaryKey, TupleDef, TupleValue, TypeDef};
use std::alloc::{alloc as _alloc, dealloc as _dealloc, Layout};
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

#[doc(hidden)]
pub mod __private {
    pub use once_cell::sync::OnceCell;
}

// #[cfg(target_arch = "wasm32")]
// #[global_allocator]
// static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

extern "C" {
    fn _create_table(ptr: *mut u8, len: usize) -> u32;
    fn _get_table_id(ptr: *mut u8, len: usize) -> u32;
    fn _create_index(table_id: u32, col_id: u32, index_type: u8);

    fn _insert(table_id: u32, ptr: *mut u8, len: usize);

    fn _delete_pk(table_id: u32, ptr: *mut u8, len: usize) -> u8;
    fn _delete_value(table_id: u32, ptr: *mut u8, len: usize) -> u8;
    fn _delete_eq(table_id: u32, col_id: u32, ptr: *mut u8, len: usize) -> i32;
    fn _delete_range(table_id: u32, col_id: u32, ptr: *mut u8, len: usize) -> i32;

    // TODO: should have lens associated with ptrs
    fn _filter_eq(table_id: u32, col_id: u32, src_ptr: *mut u8, result_ptr: *mut u8);

    fn _iter(table_id: u32) -> u64;
    fn _console_log(level: u8, ptr: *const u8, len: u32);
}

// this gets optimized away to a normal global since wasm32 doesn't have threads by default
thread_local! {
    static ROW_BUF: RefCell<Vec<u8>> = RefCell::new(Vec::with_capacity(8 * 1024));
}

#[no_mangle]
extern "C" fn alloc(size: usize) -> *mut u8 {
    let align = std::mem::align_of::<usize>();
    unsafe {
        let layout = Layout::from_size_align_unchecked(size, align);
        _alloc(layout)
    }
}

#[no_mangle]
extern "C" fn dealloc(ptr: *mut u8, size: usize) {
    let align = std::mem::align_of::<usize>();
    unsafe {
        let layout = Layout::from_size_align_unchecked(size, align);
        _dealloc(ptr, layout);
    }
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

        unsafe { _create_table(bytes.as_mut_ptr(), bytes.len()) }
    })
}

pub fn get_table_id(table_name: &str) -> u32 {
    with_row_buf(|bytes| {
        let table_name = TypeValue::String(table_name.to_string());
        table_name.encode(bytes);

        unsafe { _get_table_id(bytes.as_mut_ptr(), bytes.len()) }
    })
}

pub fn insert(table_id: u32, row: TupleValue) {
    with_row_buf(|bytes| {
        row.encode(bytes);
        unsafe { _insert(table_id, bytes.as_mut_ptr(), bytes.len()) }
    })
}

pub fn delete_pk(table_id: u32, primary_key: PrimaryKey) -> Option<usize> {
    with_row_buf(|bytes| {
        primary_key.encode(bytes);
        let result = unsafe { _delete_pk(table_id, bytes.as_mut_ptr(), bytes.len()) };
        (result != 0).then_some(1)
    })
}

pub fn delete_filter<F: Fn(&TupleValue) -> bool>(table_id: u32, f: F) -> Option<usize> {
    with_row_buf(|bytes| {
        let mut count = 0;
        for tuple_value in __iter__(table_id).unwrap() {
            if f(&tuple_value) {
                count += 1;
                tuple_value.encode(bytes);
                if unsafe { _delete_value(table_id, bytes.as_mut_ptr(), bytes.len()) } == 0 {
                    panic!("Something ain't right.");
                }
            }
        }
        Some(count)
    })
}

pub fn delete_eq(table_id: u32, col_id: u8, eq_value: TypeValue) -> Option<usize> {
    with_row_buf(|bytes| {
        eq_value.encode(bytes);
        let result = unsafe { _delete_eq(table_id, col_id.into(), bytes.as_mut_ptr(), bytes.len()) };
        (result != -1).then_some(result as usize)
    })
}

pub fn delete_range(table_id: u32, col_id: u8, range: Range<TypeValue>) -> Option<usize> {
    with_row_buf(|bytes| {
        let start = TypeValue::from(range.start);
        let end = TypeValue::from(range.end);
        let tuple = TupleValue {
            elements: vec![start, end].into(),
        };
        tuple.encode(bytes);
        let result = unsafe { _delete_range(table_id, col_id.into(), bytes.as_mut_ptr(), bytes.len()) };
        if result == -1 {
            return None;
        }
        return Some(result as usize);
    })
}

pub fn create_index(_table_id: u32, _index_type: u8, _col_ids: Vec<u8>) {}

// TODO: going to have to somehow ensure TypeValue is equatable
pub fn filter_eq(_table_id: u32, _col_id: u8, _eq_value: TypeValue) -> Option<TupleValue> {
    return None;
}

//
// fn page_table(table_id : u32, pager_token : u32, read_entries : u32) {
//
// }

pub fn __iter__(table_id: u32) -> Option<TableIter> {
    let data = unsafe { _iter(table_id) };
    let ptr = (data >> 32) as u32 as *mut u8;
    let size = data as u32;
    let bytes: Vec<u8> = unsafe { Vec::from_raw_parts(ptr, size as usize, size as usize) };

    let mut buffer = Cursor::new(bytes);
    let schema = decode_schema(&mut buffer);
    let schema = schema.unwrap_or_else(|e| {
        panic!("__iter__: Could not decode schema. Err: {}", e);
    });

    Some(TableIter { buffer, schema })
}

pub struct TableIter {
    buffer: Cursor<Vec<u8>>,
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

    fn create_table() -> u32 {
        let tuple_def = Self::get_tupledef();
        create_table(Self::TABLE_NAME, tuple_def)
    }

    fn get_tabledef() -> TableDef {
        TableDef {
            tuple: Self::get_tupledef(),
            unique_columns: Self::UNIQUE_COLUMNS.to_owned(),
        }
    }

    fn describe_table() -> u64 {
        let table_def = Self::get_tabledef();
        let mut bytes = vec![];
        table_def.encode(&mut bytes);
        let offset = bytes.as_ptr() as u64;
        let length = bytes.len() as u64;
        std::mem::forget(bytes);
        offset << 32 | length
    }
}

pub trait FilterableValue: FromValue + IntoValue {
    fn equals(&self, other: &TypeValue) -> bool;
}

pub trait UniqueValue: FilterableValue {
    fn into_primarykey(self) -> PrimaryKey;
}
