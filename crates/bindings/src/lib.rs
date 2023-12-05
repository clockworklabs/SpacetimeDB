//! Provides safe abstractions around `bindings-sys`
//! and re-exports `#[spacetimedb]` and `#[duration]`.

#[macro_use]
mod io;
mod impls;
mod logger;
#[doc(hidden)]
pub mod rt;
pub mod time_span;
mod timestamp;

use crate::sats::db::def::IndexType;
use spacetimedb_lib::buffer::{BufReader, BufWriter, Cursor, DecodeError};
use spacetimedb_lib::sats::{impl_deserialize, impl_serialize, impl_st};
use spacetimedb_lib::{bsatn, PrimaryKey, ProductType, ProductValue};
use std::cell::RefCell;
use std::marker::PhantomData;
use std::slice::from_ref;
use std::{fmt, panic};
use sys::{Buffer, BufferIter};

use crate::sats::db::attr::ColumnAttribute;
pub use log;
pub use sats::SpacetimeType;
pub use spacetimedb_bindings_macro::{duration, query, spacetimedb, TableType};
pub use spacetimedb_bindings_sys as sys;
pub use spacetimedb_lib;
pub use spacetimedb_lib::de::{Deserialize, DeserializeOwned};
pub use spacetimedb_lib::sats;
pub use spacetimedb_lib::ser::Serialize;
pub use spacetimedb_lib::Address;
pub use spacetimedb_lib::AlgebraicValue;
pub use spacetimedb_lib::Identity;
pub use spacetimedb_primitives::TableId;
pub use sys::Errno;
pub use timestamp::Timestamp;

pub type Result<T = (), E = Errno> = core::result::Result<T, E>;

/// A context that any reducer is provided with.
#[non_exhaustive]
#[derive(Copy, Clone)]
pub struct ReducerContext {
    /// The `Identity` of the client that invoked the reducer.
    pub sender: Identity,
    /// The time at which the reducer was started.
    pub timestamp: Timestamp,
    /// The `Address` of the client that invoked the reducer.
    ///
    /// `None` if no `Address` was supplied to the `/database/call` HTTP endpoint,
    /// or via the CLI's `spacetime call` subcommand.
    ///
    /// For automatic reducers, i.e. `init`, `update` and scheduled reducers,
    /// this will be the module's `Address`.
    pub address: Option<Address>,
}

impl ReducerContext {
    #[doc(hidden)]
    pub fn __dummy() -> Self {
        Self {
            sender: Identity::__dummy(),
            timestamp: Timestamp::UNIX_EPOCH,
            address: None,
        }
    }
}

// #[cfg(target_arch = "wasm32")]
// #[global_allocator]
// static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

/// Run a function `f` provided with an empty mutable row buffer
/// and return the result of the function.
fn with_row_buf<R>(f: impl FnOnce(&mut Vec<u8>) -> R) -> R {
    thread_local! {
        /// A global buffer used for row data.
        // This gets optimized away to a normal global since wasm32 doesn't have threads by default.
        static ROW_BUF: RefCell<Vec<u8>> = RefCell::new(Vec::with_capacity(8 * 1024));
    }

    ROW_BUF.with(|r| {
        let mut buf = r.borrow_mut();
        buf.clear();
        f(&mut buf)
    })
}

pub fn encode_row(row: ProductValue, bytes: &mut impl BufWriter) {
    row.encode(bytes);
}

pub fn decode_row<'a>(schema: &ProductType, bytes: &mut impl BufReader<'a>) -> Result<ProductValue, DecodeError> {
    ProductValue::decode(schema, bytes)
}

pub fn encode_schema(schema: ProductType, bytes: &mut impl BufWriter) {
    schema.encode(bytes);
}

pub fn decode_schema<'a>(bytes: &mut impl BufReader<'a>) -> Result<ProductType, DecodeError> {
    ProductType::decode(bytes)
}

/// Queries and returns the `table_id` associated with the given (table) `name`.
///
/// Panics if the table does not exist.
pub fn get_table_id(table_name: &str) -> TableId {
    sys::get_table_id(table_name).unwrap_or_else(|_| {
        panic!("Failed to get table with name: {}", table_name);
    })
}

