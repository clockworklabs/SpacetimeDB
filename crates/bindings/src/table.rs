use crate::{bsatn, rt::ExplicitNames, sys, DeserializeOwned, IterBuf, Serialize, SpacetimeType, TableId};
use core::borrow::Borrow;
use core::convert::Infallible;
use core::fmt;
use core::marker::PhantomData;
pub use spacetimedb_lib::db::raw_def::v9::TableAccess;
use spacetimedb_lib::{
    buffer::{BufReader, Cursor, DecodeError},
    AlgebraicValue,
};
use spacetimedb_lib::{FilterableValue, IndexScanRangeBoundsTerminator};
pub use spacetimedb_primitives::{ColId, IndexId};

#[doc(hidden)]
#[derive(Clone)]
pub enum LocalBackend {
    Host,
    #[cfg(all(feature = "test-utils", not(target_arch = "wasm32")))]
    Test(std::sync::Arc<spacetimedb_test_datastore::TestDatastore>),
}

impl LocalBackend {
    #[doc(hidden)]
    pub fn as_table_handle_backend(&self) -> TableHandleBackend {
        match self {
            Self::Host => TableHandleBackend::Host,
            #[cfg(all(feature = "test-utils", not(target_arch = "wasm32")))]
            Self::Test(datastore) => TableHandleBackend::Test(datastore.clone()),
        }
    }
}

#[doc(hidden)]
#[derive(Clone)]
pub enum TableHandleBackend {
    Host,
    #[cfg(all(feature = "test-utils", not(target_arch = "wasm32")))]
    Test(std::sync::Arc<spacetimedb_test_datastore::TestDatastore>),
}

impl TableHandleBackend {
    pub fn table_id(&self, table_name: &str) -> Result<TableId, sys::Errno> {
        match self {
            Self::Host => {
                #[cfg(target_arch = "wasm32")]
                {
                    sys::table_id_from_name(table_name)
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let _ = table_name;
                    Err(sys::Errno::HOST_CALL_FAILURE)
                }
            }
            #[cfg(all(feature = "test-utils", not(target_arch = "wasm32")))]
            Self::Test(datastore) => datastore.table_id(table_name).map_err(|_| sys::Errno::NO_SUCH_TABLE),
        }
    }

    pub fn index_id(&self, index_name: &str) -> Result<IndexId, sys::Errno> {
        match self {
            Self::Host => {
                #[cfg(target_arch = "wasm32")]
                {
                    sys::index_id_from_name(index_name)
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let _ = index_name;
                    Err(sys::Errno::HOST_CALL_FAILURE)
                }
            }
            #[cfg(all(feature = "test-utils", not(target_arch = "wasm32")))]
            Self::Test(datastore) => datastore.index_id(index_name).map_err(|_| sys::Errno::NO_SUCH_INDEX),
        }
    }

    fn insert_bsatn(&self, table_id: TableId, row: &mut [u8]) -> Result<Vec<u8>, sys::Errno> {
        match self {
            Self::Host => {
                #[cfg(target_arch = "wasm32")]
                {
                    sys::datastore_insert_bsatn(table_id, row).map(<[u8]>::to_vec)
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let _ = (table_id, row);
                    Err(sys::Errno::HOST_CALL_FAILURE)
                }
            }
            #[cfg(all(feature = "test-utils", not(target_arch = "wasm32")))]
            Self::Test(datastore) => datastore.insert_bsatn_generated_cols(table_id, row).map_err(|err| {
                err.insert_errno_code()
                    .and_then(sys::Errno::from_code)
                    .unwrap_or(sys::Errno::HOST_CALL_FAILURE)
            }),
        }
    }

    fn table_scan_bsatn(&self, table_id: TableId) -> Result<TableIterInner, sys::Errno> {
        match self {
            Self::Host => {
                #[cfg(target_arch = "wasm32")]
                {
                    sys::datastore_table_scan_bsatn(table_id).map(TableIterInner::Host)
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let _ = table_id;
                    Err(sys::Errno::HOST_CALL_FAILURE)
                }
            }
            #[cfg(all(feature = "test-utils", not(target_arch = "wasm32")))]
            Self::Test(datastore) => datastore
                .table_rows_bsatn(table_id)
                .map(|rows| TableIterInner::Test(rows.into_iter()))
                .map_err(|_| sys::Errno::HOST_CALL_FAILURE),
        }
    }

    pub fn table_row_count(&self, table_id: TableId) -> Result<u64, sys::Errno> {
        match self {
            Self::Host => {
                #[cfg(target_arch = "wasm32")]
                {
                    sys::datastore_table_row_count(table_id)
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let _ = table_id;
                    Err(sys::Errno::HOST_CALL_FAILURE)
                }
            }
            #[cfg(all(feature = "test-utils", not(target_arch = "wasm32")))]
            Self::Test(datastore) => datastore
                .table_row_count(table_id)
                .map_err(|_| sys::Errno::NO_SUCH_TABLE),
        }
    }

    fn index_scan_point_bsatn(&self, index_id: IndexId, point: &[u8]) -> Result<TableIterInner, sys::Errno> {
        match self {
            Self::Host => {
                #[cfg(target_arch = "wasm32")]
                {
                    sys::datastore_index_scan_point_bsatn(index_id, point).map(TableIterInner::Host)
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let _ = (index_id, point);
                    Err(sys::Errno::HOST_CALL_FAILURE)
                }
            }
            #[cfg(all(feature = "test-utils", not(target_arch = "wasm32")))]
            Self::Test(datastore) => datastore
                .index_scan_point_bsatn(index_id, point)
                .map(|rows| TableIterInner::Test(rows.into_iter()))
                .map_err(|_| sys::Errno::HOST_CALL_FAILURE),
        }
    }

    fn index_scan_range_bsatn(
        &self,
        index_id: IndexId,
        prefix: &[u8],
        prefix_elems: ColId,
        rstart: &[u8],
        rend: &[u8],
    ) -> Result<TableIterInner, sys::Errno> {
        match self {
            Self::Host => {
                #[cfg(target_arch = "wasm32")]
                {
                    sys::datastore_index_scan_range_bsatn(index_id, prefix, prefix_elems, rstart, rend)
                        .map(TableIterInner::Host)
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let _ = (index_id, prefix, prefix_elems, rstart, rend);
                    Err(sys::Errno::HOST_CALL_FAILURE)
                }
            }
            #[cfg(all(feature = "test-utils", not(target_arch = "wasm32")))]
            Self::Test(datastore) => datastore
                .index_scan_range_bsatn(index_id, prefix, prefix_elems, rstart, rend)
                .map(|rows| TableIterInner::Test(rows.into_iter()))
                .map_err(|_| sys::Errno::HOST_CALL_FAILURE),
        }
    }

    fn delete_by_index_scan_point_bsatn(&self, index_id: IndexId, point: &[u8]) -> Result<u32, sys::Errno> {
        match self {
            Self::Host => {
                #[cfg(target_arch = "wasm32")]
                {
                    sys::datastore_delete_by_index_scan_point_bsatn(index_id, point)
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let _ = (index_id, point);
                    Err(sys::Errno::HOST_CALL_FAILURE)
                }
            }
            #[cfg(all(feature = "test-utils", not(target_arch = "wasm32")))]
            Self::Test(datastore) => datastore
                .delete_by_index_scan_point_bsatn(index_id, point)
                .map_err(|_| sys::Errno::HOST_CALL_FAILURE),
        }
    }

    fn delete_by_index_scan_range_bsatn(
        &self,
        index_id: IndexId,
        prefix: &[u8],
        prefix_elems: ColId,
        rstart: &[u8],
        rend: &[u8],
    ) -> Result<u32, sys::Errno> {
        match self {
            Self::Host => {
                #[cfg(target_arch = "wasm32")]
                {
                    sys::datastore_delete_by_index_scan_range_bsatn(index_id, prefix, prefix_elems, rstart, rend)
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let _ = (index_id, prefix, prefix_elems, rstart, rend);
                    Err(sys::Errno::HOST_CALL_FAILURE)
                }
            }
            #[cfg(all(feature = "test-utils", not(target_arch = "wasm32")))]
            Self::Test(datastore) => datastore
                .delete_by_index_scan_range_bsatn(index_id, prefix, prefix_elems, rstart, rend)
                .map_err(|_| sys::Errno::HOST_CALL_FAILURE),
        }
    }

