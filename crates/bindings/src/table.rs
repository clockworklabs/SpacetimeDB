use std::borrow::Borrow;
use std::convert::Infallible;
use std::marker::PhantomData;
use std::{fmt, ops};

use spacetimedb_lib::buffer::{BufReader, Cursor};

pub use spacetimedb_lib::db::raw_def::v9::TableAccess;
pub use spacetimedb_primitives::{ColId, IndexId};

use crate::{bsatn, sys, DeserializeOwned, IterBuf, Serialize, SpacetimeType, TableId};

/// Implemented for every `TableHandle` struct generated in the client `module_bindings`
/// and the module macroexpansion.
pub trait Table: TableInternal {
    /// The type of rows stored in this table.
    type Row: SpacetimeType + Serialize + DeserializeOwned + Sized + 'static;

    /// Returns the number of rows of this table in the TX state,
    /// i.e. num(committed_state) + num(insert_table) - num(delete_table).
    ///
    /// This API is new to modules (though it previously existed in the Rust SDK)
    /// and will require a new host function in the ABI.
    fn count(&self) -> u64 {
        sys::datastore_table_row_count(Self::table_id()).expect("datastore_table_row_count() call failed")
    }

    /// Iterate over all rows in the TX state,
    /// i.e. committed_state ∪ insert_table ∖ delete_table.
    #[inline]
    fn iter(&self) -> impl Iterator<Item = Self::Row> {
        let table_id = Self::table_id();
        let iter = sys::datastore_table_scan_bsatn(table_id).expect("datastore_table_scan_bsatn() call failed");
        TableIter::new(iter)
    }

    /// Inserts `row` into the TX state,
    /// i.e. removes it from the delete table or adds it to the insert table as appropriate.
    ///
    /// The return value is the inserted row, with any auto-incrementing columns replaced with computed values.
    /// The `insert` method always returns the inserted row,
    /// even when the table contains no auto-incrementing columns.
    ///
    /// May panic if inserting the row violates any constraints.
    /// Callers which intend to handle constraint violation errors should instead use [`Self::try_insert`].
    ///
    /// Note that, in languages where error handling is based on exceptions,
    /// no distinction is provided between `Table::insert` and `Table::try_insert`.
    /// A single method `insert` is defined which throws an exception on failure,
    /// and callers may either install handlers around it or allow the exception to bubble up.
    ///
    /// Note on MVCC: because callers have no way to determine if the row was previously present,
    /// two concurrent transactions which delete the same row
    /// may be ordered arbitrarily with respect to one another
    /// while maintaining sequential consistency, assuming no other conflicts.
    #[track_caller]
    fn insert(&self, row: Self::Row) -> Self::Row {
        self.try_insert(row).unwrap_or_else(|e| panic!("{e}"))
    }

    /// The error type for this table for unique constraint violations. Will either be
    /// [`UniqueConstraintViolation`] if the table has any unique constraints, or [`Infallible`]
    /// otherwise.
    type UniqueConstraintViolation: MaybeError<UniqueConstraintViolation>;
    /// The error type for this table for auto-increment overflows. Will either be
    /// [`AutoIncOverflow`] if the table has any auto-incrementing columns, or [`Infallible`]
    /// otherwise.
    type AutoIncOverflow: MaybeError<AutoIncOverflow>;

    /// Counterpart to [`Self::insert`] which allows handling failed insertions.
    ///
    /// For tables without any constraints, [`Self::TryInsertError`] will be [`std::convert::Infallible`],
    /// and this will be a more-verbose [`Self::insert`].
    /// For tables with constraints, this method returns an `Err` when the insertion fails rather than panicking.
    ///
    /// Note that, in languages where error handling is based on exceptions,
    /// no distinction is provided between `Table::insert` and `Table::try_insert`.
    /// A single method `insert` is defined which throws an exception on failure,
    /// and callers may either install handlers around it or allow the exception to bubble up.
    #[track_caller]
    fn try_insert(&self, row: Self::Row) -> Result<Self::Row, TryInsertError<Self>> {
        insert::<Self>(row)
    }