/// Insert a row of type `T` into the table identified by `table_id`.
pub fn insert<T: TableType>(table_id: TableId, row: T) -> T::InsertResult {
    trait HasAutoinc: TableType {
        const HAS_AUTOINC: bool;
    }
    impl<T: TableType> HasAutoinc for T {
        const HAS_AUTOINC: bool = {
            // NOTE: Written this way to work on a stable compiler since we don't use nightly.
            // Same as `T::COLUMN_ATTRS.iter().any(|attr| attr.is_auto_inc())`.
            let mut i = 0;
            let mut x = false;
            while i < T::COLUMN_ATTRS.len() {
                if T::COLUMN_ATTRS[i].has_autoinc() {
                    x = true;
                    break;
                }
                i += 1;
            }
            x
        };
    }
    with_row_buf(|bytes| {
        // Encode the row as bsatn into the buffer `bytes`.
        bsatn::to_writer(bytes, &row).unwrap();

        // Insert row into table.
        // When table has an auto-incrementing column, we must re-decode the changed `bytes`.
        let res = sys::insert(table_id, bytes).map(|()| {
            if <T as HasAutoinc>::HAS_AUTOINC {
                bsatn::from_slice(bytes).expect("decode error")
            } else {
                row
            }
        });
        sealed::InsertResult::from_res(res)
    })
}

/// Finds all rows in the table identified by `table_id`,
/// where the row has a column, identified by `col_id`,
/// with data matching `val` that can be serialized.
///
/// Matching is defined by decoding of `value` to an `AlgebraicValue`
/// according to the column's schema and then `Ord for AlgebraicValue`.
///
/// The rows found are BSATN encoded and then concatenated.
/// The resulting byte string from the concatenation is written
/// to a fresh buffer with a handle to it returned as a `Buffer`.
///
/// Panics if
/// - BSATN serialization fails
/// - there were unique constraint violations
/// - `row` doesn't decode from BSATN to a `ProductValue`
///   according to the `ProductType` that the table's schema specifies
pub fn iter_by_col_eq(table_id: TableId, col_id: u8, val: &impl Serialize) -> Result<Buffer> {
    with_row_buf(|bytes| {
        // Encode `val` as BSATN into `bytes` and then use that.
        bsatn::to_writer(bytes, val).unwrap();
        sys::iter_by_col_eq(table_id, col_id.into(), bytes)
    })
}

/// Deletes all rows in the table identified by `table_id`
/// where the column identified by `col_id` matches a `value` that can be serialized.
///
/// Matching is defined by decoding of `value` to an `AlgebraicValue`
/// according to the column's schema and then `Ord for AlgebraicValue`.
///
/// Returns the number of rows deleted.
///
/// Returns an error if
/// - a table with the provided `table_id` doesn't exist
/// - no columns were deleted
/// - `col_id` does not identify a column of the table,
/// - `value` doesn't decode from BSATN to an `AlgebraicValue`
///   according to the `AlgebraicType` that the table's schema specifies for `col_id`.
///
/// Panics when serialization fails.
pub fn delete_by_col_eq(table_id: TableId, col_id: u8, value: &impl Serialize) -> Result<u32> {
    with_row_buf(|bytes| {
        // Encode `value` as BSATN into `bytes` and then use that.
        bsatn::to_writer(bytes, value).unwrap();
        sys::delete_by_col_eq(table_id, col_id.into(), bytes)
    })
}

/// Deletes those rows, in the table identified by `table_id`,
/// that match any row in `relation`.
///
/// The `relation` will be BSATN encoded to `[ProductValue]`
/// i.e., a list of product values, so each element in `relation`
/// must serialize to what a `ProductValue` would in BSATN.
///
/// Matching is then defined by first BSATN-decoding
/// the resulting bsatn to a `Vec<ProductValue>`
/// according to the row schema of the table
/// and then using `Ord for AlgebraicValue`.
///
/// Returns the number of rows deleted.
///
/// Returns an error if
/// - a table with the provided `table_id` doesn't exist
/// - `(relation, relation_len)` doesn't decode from BSATN to a `Vec<ProductValue>`
///
/// Panics when serialization fails.
pub fn delete_by_rel(table_id: TableId, relation: &[impl Serialize]) -> Result<u32> {
    with_row_buf(|bytes| {
        // Encode `value` as BSATN into `bytes` and then use that.
        bsatn::to_writer(bytes, relation).unwrap();
        sys::delete_by_rel(table_id, bytes)
    })
}

/// A table iterator which yields values of the `TableType` corresponding to the table.
type TableTypeTableIter<T> = RawTableIter<TableTypeBufferDeserialize<T>>;