    fn update_bsatn(&self, table_id: TableId, index_id: IndexId, row: &mut [u8]) -> Result<Vec<u8>, sys::Errno> {
        match self {
            Self::Host => {
                #[cfg(target_arch = "wasm32")]
                {
                    sys::datastore_update_bsatn(table_id, index_id, row).map(<[u8]>::to_vec)
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let _ = (table_id, index_id, row);
                    Err(sys::Errno::HOST_CALL_FAILURE)
                }
            }
            #[cfg(all(feature = "test-utils", not(target_arch = "wasm32")))]
            Self::Test(datastore) => datastore
                .update_bsatn_generated_cols(table_id, index_id, row)
                .map_err(|_| sys::Errno::HOST_CALL_FAILURE),
        }
    }
}

/// Implemented for every `TableHandle` struct generated by the [`table`](macro@crate::table) macro.
/// Contains methods that are present for every table, regardless of what unique constraints
/// and indexes are present.
///
/// To get a `TableHandle`
// TODO: should we rename this `TableHandle`? Documenting this, I think that's much clearer.
pub trait Table: TableInternal + ExplicitNames {
    /// The type of rows stored in this table.
    type Row: SpacetimeType + Serialize + DeserializeOwned + Sized + 'static;

    /// Returns the number of rows in this table.
    ///
    /// This reads datastore metadata, so it runs in constant time.
    /// It also takes into account modifications by the current transaction.
    fn count(&self) -> u64 {
        let table_id = self
            .__backend()
            .table_id(Self::TABLE_NAME)
            .expect("table_id_from_name() call failed");
        self.__backend()
            .table_row_count(table_id)
            .expect("datastore_table_row_count() call failed")
    }

    /// Iterate over all rows of the table.
    ///
    /// For large tables, this can be a slow operation!
    /// Prefer [filtering](RangedIndex::filter) a [`RangedIndex`] or [finding](UniqueColumn::find) a [`UniqueColumn`] if
    /// possible.
    ///
    /// (This keeps track of changes made to the table since the start of this reducer invocation. For example, if rows have been deleted since the start of this reducer invocation, those rows will not be returned by `iter`. Similarly, inserted rows WILL be returned.)
    #[inline]
    fn iter(&self) -> impl Iterator<Item = Self::Row> {
        let table_id = self
            .__backend()
            .table_id(Self::TABLE_NAME)
            .expect("table_id_from_name() call failed");
        let iter = self
            .__backend()
            .table_scan_bsatn(table_id)
            .expect("datastore_table_scan_bsatn() call failed");
        TableIter::new(iter)
    }

    /// Inserts `row` into the table.
    ///
    /// The return value is the inserted row, with any auto-incrementing columns replaced with computed values.
    /// The `insert` method always returns the inserted row,
    /// even when the table contains no auto-incrementing columns.
    ///
    /// (The returned row is a copy of the row in the database.
    /// Modifying this copy does not directly modify the database.
    /// See [`UniqueColumn::update`] if you want to update the row.)
    ///
    /// May panic if inserting the row violates any constraints.
    /// Callers which intend to handle constraint violation errors should instead use [`Self::try_insert`].
    ///
    /// Inserting an exact duplicate of a row already present in the table is a no-op,
    /// as SpacetimeDB is a set-semantic database.
    /// This is true even for tables with unique constraints;
    /// inserting an exact duplicate of an already-present row will not panic.
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
    /// For tables with constraints, this method returns an `Err` when the insertion fails rather than panicking.
    /// For tables without any constraints, [`Self::UniqueConstraintViolation`] and [`Self::AutoIncOverflow`]
    /// will be [`std::convert::Infallible`], and this will be a more-verbose [`Self::insert`].
    ///
    /// Inserting an exact duplicate of a row already present in the table is a no-op and returns `Ok`,
    /// as SpacetimeDB is a set-semantic database.
    /// This is true even for tables with unique constraints;
    /// inserting an exact duplicate of an already-present row will return `Ok`.
    #[track_caller]
    fn try_insert(&self, row: Self::Row) -> Result<Self::Row, TryInsertError<Self>> {
        insert::<Self>(self, row, IterBuf::take())
    }

    /// Deletes a row equal to `row` from the table.
    ///
    /// Returns `true` if the row was present and has been deleted,
    /// or `false` if the row was not present and therefore the tables have not changed.
    ///
    /// Unlike [`Self::insert`], there is no need to return the deleted row,
    /// as it must necessarily have been exactly equal to the `row` argument.
    /// No analogue to auto-increment placeholders exists for deletions.
    ///
    /// May panic if deleting the row violates any constraints.
    fn delete(&self, row: Self::Row) -> bool {
        // Note that as of writing deletion is infallible, but future work may define new constraints,
        // e.g. foreign keys, which cause deletion to fail in some cases.
        // If and when these new constraints are added,
        // we should define `Self::ForeignKeyViolation`,
        // analogous to [`Self::UniqueConstraintViolation`].

        let relation = std::slice::from_ref(&row);
        let buf = IterBuf::serialize(relation).unwrap();
        let count = sys::datastore_delete_all_by_eq_bsatn(Self::table_id(), &buf).unwrap();
        count > 0
    }

    /// Clears the table of all rows.
    ///
    /// Returns the number of rows that were deleted,
    /// i.e., the value of [`self.count()`](Table::count) before this call.
    fn clear(&self) -> u64 {
        sys::datastore_clear(Self::table_id()).expect("datastore_clear() call failed")
    }

    // Re-integrates the BSATN of the `generated_cols` into `row`.
    #[doc(hidden)]
    fn integrate_generated_columns(row: &mut Self::Row, generated_cols: &[u8]);
}

#[doc(hidden)]
#[inline]
pub fn count<Tbl: Table>() -> u64 {
    sys::datastore_table_row_count(Tbl::table_id()).expect("datastore_table_row_count() call failed")
}

#[doc(hidden)]
pub trait TableInternal: Sized {
    const TABLE_NAME: &'static str;
    const TABLE_ACCESS: TableAccess = TableAccess::Private;
    const UNIQUE_COLUMNS: &'static [u16];
    const INDEXES: &'static [IndexDesc<'static>];
    const PRIMARY_KEY: Option<u16> = None;
    const SEQUENCES: &'static [u16];
    const SCHEDULE: Option<ScheduleDesc<'static>> = None;
    const IS_EVENT: bool = false;

    /// Returns the ID of this table.
    fn table_id() -> TableId;

    #[doc(hidden)]
    fn __backend(&self) -> &TableHandleBackend;

    fn get_default_col_values() -> Vec<ColumnDefault>;
}

/// Describe a named index with an index type over a set of columns identified by their IDs.
#[derive(Clone, Copy)]
pub struct IndexDesc<'a> {
    pub source_name: &'a str,
    pub accessor_name: &'a str,
    pub algo: IndexAlgo<'a>,
}

#[derive(Clone, Copy)]
pub enum IndexAlgo<'a> {
    BTree { columns: &'a [u16] },
    Hash { columns: &'a [u16] },
    Direct { column: u16 },
}

pub struct ScheduleDesc<'a> {
    pub reducer_or_procedure_name: &'a str,
    pub scheduled_at_column: u16,
}

#[derive(Debug, Clone)]
pub struct ColumnDefault {
    pub col_id: u16,
    pub value: AlgebraicValue,
}

/// A row operation was attempted that would violate a unique constraint.
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
    /// Returned from [`Table::try_insert`] if an attempted insertion
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

pub trait Column {
    type Table: Table;
    type ColType: SpacetimeType + Serialize + DeserializeOwned;
    const COLUMN_NAME: &'static str;
    fn get_field(row: &<Self::Table as Table>::Row) -> &Self::ColType;
}

/// A marker trait for columns that are the primary key of their table.
///
/// This is used to restrict [`UniqueColumn::update`] to only work on primary key columns.
pub trait PrimaryKey {}

