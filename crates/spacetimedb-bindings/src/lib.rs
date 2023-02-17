#[macro_use]
mod io;
mod impls;
mod logger;
#[doc(hidden)]
pub mod rt;
mod types;

use spacetimedb_lib::buffer::{BufReader, BufWriter, Cursor, DecodeError};
use spacetimedb_lib::de::DeserializeOwned;
use spacetimedb_lib::sats::AlgebraicTypeRef;
use spacetimedb_lib::ser::Serialize;
use spacetimedb_lib::{bsatn, PrimaryKey, TableDef, TupleDef, TupleValue, TypeDef};
use std::cell::RefCell;
use std::marker::PhantomData;
use std::ops::Range;
use std::panic;

#[cfg(feature = "macro")]
pub use spacetimedb_bindings_macro::{duration, spacetimedb};

pub use spacetimedb_lib;
pub use spacetimedb_lib::hash;
pub use spacetimedb_lib::sats;
pub use spacetimedb_lib::Hash;
pub use spacetimedb_lib::TypeValue;
pub use types::Timestamp;

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

pub fn decode_row<'a>(schema: &TupleDef, bytes: &mut impl BufReader<'a>) -> Result<TupleValue, DecodeError> {
    TupleValue::decode(schema, bytes)
}

pub fn encode_schema(schema: TupleDef, bytes: &mut impl BufWriter) {
    schema.encode(bytes);
}

pub fn decode_schema<'a>(bytes: &mut impl BufReader<'a>) -> Result<TupleDef, DecodeError> {
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

pub fn insert(table_id: u32, row: impl Serialize) -> Result<()> {
    with_row_buf(|bytes| {
        bsatn::to_writer(bytes, &row).unwrap();
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

pub fn delete_eq(table_id: u32, col_id: u8, eq_value: impl Serialize) -> Option<u32> {
    with_row_buf(|bytes| {
        bsatn::to_writer(bytes, &eq_value).unwrap();
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

    let buffer = Cursor::new(bytes);
    let schema = (&buffer).get_u16().and_then(|_schema_len| decode_schema(&mut &buffer));
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
        if (&self.buffer).remaining() == 0 {
            return None;
        }
        let row = decode_row(&self.schema, &mut &self.buffer).unwrap_or_else(|e| {
            panic!("TableIter::next: Failed to decode row! Err: {}", e);
        });
        Some(row)
    }
}

pub trait SchemaType: Sized {
    fn get_schema() -> TypeDef;
}

pub trait RefType: Sized {
    fn typeref() -> AlgebraicTypeRef;
}

impl<T: RefType> SchemaType for T {
    fn get_schema() -> TypeDef {
        TypeDef::Ref(T::typeref())
    }
}

pub trait TableType: RefType + DeserializeOwned + Serialize {
    const TABLE_NAME: &'static str;
    const UNIQUE_COLUMNS: &'static [u8];

    fn get_tabledef() -> TableDef {
        TableDef {
            name: Self::TABLE_NAME.into(),
            data: Self::typeref(),
            unique_columns: Self::UNIQUE_COLUMNS.to_owned(),
        }
    }

    fn table_id() -> u32;

    fn insert(ins: Self) {
        // TODO: how should we handle this kind of error?
        let _ = insert(Self::table_id(), ins);
    }

    fn iter() -> TableIter<Self> {
        let bytes = sys::iter(Self::table_id()).unwrap();

        let buffer = Cursor::new(bytes);
        (&buffer)
            .get_u16()
            .and_then(|schema_len| (&buffer).get_slice(schema_len as usize))
            .unwrap_or_else(|e| {
                panic!("__iter__: Could not skip schema. Err: {}", e);
            });

        TableIter {
            buffer,
            _marker: PhantomData,
        }
    }
}

pub struct TableIter<T: TableType> {
    buffer: Cursor<Box<[u8]>>,
    _marker: PhantomData<T>,
}
impl<T: TableType> Iterator for TableIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if (&self.buffer).remaining() == 0 {
            return None;
        }
        let row = bsatn::from_reader(&mut &self.buffer).unwrap_or_else(|e| {
            panic!("TableIter::next: Failed to decode row! Err: {}", e);
        });
        Some(row)
    }
}

pub trait FilterableValue: Serialize + Eq {}

pub trait UniqueValue: FilterableValue {
    fn into_primarykey(self) -> PrimaryKey;
}

#[doc(hidden)]
pub mod query {
    use super::*;

    pub trait FieldAccess<const N: u8> {
        type Field;
        fn get_field(&self) -> &Self::Field;
    }

    #[doc(hidden)]
    pub fn filter_by_unique_field<Table: TableType, T: UniqueValue, const COL_IDX: u8>(val: T) -> Option<Table>
    where
        Table: FieldAccess<COL_IDX, Field = T>,
    {
        for row in Table::iter() {
            if row.get_field() == &val {
                return Some(row);
            }
        }
        None
    }

    #[doc(hidden)]
    pub fn filter_by_field<Table: TableType, T: FilterableValue, const COL_IDX: u8>(
        val: T,
    ) -> FilterByIter<Table, COL_IDX, T> {
        FilterByIter {
            inner: Table::iter(),
            val,
        }
    }

    #[doc(hidden)]
    pub fn delete_by_field<Table: TableType, T: UniqueValue, const COL_IDX: u8>(val: T) -> bool {
        let result = delete_eq(Table::table_id(), COL_IDX, val);
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

    #[doc(hidden)]
    pub struct FilterByIter<Table: TableType, const COL_IDX: u8, T: FilterableValue> {
        inner: TableIter<Table>,
        val: T,
    }
    impl<Table: TableType, const COL_IDX: u8, T: FilterableValue> Iterator for FilterByIter<Table, COL_IDX, T>
    where
        Table: FieldAccess<COL_IDX, Field = T>,
    {
        type Item = Table;
        fn next(&mut self) -> Option<Self::Item> {
            self.inner.find_map(|row| (row.get_field() == &self.val).then_some(row))
        }
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