// Get the iterator for this table with an optional filter,
fn table_iter<T: TableType>(table_id: TableId, filter: Option<spacetimedb_lib::filter::Expr>) -> Result<TableIter<T>> {
    // Decode the filter, if any.
    let filter = filter
        .as_ref()
        .map(bsatn::to_vec)
        .transpose()
        .expect("Couldn't decode the filter query");

    // Create the iterator.
    let iter = sys::iter(table_id, filter.as_deref())?;

    let deserializer = TableTypeBufferDeserialize::new();
    Ok(RawTableIter::new(iter, deserializer).into())
}

/// A trait for deserializing mulitple items out of a single `BufReader`.
///
/// Each `BufReader` holds a number of concatenated serialized objects.
trait BufferDeserialize {
    /// The type of the items being deserialized.
    type Item;

    /// Deserialize one entry from the `reader`, which must not be empty.
    fn deserialize<'de>(&mut self, reader: impl BufReader<'de>) -> Self::Item;
}

/// Deserialize bsatn values to a particular `T` where `T: TableType`.
struct TableTypeBufferDeserialize<T> {
    _marker: PhantomData<T>,
}

impl<T> TableTypeBufferDeserialize<T> {
    fn new() -> Self {
        Self { _marker: PhantomData }
    }
}

impl<T: TableType> BufferDeserialize for TableTypeBufferDeserialize<T> {
    type Item = T;

    fn deserialize<'de>(&mut self, mut reader: impl BufReader<'de>) -> Self::Item {
        bsatn::from_reader(&mut reader).expect("Failed to decode row!")
    }
}

/// Iterate over a sequence of `Buffer`s
/// and deserialize a number of `<De as BufferDeserialize>::Item` out of each.
struct RawTableIter<De> {
    /// The underlying source of our `Buffer`s.
    inner: BufferIter,

    /// The current position in the buffer, from which `deserializer` can read.
    reader: Cursor<Vec<u8>>,

    deserializer: De,
}

impl<De: BufferDeserialize> RawTableIter<De> {
    fn new(iter: BufferIter, deserializer: De) -> Self {
        RawTableIter {
            inner: iter,
            reader: Cursor::new(Vec::new()),
            deserializer,
        }
    }
}

impl<T, De: BufferDeserialize<Item = T>> Iterator for RawTableIter<De> {
    type Item = De::Item;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // If we currently have some bytes in the buffer to still decode, do that.
            if (&self.reader).remaining() > 0 {
                let row = self.deserializer.deserialize(&self.reader);
                return Some(row);
            }
            // Otherwise, try to fetch the next chunk while reusing the buffer.
            let buffer = self.inner.next()?;
            let buffer = buffer.expect("RawTableIter::next: Failed to get buffer!");
            self.reader.pos.set(0);
            buffer.read_into(&mut self.reader.buf);
        }
    }
}

/// Describe a named index with an index type over a set of columns identified by their IDs.
#[derive(Clone, Copy)]
pub struct IndexDesc<'a> {
    /// The name of the index.
    pub name: &'a str,
    /// The type of index used, i.e. the strategy used for indexing.
    pub ty: IndexType,
    /// The set of columns indexed over given by the identifiers of the columns.
    pub col_ids: &'a [u8],
}

/// A table iterator which yields values of the `TableType` corresponding to the table.
#[derive(derive_more::From)]
pub struct TableIter<T: TableType> {
    iter: TableTypeTableIter<T>,
}

impl<T: TableType> Iterator for TableIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

/// A trait for the set of types serializable, deserializable, and convertible to `AlgebraicType`.
///
/// Additionally, the type knows its own table name, its column attributes, and indices.
pub trait TableType: SpacetimeType + DeserializeOwned + Serialize {
    const TABLE_NAME: &'static str;
    const COLUMN_ATTRS: &'static [ColumnAttribute];
    const INDEXES: &'static [IndexDesc<'static>];
    type InsertResult: sealed::InsertResult<T = Self>;

    /// Returns the ID of this table.
    fn table_id() -> TableId;

    /// Insert `ins` as a row in this table.
    fn insert(ins: Self) -> Self::InsertResult {
        insert(Self::table_id(), ins)
    }

    /// Returns an iterator over the rows in this table.
    fn iter() -> TableIter<Self> {
        table_iter(Self::table_id(), None).unwrap()
    }

    /// Returns an iterator filtered by `filter` over the rows in this table.
    ///
    /// **NOTE:** Do not use directly. This is exposed as `query!(...)`.
    #[doc(hidden)]
    fn iter_filtered(filter: spacetimedb_lib::filter::Expr) -> TableIter<Self> {
        table_iter(Self::table_id(), Some(filter)).unwrap()
    }

    /// Deletes this row `self` from the table.
    ///
    /// Returns `true` if the row was deleted.
    fn delete(&self) -> bool {
        let count = delete_by_rel(Self::table_id(), from_ref(self)).unwrap();
        debug_assert!(count < 2);
        count == 1
    }
}