/// A handle to a unique index on a column.
/// Available for `#[unique]` and `#[primary_key]` columns.
///
/// For a table *table* with a column *column*, use `ctx.db.{table}().{column}()`
/// to get a `UniqueColumn` from a [`ReducerContext`](crate::ReducerContext).
///
/// Example:
///
/// ```no_run
/// # #[cfg(target_arch = "wasm32")] mod demo {
/// use spacetimedb::{table, UniqueColumn, ReducerContext, DbContext};
///
/// #[table(accessor = user)]
/// struct User {
///     #[primary_key]
///     id: u32,
///     #[unique]
///     username: String,
///     dog_count: u64
/// }
///
/// fn demo(ctx: &ReducerContext) {
///     let user = ctx.db().user();
///
///     let by_id: UniqueColumn<_, u32, _> = user.id();
///
///     let mut example_user: User = by_id.find(357).unwrap();
///     example_user.dog_count += 5;
///     by_id.update(example_user);
///
///     let by_username: UniqueColumn<_, String, _> = user.username();
///     by_username.delete(&"Evil Bob".to_string());
/// }
/// # }
/// ```
///
/// <!-- TODO: do we need integer type suffixes on literal arguments, like for RangedIndex? -->
pub struct UniqueColumn<Tbl, ColType, Col> {
    backend: TableHandleBackend,
    _marker: PhantomData<(Tbl, ColType, Col)>,
}

impl<Tbl: Table, Col: Index + Column<Table = Tbl>> UniqueColumn<Tbl, Col::ColType, Col> {
    #[doc(hidden)]
    pub fn __new(backend: TableHandleBackend) -> Self {
        Self {
            backend,
            _marker: PhantomData,
        }
    }

    /// Finds and returns the row where the value in the unique column matches the supplied `col_val`,
    /// or `None` if no such row is present in the database state.
    //
    // TODO: consider whether we should accept the sought value by ref or by value.
    // Should be consistent with the implementors of `IndexScanRangeBounds` (see below).
    // By-value makes passing `Copy` fields more convenient,
    // whereas by-ref makes passing `!Copy` fields more performant.
    // Can we do something smart with `std::borrow::Borrow`?
    #[inline]
    pub fn find(&self, col_val: impl Borrow<Col::ColType>) -> Option<Tbl::Row>
    where
        for<'a> &'a Col::ColType: FilterableValue,
    {
        find::<Tbl, Col>(&self.backend, col_val.borrow())
    }

    /// Deletes the row where the value in the unique column matches the supplied `col_val`,
    /// if any such row is present in the database state.
    ///
    /// Returns `true` if a row with the specified `col_val` was previously present and has been deleted,
    /// or `false` if no such row was present.
    #[inline]
    pub fn delete(&self, col_val: impl Borrow<Col::ColType>) -> bool {
        self._delete(col_val.borrow()).0
    }

    fn _delete(&self, col_val: &Col::ColType) -> (bool, IterBuf) {
        let index_id = self
            .backend
            .index_id(Col::INDEX_NAME)
            .expect("index_id_from_name() call failed");
        let point = IterBuf::serialize(col_val).unwrap();
        let n_del = self
            .backend
            .delete_by_index_scan_point_bsatn(index_id, &point)
            .unwrap_or_else(|e| {
                panic!("unique: unexpected error from datastore_delete_by_index_scan_point_bsatn: {e}")
            });

        (n_del > 0, point)
    }

    /// Deletes the row where the value in the unique column matches that in the corresponding field of `new_row`, and
    /// then inserts the `new_row`.
    ///
    /// Returns the new row as actually inserted, with computed values substituted for any auto-inc placeholders.
    ///
    /// This method can only be called on primary key columns, not any unique column.
    /// This prevents confusion regarding what constitutes a row update vs. a delete+insert.
    /// To perform this operation for a non-primary unique column, call
    /// `.delete(key)` followed by `.insert(row)`.
    ///
    /// # Panics
    /// Panics if no row was previously present with the matching value in the unique column,
    /// or if either the delete or the insertion would violate a constraint.
    #[track_caller]
    pub fn update(&self, new_row: Tbl::Row) -> Tbl::Row
    where
        Col: PrimaryKey,
    {
        let buf = IterBuf::take();
        let index_id = self
            .backend
            .index_id(Col::INDEX_NAME)
            .expect("index_id_from_name() call failed");
        update::<Tbl>(&self.backend, index_id, new_row, buf)
    }

    /// Inserts `new_row` into the table, first checking for an existing
    /// row with a matching value in the unique column and deleting it if present.
    ///
    /// Be careful: in case of a constraint violation, this method will return Err,
    /// but the previous row will be deleted. If you propagate the error, SpacetimeDB will
    /// rollback the transaction and the old row will be restored. If you ignore the error,
    /// the old row will be lost.
    #[track_caller]
    #[doc(alias = "try_upsert")]
    #[cfg(feature = "unstable")]
    pub fn try_insert_or_update(&self, new_row: Tbl::Row) -> Result<Tbl::Row, TryInsertError<Tbl>> {
        let col_val = Col::get_field(&new_row);
        // If the row doesn't exist, delete will return false, which we ignore.
        let _ = self.delete(col_val);

        // Then, insert the new row.
        let buf = IterBuf::take();
        insert::<Tbl>(new_row, buf)
    }

    /// Inserts `new_row` into the table, first checking for an existing
    /// row with a matching value in the unique column and deleting it if present.
    ///
    /// # Panics
    /// Panics if either the delete or the insertion would violate a constraint.
    #[track_caller]
    #[doc(alias = "upsert")]
    #[cfg(feature = "unstable")]
    pub fn insert_or_update(&self, new_row: Tbl::Row) -> Tbl::Row {
        self.try_insert_or_update(new_row).unwrap_or_else(|e| panic!("{e}"))
    }
}

#[inline]
fn find<Tbl: Table, Col: Index + Column<Table = Tbl>>(
    backend: &TableHandleBackend,
    col_val: &Col::ColType,
) -> Option<Tbl::Row> {
    // Find the row with a match.
    let index_id = backend
        .index_id(Col::INDEX_NAME)
        .expect("index_id_from_name() call failed");
    let point = IterBuf::serialize(col_val).unwrap();

    let iter = datastore_index_scan_point_bsatn(backend, index_id, &point);
    let mut iter = TableIter::new_with_buf(iter, point);

    // We will always find either 0 or 1 rows here due to the unique constraint.
    let row = iter.next();
    assert!(
        iter.is_exhausted(),
        "`datastore_index_scan_point_bsatn` on unique field cannot return >1 rows"
    );
    row
}

/// See `sys::datastore_index_scan_point_bsatn`.
/// Panics when the aforementioned errors.
fn datastore_index_scan_point_bsatn(backend: &TableHandleBackend, index_id: IndexId, point: &[u8]) -> TableIterInner {
    backend
        .index_scan_point_bsatn(index_id, point)
        .unwrap_or_else(|e| panic!("unexpected error from `datastore_index_scan_point_bsatn`: {e}"))
}

/// A read-only handle to a unique (single-column) index.
///
/// This is the read-only version of [`UniqueColumn`].
/// It mirrors [`UniqueColumn`] but only exposes read APIs.
/// It cannot insert or delete rows.
/// It is used by `{table}__ViewHandle` to keep view code read-only at compile time.
///
/// Note, the `Tbl` generic is the read-write table handle `{table}__TableHandle`.
/// This is because read-only indexes still need [`Table`] metadata.
/// The view handle itself deliberately does not implement `Table`.
pub struct UniqueColumnReadOnly<Tbl, ColType, Col> {
    backend: TableHandleBackend,
    _marker: PhantomData<(Tbl, ColType, Col)>,
}