    /// Deletes a row equal to `row` from the TX state,
    /// i.e. deletes it from the insert table or adds it to the delete table as appropriate.
    ///
    /// Returns `true` if the row was present and has been deleted,
    /// or `false` if the row was not present and therefore the tables have not changed.
    ///
    /// Unlike [`Self::insert`], there is no need to return the deleted row,
    /// as it must necessarily have been exactly equal to the `row` argument.
    /// No analogue to auto-increment placeholders exists for deletions.
    ///
    /// May panic if deleting the row violates any constraints.
    /// Note that as of writing deletion is infallible, but future work may define new constraints,
    /// e.g. foreign keys, which cause deletion to fail in some cases.
    /// If and when these new constraints are added,
    /// we should define `Self::try_delete` and `Self::TryDeleteError`,
    /// analogous to [`Self::try_insert`] and [`Self::TryInsertError`].
    ///
    /// Note on MVCC: the return value means that logically a `delete` performs a query
    /// to see if the row is present.
    /// As such, two concurrent transactions which delete the same row
    /// cannot be placed in a sequentially-consistent ordering,
    /// and one of them must be retried.
    fn delete(&self, row: Self::Row) -> bool {
        let relation = std::slice::from_ref(&row);
        let buf = IterBuf::serialize(relation).unwrap();
        let count = sys::datastore_delete_all_by_eq_bsatn(Self::table_id(), &buf).unwrap();
        count == 1
    }

    // Re-integrates the BSATN of the `generated_cols` into `row`.
    #[doc(hidden)]
    fn integrate_generated_columns(row: &mut Self::Row, generated_cols: &[u8]);
}

#[doc(hidden)]
pub trait TableInternal: Sized {
    const TABLE_NAME: &'static str;
    const TABLE_ACCESS: TableAccess = TableAccess::Private;
    const UNIQUE_COLUMNS: &'static [u16];
    const INDEXES: &'static [IndexDesc<'static>];
    const PRIMARY_KEY: Option<u16> = None;
    const SEQUENCES: &'static [u16];
    const SCHEDULED_REDUCER_NAME: Option<&'static str> = None;

    /// Returns the ID of this table.
    fn table_id() -> TableId;
}

/// Describe a named index with an index type over a set of columns identified by their IDs.
#[derive(Clone, Copy)]
pub struct IndexDesc<'a> {
    pub name: &'a str,
    pub accessor_name: &'a str,
    pub algo: IndexAlgo<'a>,
}

#[derive(Clone, Copy)]
pub enum IndexAlgo<'a> {
    BTree { columns: &'a [u16] },
}

#[doc(hidden)]
pub trait __MapRowTypeToTable {
    type Table: Table;
}

/// A UNIQUE constraint violation on a table was attempted.
// TODO: add column name for better error message
#[derive(Debug)]
#[non_exhaustive]
pub struct UniqueConstraintViolation;

impl fmt::Display for UniqueConstraintViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "duplicate unique column")
    }
}

impl std::error::Error for UniqueConstraintViolation {}

/// An auto-inc column overflowed its data type.
#[derive(Debug)]
#[non_exhaustive]
// TODO: add column name for better error message
pub struct AutoIncOverflow;

impl fmt::Display for AutoIncOverflow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "auto-inc sequence overflowed its column type")
    }
}

impl std::error::Error for AutoIncOverflow {}

/// The error type returned from [`Table::try_insert()`], signalling a constraint violation.
pub enum TryInsertError<Tbl: Table> {
    /// A [`UniqueConstraintViolation`].
    ///
    /// Returned from [`Table::try_insert`] if an attempted insertion
    /// has the same value in a unique column as an already-present row.
    ///
    /// This variant is only possible if the table has at least one unique column,
    /// and is otherwise [`std::convert::Infallible`].
    UniqueConstraintViolation(Tbl::UniqueConstraintViolation),

    /// An [`AutoIncOverflow`].
    ///
    /// Returned from [`TableHandle::try_insert`] if an attempted insertion
    /// advances an auto-inc sequence past the bounds of the column type.
    ///
    /// This variant is only possible if the table has at least one auto-inc column,
    /// and is otherwise [`std::convert::Infallible`].
    AutoIncOverflow(Tbl::AutoIncOverflow),
}

impl<Tbl: Table> fmt::Debug for TryInsertError<Tbl> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TryInsertError::<{}>::", Tbl::TABLE_NAME)?;
        match self {
            Self::UniqueConstraintViolation(e) => fmt::Debug::fmt(e, f),
            Self::AutoIncOverflow(e) => fmt::Debug::fmt(e, f),
        }
    }
}

