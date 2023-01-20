#[macro_use]
pub mod io;
mod impls;
#[doc(hidden)]
pub mod rt;
mod types;

use once_cell::sync::OnceCell;
use spacetimedb_lib::buffer::{BufReader, BufWriter, Cursor, DecodeError};
use spacetimedb_lib::type_def::TableDef;
use spacetimedb_lib::{PrimaryKey, TupleDef, TupleValue, TypeDef};
use std::cell::RefCell;
use std::marker::PhantomData;
use std::ops::Range;
use std::panic;

#[cfg(feature = "macro")]
pub use spacetimedb_bindings_macro::{duration, spacetimedb};

pub use spacetimedb_lib;
pub use spacetimedb_lib::hash;
pub use spacetimedb_lib::Hash;
pub use spacetimedb_lib::TypeValue;
pub use types::Timestamp;

pub use serde_json;

pub use spacetimedb_bindings_sys as sys;
pub use sys::Errno;

pub type Result<T = (), E = Errno> = core::result::Result<T, E>;

#[no_mangle]
static SPACETIME_ABI_VERSION: u32 = (spacetimedb_lib::SCHEMA_FORMAT_VERSION as u32) << 16 | sys::ABI_VERSION as u32;
#[no_mangle]
static SPACETIME_ABI_VERSION_IS_ADDR: () = ();

#[non_exhaustive]
pub struct ReducerContext {
    pub sender: Hash,
    pub timestamp: Timestamp,
}

impl ReducerContext {
    #[doc(hidden)]
    pub fn __dummy() -> Self {
        Self {
            sender: Hash { data: [0; 32] },
            timestamp: Timestamp::UNIX_EPOCH,
        }
    }
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

pub fn create_table(table_name: &str, schema: TupleDef) -> Result<u32> {
    with_row_buf(|bytes| {
        schema.encode(bytes);
        sys::create_table(table_name, bytes)
    })
}

pub fn get_table_id(table_name: &str) -> u32 {
    sys::get_table_id(table_name).unwrap_or_else(|_| {
        panic!("Failed to get table with name: {}", table_name);
    })
}

pub fn insert(table_id: u32, row: TupleValue) -> Result<()> {
    with_row_buf(|bytes| {
        row.encode(bytes);
        sys::insert(table_id, bytes)
    })
}

// TODO: these return types should be fixed up, turned into Results

pub fn delete_pk(table_id: u32, primary_key: PrimaryKey) -> Result<()> {
    with_row_buf(|bytes| {
        primary_key.encode(bytes);
        sys::delete_pk(table_id, bytes)
    })
}

pub fn delete_filter<F: Fn(&TupleValue) -> bool>(table_id: u32, f: F) -> Result<usize> {
    with_row_buf(|bytes| {
        let mut count = 0;
        for tuple_value in __iter__(table_id)? {
            if f(&tuple_value) {
                count += 1;
                bytes.clear();
                tuple_value.encode(bytes);
                sys::delete_value(table_id, bytes)?;
            }
        }
        Ok(count)
    })
}

pub fn delete_eq(table_id: u32, col_id: u8, eq_value: TypeValue) -> Option<u32> {
    with_row_buf(|bytes| {
        eq_value.encode(bytes);
        sys::delete_eq(table_id, col_id.into(), bytes).ok()
    })
}

pub fn delete_range(table_id: u32, col_id: u8, range: Range<TypeValue>) -> Result<u32> {
    with_row_buf(|bytes| {
        range.start.encode(bytes);
        let mid = bytes.len();
        range.end.encode(bytes);
        let (range_start, range_end) = bytes.split_at(mid);
        sys::delete_range(table_id, col_id.into(), range_start, range_end)
    })
}

// pub fn create_index(_table_id: u32, _index_type: u8, _col_ids: Vec<u8>) {}

// TODO: going to have to somehow ensure TypeValue is equatable
// pub fn filter_eq(_table_id: u32, _col_id: u8, _eq_value: TypeValue) -> Option<TupleValue> {
//     return None;
// }

//
// fn page_table(table_id : u32, pager_token : u32, read_entries : u32) {
//
// }

pub fn __iter__(table_id: u32) -> Result<RawTableIter> {
    let bytes = sys::iter(table_id)?;

    let mut buffer = Cursor::new(bytes);
    let schema = buffer.get_u16().and_then(|_schema_len| decode_schema(&mut buffer));
    let schema = schema.unwrap_or_else(|e| {
        panic!("__iter__: Could not decode schema. Err: {}", e);
    });

    Ok(RawTableIter { buffer, schema })
}

pub struct RawTableIter {
    buffer: Cursor<Box<[u8]>>,
    schema: TupleDef,
}

impl Iterator for RawTableIter {
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
    fn describe_tuple() -> Vec<u8> {
        // const _: () = assert!(std::mem::size_of::<usize>() == std::mem::size_of::<u32>());
        let tuple_def = Self::get_tupledef();
        let mut bytes = vec![];
        tuple_def.encode(&mut bytes);
        bytes
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
        create_table(Self::TABLE_NAME, tuple_def).unwrap()
    }

    fn __tabledef_cell() -> &'static OnceCell<TableDef>;
    fn get_tabledef() -> &'static TableDef {
        Self::__tabledef_cell().get_or_init(|| TableDef {
            tuple: Self::get_tupledef(),
            unique_columns: Self::UNIQUE_COLUMNS.to_owned(),
        })
    }

    fn describe_table() -> Vec<u8> {
        let table_def = Self::get_tabledef();
        let mut bytes = vec![];
        table_def.encode(&mut bytes);
        bytes
    }