mod sealed {
    use super::*;

    /// A trait of result types which know how to convert a `Result<T: TableType>` into itself.
    pub trait InsertResult {
        type T: TableType;
        fn from_res(res: Result<Self::T>) -> Self;
    }
}

/// A UNIQUE constraint violation on table type `T` was attempted.
pub struct UniqueConstraintViolation<T: TableType>(PhantomData<T>);
impl<T: TableType> fmt::Debug for UniqueConstraintViolation<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "UniqueConstraintViolation({})", T::TABLE_NAME)
    }
}
impl<T: TableType> fmt::Display for UniqueConstraintViolation<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "not able to insert into table {}; duplicate unique column",
            T::TABLE_NAME
        )
    }
}
impl<T: TableType> From<UniqueConstraintViolation<T>> for String {
    fn from(err: UniqueConstraintViolation<T>) -> Self {
        err.to_string()
    }
}
impl<T: TableType> std::error::Error for UniqueConstraintViolation<T> {}

impl<T: TableType> sealed::InsertResult for Result<T, UniqueConstraintViolation<T>> {
    type T = T;
    fn from_res(res: Result<Self::T>) -> Self {
        res.map_err(|e| match e {
            Errno::UNIQUE_ALREADY_EXISTS => UniqueConstraintViolation(PhantomData),
            _ => panic!("unexpected error from insert(): {e}"),
        })
    }
}

impl<T: TableType> sealed::InsertResult for T {
    type T = T;
    fn from_res(res: Result<Self::T>) -> Self {
        res.unwrap_or_else(|e| panic!("unexpected error from insert(): {e}"))
    }
}

/// A trait for types that can be serialized and tested for equality.
///
/// A type `T` implementing this trait should uphold the invariant:
/// ```text
/// ∀ a, b ∈ T. a == b <=> serialize(a) == serialize(b)
/// ```
/// That is, if two values `a: T` and `b: T` are equal,
/// then so are the values in their serialized representation.
pub trait FilterableValue: Serialize + Eq {}

/// A trait for types that can be converted into primary keys.
pub trait UniqueValue: FilterableValue {
    fn into_primarykey(self) -> PrimaryKey;
}

#[doc(hidden)]
pub mod query {
    use super::*;

    /// A trait for types exposing an operation to access their `N`th field.
    ///
    /// In other words, a type implementing `FieldAccess<N>` allows
    /// shared projection from `self` to its `N`th field.
    pub trait FieldAccess<const N: u8> {
        /// The type of the field at the `N`th position.
        type Field;

        /// Project to the value of the field at position `N`.
        fn get_field(&self) -> &Self::Field;
    }

    /// Finds the row of `Table` where the column at `COL_IDX` matches `val`,
    /// as defined by decoding to an `AlgebraicValue`
    /// according to the column's schema and then `Ord for AlgebraicValue`.
    ///
    /// **NOTE:** Do not use directly.
    /// This is exposed as `filter_by_{$field_name}` on types with `#[spacetimedb(table)]`.
    #[doc(hidden)]
    pub fn filter_by_unique_field<
        Table: TableType + FieldAccess<COL_IDX, Field = T>,
        T: UniqueValue,
        const COL_IDX: u8,
    >(
        val: &T,
    ) -> Option<Table> {
        // Find the row with a match.
        let slice: &mut &[u8] = &mut &*iter_by_col_eq(Table::table_id(), COL_IDX, val).unwrap().read();
        // We will always find either 0 or 1 rows here due to the unique constraint.
        match slice.remaining() {
            0 => None,
            _ => {
                let t = bsatn::from_reader(slice).unwrap();
                assert_eq!(slice.remaining(), 0);
                Some(t)
            }
        }
    }

    /// Finds all rows of `Table` where the column at `COL_IDX` matches `val`,
    /// as defined by decoding to an `AlgebraicValue`
    /// according to the column's schema and then `Ord for AlgebraicValue`.
    ///
    /// **NOTE:** Do not use directly.
    /// This is exposed as `filter_by_{$field_name}` on types with `#[spacetimedb(table)]`.
    #[doc(hidden)]
    pub fn filter_by_field<Table: TableType, T: FilterableValue, const COL_IDX: u8>(val: &T) -> FilterByIter<Table> {
        let rows = iter_by_col_eq(Table::table_id(), COL_IDX, val)
            .expect("iter_by_col_eq failed")
            .read();
        FilterByIter {
            cursor: Cursor::new(rows),
            _phantom: PhantomData,
        }
    }