impl<Tbl: Table> fmt::Display for TryInsertError<Tbl> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "insertion error on table `{}`:", Tbl::TABLE_NAME)?;
        match self {
            Self::UniqueConstraintViolation(e) => fmt::Display::fmt(e, f),
            Self::AutoIncOverflow(e) => fmt::Display::fmt(e, f),
        }
    }
}

impl<Tbl: Table> std::error::Error for TryInsertError<Tbl> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::UniqueConstraintViolation(e) => e,
            Self::AutoIncOverflow(e) => e,
        })
    }
}

impl<Tbl: Table> From<TryInsertError<Tbl>> for String {
    fn from(err: TryInsertError<Tbl>) -> Self {
        err.to_string()
    }
}

#[doc(hidden)]
pub trait MaybeError<E = Self>: std::error::Error + Send + Sync + Sized + 'static {
    fn get() -> Option<Self>;
}

impl<E> MaybeError<E> for Infallible {
    fn get() -> Option<Self> {
        None
    }
}

impl MaybeError for UniqueConstraintViolation {
    fn get() -> Option<Self> {
        Some(UniqueConstraintViolation)
    }
}

impl MaybeError for AutoIncOverflow {
    fn get() -> Option<AutoIncOverflow> {
        Some(AutoIncOverflow)
    }
}

/// A trait for types exposing an operation to access their `N`th field.
///
/// In other words, a type implementing `FieldAccess<N>` allows
/// shared projection from `self` to its `N`th field.
#[doc(hidden)]
pub trait FieldAccess<const N: u16> {
    /// The type of the field at the `N`th position.
    type Field;

    /// Project to the value of the field at position `N`.
    fn get_field(&self) -> &Self::Field;
}

pub struct UniqueColumn<Tbl: Table, ColType, const COL_IDX: u16>
where
    ColType: SpacetimeType + Serialize + DeserializeOwned,
    Tbl::Row: FieldAccess<COL_IDX, Field = ColType>,
{
    _marker: PhantomData<Tbl>,
}

impl<Tbl: Table, ColType, const COL_IDX: u16> UniqueColumn<Tbl, ColType, COL_IDX>
where
    ColType: SpacetimeType + Serialize + DeserializeOwned,
    Tbl::Row: FieldAccess<COL_IDX, Field = ColType>,
{
    #[doc(hidden)]
    pub fn __new() -> Self {
        Self { _marker: PhantomData }
    }

    /// Finds and returns the row where the value in the unique column matches the supplied `col_val`,
    /// or `None` if no such row is present in the database state.
    //
    // TODO: consider whether we should accept the sought value by ref or by value.
    // Should be consistent with the implementors of `BTreeIndexBounds` (see below).
    // By-value makes passing `Copy` fields more convenient,
    // whereas by-ref makes passing `!Copy` fields more performant.
    // Can we do something smart with `std::borrow::Borrow`?
    pub fn find(&self, col_val: impl Borrow<ColType>) -> Option<Tbl::Row> {
        // Find the row with a match.
        let buf = IterBuf::serialize(col_val.borrow()).unwrap();
        let iter = sys::iter_by_col_eq(Tbl::table_id(), COL_IDX.into(), &buf).unwrap();
        let mut iter = TableIter::new_with_buf(iter, buf);

        // We will always find either 0 or 1 rows here due to the unique constraint.
        let row = iter.next();
        assert!(
            iter.is_exhausted(),
            "iter_by_col_eq on unique field cannot return >1 rows"
        );
        row
    }

    /// Deletes the row where the value in the unique column matches the supplied `col_val`,
    /// if any such row is present in the database state.
    ///
    /// Returns `true` if a row with the specified `col_val` was previously present and has been deleted,
    /// or `false` if no such row was present.
    ///
    /// May panic if deleting the row would violate a constraint,
    /// though as of proposing no such constraints exist.
    pub fn delete(&self, col_val: impl Borrow<ColType>) -> bool {
        let buf = IterBuf::serialize(col_val.borrow()).unwrap();
        sys::delete_by_col_eq(Tbl::table_id(), COL_IDX.into(), &buf)
            // TODO: Returning `Err` here was supposed to signify an error,
            //       but it can also return `Err(_)` when there is nothing to delete.
            .unwrap_or(0)
            > 0
    }

    /// Deletes the row where the value in the unique column matches that in the corresponding field of `new_row`,
    /// then inserts the `new_row`.
    ///
    /// Returns the new row as actually inserted, with any auto-inc placeholders substituted for computed values.
    ///
    /// Panics if no row was previously present with the matching value in the unique column,
    /// or if either the delete or the insertion would violate a constraint.
    ///
    /// Implementors are encouraged to include the table name, unique column name, and unique column value
    /// in the panic message when no such row previously existed.
    #[track_caller]
    pub fn update(&self, new_row: Tbl::Row) -> Tbl::Row {
        assert!(
            self.delete(new_row.get_field()),
            "Row passed to UniqueColumn::update() did not already exist in table."
        );
        insert::<Tbl>(new_row).unwrap_or_else(|e| panic!("{e}"))
    }
}