impl<Tbl: Table, Col: Index + Column<Table = Tbl>> UniqueColumnReadOnly<Tbl, Col::ColType, Col> {
    #[doc(hidden)]
    pub fn __new(backend: TableHandleBackend) -> Self {
        Self {
            backend,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn find(&self, col_val: impl Borrow<Col::ColType>) -> Option<Tbl::Row>
    where
        for<'a> &'a Col::ColType: FilterableValue,
    {
        find::<Tbl, Col>(&self.backend, col_val.borrow())
    }
}

/// Information about the `index_id` of an index
/// and the number of columns the index indexes.
pub trait Index {
    /// The generated runtime name of this index.
    const INDEX_NAME: &'static str;

    /// The number of columns the index indexes.
    ///
    /// Used to determine whether a scan for e.g., `(a, b)`,
    /// is actually a point scan or whether there's a suffix, e.g., `(c, d)`.
    const NUM_COLS_INDEXED: usize;

    /// Determine the `IndexId` of this index.
    ///
    /// For generated implementations,
    /// this results in a *memoized* syscall to determine the index,
    /// based on the hard coded name of the index.
    fn index_id() -> IndexId;
}

/// Marks an index as only having point query capabilities.
///
/// This applies to Hash indices but not BTree and Direct indices.
pub trait IndexIsPointed: Index {}

/// A handle to a Hash index on a table.
///
/// To get one of these from a `ReducerContext`, use:
/// ```text
/// ctx.db.{table}().{index}()
/// ```
/// for a table *table* and an index *index*.
///
/// Example:
///
/// ```no_run
/// # #[cfg(target_arch = "wasm32")] mod demo {
/// use spacetimedb::{table, PointIndex, ReducerContext, DbContext};
///
/// #[table(accessor = user,
///     index(accessor = dogs_and_name, hash(columns = [dogs, name])))]
/// struct User {
///     id: u32,
///     name: String,
///     /// Number of dogs owned by the user.
///     dogs: u64
/// }
///
/// fn demo(ctx: &ReducerContext) {
///     let by_dogs_and_name: PointIndex<_, (u64, String), _> = ctx.db.user().dogs_and_name();
/// }
/// # }
/// ```
///
/// For single-column indexes, use the name of the column:
///
/// ```no_run
/// # #[cfg(target_arch = "wasm32")] mod demo {
/// use spacetimedb::{table, PointIndex, ReducerContext, DbContext};
///
/// #[table(accessor = user)]
/// struct User {
///     id: u32,
///     username: String,
///     #[index(btree)]
///     dogs: u64
/// }
///
/// fn demo(ctx: &ReducerContext) {
///     let by_dogs: PointIndex<_, (u64,), _> = ctx.db().user().dogs();
/// }
/// # }
/// ```
///
pub struct PointIndex<Tbl: Table, IndexType, Idx: Index> {
    backend: TableHandleBackend,
    _marker: PhantomData<(Tbl, IndexType, Idx)>,
}

impl<Tbl: Table, IndexType, Idx: IndexIsPointed> PointIndex<Tbl, IndexType, Idx> {
    #[doc(hidden)]
    pub fn __new(backend: TableHandleBackend) -> Self {
        Self {
            backend,
            _marker: PhantomData,
        }
    }

    /// Returns an iterator over all rows in the database state
    /// where the indexed column(s) equal `point`.
    ///
    /// Unlike for ranged indices,
    /// this method only accepts a `point` and not any prefix or range.
    ///
    /// For example:
    ///
    /// ```no_run
    /// # #[cfg(target_arch = "wasm32")] mod demo {
    /// use spacetimedb::{table, ReducerContext, PointIndex};
    ///
    /// #[table(accessor = user,
    ///     index(accessor = dogs_and_name, hash(columns = [dogs, name])))]
    /// struct User {
    ///     id: u32,
    ///     name: String,
    ///     dogs: u64
    /// }
    ///
    /// fn demo(ctx: &ReducerContext) {
    ///     let by_dogs_and_name: PointIndex<_, (u64, String), _> = ctx.db.user().dogs_and_name();
    ///
    ///     // Find user with exactly 25 dogs and exactly the name "Joseph".
    ///     for user in by_dogs_and_name.filter((25u64, "Joseph")) {
    ///         /* ... */
    ///     }
    ///
    ///     // You can also pass arguments by reference if desired.
    ///     for user in by_dogs_and_name.filter((&25u64, &"Joseph".to_string())) {
    ///         /* ... */
    ///     }
    /// }
    /// # }
    /// ```
    pub fn filter<P, K>(&self, point: P) -> impl Iterator<Item = Tbl::Row> + use<P, K, Tbl, IndexType, Idx>
    where
        P: WithPointArg<K>,
    {
        filter_point::<Tbl, Idx, P, K>(&self.backend, point)
    }

    /// Deletes all rows in the database state
    /// where the indexed column(s) equal `point`.
    ///
    /// Unlike for ranged indices,
    /// this method only accepts a `point` and not any prefix or range.
    ///
    /// For example:
    ///
    /// ```no_run
    /// # #[cfg(target_arch = "wasm32")] mod demo {
    /// use spacetimedb::{table, ReducerContext, PointIndex};
    ///
    /// #[table(accessor = user,
    ///     index(accessor = dogs_and_name, hash(columns = [dogs, name])))]
    /// struct User {
    ///     id: u32,
    ///     name: String,
    ///     dogs: u64
    /// }
    ///
    /// fn demo(ctx: &ReducerContext) {
    ///     let by_dogs_and_name: PointIndex<_, (u64, String), _> = ctx.db.user().dogs_and_name();
    ///
    ///     // Delete users with exactly 25 dogs, and exactly the name "Joseph".
    ///     by_dogs_and_name.delete((25u64, "Joseph"));
    ///
    ///     // You can also pass arguments by reference if desired.
    ///     by_dogs_and_name.delete((&25u64, &"Joseph".to_string()));
    /// }
    /// # }
    /// ```
    ///
    /// May panic if deleting any one of the rows would violate a constraint,
    /// though at present no such constraints exist.
    pub fn delete<P, K>(&self, point: P) -> u64
    where
        P: WithPointArg<K>,
    {
        let index_id = self
            .backend
            .index_id(Idx::INDEX_NAME)
            .expect("index_id_from_name() call failed");
        point.with_point_arg(|point| {
            self.backend
                .delete_by_index_scan_point_bsatn(index_id, point)
                .unwrap_or_else(|e| panic!("unexpected error from `datastore_delete_by_index_scan_point_bsatn`: {e}"))
                .into()
        })
    }
}

/// Scans `Tbl` for `point` using the index `Idx`.
///
/// The type parameter `K` is either `()` or [`SingleBound`]
/// and is used to workaround the orphan rule.
fn filter_point<Tbl, Idx, P, K>(
    backend: &TableHandleBackend,
    point: P,
) -> impl Iterator<Item = Tbl::Row> + use<Tbl, Idx, P, K>
where
    Tbl: Table,
    Idx: IndexIsPointed,
    P: WithPointArg<K>,
{
    let index_id = backend
        .index_id(Idx::INDEX_NAME)
        .expect("index_id_from_name() call failed");
    let iter = point.with_point_arg(|point| datastore_index_scan_point_bsatn(backend, index_id, point));
    TableIter::new(iter)
}

/// A read-only handle to a Hash index.
///
/// This is the read-only version of [`PointIndex`].
/// It mirrors [`PointIndex`] but exposes only `.filter(..)`, not `.delete(..)`.
/// It is used by `{table}__ViewHandle` to keep view code read-only at compile time.
///
/// Note, the `Tbl` generic is the read-write table handle `{table}__TableHandle`.
/// This is because read-only indexes still need [`Table`] metadata.
/// The view handle itself deliberately does not implement `Table`.
pub struct PointIndexReadOnly<Tbl: Table, IndexType, Idx: Index> {
    backend: TableHandleBackend,
    _marker: PhantomData<(Tbl, IndexType, Idx)>,
}

impl<Tbl: Table, IndexType, Idx: IndexIsPointed> PointIndexReadOnly<Tbl, IndexType, Idx> {
    #[doc(hidden)]
    pub fn __new(backend: TableHandleBackend) -> Self {
        Self {
            backend,
            _marker: PhantomData,
        }
    }

    pub fn filter<P, K>(&self, point: P) -> impl Iterator<Item = Tbl::Row> + use<P, K, Tbl, IndexType, Idx>
    where
        P: WithPointArg<K>,
    {
        filter_point::<Tbl, Idx, P, K>(&self.backend, point)
    }
}

/// Trait used for running point index scans.
///
/// The type parameter `K` is either `()` or [`SingleBound`]
/// and is used to workaround the orphan rule.
pub trait WithPointArg<K = ()> {
    /// Runs `run` with the BSATN-serialized point to pass to the index scan.
    // TODO(perf, centril): once we have stable specialization,
    // just use `to_le_bytes` internally instead.
    #[doc(hidden)]
    fn with_point_arg<R>(&self, run: impl FnOnce(&[u8]) -> R) -> R;
}

impl<Arg: FilterableValue> WithPointArg<SingleBound> for Arg {
    fn with_point_arg<R>(&self, run: impl FnOnce(&[u8]) -> R) -> R {
        run(&IterBuf::serialize(self).unwrap())
    }
}

macro_rules! impl_with_point_arg {
    ($($arg:ident),+) => {
        impl<$($arg: FilterableValue),+> WithPointArg for ($($arg,)+) {
            fn with_point_arg<R>(&self, run: impl FnOnce(&[u8]) -> R) -> R {
                // We can assume here that we have a point bound.
                let mut data = IterBuf::take();

                // Destructure the argument tuple into variables with the same names as their types.
                #[allow(non_snake_case)]
                let ($($arg,)+) = self;

                // For each part in the tuple queried, serialize it into the `data` buffer.
                Ok(())
                    $(.and_then(|()| data.serialize_into($arg)))+
                    .unwrap();

                run(&*data)
            }
        }
    };
}

impl_with_point_arg!(A);
impl_with_point_arg!(A, B);
impl_with_point_arg!(A, B, C);
impl_with_point_arg!(A, B, C, D);
impl_with_point_arg!(A, B, C, D, E);
impl_with_point_arg!(A, B, C, D, E, F);

/// Marks an index as having range query capabilities.
///
/// This applies to BTree and Direct indices but not Hash indices.
pub trait IndexIsRanged: Index {}

/// A handle to a B-Tree or Direct index on a table.
///
/// To get one of these from a `ReducerContext`, use:
/// ```text
/// ctx.db.{table}().{index}()
/// ```
/// for a table *table* and an index *index*.
///
/// Example:
///
/// ```no_run
/// # #[cfg(target_arch = "wasm32")] mod demo {
/// use spacetimedb::{table, RangedIndex, ReducerContext, DbContext};
///
/// #[table(accessor = user,
///     index(accessor = dogs_and_name, btree(columns = [dogs, name])))]
/// struct User {
///     id: u32,
///     name: String,
///     /// Number of dogs owned by the user.
///     dogs: u64
/// }
///
/// fn demo(ctx: &ReducerContext) {
///     let by_dogs_and_name: RangedIndex<_, (u64, String), _> = ctx.db.user().dogs_and_name();
/// }
/// # }
/// ```
///
/// For single-column indexes, use the name of the column:
///
/// ```no_run
/// # #[cfg(target_arch = "wasm32")] mod demo {
/// use spacetimedb::{table, RangedIndex, ReducerContext, DbContext};
///
/// #[table(accessor = user)]
/// struct User {
///     id: u32,
///     username: String,
///     #[index(btree)]
///     dogs: u64
/// }
///
/// fn demo(ctx: &ReducerContext) {
///     let by_dogs: RangedIndex<_, (u64,), _> = ctx.db().user().dogs();
/// }
/// # }
/// ```
///
pub struct RangedIndex<Tbl: Table, IndexType, Idx: IndexIsRanged> {
    backend: TableHandleBackend,
    _marker: PhantomData<(Tbl, IndexType, Idx)>,
}

impl<Tbl: Table, IndexType, Idx: IndexIsRanged> RangedIndex<Tbl, IndexType, Idx> {
    #[doc(hidden)]
    pub fn __new(backend: TableHandleBackend) -> Self {
        Self {
            backend,
            _marker: PhantomData,
        }
    }

    /// Returns an iterator over all rows in the database state where the indexed column(s) match the bounds `b`.
    ///
    /// This method accepts a variable numbers of arguments using the [`IndexScanRangeBounds`] trait.
    /// This depends on the type of the B-Tree index. `b` may be:
    /// - A value for the first indexed column.
    /// - A range of values for the first indexed column.
    /// - A tuple of values for any prefix of the indexed columns, optionally terminated by a range for the next.
    ///
    /// For example:
    ///
    /// ```no_run
    /// # #[cfg(target_arch = "wasm32")] mod demo {
    /// use spacetimedb::{table, ReducerContext, RangedIndex};
    ///
    /// #[table(accessor = user,
    ///     index(accessor = dogs_and_name, btree(columns = [dogs, name])))]
    /// struct User {
    ///     id: u32,
    ///     name: String,
    ///     dogs: u64
    /// }
    ///
    /// fn demo(ctx: &ReducerContext) {
    ///     let by_dogs_and_name: RangedIndex<_, (u64, String), _> = ctx.db.user().dogs_and_name();
    ///
    ///     // Find user with exactly 25 dogs.
    ///     for user in by_dogs_and_name.filter(25u64) { // The `u64` is required, see below.
    ///         /* ... */
    ///     }
    ///
    ///     // Find user with at least 25 dogs.
    ///     for user in by_dogs_and_name.filter(25u64..) {
    ///         /* ... */
    ///     }
    ///
    ///     // Find user with exactly 25 dogs, and a name beginning with "J".
    ///     for user in by_dogs_and_name.filter((25u64, "J".."K")) {
    ///         /* ... */
    ///     }
    ///
    ///     // Find user with exactly 25 dogs, and exactly the name "Joseph".
    ///     for user in by_dogs_and_name.filter((25u64, "Joseph")) {
    ///         /* ... */
    ///     }
    ///
    ///     // You can also pass arguments by reference if desired.
    ///     for user in by_dogs_and_name.filter((&25u64, &"Joseph".to_string())) {
    ///         /* ... */
    ///     }
    /// }
    /// # }
    /// ```
    ///
    /// **NOTE:** An unfortunate interaction between Rust's trait solver and integer literal defaulting rules means that you must specify the types of integer literals passed to `filter` and `find` methods via the suffix syntax, like `21u32`.
    ///
    /// If you don't, you'll see a compiler error like:
    /// > ```text
    /// > error[E0271]: type mismatch resolving `<i32 as FilterableValue>::Column == u32`
    /// >    --> modules/rust-wasm-test/src/lib.rs:356:48
    /// >     |
    /// > 356 |     for person in ctx.db.person().age().filter(21) {
    /// >     |                                         ------ ^^ expected `u32`, found `i32`
    /// >     |                                         |
    /// >     |                                         required by a bound introduced by this call
    /// >     |
    /// >     = note: required for `i32` to implement `IndexScanRangeBounds<(u32,), SingleBound>`
    /// > note: required by a bound in `RangedIndex::<Tbl, IndexType, Idx>::filter`
    /// >     |
    /// > 410 |     pub fn filter<B, K>(&self, b: B) -> impl Iterator<Item = Tbl::Row>
    /// >     |            ------ required by a bound in this associated function
    /// > 411 |     where
    /// > 412 |         B: IndexScanRangeBounds<IndexType, K>,
    /// >     |            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ required by this bound in `RangedIndex::<Tbl, IndexType, Idx>::filter`
    /// > ```
    /// <!-- TODO: check if that error is up to date! -->
    pub fn filter<B, K>(&self, b: B) -> impl Iterator<Item = Tbl::Row> + use<B, K, Tbl, IndexType, Idx>
    where
        B: IndexScanRangeBounds<IndexType, K>,
    {
        filter::<Tbl, Idx, IndexType, B, K>(&self.backend, b)
    }

    /// Deletes all rows in the database state where the indexed column(s) match the bounds `b`.
    ///
    /// This method accepts a variable numbers of arguments using the [`IndexScanRangeBounds`] trait.
    /// This depends on the type of the B-Tree index. `b` may be:
    /// - A value for the first indexed column.
    /// - A range of values for the first indexed column.
    /// - A tuple of values for any prefix of the indexed columns, optionally terminated by a range for the next.
    ///
    /// For example:
    ///
    /// ```no_run
    /// # #[cfg(target_arch = "wasm32")] mod demo {
    /// use spacetimedb::{table, ReducerContext, RangedIndex};
    ///
    /// #[table(accessor = user,
    ///     index(accessor = dogs_and_name, btree(columns = [dogs, name])))]
    /// struct User {
    ///     id: u32,
    ///     name: String,
    ///     dogs: u64
    /// }
    ///
    /// fn demo(ctx: &ReducerContext) {
    ///     let by_dogs_and_name: RangedIndex<_, (u64, String), _> = ctx.db.user().dogs_and_name();
    ///
    ///     // Delete users with exactly 25 dogs.
    ///     by_dogs_and_name.delete(25u64); // The `u64` is required, see below.
    ///
    ///     // Delete users with at least 25 dogs.
    ///     by_dogs_and_name.delete(25u64..);
    ///
    ///     // Delete users with exactly 25 dogs, and a name beginning with "J".
    ///     by_dogs_and_name.delete((25u64, "J".."K"));
    ///
    ///     // Delete users with exactly 25 dogs, and exactly the name "Joseph".
    ///     by_dogs_and_name.delete((25u64, "Joseph"));
    ///
    ///     // You can also pass arguments by reference if desired.
    ///     by_dogs_and_name.delete((&25u64, &"Joseph".to_string()));
    /// }
    /// # }
    /// ```
    ///
    /// **NOTE:** An unfortunate interaction between Rust's trait solver and integer literal defaulting rules means that you must specify the types of integer literals passed to `filter` and `find` methods via the suffix syntax, like `21u32`.
    ///
    /// If you don't, you'll see a compiler error like:
    /// > ```text
    /// > error[E0271]: type mismatch resolving `<i32 as FilterableValue>::Column == u32`
    /// >    --> modules/rust-wasm-test/src/lib.rs:356:48
    /// >     |
    /// > 356 |     for person in ctx.db.person().age().filter(21) {
    /// >     |                                         ------ ^^ expected `u32`, found `i32`
    /// >     |                                         |
    /// >     |                                         required by a bound introduced by this call
    /// >     |
    /// >     = note: required for `i32` to implement `IndexScanRangeBounds<(u32,), SingleBound>`
    /// > note: required by a bound in `RangedIndex::<Tbl, IndexType, Idx>::filter`
    /// >     |
    /// > 410 |     pub fn filter<B, K>(&self, b: B) -> impl Iterator<Item = Tbl::Row>
    /// >     |            ------ required by a bound in this associated function
    /// > 411 |     where
    /// > 412 |         B: IndexScanRangeBounds<IndexType, K>,
    /// >     |            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ required by this bound in `RangedIndex::<Tbl, IndexType, Idx>::filter`
    /// > ```
    ///
    /// May panic if deleting any one of the rows would violate a constraint,
    /// though at present no such constraints exist.
    pub fn delete<B, K>(&self, b: B) -> u64
    where
        B: IndexScanRangeBounds<IndexType, K>,
    {
        let index_id = self
            .backend
            .index_id(Idx::INDEX_NAME)
            .expect("index_id_from_name() call failed");
        if const { is_point_scan::<Idx, B, _, _>() } {
            b.with_point_arg(|point| {
                self.backend
                    .delete_by_index_scan_point_bsatn(index_id, point)
                    .unwrap_or_else(|e| {
                        panic!("unexpected error from `datastore_delete_by_index_scan_point_bsatn`: {e}")
                    })
                    .into()
            })
        } else {
            let args = b.get_range_args();
            let (prefix, prefix_elems, rstart, rend) = args.args_for_syscall();
            self.backend
                .delete_by_index_scan_range_bsatn(index_id, prefix, prefix_elems, rstart, rend)
                .unwrap_or_else(|e| panic!("unexpected error from `datastore_delete_by_index_scan_range_bsatn`: {e}"))
                .into()
        }
    }
}

/// Performs a ranged scan using the range arguments `B` in `Tbl` using `Idx`.
///
/// The type parameter `K` is either `()` or [`SingleBound`]
/// and is used to workaround the orphan rule.
fn filter<Tbl, Idx, IndexType, B, K>(
    backend: &TableHandleBackend,
    b: B,
) -> impl Iterator<Item = Tbl::Row> + use<Tbl, Idx, IndexType, B, K>
where
    Tbl: Table,
    Idx: Index,
    B: IndexScanRangeBounds<IndexType, K>,
{
    let index_id = backend
        .index_id(Idx::INDEX_NAME)
        .expect("index_id_from_name() call failed");

    let iter = if const { is_point_scan::<Idx, B, _, _>() } {
        b.with_point_arg(|point| datastore_index_scan_point_bsatn(backend, index_id, point))
    } else {
        let args = b.get_range_args();
        let (prefix, prefix_elems, rstart, rend) = args.args_for_syscall();
        backend
            .index_scan_range_bsatn(index_id, prefix, prefix_elems, rstart, rend)
            .unwrap_or_else(|e| panic!("unexpected error from `datastore_index_scan_range_bsatn`: {e}"))
    };

    TableIter::new(iter)
}

/// A read-only handle to a B-tree or Direct index.
///
/// This is the read-only version of [`RangedIndex`].
/// It mirrors [`RangedIndex`] but exposes only `.filter(..)`, not `.delete(..)`.
/// It is used by `{table}__ViewHandle` to keep view code read-only at compile time.
///
/// Note, the `Tbl` generic is the read-write table handle `{table}__TableHandle`.
/// This is because read-only indexes still need [`Table`] metadata.
/// The view handle itself deliberately does not implement `Table`.
pub struct RangedIndexReadOnly<Tbl: Table, IndexType, Idx: Index> {
    backend: TableHandleBackend,
    _marker: PhantomData<(Tbl, IndexType, Idx)>,
}

impl<Tbl: Table, IndexType, Idx: Index> RangedIndexReadOnly<Tbl, IndexType, Idx> {
    #[doc(hidden)]
    pub fn __new(backend: TableHandleBackend) -> Self {
        Self {
            backend,
            _marker: PhantomData,
        }
    }

    pub fn filter<B, K>(&self, b: B) -> impl Iterator<Item = Tbl::Row> + use<B, K, Tbl, IndexType, Idx>
    where
        B: IndexScanRangeBounds<IndexType, K>,
    {
        filter::<Tbl, Idx, IndexType, B, K>(&self.backend, b)
    }
}

/// Returns whether `B` is a point scan on `I`.
///
/// The type parameter `K` is either `()` or [`SingleBound`]
/// and is used to workaround the orphan rule.
const fn is_point_scan<I: Index, B: IndexScanRangeBounds<T, K>, T, K>() -> bool {
    B::POINT && B::COLS_PROVIDED == I::NUM_COLS_INDEXED
}

/// Trait used for overloading methods on [`RangedIndex`].
/// See [`RangedIndex`] for more information.
///
/// The type parameter `K` is either `()` or [`SingleBound`]
/// and is used to workaround the orphan rule.
pub trait IndexScanRangeBounds<T, K = ()> {
    /// True if no range occurs in this range bounds.
    #[doc(hidden)]
    const POINT: bool;

    /// The number of columns mentioned in this range bounds.
    /// For `(42, 12..24)` it's `2`.
    #[doc(hidden)]
    const COLS_PROVIDED: usize;

    // TODO(perf, centril): once we have stable specialization,
    // just use `to_le_bytes` internally instead.
    #[doc(hidden)]
    fn with_point_arg<R>(&self, run: impl FnOnce(&[u8]) -> R) -> R;

    #[doc(hidden)]
    fn get_range_args(&self) -> IndexScanRangeArgs;
}

#[doc(hidden)]
/// Arguments to one of the ranged-index-scan-related host-/sys-calls.
///
/// All pointers passed into the syscall are packed into a single buffer, `data`,
/// with slices taken at the appropriate offsets, to save allocatons in WASM.
pub struct IndexScanRangeArgs {
    data: IterBuf,
    prefix_elems: usize,
    rstart_idx: usize,
    // None if rstart and rend are the same
    rend_idx: Option<usize>,
}

impl IndexScanRangeArgs {
    /// Get slices into `self.data` for the prefix, range start and range end.
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

// Implement `IndexScanRangeBounds` for all the different index column types
// and filter argument types we support.
macro_rules! impl_index_scan_range_bounds {
    // In the first pattern, we accept two Prolog-style lists of type variables,
    // the first of which we use for the column types in the index,
    // and the second for the arguments supplied to the filter function.
    // We do our "outer recursion" to visit the sublists of these two lists,
    // at each step implementing the trait for indexes of that many columns.
    //
    // There's also an "inner recursion" later on, which, given a fixed number of columns,
    // implements the trait with the arguments being all the prefixes of that list.
    (($ColTerminator:ident $(, $ColPrefix:ident)*), ($ArgTerminator:ident $(, $ArgPrefix:ident)*)) => {
        // Implement the trait for all arguments N-column indexes.
        // The "inner recursion" described above happens in here.
        impl_index_scan_range_bounds!(@inner_recursion (), ($ColTerminator $(, $ColPrefix)*), ($ArgTerminator $(, $ArgPrefix)*));

        // Recurse on the suffix of the two lists, to implement the trait for all arguments to (N - 1)-column indexes.
        impl_index_scan_range_bounds!(($($ColPrefix),*), ($($ArgPrefix),*));
    };
    // Base case for the previous "outer recursion."
    ((), ()) => {};

    // The recursive case for the inner loop.
    //
    // When we start this recursion, `$ColUnused` will be empty,
    // so we'll implement N-element queries on N-column indexes.
    // The next call will move one type name from `($ColTerminator, $ColPrefix)` into `$ColUnused`,
    // so we'll implement (N - 1)-element queries on N-column indexes.
    // And so on.
    (@inner_recursion ($($ColUnused:ident),*), ($ColTerminator:ident $(, $ColPrefix:ident)+), ($ArgTerminator:ident $(, $ArgPrefix:ident)+)) => {
        // Emit the actual `impl IndexScanRangeBounds` form for M-element queries on N-column indexes.
        impl_index_scan_range_bounds!(@emit_impl ($($ColUnused),*), ($ColTerminator $(,$ColPrefix)*), ($ArgTerminator $(, $ArgPrefix)*));
        // Recurse, to implement for (M - 1)-element queries on N-column indexes.
        impl_index_scan_range_bounds!(@inner_recursion ($($ColUnused,)* $ColTerminator), ($($ColPrefix),*), ($($ArgPrefix),*));
    };
    // Base case for the inner recursive loop, when there is only one column remaining.
    // Implement the trait for both single-element tuples of arguments,
    // and for an argument passed outside of a tuple.
    //
    // As in the following `@emit_impl` case:
    // - `$ColUnused` are the types of the ignored suffix of the indexed columns.
    // - `$ColTerminator` is the type of the queried indexed column,
    //   which may have a range supplied as its argument.
    // - `$ArgTerminator` is the type of the argument provided for the queried column.
    //   More precisely it is the "inner" type, like `i32` or `&str`,
    //   which may be wrapped in a range like `std::ops::Range<$ArgTerminator>`.
    // - `Term` (not a meta-variable) is the type of the range wrapped around the `$ArgTerminator`.
    (@inner_recursion ($($ColUnused:ident),*), ($ColTerminator:ident), ($ArgTerminator:ident)) => {
        // Implementation for one-element tuples: defer to the implementation for bare values.
        impl<
            $($ColUnused,)*
            $ColTerminator,
            Term: IndexScanRangeBoundsTerminator<Arg = $ArgTerminator>,
            $ArgTerminator: FilterableValue<Column = $ColTerminator>,
        > IndexScanRangeBounds<($ColTerminator, $($ColUnused,)*)> for (Term,) {
            const POINT: bool = Term::POINT;
            const COLS_PROVIDED: usize = 1;

            fn with_point_arg<R>(&self, run: impl FnOnce(&[u8]) -> R) -> R {
                IndexScanRangeBounds::<($ColTerminator, $($ColUnused,)*), SingleBound>::with_point_arg(&self.0, run)
            }

            fn get_range_args(&self) -> IndexScanRangeArgs {
                IndexScanRangeBounds::<($ColTerminator, $($ColUnused,)*), SingleBound>::get_range_args(&self.0)
            }
        }
        // Implementation for bare values: serialize the value as the terminating bounds.
        impl<
            $($ColUnused,)*
            $ColTerminator,
            Term: IndexScanRangeBoundsTerminator<Arg = $ArgTerminator>,
            $ArgTerminator: FilterableValue<Column = $ColTerminator>,
        > IndexScanRangeBounds<($ColTerminator, $($ColUnused,)*), SingleBound> for Term {
            const POINT: bool = Term::POINT;
            const COLS_PROVIDED: usize = 1;

            fn with_point_arg<R>(&self, run: impl FnOnce(&[u8]) -> R) -> R {
                // We can assume here that we have a point bound.
                run(&IterBuf::serialize(self.point()).unwrap())
            }

            fn get_range_args(&self) -> IndexScanRangeArgs {
                let mut data = IterBuf::take();
                let rend_idx = self.bounds().serialize_into(&mut data);
                IndexScanRangeArgs { data, prefix_elems: 0, rstart_idx: 0, rend_idx }
            }
        }
    };

    // - `$ColUnused` are the types of the ignored suffix of the indexed columns.
    // - `$ColTerminator` is the type of the last queried indexed column,
    //   which may have a range supplied as its argument.
    // - `$ColPrefix` are the types of the queried prefix of the indexed columns,
    //   which must have single values supplied as their arguments.
    // - `$ArgTerminator` is the type of the argument provided for the last queried column.
    //   More precisely it is the "inner" type, like `i32` or `&str`,
    //   which may be wrapped in a range like `std::ops::Range<$ArgTerminator>`.
    // - `Term` (not a meta-variable) is the type of the range wrapped around the `$ArgTerminator`.
    // - `$ArgPrefix` are the types of the arguments provided for the queried prefix columns.
    (@emit_impl ($($ColUnused:ident),*), ($ColTerminator:ident $(, $ColPrefix:ident)+), ($ArgTerminator:ident $(, $ArgPrefix:ident)+)) => {
        impl<
            $($ColUnused,)*
            $ColTerminator,
            $($ColPrefix,)*
            Term: IndexScanRangeBoundsTerminator<Arg = $ArgTerminator>,
            $ArgTerminator: FilterableValue<Column = $ColTerminator>,
            $($ArgPrefix: FilterableValue<Column = $ColPrefix>,)+
        > IndexScanRangeBounds<
            ($($ColPrefix,)+
             $ColTerminator,
             $($ColUnused,)*)
          > for ($($ArgPrefix,)+ Term,) {
            const POINT: bool = Term::POINT;
            const COLS_PROVIDED: usize = 1 + impl_index_scan_range_bounds!(@count $($ColPrefix)+);

            fn with_point_arg<R>(&self, run: impl FnOnce(&[u8]) -> R) -> R {
                // We can assume here that we have a point bound.
                let mut data = IterBuf::take();

                // Destructure the argument tuple into variables with the same names as their types.
                #[allow(non_snake_case)]
                let ($($ArgPrefix,)+ term,) = self;

                // For each part in the tuple queried, serialize it into the `data` buffer.
                Ok(())
                    $(.and_then(|()| data.serialize_into($ArgPrefix)))+
                    .and_then(|()| data.serialize_into(term.point()))
                    .unwrap();

                run(&*data)
            }

            fn get_range_args(&self) -> IndexScanRangeArgs {
                let mut data = IterBuf::take();

                // Get the number of prefix elements.
                let prefix_elems = impl_index_scan_range_bounds!(@count $($ColPrefix)+);

                // Destructure the argument tuple into variables with the same names as their types.
                #[allow(non_snake_case)]
                let ($($ArgPrefix,)+ term,) = self;

                // For each prefix queried, serialize it into the `data` buffer.
                Ok(())
                    $(.and_then(|()| data.serialize_into($ArgPrefix)))+
                    .unwrap();

                // Remember the separator between the prefix and the terminator,
                // so that we can slice them separately and pass them to the appropriate filter host call.
                let rstart_idx = data.len();

                // Serialize the terminating range,
                // and get the info required to separately slice the lower and upper bounds of that range
                // since the host call takes those as separate slices.
                let rend_idx = term.bounds().serialize_into(&mut data);
                IndexScanRangeArgs { data, prefix_elems, rstart_idx, rend_idx }
            }
        }
    };

    // Counts the number of elements in the tuple.
    (@count $($T:ident)*) => {
        0 $(+ impl_index_scan_range_bounds!(@drop $T 1))*
    };
    (@drop $a:tt $b:tt) => { $b };
}

pub struct SingleBound;

impl_index_scan_range_bounds!(
    (ColA, ColB, ColC, ColD, ColE, ColF),
    (ArgA, ArgB, ArgC, ArgD, ArgE, ArgF)
);

// Single-column indexes
// impl<T> IndexScanRangeBounds<(T,)> for Range<T> {}
// impl<T> IndexScanRangeBounds<(T,)> for T {}

// // Two-column indexes
// impl<T, U> IndexScanRangeBounds<(T, U)> for Range<T> {}
// impl<T, U> IndexScanRangeBounds<(T, U)> for T {}
// impl<T, U> IndexScanRangeBounds<(T, U)> for (T, Range<U>) {}
// impl<T, U> IndexScanRangeBounds<(T, U)> for (T, U) {}

// // Three-column indexes
// impl<T, U, V> IndexScanRangeBounds<(T, U, V)> for Range<T> {}
// impl<T, U, V> IndexScanRangeBounds<(T, U, V)> for T {}
// impl<T, U, V> IndexScanRangeBounds<(T, U, V)> for (T, Range<U>) {}
// impl<T, U, V> IndexScanRangeBounds<(T, U, V)> for (T, U) {}
// impl<T, U, V> IndexScanRangeBounds<(T, U, V)> for (T, U, Range<V>) {}
// impl<T, U, V> IndexScanRangeBounds<(T, U, V)> for (T, U, V) {}

/// A trait for types that can have a sequence based on them.
/// This is used for auto-inc columns to determine if an insertion of a row
/// will require the column to be updated in the row.
pub trait SequenceTrigger: Sized {
    /// Is this value one that will trigger a sequence, if any,
    /// when used as a column value.
    /// For numeric types, this is `0`.
    fn is_sequence_trigger(&self) -> bool;
    /// Should invoke `BufReader::get_{Self}`, for example `BufReader::get_u32`.
    fn decode(reader: &mut &[u8]) -> Result<Self, DecodeError>;
    /// Read a generated column from the slice, if this row was a sequence trigger.
    #[inline(always)]
    fn maybe_decode_into(&mut self, gen_cols: &mut &[u8]) {
        if self.is_sequence_trigger() {
            *self = Self::decode(gen_cols).unwrap_or_else(|_| sequence_decode_error())
        }
    }
}

#[cold]
#[inline(never)]
fn sequence_decode_error() -> ! {
    unreachable!("a row was a sequence trigger but there was no generated column for it.")
}

macro_rules! impl_seq_trigger {
    ($($get:ident($t:ty),)*) => {
        $(
            impl SequenceTrigger for $t {
                #[inline(always)]
                fn is_sequence_trigger(&self) -> bool { *self == 0 }
                #[inline(always)]
                fn decode(reader: &mut &[u8]) -> Result<Self, DecodeError> {
                    reader.$get()
                }
            }
        )*
    };
}

impl_seq_trigger!(
    get_u8(u8),
    get_i8(i8),
    get_u16(u16),
    get_i16(i16),
    get_u32(u32),
    get_i32(i32),
    get_u64(u64),
    get_i64(i64),
    get_u128(u128),
    get_i128(i128),
);

impl SequenceTrigger for crate::sats::i256 {
    #[inline(always)]
    fn is_sequence_trigger(&self) -> bool {
        *self == Self::ZERO
    }
    #[inline(always)]
    fn decode(reader: &mut &[u8]) -> Result<Self, DecodeError> {
        reader.get_i256()
    }
}

impl SequenceTrigger for crate::sats::u256 {
    #[inline(always)]
    fn is_sequence_trigger(&self) -> bool {
        *self == Self::ZERO
    }
    #[inline(always)]
    fn decode(reader: &mut &[u8]) -> Result<Self, DecodeError> {
        reader.get_u256()
    }
}

/// Insert a row of type `T` into the table identified by `table_id`.
#[track_caller]
fn insert<T: Table>(table: &T, mut row: T::Row, mut buf: IterBuf) -> Result<T::Row, TryInsertError<T>> {
    let table_id = table
        .__backend()
        .table_id(T::TABLE_NAME)
        .expect("table_id_from_name() call failed");
    // Encode the row as bsatn into the buffer `buf`.
    buf.clear();
    buf.serialize_into(&row).unwrap();

    // Insert row into table.
    // When table has an auto-incrementing column, we must re-decode the changed `buf`.
    let res = table.__backend().insert_bsatn(table_id, &mut buf).map(|gen_cols| {
        // Let the caller handle any generated columns written back by `sys::datastore_insert_bsatn` to `buf`.
        T::integrate_generated_columns(&mut row, &gen_cols);
        row
    });
    res.map_err(|e| {
        let err = match e {
            sys::Errno::UNIQUE_ALREADY_EXISTS => {
                T::UniqueConstraintViolation::get().map(TryInsertError::UniqueConstraintViolation)
            }
            sys::Errno::AUTO_INC_OVERFLOW => T::AutoIncOverflow::get().map(TryInsertError::AutoIncOverflow),
            _ => None,
        };
        err.unwrap_or_else(|| panic!("unexpected insertion error: {e}"))
    })
}

/// Update a row of type `T` to `row` using the index identified by `index_id`.
#[track_caller]
fn update<T: Table>(backend: &TableHandleBackend, index_id: IndexId, mut row: T::Row, mut buf: IterBuf) -> T::Row {
    let table_id = backend
        .table_id(T::TABLE_NAME)
        .expect("table_id_from_name() call failed");
    // Encode the row as bsatn into the buffer `buf`.
    buf.clear();
    buf.serialize_into(&row).unwrap();

    // Insert row into table.
    // When table has an auto-incrementing column, we must re-decode the changed `buf`.
    let res = backend.update_bsatn(table_id, index_id, &mut buf).map(|gen_cols| {
        // Let the caller handle any generated columns written back by `sys::datastore_update_bsatn` to `buf`.
        T::integrate_generated_columns(&mut row, &gen_cols);
        row
    });

    // TODO(centril): introduce a `TryUpdateError`.
    res.unwrap_or_else(|e| panic!("unexpected update error: {e}"))
}

/// A table iterator which yields values of the `TableType` corresponding to the table.
enum TableIterInner {
    #[cfg(target_arch = "wasm32")]
    Host(sys::RowIter),
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(dead_code)]
    Host,
    #[cfg(all(feature = "test-utils", not(target_arch = "wasm32")))]
    Test(std::vec::IntoIter<Vec<u8>>),
}

impl TableIterInner {
    fn is_exhausted(&self) -> bool {
        match self {
            #[cfg(target_arch = "wasm32")]
            Self::Host(iter) => iter.is_exhausted(),
            #[cfg(not(target_arch = "wasm32"))]
            Self::Host => true,
            #[cfg(all(feature = "test-utils", not(target_arch = "wasm32")))]
            Self::Test(iter) => iter.as_slice().is_empty(),
        }
    }
}

struct TableIter<T: DeserializeOwned> {
    /// The underlying source of our `Buffer`s.
    inner: TableIterInner,

    /// The current position in the buffer, from which `deserializer` can read.
    reader: Cursor<IterBuf>,

    _marker: PhantomData<T>,
}

impl<T: DeserializeOwned> TableIter<T> {
    #[inline]
    fn new(iter: TableIterInner) -> Self {
        TableIter::new_with_buf(iter, IterBuf::take())
    }

    #[inline]
    fn new_with_buf(iter: TableIterInner, mut buf: IterBuf) -> Self {
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
            match &mut self.inner {
                #[cfg(target_arch = "wasm32")]
                TableIterInner::Host(iter) => {
                    if iter.is_exhausted() {
                        return None;
                    }

                    // Otherwise, try to fetch the next chunk while reusing the buffer.
                    self.reader.buf.clear();
                    self.reader.pos.set(0);
                    iter.read(&mut self.reader.buf);
                }
                #[cfg(not(target_arch = "wasm32"))]
                TableIterInner::Host => return None,
                #[cfg(all(feature = "test-utils", not(target_arch = "wasm32")))]
                TableIterInner::Test(iter) => {
                    let row = iter.next()?;
                    return Some(bsatn::from_slice(&row).expect("Failed to decode row!"));
                }
            }
        }
    }
}