    /// Deletes the row of `Table` where the column at `COL_IDX` matches `val`,
    /// as defined by decoding to an `AlgebraicValue`
    /// according to the column's schema and then `Ord for AlgebraicValue`.
    ///
    /// Returns whether any rows were deleted.
    ///
    /// **NOTE:** Do not use directly.
    /// This is exposed as `delete_by_{$field_name}` on types with `#[spacetimedb(table)]`.
    #[doc(hidden)]
    pub fn delete_by_field<Table: TableType, T: UniqueValue, const COL_IDX: u8>(val: &T) -> bool {
        let result = delete_by_col_eq(Table::table_id(), COL_IDX, val);
        match result {
            Err(_) => {
                // TODO: Returning here was supposed to signify an error,
                //       but it can also return `Err(_)` when there is nothing to delete.
                //spacetimedb::println!("Internal server error on equatable type: {}", #primary_key_tuple_type_str);
                false
            }
            // Should never be `> 1`.
            Ok(count) => {
                debug_assert!(count <= 1);
                count > 0
            }
        }
    }

    /// Updates the row of `Table`, where the column at `COL_IDX` matches `old`, to be `new` instead.
    ///
    /// Matching is defined by decoding to an `AlgebraicValue`
    /// according to the column's schema and then `Ord for AlgebraicValue`.
    ///
    /// **NOTE:** Do not use directly.
    /// This is exposed as `update_by_{$field_name}` on types with `#[spacetimedb(table)]`.
    #[doc(hidden)]
    pub fn update_by_field<Table: TableType, T: UniqueValue, const COL_IDX: u8>(old: &T, new: Table) -> bool {
        // Delete the existing row, if any.
        delete_by_field::<Table, T, COL_IDX>(old);

        // Insert the new row.
        Table::insert(new);

        // TODO: For now this is always successful.
        //       In the future, this could return what `delete_by_field` returns?
        true
    }

    /// An iterator returned by `filter_by_field`,
    /// which yields all of the rows of a table where a particular column's value
    /// matches a given target value.
    ///
    /// Matching is defined by decoding to an `AlgebraicValue`
    /// according to the column's schema and then `Ord for AlgebraicValue`.
    #[doc(hidden)]
    pub struct FilterByIter<Table: TableType> {
        /// The buffer of rows returned by `iter_by_col_eq`.
        cursor: Cursor<Box<[u8]>>,

        _phantom: PhantomData<Table>,
    }

    impl<Table> Iterator for FilterByIter<Table>
    where
        Table: TableType,
    {
        type Item = Table;

        fn next(&mut self) -> Option<Self::Item> {
            let mut cursor = &self.cursor;
            (cursor.remaining() != 0).then(|| bsatn::from_reader(&mut cursor).unwrap())
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

/// An identifier for the schedule to call reducer `R`.
pub struct ScheduleToken<R = AnyReducer> {
    id: u64,
    _marker: PhantomData<R>,
}

impl<R> Clone for ScheduleToken<R> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<R> Copy for ScheduleToken<R> {}

impl_serialize!([R] ScheduleToken<R>, (self, ser) => self.id.serialize(ser));
impl_deserialize!([R] ScheduleToken<R>, de => u64::deserialize(de).map(Self::new));
impl_st!([R] ScheduleToken<R>, _ts => spacetimedb_lib::AlgebraicType::U64);

impl<R> ScheduleToken<R> {
    /// Wrap the ID under which a reducer is scheduled in a [`ScheduleToken`].
    #[inline]
    fn new(id: u64) -> Self {
        Self {
            id,
            _marker: PhantomData,
        }
    }

    /// Erase the `R` type parameter from the token.
    ///
    /// In other words, forget what reducer this is for.
    #[inline]
    pub fn erase(self) -> ScheduleToken {
        ScheduleToken::new(self.id)
    }

    /// Cancel this scheduled reducer.
    ///
    /// Cancelling the same ID again has no effect.
    #[inline]
    pub fn cancel(self) {
        sys::cancel_reducer(self.id)
    }
}

/// An erased reducer.
pub struct AnyReducer {
    _never: std::convert::Infallible,
}