pub trait Index {
    fn index_id() -> IndexId;
}

pub struct BTreeIndex<Tbl: Table, IndexType, Idx: Index> {
    _marker: PhantomData<(Tbl, IndexType, Idx)>,
}

impl<Tbl: Table, IndexType, Idx: Index> BTreeIndex<Tbl, IndexType, Idx> {
    #[doc(hidden)]
    pub fn __new() -> Self {
        Self { _marker: PhantomData }
    }

    /// Returns an iterator over all rows in the database state where the indexed column(s) match the bounds `b`.
    ///
    /// `b` may be:
    /// - A value for the first indexed column.
    /// - A range of values for the first indexed column.
    /// - A tuple of values for any prefix of the indexed columns, optionally terminated by a range for the next.
    pub fn filter<B, K>(&self, b: B) -> impl Iterator<Item = Tbl::Row>
    where
        B: BTreeIndexBounds<IndexType, K>,
    {
        let index_id = Idx::index_id();
        let args = b.get_args();
        let (prefix, prefix_elems, rstart, rend) = args.args_for_syscall();
        let iter = sys::datastore_btree_scan_bsatn(index_id, prefix, prefix_elems, rstart, rend)
            .unwrap_or_else(|e| panic!("unexpected error from datastore_btree_scan_bsatn: {e}"));
        TableIter::new(iter)
    }

    /// Deletes all rows in the database state where the indexed column(s) match the bounds `b`.
    ///
    /// `b` may be:
    /// - A value for the first indexed column.
    /// - A range of values for the first indexed column.
    /// - A tuple of values for any prefix of the indexed columns, optionally terminated by a range for the next.
    ///
    /// May panic if deleting any one of the rows would violate a constraint,
    /// though as of proposing no such constraints exist.
    pub fn delete<B, K>(&self, b: B) -> u64
    where
        B: BTreeIndexBounds<IndexType, K>,
    {
        let index_id = Idx::index_id();
        let args = b.get_args();
        let (prefix, prefix_elems, rstart, rend) = args.args_for_syscall();
        sys::datastore_delete_by_btree_scan_bsatn(index_id, prefix, prefix_elems, rstart, rend)
            .unwrap_or_else(|e| panic!("unexpected error from datastore_delete_by_btree_scan_bsatn: {e}"))
            .into()
    }
}

pub trait BTreeIndexBounds<T, K = ()> {
    #[doc(hidden)]
    fn get_args(&self) -> BTreeScanArgs;
}

#[doc(hidden)]
pub struct BTreeScanArgs {
    data: IterBuf,
    prefix_elems: usize,
    rstart_idx: usize,
    // None if rstart and rend are the same
    rend_idx: Option<usize>,
}

impl BTreeScanArgs {
    pub(crate) fn args_for_syscall(&self) -> (&[u8], ColId, &[u8], &[u8]) {
        let prefix = &self.data[..self.rstart_idx];
        let (rstart, rend) = if let Some(rend_idx) = self.rend_idx {
            (&self.data[self.rstart_idx..rend_idx], &self.data[rend_idx..])
        } else {
            let elem = &self.data[self.rstart_idx..];
            (elem, elem)
        };
        (prefix, ColId::from(self.prefix_elems), rstart, rend)
    }
}