    fn table_id() -> u32;

    fn insert(ins: Self) {
        // TODO: how should we handle this kind of error?
        let _ = insert(Self::table_id(), ins.into_tuple());
    }

    fn iter() -> TableIter<Self> {
        TableIter {
            inner: Self::iter_tuples(),
            _marker: PhantomData,
        }
    }

    fn iter_tuples() -> RawTableIter {
        __iter__(Self::table_id()).expect("failed to get iterator from table")
    }
}

pub struct TableIter<T: TableType> {
    inner: RawTableIter,
    _marker: PhantomData<T>,
}
impl<T: TableType> Iterator for TableIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner
            .next()
            .map(|row| T::from_tuple(row).expect("failed to convert tuple to struct"))
    }
}

pub trait FilterableValue: FromValue + IntoValue {
    fn equals(&self, other: &TypeValue) -> bool;
}

pub trait UniqueValue: FilterableValue {
    fn into_primarykey(self) -> PrimaryKey;
}

#[doc(hidden)]
pub mod query {
    use super::*;

    #[doc(hidden)]
    pub fn filter_by_unique_field<Table: TableType, T: UniqueValue, const COL_IDX: u8>(val: T) -> Option<Table> {
        for row in Table::iter_tuples() {
            if let Some(ret) = check_eq(row, COL_IDX, &val) {
                return ret.ok();
            }
        }
        None
    }

    #[doc(hidden)]
    pub fn filter_by_field<Table: TableType, T: FilterableValue, const COL_IDX: u8>(
        val: T,
    ) -> FilterByIter<Table, COL_IDX, T> {
        FilterByIter {
            inner: Table::iter_tuples(),
            val,
            _marker: PhantomData,
        }
    }

    #[doc(hidden)]
    pub fn delete_by_field<Table: TableType, T: UniqueValue, const COL_IDX: u8>(val: T) -> bool {
        let value = val.into_value();
        let result = delete_eq(Table::table_id(), COL_IDX, value);
        match result {
            None => {
                //TODO: Returning here was supposed to signify an error, but it can also return none when there is nothing to delete.
                //spacetimedb::println!("Internal server error on equatable type: {}", #primary_key_tuple_type_str);
                false
            }
            Some(count) => count > 0,
        }
    }

    #[doc(hidden)]
    pub fn update_by_field<Table: TableType, T: UniqueValue, const COL_IDX: u8>(val: T, new_value: Table) -> bool {
        delete_by_field::<Table, T, COL_IDX>(val);
        Table::insert(new_value);

        // For now this is always successful
        true
    }

    #[inline]
    fn check_eq<T: TableType>(row: TupleValue, col_idx: u8, val: &impl FilterableValue) -> Option<Result<T, ()>> {
        let column_data = &row.elements[usize::from(col_idx)];
        val.equals(column_data).then(|| {
            let ret = T::from_tuple(row).ok_or(());
            if ret.is_err() {
                fromtuple_failed(T::TABLE_NAME)
            }
            ret
        })
    }

    #[doc(hidden)]
    pub struct FilterByIter<Table: TableType, const COL_IDX: u8, T: FilterableValue> {
        inner: RawTableIter,
        val: T,
        _marker: PhantomData<Table>,
    }
    impl<Table: TableType, const COL_IDX: u8, T: FilterableValue> Iterator for FilterByIter<Table, COL_IDX, T> {
        type Item = Table;
        fn next(&mut self) -> Option<Self::Item> {
            self.inner.find_map(|row| {
                if let Some(Ok(ret)) = check_eq(row, COL_IDX, &self.val) {
                    Some(ret)
                } else {
                    None
                }
            })
        }
    }

    #[inline(never)]
    #[cold]
    fn fromtuple_failed(tablename: &str) {
        eprintln!(
            "Internal SpacetimeDB error: Can't convert from tuple to struct (wrong version?) {}",
            tablename
        )
    }
}

#[macro_export]
macro_rules! schedule {
    // this errors on literals with time unit suffixes, e.g. 100ms
    // I swear I saw a rustc tracking issue to allow :literal to match even an invalid suffix but I can't seem to find it
    ($dur:literal, $($args:tt)*) => {
        $crate::schedule!($crate::duration!($dur), $($args)*)
    };
    ($dur:expr, $($args:tt)*) => {
        $crate::__schedule_impl!($crate::rt::schedule_in($dur), [] [$($args)*])
    };
}
#[macro_export]
macro_rules! schedule_at {
    ($time:expr, $($args:tt)*) => {
        $crate::__schedule_impl!($time, [] [$($args)*])
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __schedule_impl {
    ($time:expr, [$repeater:path] [($($args:tt)*)]) => {
        $crate::__schedule_impl!(@process_args $time, $repeater, ($($args)*))
    };
    ($time:expr, [$($cur:tt)*] [$next:tt $($rest:tt)*]) => {
        $crate::__schedule_impl!($time, [$($cur)* $next] [$($rest)*])
    };
    (@process_args $time:expr, $repeater:path, (_$(, $args:expr)* $(,)?)) => {
        $crate::__schedule_impl!(@call $time, $repeater, $crate::ReducerContext::__dummy(), ($($args),*))
    };
    (@process_args $time:expr, $repeater:path, ($($args:expr),* $(,)?)) => {
        $crate::__schedule_impl!(@call $time, $repeater, , ($($args),*))
    };
    (@call $time:expr, $repeater:path, $($ctx:expr)?, ($($args:expr),*)) => {
        <$repeater>::schedule($time, $($ctx,)? $($args),*);
    };
}