macro_rules! impl_btree_index_bounds {
    ($T:ident $(, $U:ident)*) => {
        impl_btree_index_bounds!(@impl (), ($T $(, $U)*));

        impl_btree_index_bounds!($($U),*);
    };
    () => {};
    (@impl ($($V:ident),*), ($T:ident $(, $U:ident)+)) => {
        impl<$($V,)* $T: Serialize, $($U: Serialize,)+ Term: BTreeIndexBoundsTerminator<$T>> BTreeIndexBounds<($($U,)+ $T, $($V,)*)> for ($($U,)+ Term,) {
            fn get_args(&self) -> BTreeScanArgs {
                let mut data = IterBuf::take();
                let prefix_elems = impl_btree_index_bounds!(@count $($U)+);
                #[allow(non_snake_case)]
                let ($($U,)+ term,) = self;
                Ok(())
                    $(.and_then(|()| data.serialize_into($U)))+
                    .unwrap();
                let rstart_idx = data.len();
                let rend_idx = term.bounds().serialize_into(&mut data);
                BTreeScanArgs { data, prefix_elems, rstart_idx, rend_idx }
            }
        }
        impl_btree_index_bounds!(@impl ($($V,)* $T), ($($U),*));
    };
    (@impl ($($V:ident),*), ($T:ident)) => {
        impl<$($V,)* $T: Serialize, Term: BTreeIndexBoundsTerminator<$T>> BTreeIndexBounds<($T, $($V,)*)> for (Term,) {
            fn get_args(&self) -> BTreeScanArgs {
                BTreeIndexBounds::<($T, $($V,)*), SingleBound>::get_args(&self.0)
            }
        }
        impl<$($V,)* $T: Serialize, Term: BTreeIndexBoundsTerminator<$T>> BTreeIndexBounds<($T, $($V,)*), SingleBound> for Term {
            fn get_args(&self) -> BTreeScanArgs {
                let mut data = IterBuf::take();
                let rend_idx = self.bounds().serialize_into(&mut data);
                BTreeScanArgs { data, prefix_elems: 0, rstart_idx: 0, rend_idx }
            }
        }
    };
    // Counts the number of elements in the tuple.
    (@count $($T:ident)*) => {
        0 $(+ impl_btree_index_bounds!(@drop $T 1))*
    };
    (@drop $a:tt $b:tt) => { $b };
}

pub struct SingleBound;

impl_btree_index_bounds!(A, B, C, D, E, F);

pub enum TermBound<T> {
    Single(ops::Bound<T>),
    Range(ops::Bound<T>, ops::Bound<T>),
}
impl<T: Serialize> TermBound<&T> {
    #[inline]
    fn serialize_into(&self, buf: &mut Vec<u8>) -> Option<usize> {
        let (start, end) = match self {
            TermBound::Single(elem) => (elem, None),
            TermBound::Range(start, end) => (start, Some(end)),
        };
        bsatn::to_writer(buf, start).unwrap();
        end.map(|end| {
            let rend_idx = buf.len();
            bsatn::to_writer(buf, end).unwrap();
            rend_idx
        })
    }
}
pub trait BTreeIndexBoundsTerminator<T> {
    fn bounds(&self) -> TermBound<&T>;
}

impl<T> BTreeIndexBoundsTerminator<T> for T {
    fn bounds(&self) -> TermBound<&T> {
        TermBound::Single(ops::Bound::Included(self))
    }
}
impl<T> BTreeIndexBoundsTerminator<T> for &T {
    fn bounds(&self) -> TermBound<&T> {
        TermBound::Single(ops::Bound::Included(self))
    }
}

macro_rules! impl_terminator {
    ($($range:ty,)*) => {
        $(impl<T> BTreeIndexBoundsTerminator<T> for $range {
            fn bounds(&self) -> TermBound<&T> {
                TermBound::Range(
                    ops::RangeBounds::start_bound(self),
                    ops::RangeBounds::end_bound(self),
                )
            }
        })*
    };
}

impl_terminator!(
    ops::Range<T>,
    ops::Range<&T>,
    ops::RangeFrom<T>,
    ops::RangeFrom<&T>,
    ops::RangeInclusive<T>,
    ops::RangeInclusive<&T>,
    ops::RangeTo<T>,
    ops::RangeTo<&T>,
    ops::RangeToInclusive<T>,
    ops::RangeToInclusive<&T>,
    (ops::Bound<T>, ops::Bound<T>),
    (ops::Bound<&T>, ops::Bound<&T>),
);

// Single-column indices
// impl<T> BTreeIndexBounds<(T,)> for Range<T> {}
// impl<T> BTreeIndexBounds<(T,)> for T {}

// // Two-column indices
// impl<T, U> BTreeIndexBounds<(T, U)> for Range<T> {}
// impl<T, U> BTreeIndexBounds<(T, U)> for T {}
// impl<T, U> BTreeIndexBounds<(T, U)> for (T, Range<U>) {}
// impl<T, U> BTreeIndexBounds<(T, U)> for (T, U) {}

// // Three-column indices
// impl<T, U, V> BTreeIndexBounds<(T, U, V)> for Range<T> {}
// impl<T, U, V> BTreeIndexBounds<(T, U, V)> for T {}
// impl<T, U, V> BTreeIndexBounds<(T, U, V)> for (T, Range<U>) {}
// impl<T, U, V> BTreeIndexBounds<(T, U, V)> for (T, U) {}
// impl<T, U, V> BTreeIndexBounds<(T, U, V)> for (T, U, Range<V>) {}
// impl<T, U, V> BTreeIndexBounds<(T, U, V)> for (T, U, V) {}

/// A trait for types that know if their value will trigger a sequence.
/// This is used for auto-inc columns to determine if an insertion of a row
/// will require the column to be updated in the row.
///
/// For now, this is equivalent to a "is zero" test.
pub trait IsSequenceTrigger {
    /// Is this value one that will trigger a sequence, if any,
    /// when used as a column value.
    fn is_sequence_trigger(&self) -> bool;
}

macro_rules! impl_is_seq_trigger {
    ($($t:ty),*) => {
        $(
            impl IsSequenceTrigger for $t {
                fn is_sequence_trigger(&self) -> bool { *self == 0 }
            }
        )*
    };
}

impl_is_seq_trigger![u8, i8, u16, i16, u32, i32, u64, i64, u128, i128];

impl IsSequenceTrigger for crate::sats::i256 {
    fn is_sequence_trigger(&self) -> bool {
        *self == Self::ZERO
    }
}

impl IsSequenceTrigger for crate::sats::u256 {
    fn is_sequence_trigger(&self) -> bool {
        *self == Self::ZERO
    }
}

/// Insert a row of type `T` into the table identified by `table_id`.
#[track_caller]
fn insert<T: Table>(mut row: T::Row) -> Result<T::Row, TryInsertError<T>> {
    let table_id = T::table_id();
    // Encode the row as bsatn into the buffer `buf`.
    let mut buf = IterBuf::serialize(&row).unwrap();

    // Insert row into table.
    // When table has an auto-incrementing column, we must re-decode the changed `buf`.
    let res = sys::insert(table_id, &mut buf).map(|gen_cols| {
        // Let the caller handle any generated columns written back by `sys::insert` to `buf`.
        T::integrate_generated_columns(&mut row, gen_cols);
        row
    });
    res.map_err(|e| {
        let err = match e {
            sys::Errno::UNIQUE_ALREADY_EXISTS => {
                T::UniqueConstraintViolation::get().map(TryInsertError::UniqueConstraintViolation)
            }
            // sys::Errno::AUTO_INC_OVERFLOW => Tbl::AutoIncOverflow::get().map(TryInsertError::AutoIncOverflow),
            _ => None,
        };
        err.unwrap_or_else(|| panic!("unexpected insertion error: {e}"))
    })
}

/// A table iterator which yields values of the `TableType` corresponding to the table.
struct TableIter<T: DeserializeOwned> {
    /// The underlying source of our `Buffer`s.
    inner: sys::RowIter,

    /// The current position in the buffer, from which `deserializer` can read.
    reader: Cursor<IterBuf>,

    _marker: PhantomData<T>,
}

impl<T: DeserializeOwned> TableIter<T> {
    #[inline]
    fn new(iter: sys::RowIter) -> Self {
        TableIter::new_with_buf(iter, IterBuf::take())
    }

    #[inline]
    fn new_with_buf(iter: sys::RowIter, mut buf: IterBuf) -> Self {
        buf.clear();
        TableIter {
            inner: iter,
            reader: Cursor::new(buf),
            _marker: PhantomData,
        }
    }

    fn is_exhausted(&self) -> bool {
        (&self.reader).remaining() == 0 && self.inner.is_exhausted()
    }
}

impl<T: DeserializeOwned> Iterator for TableIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // If we currently have some bytes in the buffer to still decode, do that.
            if (&self.reader).remaining() > 0 {
                let row = bsatn::from_reader(&mut &self.reader).expect("Failed to decode row!");
                return Some(row);
            }

            // Don't fetch the next chunk if there is none.
            if self.inner.is_exhausted() {
                return None;
            }

            // Otherwise, try to fetch the next chunk while reusing the buffer.
            self.reader.buf.clear();
            self.reader.pos.set(0);
            self.inner.read(&mut self.reader.buf);
        }
    }
}
