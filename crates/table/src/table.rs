use crate::var_len::VarLenMembers;

use super::{
    bflatn_from::serialize_row_from_page,
    bflatn_to::write_row_to_pages,
    bflatn_to_bsatn_fast_path::StaticBsatnLayout,
    blob_store::{BlobStore, NullBlobStore},
    btree_index::{BTreeIndex, BTreeIndexRangeIter},
    eq::eq_row_in_page,
    eq_to_pv::eq_row_in_page_to_pv,
    indexes::{Bytes, PageIndex, PageOffset, RowHash, RowPointer, Size, SquashedOffset, PAGE_DATA_SIZE},
    layout::RowTypeLayout,
    page::{FixedLenRowsIter, Page},
    pages::Pages,
    pointer_map::PointerMap,
    read_column::{ReadColumn, TypeError},
    row_hash::hash_row_in_page,
    row_type_visitor::{row_type_visitor, VarLenVisitorProgram},
    static_assert_size, MemoryUsage,
};
use core::hash::{Hash, Hasher};
use core::ops::RangeBounds;
use core::{fmt, ptr};
use derive_more::{Add, AddAssign, From, Sub};
use spacetimedb_data_structures::map::{DefaultHashBuilder, HashCollectionExt, HashMap};
use spacetimedb_lib::{bsatn::DecodeError, de::DeserializeOwned};
use spacetimedb_primitives::{ColId, ColList, IndexId};
use spacetimedb_sats::{
    algebraic_value::ser::ValueSerializer,
    bsatn::{self, ser::BsatnError, ToBsatn},
    product_value::InvalidFieldError,
    satn::Satn,
    ser::{Serialize, Serializer},
    AlgebraicValue, ProductType, ProductValue,
};
use spacetimedb_schema::schema::TableSchema;
use std::sync::Arc;
use thiserror::Error;

/// The number of bytes used by, added to, or removed from a [`Table`]'s share of a [`BlobStore`].
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default, From, Add, Sub, AddAssign)]
pub struct BlobNumBytes(usize);

impl MemoryUsage for BlobNumBytes {}

/// A database table containing the row schema, the rows, and indices.
///
/// The table stores the rows into a page manager
/// and uses an internal map to ensure that no identical row is stored more than once.
pub struct Table {
    /// Page manager and row layout grouped together, for `RowRef` purposes.
    inner: TableInner,
    /// Maps `RowHash -> [RowPointer]` where a [`RowPointer`] points into `pages`.
    pointer_map: PointerMap,
    /// The indices associated with a set of columns of the table.
    pub indexes: HashMap<ColList, BTreeIndex>,
    /// The schema of the table, from which the type, and other details are derived.
    pub schema: Arc<TableSchema>,
    /// `SquashedOffset::TX_STATE` or `SquashedOffset::COMMITTED_STATE`
    /// depending on whether this is a tx scratchpad table
    /// or a committed table.
    squashed_offset: SquashedOffset,
    /// Store number of rows present in table.
    pub row_count: u64,
    /// Stores the sum total number of bytes that each blob object in the table occupies.
    ///
    /// Note that the [`HashMapBlobStore`] does ref-counting and de-duplication,
    /// but this sum will count an object each time its hash is mentioned, rather than just once.
    blob_store_bytes: BlobNumBytes,
}

/// The part of a `Table` concerned only with storing rows.
///
/// Separated from the "outer" parts of `Table`, especially the `indexes`,
/// so that `RowRef` can borrow only the `TableInner`,
/// while other mutable references to the `indexes` exist.
/// This is necessary because index insertions and deletions take a `RowRef` as an argument,
/// from which they [`ReadColumn::read_column`] their keys.
pub(crate) struct TableInner {
    /// The type of rows this table stores, with layout information included.
    row_layout: RowTypeLayout,
    /// A [`StaticBsatnLayout`] for fast BFLATN -> BSATN serialization,
    /// if the [`RowTypeLayout`] has a static BSATN length and layout.
    static_bsatn_layout: Option<StaticBsatnLayout>,
    /// The visitor program for `row_layout`.
    ///
    /// Must be in the `TableInner` so that [`RowRef::blob_store_bytes`] can use it.
    visitor_prog: VarLenVisitorProgram,
    /// The page manager that holds rows
    /// including both their fixed and variable components.
    pages: Pages,
}

impl TableInner {
    /// Assumes `ptr` is a present row in `self` and returns a [`RowRef`] to it.
    ///
    /// # Safety
    ///
    /// The requirement is that `table.is_row_present(ptr)` must hold,
    /// where `table` is the `Table` which contains this `TableInner`.
    /// That is, `ptr` must refer to a row within `self`
    /// which was previously inserted and has not been deleted since.
    ///
    /// This means:
    /// - The `PageIndex` of `ptr` must be in-bounds for `self.pages`.
    /// - The `PageOffset` of `ptr` must be properly aligned for the row type of `self`,
    ///   and must refer to a valid, live row in that page.
    /// - The `SquashedOffset` of `ptr` must match the enclosing table's `table.squashed_offset`.
    ///
    /// Showing that `ptr` was the result of a call to [`Table::insert(table, ..)`]
    /// and has not been passed to [`Table::delete(table, ..)`]
    /// is sufficient to demonstrate all of these properties.
    unsafe fn get_row_ref_unchecked<'a>(&'a self, blob_store: &'a dyn BlobStore, ptr: RowPointer) -> RowRef<'a> {
        // SAFETY: Forward caller requirements.
        unsafe { RowRef::new(self, blob_store, ptr) }
    }

    fn try_page_and_offset(&self, ptr: RowPointer) -> Option<(&Page, PageOffset)> {
        (ptr.page_index().idx() < self.pages.len()).then(|| (&self.pages[ptr.page_index()], ptr.page_offset()))
    }

    /// Returns the page and page offset that `ptr` points to.
    fn page_and_offset(&self, ptr: RowPointer) -> (&Page, PageOffset) {
        self.try_page_and_offset(ptr).unwrap()
    }
}

static_assert_size!(Table, 256);

impl MemoryUsage for Table {
    fn heap_usage(&self) -> usize {
        let Self {
            inner,
            pointer_map,
            indexes,
            // MEMUSE: intentionally ignoring schema
            schema: _,
            squashed_offset,
            row_count,
            blob_store_bytes,
        } = self;
        inner.heap_usage()
            + pointer_map.heap_usage()
            + indexes.heap_usage()
            + squashed_offset.heap_usage()
            + row_count.heap_usage()
            + blob_store_bytes.heap_usage()
    }
}

impl MemoryUsage for TableInner {
    fn heap_usage(&self) -> usize {
        let Self {
            row_layout,
            static_bsatn_layout,
            visitor_prog,
            pages,
        } = self;
        row_layout.heap_usage() + static_bsatn_layout.heap_usage() + visitor_prog.heap_usage() + pages.heap_usage()
    }
}

/// Various error that can happen on table insertion.
#[derive(Error, Debug)]
pub enum InsertError {
    /// There was already a row with the same value.
    #[error("Duplicate insertion of row {0:?} violates set semantics")]
    Duplicate(RowPointer),

    /// Couldn't write the row to the page manager.
    #[error(transparent)]
    Bflatn(#[from] super::bflatn_to::Error),

    /// Some index related error occurred.
    #[error(transparent)]
    IndexError(#[from] UniqueConstraintViolation),
}

/// Errors that can occur while trying to read a value via bsatn.
#[derive(Error, Debug)]
pub enum ReadViaBsatnError {
    #[error(transparent)]
    BSatnError(#[from] BsatnError),

    #[error(transparent)]
    DecodeError(#[from] DecodeError),
}

// Public API:
impl Table {
    /// Creates a new empty table with the given `schema` and `squashed_offset`.
    pub fn new(schema: Arc<TableSchema>, squashed_offset: SquashedOffset) -> Self {
        let row_layout: RowTypeLayout = schema.get_row_type().clone().into();
        let static_bsatn_layout = StaticBsatnLayout::for_row_type(&row_layout);
        let visitor_prog = row_type_visitor(&row_layout);
        Self::new_with_indexes_capacity(
            schema,
            row_layout,
            static_bsatn_layout,
            visitor_prog,
            squashed_offset,
            0,
        )
    }

    /// Check if the `row` conflicts with any unique index on `self`,
    /// and if there is a conflict, return `Err`.
    ///
    /// `is_deleted` is a predicate which, for a given row pointer,
    /// returns true if and only if that row should be ignored.
    /// While checking unique constraints against the committed state,
    /// `MutTxId::insert` will ignore rows which are listed in the delete table.
    pub fn check_unique_constraints(
        &self,
        row: &ProductValue,
        mut is_deleted: impl FnMut(RowPointer) -> bool,
    ) -> Result<(), UniqueConstraintViolation> {
        for (cols, index) in self.indexes.iter().filter(|(_, index)| index.is_unique) {
            let value = row.project(cols).unwrap();
            if let Some(mut conflicts) = index.get_rows_that_violate_unique_constraint(&value) {
                if conflicts.any(|ptr| !is_deleted(ptr)) {
                    return Err(self.build_error_unique(index, cols, value));
                }
            }
        }
        Ok(())
    }

    /// Insert a `row` into this table, storing its large var-len members in the `blob_store`.
    ///
    /// On success, returns the hash of the newly-inserted row,
    /// and a `RowRef` referring to the row.
    ///
    /// When a row equal to `row` already exists in `self`,
    /// returns `InsertError::Duplicate(existing_row_pointer)`,
    /// where `existing_row_pointer` is a `RowPointer` which identifies the existing row.
    /// In this case, the duplicate is not inserted,
    /// but internal data structures may be altered in ways that affect performance and fragmentation.
    ///
    /// TODO(error-handling): describe errors from `write_row_to_pages` and return meaningful errors.
    pub fn insert<'a>(
        &'a mut self,
        blob_store: &'a mut dyn BlobStore,
        row: &ProductValue,
    ) -> Result<(RowHash, RowRef<'a>), InsertError> {
        // Check unique constraints.
        // This error should take precedence over any other potential failures.
        self.check_unique_constraints(
            row,
            // No need to worry about the committed vs tx state dichotomy here;
            // just treat all rows in the table as live.
            |_| false,
        )?;

        // Insert the row into the page manager.
        let (hash, ptr) = self.insert_internal(blob_store, row)?;

        // SAFETY: We just inserted `ptr`, so it must be present.
        let row_ref = unsafe { self.inner.get_row_ref_unchecked(blob_store, ptr) };

        // Insert row into indices.
        for (cols, index) in self.indexes.iter_mut() {
            index.insert(cols, row_ref).unwrap();
        }

        Ok((hash, row_ref))
    }

    /// Insert a `row` into this table.
    /// NOTE: This method skips index updating. Use `insert` to insert a row with index updating.
    pub fn insert_internal(
        &mut self,
        blob_store: &mut dyn BlobStore,
        row: &ProductValue,
    ) -> Result<(RowHash, RowPointer), InsertError> {
        // Optimistically insert the `row` before checking for set-semantic collisions,
        // under the assumption that set-semantic collisions are rare.
        let (row_ref, blob_bytes) = self.insert_internal_allow_duplicate(blob_store, row)?;

        // Ensure row isn't already there.
        // SAFETY: We just inserted `ptr`, so we know it's valid.
        let hash = row_ref.row_hash();
        // Safety:
        // We just inserted `ptr` and computed `hash`, so they're valid.
        // `self` trivially has the same `row_layout` as `self`.
        let ptr = row_ref.pointer();
        let existing_row = unsafe { Self::find_same_row(self, self, ptr, hash) };

        if let Some(existing_row) = existing_row {
            // If an equal row was already present,
            // roll back our optimistic insert to avoid violating set semantics.

            // SAFETY: we just inserted `ptr`, so it must be valid.
            unsafe {
                self.inner
                    .pages
                    .delete_row(&self.inner.visitor_prog, self.row_size(), ptr, blob_store)
            };
            return Err(InsertError::Duplicate(existing_row));
        }
        self.row_count += 1;
        self.blob_store_bytes += blob_bytes;

        // If the optimistic insertion was correct,
        // i.e. this is not a set-semantic duplicate,
        // add it to the `pointer_map`.
        self.pointer_map.insert(hash, ptr);

        Ok((hash, ptr))
    }

    /// Physically inserts `row` into the page
    /// without inserting it logically into the pointer map.
    ///
    /// This is useful when we need to insert a row temporarily to get back a `RowPointer`.
    /// A call to this method should be followed by a call to [`delete_internal_skip_pointer_map`].
    pub fn insert_internal_allow_duplicate<'a>(
        &'a mut self,
        blob_store: &'a mut dyn BlobStore,
        row: &ProductValue,
    ) -> Result<(RowRef<'a>, BlobNumBytes), InsertError> {
        // SAFETY: `self.pages` is known to be specialized for `self.row_layout`,
        // as `self.pages` was constructed from `self.row_layout` in `Table::new`.
        let (ptr, blob_bytes) = unsafe {
            write_row_to_pages(
                &mut self.inner.pages,
                &self.inner.visitor_prog,
                blob_store,
                &self.inner.row_layout,
                row,
                self.squashed_offset,
            )
        }?;
        // SAFETY: We just inserted `ptr`, so it must be present.
        let row_ref = unsafe { self.inner.get_row_ref_unchecked(blob_store, ptr) };

        Ok((row_ref, blob_bytes))
    }

    /// Finds the [`RowPointer`] to the row in `committed_table`
    /// equal, by [`eq_row_in_page`], to the row at `tx_ptr` within `tx_table`, if any.
    ///
    /// Used for detecting set-semantic duplicates when inserting.
    ///
    /// Note that we don't need the blob store to compute equality,
    /// as content-addressing means it's sufficient to compare the hashes of large blobs.
    /// (If we see a collision in `BlobHash` we have bigger problems.)
    ///
    /// # Safety
    ///
    /// - The two tables must have the same `row_layout`.
    /// - `tx_ptr` must refer to a valid row in `tx_table`.
    pub unsafe fn find_same_row(
        committed_table: &Table,
        tx_table: &Table,
        tx_ptr: RowPointer,
        row_hash: RowHash,
    ) -> Option<RowPointer> {
        // Scan all the frow pointers with `row_hash` in the `committed_table`.
        committed_table
            .pointer_map
            .pointers_for(row_hash)
            .iter()
            .copied()
            .find(|committed_ptr| {
                let (committed_page, committed_offset) = committed_table.inner.page_and_offset(*committed_ptr);
                let (tx_page, tx_offset) = tx_table.inner.page_and_offset(tx_ptr);

                // SAFETY:
                // Our invariants mean `tx_ptr` is valid, so `tx_page` and `tx_offset` are both valid.
                // `committed_ptr` is in `committed_table.pointer_map`,
                // so it must be valid and therefore `committed_page` and `committed_offset` are valid.
                // Our invariants mean `committed_table.row_layout` applies to both tables.
                unsafe {
                    eq_row_in_page(
                        committed_page,
                        tx_page,
                        committed_offset,
                        tx_offset,
                        &committed_table.inner.row_layout,
                    )
                }
            })
    }

    /// Returns a [`RowRef`] for `ptr` or `None` if the row isn't present.
    pub fn get_row_ref<'a>(&'a self, blob_store: &'a dyn BlobStore, ptr: RowPointer) -> Option<RowRef<'a>> {
        self.is_row_present(ptr)
            // SAFETY: We only call `get_row_ref_unchecked` when `is_row_present` holds.
            .then(|| unsafe { self.get_row_ref_unchecked(blob_store, ptr) })
    }

    /// Assumes `ptr` is a present row in `self` and returns a [`RowRef`] to it.
    ///
    /// # Safety
    ///
    /// The requirement is that `self.is_row_present(ptr)` must hold.
    /// That is, `ptr` must refer to a row within `self`
    /// which was previously inserted and has not been deleted since.
    ///
    /// This means:
    /// - The `PageIndex` of `ptr` must be in-bounds for `self.pages`.
    /// - The `PageOffset` of `ptr` must be properly aligned for the row type of `self`,
    ///   and must refer to a valid, live row in that page.
    /// - The `SquashedOffset` of `ptr` must match `self.squashed_offset`.
    ///
    /// Showing that `ptr` was the result of a call to [`Table::insert(table, ..)`]
    /// and has not been passed to [`Table::delete(table, ..)`]
    /// is sufficient to demonstrate all of these properties.
    pub unsafe fn get_row_ref_unchecked<'a>(&'a self, blob_store: &'a dyn BlobStore, ptr: RowPointer) -> RowRef<'a> {
        debug_assert!(self.is_row_present(ptr));
        // SAFETY: Caller promised that ^-- holds.
        unsafe { RowRef::new(&self.inner, blob_store, ptr) }
    }

    /// Deletes a row in the page manager
    /// without deleting it logically in the pointer map.
    ///
    /// # Safety
    ///
    /// `ptr` must point to a valid, live row in this table.
    pub unsafe fn delete_internal_skip_pointer_map(
        &mut self,
        blob_store: &mut dyn BlobStore,
        ptr: RowPointer,
    ) -> BlobNumBytes {
        // Delete the physical row.
        //
        // SAFETY:
        // - `ptr` points to a valid row in this table, per our invariants.
        // - `self.row_size` known to be consistent with `self.pages`,
        //    as the two are tied together in `Table::new`.
        unsafe {
            self.inner
                .pages
                .delete_row(&self.inner.visitor_prog, self.row_size(), ptr, blob_store)
        }
    }

    /// Deletes the row identified by `ptr` from the table.
    ///
    /// NOTE: This method skips updating indexes.
    /// Use `delete_unchecked` or `delete` to delete a row with index updating.
    ///
    /// SAFETY: `self.is_row_present(row)` must hold.
    unsafe fn delete_internal(&mut self, blob_store: &mut dyn BlobStore, ptr: RowPointer) {
        // SAFETY: `self.is_row_present(row)` holds.
        let row = unsafe { self.get_row_ref_unchecked(blob_store, ptr) };

        // Remove the set semantic association.
        let _remove_result = self.pointer_map.remove(row.row_hash(), ptr);
        debug_assert!(_remove_result);
        self.row_count -= 1;

        // Delete the physical row.
        // SAFETY: `ptr` points to a valid row in this table as `self.is_row_present(row)` holds.
        let blob_store_deleted_bytes = unsafe { self.delete_internal_skip_pointer_map(blob_store, ptr) };
        // Just deleted bytes (`blob_store_deleted_bytes`)
        // cannot be greater than the total number of bytes (`self.blob_store_bytes`).
        self.blob_store_bytes = self.blob_store_bytes - blob_store_deleted_bytes;
    }

    /// Deletes the row identified by `ptr` from the table.
    ///
    /// SAFETY: `self.is_row_present(row)` must hold.
    unsafe fn delete_unchecked(&mut self, blob_store: &mut dyn BlobStore, ptr: RowPointer) {
        // SAFETY: `self.is_row_present(row)` holds.
        let row_ref = unsafe { self.inner.get_row_ref_unchecked(blob_store, ptr) };

        // Delete row from indices.
        // Do this before the actual deletion, as `index.delete` needs a `RowRef`
        // so it can extract the appropriate value.
        for (cols, index) in self.indexes.iter_mut() {
            let deleted = index.delete(cols, row_ref).unwrap();
            debug_assert!(deleted);
        }

        // SAFETY: We've checked above that `self.is_row_present(ptr)`.
        unsafe { self.delete_internal(blob_store, ptr) }
    }

    /// Deletes the row identified by `ptr` from the table.
    ///
    /// The function `before` is run on the to-be-deleted row,
    /// if it is present, before deleting.
    /// This enables callers to extract the deleted row.
    /// E.g. applying deletes when squashing/merging a transaction into the committed state
    /// passes `|row| row.to_product_value()` as `before`
    /// so that the resulting `ProductValue`s can be passed to the subscription evaluator.
    pub fn delete<'a, R>(
        &'a mut self,
        blob_store: &'a mut dyn BlobStore,
        ptr: RowPointer,
        before: impl for<'b> FnOnce(RowRef<'b>) -> R,
    ) -> Option<R> {
        if !self.is_row_present(ptr) {
            return None;
        };

        // SAFETY: We only call `get_row_ref_unchecked` when `is_row_present` holds.
        let row_ref = unsafe { self.inner.get_row_ref_unchecked(blob_store, ptr) };

        let ret = before(row_ref);

        // SAFETY: We've checked above that `self.is_row_present(ptr)`.
        unsafe { self.delete_unchecked(blob_store, ptr) }

        Some(ret)
    }

    /// If a row exists in `self` which matches `row`
    /// by [`Table::find_same_row`],
    /// delete that row.
    ///
    /// If a matching row was found, returns the pointer to that row.
    /// The returned pointer is now invalid, as the row to which it referred has been deleted.
    ///
    /// This operation works by temporarily inserting the `row` into `self`,
    /// checking `find_same_row` on the newly-inserted row,
    /// deleting the matching row if it exists,
    /// then deleting the temporary insertion.
    pub fn delete_equal_row(
        &mut self,
        blob_store: &mut dyn BlobStore,
        row: &ProductValue,
        skip_index_update: bool,
    ) -> Result<Option<RowPointer>, InsertError> {
        // Insert `row` temporarily so `temp_ptr` and `hash` can be used to find the row.
        // This must avoid consulting and inserting to the pointer map,
        // as the row is already present, set-semantically.
        let (temp_row, _) = self.insert_internal_allow_duplicate(blob_store, row)?;
        let temp_ptr = temp_row.pointer();
        let hash = temp_row.row_hash();

        // Find the row equal to the passed-in `row`.
        // SAFETY:
        // - `self` trivially has the same `row_layout` as `self`.
        // - We just inserted `temp_ptr` and computed `hash`, so they're valid.
        let existing_row_ptr = unsafe { Self::find_same_row(self, self, temp_ptr, hash) };

        // If an equal row was present, delete it.
        if let Some(existing_row_ptr) = existing_row_ptr {
            if skip_index_update {
                // SAFETY: `find_same_row` ensures that the pointer is valid.
                unsafe { self.delete_internal(blob_store, existing_row_ptr) }
            } else {
                // SAFETY: `find_same_row` ensures that the pointer is valid.
                unsafe { self.delete_unchecked(blob_store, existing_row_ptr) }
            }
        }

        // Remove the temporary row we inserted in the beginning.
        // Avoid the pointer map, since we don't want to delete it twice.
        // SAFETY: `ptr` is valid as we just inserted it.
        unsafe {
            self.delete_internal_skip_pointer_map(blob_store, temp_ptr);
        }

        Ok(existing_row_ptr)
    }

    /// Returns the row type for rows in this table.
    pub fn get_row_type(&self) -> &ProductType {
        self.get_schema().get_row_type()
    }

    /// Returns the schema for this table.
    pub fn get_schema(&self) -> &Arc<TableSchema> {
        &self.schema
    }

    /// Runs a mutation on the [`TableSchema`] of this table.
    ///
    /// This uses a clone-on-write mechanism.
    /// If none but `self` refers to the schema, then the mutation will be in-place.
    /// Otherwise, the schema must be cloned, mutated,
    /// and then the cloned version is written back to the table.
    pub fn with_mut_schema(&mut self, with: impl FnOnce(&mut TableSchema)) {
        with(Arc::make_mut(&mut self.schema));
    }

    /// Returns a new [`BTreeIndex`] for `table`.
    pub fn new_index(&self, id: IndexId, cols: &ColList, is_unique: bool) -> Result<BTreeIndex, InvalidFieldError> {
        BTreeIndex::new(id, self.get_schema().get_row_type(), cols, is_unique)
    }

    /// Inserts a new `index` into the table.
    ///
    /// The index will be populated using the rows of the table.
    /// Panics if `cols` has some column that is out of bounds of the table's row layout.
    pub fn insert_index(&mut self, blob_store: &dyn BlobStore, cols: ColList, mut index: BTreeIndex) {
        index.build_from_rows(&cols, self.scan_rows(blob_store)).unwrap();
        self.indexes.insert(cols, index);
    }

    /// Returns an iterator over all the rows of `self`, yielded as [`RefRef`]s.
    pub fn scan_rows<'a>(&'a self, blob_store: &'a dyn BlobStore) -> TableScanIter<'a> {
        TableScanIter {
            current_page: None, // Will be filled by the iterator.
            current_page_idx: PageIndex(0),
            table: self,
            blob_store,
        }
    }

    /// When there's an index for `cols`,
    /// returns an iterator over the [`BTreeIndex`] that yields all the [`RowRef`]s
    /// matching the specified `range` in the indexed column.
    ///
    /// Matching is defined by `Ord for AlgebraicValue`.
    pub fn index_seek<'a>(
        &'a self,
        blob_store: &'a dyn BlobStore,
        cols: &ColList,
        range: &impl RangeBounds<AlgebraicValue>,
    ) -> Option<IndexScanIter<'a>> {
        self.indexes.get(cols).map(|index| {
            let btree_index_iter = index.seek(range);
            IndexScanIter {
                table: self,
                blob_store,
                btree_index_iter,
            }
        })
    }

    /// Clones the structure of this table into a new one with
    /// the same schema, visitor program, and indices.
    /// The new table will be completely empty
    /// and will use the given `squashed_offset` instead of that of `self`.
    pub fn clone_structure(&self, squashed_offset: SquashedOffset) -> Self {
        let schema = self.schema.clone();
        let layout = self.row_layout().clone();
        let sbl = self.static_bsatn_layout().cloned();
        let visitor = self.inner.visitor_prog.clone();
        let mut new =
            Table::new_with_indexes_capacity(schema, layout, sbl, visitor, squashed_offset, self.indexes.len());

        for (cols, index) in self.indexes.iter() {
            // `new` is known to be empty (we just constructed it!),
            // so no need for an actual blob store here.
            let index = new.new_index(index.index_id, cols, index.is_unique).unwrap();
            new.insert_index(&NullBlobStore, cols.clone(), index);
        }
        new
    }

    /// Returns the number of bytes occupied by the pages and the blob store.
    /// Note that result can be more than the actual physical size occupied by the table
    /// because the blob store implementation can do internal optimizations.
    /// For more details, refer to the documentation of `self.blob_store_bytes`.
    pub fn bytes_occupied_overestimate(&self) -> usize {
        (self.num_pages() * PAGE_DATA_SIZE) + (self.blob_store_bytes.0)
    }

    /// Reset the internal storage of `self` to be `pages`.
    ///
    /// This recomputes the pointer map based on the `pages`,
    /// but does not recompute indexes.
    ///
    /// Used when restoring from a snapshot.
    ///
    /// # Safety
    ///
    /// The schema of rows stored in the `pages` must exactly match `self.schema` and `self.inner.row_layout`.
    pub unsafe fn set_pages(&mut self, pages: Vec<Box<Page>>, blob_store: &dyn BlobStore) {
        self.inner.pages.set_contents(pages, self.inner.row_layout.size());

        // Recompute table metadata based on the new pages.
        // Compute the row count first, in case later computations want to use it as a capacity to pre-allocate.
        self.compute_row_count(blob_store);
        self.rebuild_pointer_map(blob_store);
    }
}

/// A reference to a single row within a table.
///
/// # Safety
///
/// Having a `r: RowRef` is a proof that [`r.pointer()`](RowRef::pointer) refers to a valid row.
/// This makes constructing a `RowRef`, i.e., `RowRef::new`, an `unsafe` operation.
#[derive(Copy, Clone)]
pub struct RowRef<'a> {
    /// The table that has the row at `self.pointer`.
    table: &'a TableInner,
    /// The blob store used in case there are blob hashes to resolve.
    blob_store: &'a dyn BlobStore,
    /// The pointer to the row in `self.table`.
    pointer: RowPointer,
}

impl fmt::Debug for RowRef<'_> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("RowRef")
            .field("pointer", &self.pointer)
            .finish_non_exhaustive()
    }
}

impl<'a> RowRef<'a> {
    /// Construct a `RowRef` to the row at `pointer` within `table`.
    ///
    /// # Safety
    ///
    /// `pointer` must refer to a row within `table`
    /// which was previously inserted and has not been deleted since.
    ///
    /// This means:
    /// - The `PageIndex` of `pointer` must be in-bounds for `table.pages`.
    /// - The `PageOffset` of `pointer` must be properly aligned for the row type of `table`,
    ///   and must refer to a valid, live row in that page.
    /// - The `SquashedOffset` of `pointer` must match `table.squashed_offset`.
    ///
    /// Showing that `pointer` was the result of a call to `table.insert`
    /// and has not been passed to `table.delete`
    /// is sufficient to demonstrate all of these properties.
    unsafe fn new(table: &'a TableInner, blob_store: &'a dyn BlobStore, pointer: RowPointer) -> Self {
        Self {
            table,
            blob_store,
            pointer,
        }
    }

    /// Extract a `ProductValue` from the table.
    ///
    /// This is a potentially expensive operation,
    /// as it must walk the table's `ProductTypeLayout`
    /// and heap-allocate various substructures of the `ProductValue`.
    pub fn to_product_value(&self) -> ProductValue {
        let res = self
            .serialize(ValueSerializer)
            .unwrap_or_else(|x| match x {})
            .into_product();
        // SAFETY: the top layer of a row when serialized is always a product.
        unsafe { res.unwrap_unchecked() }
    }

    /// Check that the `idx`th column of the row type stored by `self` is compatible with `T`,
    /// and read the value of that column from `self`.
    #[inline]
    pub fn read_col<T: ReadColumn>(self, col: impl Into<ColId>) -> Result<T, TypeError> {
        T::read_column(self, col.into().idx())
    }

    /// Construct a projection of the row at `self` by extracting the `cols`.
    ///
    /// Returns an error if `cols` specifies an index which is out-of-bounds for the row at `self`.
    ///
    /// If `cols` contains zero or more than one column, the values of the projected columns are wrapped in a [`ProductValue`].
    /// If `cols` is a single column, the value of that column is returned without wrapping in a `ProductValue`.
    pub fn project_not_empty(self, cols: &ColList) -> Result<AlgebraicValue, InvalidFieldError> {
        if let Some(head) = cols.as_singleton() {
            return self.read_col(head).map_err(|_| head.into());
        }
        let mut elements = Vec::with_capacity(cols.len() as usize);
        for col in cols.iter() {
            let col_val = self.read_col(col).map_err(|err| match err {
                TypeError::WrongType { .. } => {
                    unreachable!("AlgebraicValue::read_column never returns a `TypeError::WrongType`")
                }
                TypeError::IndexOutOfBounds { .. } => col,
            })?;
            elements.push(col_val);
        }
        Ok(AlgebraicValue::product(elements))
    }

    /// Returns the raw row pointer for this row reference.
    pub fn pointer(&self) -> RowPointer {
        self.pointer
    }

    /// Returns the blob store that any [`crate::blob_store::BlobHash`]es within the row refer to.
    pub(crate) fn blob_store(&self) -> &dyn BlobStore {
        self.blob_store
    }

    /// Return the layout of the row.
    ///
    /// All rows within the same table will have the same layout.
    pub fn row_layout(&self) -> &RowTypeLayout {
        &self.table.row_layout
    }

    /// Returns the page the row is in and the offset of the row within that page.
    pub fn page_and_offset(&self) -> (&Page, PageOffset) {
        self.table.page_and_offset(self.pointer())
    }

    /// Returns the bytes for the fixed portion of this row.
    pub(crate) fn get_row_data(&self) -> &Bytes {
        let (page, offset) = self.page_and_offset();
        page.get_row_data(offset, self.table.row_layout.size())
    }

    /// Returns the row hash for `ptr`.
    pub fn row_hash(&self) -> RowHash {
        RowHash(RowHash::hasher_builder().hash_one(self))
    }

    /// The length of this row when BSATN-encoded.
    ///
    /// Only available for rows whose types have a static BSATN layout.
    /// Returns `None` for rows of other types, e.g. rows containing strings.
    pub fn bsatn_length(&self) -> Option<usize> {
        self.table.static_bsatn_layout.as_ref().map(|s| s.bsatn_length as usize)
    }

    /// Encode the row referred to by `self` into a `Vec<u8>` using BSATN and then deserialize it.
    /// The passed buffer is allowed to be in an arbitrary state before and after this operation.
    pub fn read_via_bsatn<T>(&self, scratch: &mut Vec<u8>) -> Result<T, ReadViaBsatnError>
    where
        T: DeserializeOwned,
    {
        scratch.clear();
        self.to_bsatn_extend(scratch)?;
        Ok(bsatn::from_slice::<T>(scratch)?)
    }

    /// Return the number of bytes in the blob store to which this object holds a reference.
    ///
    /// Used to compute the table's `blob_store_bytes` when reconstructing a snapshot.
    ///
    /// Even within a single row, this is a conservative overestimate,
    /// as a row may contain multiple references to the same large blob.
    /// This seems unlikely to occur in practice.
    fn blob_store_bytes(&self) -> usize {
        let row_data = self.get_row_data();
        let (page, _) = self.page_and_offset();
        // SAFETY:
        // - Existence of a `RowRef` treated as proof
        //   of the row's validity and type information's correctness.
        unsafe { self.table.visitor_prog.visit_var_len(row_data) }
            .filter(|vlr| vlr.is_large_blob())
            .map(|vlr| {
                // SAFETY:
                // - Because `vlr.is_large_blob`, it points to exactly one granule.
                let granule = unsafe { page.iter_var_len_object(vlr.first_granule) }.next().unwrap();
                let blob_hash = granule.blob_hash();
                let blob = self.blob_store.retrieve_blob(&blob_hash).unwrap();

                blob.len()
            })
            .sum()
    }
}

impl Serialize for RowRef<'_> {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        let table = self.table;
        let (page, offset) = table.page_and_offset(self.pointer);
        // SAFETY: `ptr` points to a valid row in this table per above check.
        unsafe { serialize_row_from_page(ser, page, self.blob_store, offset, &table.row_layout) }
    }
}

impl ToBsatn for RowRef<'_> {
    /// BSATN-encode the row referred to by `self` into a freshly-allocated `Vec<u8>`.
    ///
    /// This method will use a [`StaticBsatnLayout`] if one is available,
    /// and may therefore be faster than calling [`bsatn::to_vec`].
    fn to_bsatn_vec(&self) -> Result<Vec<u8>, BsatnError> {
        if let Some(static_bsatn_layout) = &self.table.static_bsatn_layout {
            // Use fast path, by first fetching the row data and then using the static layout.
            let row = self.get_row_data();
            // SAFETY:
            // - Existence of a `RowRef` treated as proof
            //   of row's validity and type information's correctness.
            Ok(unsafe { static_bsatn_layout.serialize_row_into_vec(row) })
        } else {
            bsatn::to_vec(self)
        }
    }

    /// BSATN-encode the row referred to by `self` into `buf`,
    /// pushing `self`'s bytes onto the end of `buf`, similar to [`Vec::extend`].
    ///
    /// This method will use a [`StaticBsatnLayout`] if one is available,
    /// and may therefore be faster than calling [`bsatn::to_writer`].
    fn to_bsatn_extend(&self, buf: &mut Vec<u8>) -> Result<(), BsatnError> {
        if let Some(static_bsatn_layout) = &self.table.static_bsatn_layout {
            // Use fast path, by first fetching the row data and then using the static layout.
            let row = self.get_row_data();
            // SAFETY:
            // - Existence of a `RowRef` treated as proof
            //   of row's validity and type information's correctness.
            unsafe {
                static_bsatn_layout.serialize_row_extend(buf, row);
            }
            Ok(())
        } else {
            // Use the slower, but more general, `bsatn_from` serializer to write the row.
            bsatn::to_writer(buf, self)
        }
    }

    fn static_bsatn_size(&self) -> Option<u16> {
        self.table.static_bsatn_layout.as_ref().map(|sbl| sbl.bsatn_length)
    }
}

impl Eq for RowRef<'_> {}
impl PartialEq for RowRef<'_> {
    fn eq(&self, other: &Self) -> bool {
        // Ensure that the layouts are the same
        // so that we can use `eq_row_in_page`.
        // To do this, we first try address equality on the layouts.
        // This should succeed when the rows originate from the same table.
        // Otherwise, actually compare the layouts, which is expensive, but unlikely to happen.
        let a_ty = self.row_layout();
        let b_ty = other.row_layout();
        if !(ptr::eq(a_ty, b_ty) || a_ty == b_ty) {
            return false;
        }
        let (page_a, offset_a) = self.page_and_offset();
        let (page_b, offset_b) = other.page_and_offset();
        // SAFETY: `offset_a/b` are valid rows in `page_a/b` typed at `a_ty`.
        unsafe { eq_row_in_page(page_a, page_b, offset_a, offset_b, a_ty) }
    }
}

impl PartialEq<ProductValue> for RowRef<'_> {
    fn eq(&self, rhs: &ProductValue) -> bool {
        let ty = self.row_layout();
        let (page, offset) = self.page_and_offset();
        // SAFETY: By having `RowRef`,
        // we know that `offset` is a valid offset for a row in `page` typed at `ty`.
        unsafe { eq_row_in_page_to_pv(self.blob_store, page, offset, rhs, ty) }
    }
}

impl Hash for RowRef<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let (page, offset) = self.table.page_and_offset(self.pointer);
        let ty = &self.table.row_layout;
        // SAFETY: A `RowRef` is a proof that `self.pointer` refers to a live fixed row in `self.table`, so:
        // 1. `offset` points at a row in `page` lasting `ty.size()` bytes.
        // 2. the row is valid for `ty`.
        // 3. for any `vlr: VarLenRef` stored in the row,
        //    `vlr.first_offset` is either `NULL` or points to a valid granule in `page`.
        unsafe { hash_row_in_page(state, page, self.blob_store, offset, ty) };
    }
}

/// An iterator over all the rows, yielded as [`RowRef`]s, in a table.
pub struct TableScanIter<'table> {
    /// The current page we're yielding rows from.
    /// When `None`, the iterator will attempt to advance to the next page, if any.
    current_page: Option<FixedLenRowsIter<'table>>,
    /// The current page index we are or will visit.
    current_page_idx: PageIndex,
    /// The table the iterator is yielding rows from.
    pub(crate) table: &'table Table,
    /// The `BlobStore` that row references may refer into.
    pub(crate) blob_store: &'table dyn BlobStore,
}

impl<'a> Iterator for TableScanIter<'a> {
    type Item = RowRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        // This could have been written using `.flat_map`,
        // but we don't have `type Foo = impl Iterator<...>;` on stable yet.
        loop {
            match &mut self.current_page {
                // We're currently visiting a page,
                Some(iter_fixed_len) => {
                    if let Some(page_offset) = iter_fixed_len.next() {
                        // There's still at least one row in that page to visit,
                        // return a ref to that row.
                        let ptr =
                            RowPointer::new(false, self.current_page_idx, page_offset, self.table.squashed_offset);

                        // SAFETY: `offset` came from the `iter_fixed_len`, so it must point to a valid row.
                        let row_ref = unsafe { self.table.get_row_ref_unchecked(self.blob_store, ptr) };
                        return Some(row_ref);
                    } else {
                        // We've finished visiting that page, so set `current_page` to `None`,
                        // increment `self.current_page_idx` to the index of the next page,
                        // and go to the `None` case (1) in the match.
                        self.current_page = None;
                        self.current_page_idx.0 += 1;
                    }
                }

                // (1) If we aren't currently visiting a page,
                // the `else` case in the `Some` match arm
                // already incremented `self.current_page_idx`,
                // or we're just beginning and so it was initialized as 0.
                None => {
                    // If there's another page, set `self.current_page` to it,
                    // and go to the `Some` case in the match.
                    let next_page = self.table.pages().get(self.current_page_idx.idx())?;
                    let iter = next_page.iter_fixed_len(self.table.row_size());
                    self.current_page = Some(iter);
                }
            }
        }
    }
}

/// An iterator using a [`BTreeIndex`] to scan a `table`
/// for all the [`RowRef`]s matching the specified `range` in the indexed column(s).
///
/// Matching is defined by `Ord for AlgebraicValue`.
pub struct IndexScanIter<'a> {
    /// The table being scanned for rows.
    table: &'a Table,
    /// The blob store; passed on to the [`RowRef`]s in case they need it.
    blob_store: &'a dyn BlobStore,
    /// The iterator performing the index scan yielding row pointers.
    btree_index_iter: BTreeIndexRangeIter<'a>,
}

impl<'a> Iterator for IndexScanIter<'a> {
    type Item = RowRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let ptr = self.btree_index_iter.next()?;
        // FIXME: Determine if this is correct and if so use `_unchecked`.
        // Will a table's index necessarily hold only pointers into that index?
        // Edge case: if an index is added during a transaction which then scans that index,
        // it appears that the newly-created `TxState` index
        // will also hold pointers into the `CommittedState`.
        //
        // SAFETY: Assuming this is correct,
        // `ptr` came from the index, which always holds pointers to valid rows.
        self.table.get_row_ref(self.blob_store, ptr)
    }
}

impl IndexScanIter<'_> {
    /// Returns the current number of pointers the iterator has returned thus far.
    pub fn num_pointers_yielded(&self) -> u64 {
        self.btree_index_iter.num_pointers_yielded()
    }
}

#[derive(Error, Debug, PartialEq, Eq)]
#[error("Unique constraint violation '{}' in table '{}': column(s): '{:?}' value: {}", constraint_name, table_name, cols, value.to_satn())]
pub struct UniqueConstraintViolation {
    pub constraint_name: Box<str>,
    pub table_name: Box<str>,
    pub cols: Vec<Box<str>>,
    pub value: AlgebraicValue,
}

// Private API:
impl Table {
    /// Returns a unique constraint violation error for the given `index`
    /// and the `value` that would have been duplicated.
    fn build_error_unique(
        &self,
        index: &BTreeIndex,
        cols: &ColList,
        value: AlgebraicValue,
    ) -> UniqueConstraintViolation {
        let schema = self.get_schema();

        // Fetch the table name.
        let table_name = schema.table_name.clone();

        // Fetch the names of the columns used in the index.
        let cols = cols
            .iter()
            .map(|x| schema.columns()[x.idx()].col_name.clone())
            .collect();

        // Fetch the name of the index.
        let constraint_name = schema
            .indexes
            .iter()
            .find(|i| i.index_id == index.index_id)
            .unwrap()
            .index_name
            .clone();

        UniqueConstraintViolation {
            constraint_name,
            table_name,
            cols,
            value,
        }
    }

    /// Returns a new empty table with the given `schema`, `row_layout`, and `static_bsatn_layout`s
    /// and with a specified capacity for the `indexes` of the table.
    fn new_with_indexes_capacity(
        schema: Arc<TableSchema>,
        row_layout: RowTypeLayout,
        static_bsatn_layout: Option<StaticBsatnLayout>,
        visitor_prog: VarLenVisitorProgram,
        squashed_offset: SquashedOffset,
        indexes_capacity: usize,
    ) -> Self {
        Self {
            inner: TableInner {
                row_layout,
                static_bsatn_layout,
                visitor_prog,
                pages: Pages::default(),
            },
            schema,
            indexes: HashMap::<_, _, DefaultHashBuilder>::with_capacity(indexes_capacity),
            pointer_map: PointerMap::default(),
            squashed_offset,
            row_count: 0,
            blob_store_bytes: BlobNumBytes::default(),
        }
    }

    /// Returns whether the row at `ptr` is present or not.
    // TODO: Remove all uses of this method,
    //       or more likely, gate them behind `debug_assert!`
    //       so they don't have semantic meaning.
    //
    //       Unlike the previous `locking_tx_datastore::Table`'s `RowId`,
    //       `RowPointer` is not content-addressed.
    //       This means it is possible to:
    //       - have a `RowPointer` A* to row A,
    //       - Delete row A,
    //       - Insert row B into the same storage as freed from A,
    //       - Test `is_row_present(A*)`, which falsely reports that row A is still present.
    //
    //       In the final interface, this method is superfluous anyways,
    //       as `RowPointer` is not part of our public interface.
    //       Instead, we will always discover a known-present `RowPointer`
    //       during a table scan or index seek.
    //       As such, our `delete` and `insert` methods can be `unsafe`
    //       and trust that the `RowPointer` is valid.
    fn is_row_present(&self, ptr: RowPointer) -> bool {
        if self.squashed_offset != ptr.squashed_offset() {
            return false;
        }
        let Some((page, offset)) = self.inner.try_page_and_offset(ptr) else {
            return false;
        };
        page.has_row_offset(self.row_size(), offset)
    }

    /// Returns the row size for a row in the table.
    fn row_size(&self) -> Size {
        self.inner.row_layout.size()
    }

    /// Returns the layout for a row in the table.
    fn row_layout(&self) -> &RowTypeLayout {
        &self.inner.row_layout
    }

    /// Returns the pages storing the physical rows of this table.
    fn pages(&self) -> &Pages {
        &self.inner.pages
    }

    /// Iterates over each [`Page`] in this table, ensuring that its hash is computed before yielding it.
    ///
    /// Used when capturing a snapshot.
    pub fn iter_pages_with_hashes(&mut self) -> impl Iterator<Item = (blake3::Hash, &Page)> {
        self.inner.pages.iter_mut().map(|page| {
            let hash = page.save_or_get_content_hash();
            (hash, &**page)
        })
    }

    /// Returns the number of pages storing the physical rows of this table.
    fn num_pages(&self) -> usize {
        self.inner.pages.len()
    }

    /// Returns the [`StaticBsatnLayout`] for this table,
    pub(crate) fn static_bsatn_layout(&self) -> Option<&StaticBsatnLayout> {
        self.inner.static_bsatn_layout.as_ref()
    }

    /// Rebuild the [`PointerMap`] by iterating over all the rows in `self` and inserting them.
    ///
    /// Called when restoring from a snapshot after installing the pages,
    /// but after computing the row count,
    /// since snapshots do not save the pointer map..
    fn rebuild_pointer_map(&mut self, blob_store: &dyn BlobStore) {
        // TODO(perf): Pre-allocate `PointerMap.map` with capacity `self.row_count`.
        // Alternatively, do this at the same time as `compute_row_count`.
        let ptrs = self
            .scan_rows(blob_store)
            .map(|row_ref| (row_ref.row_hash(), row_ref.pointer()))
            .collect::<PointerMap>();
        self.pointer_map = ptrs;
    }

    /// Compute and store `self.row_count` and `self.blob_store_bytes`
    /// by iterating over all the rows in `self` and counting them.
    ///
    /// Called when restoring from a snapshot after installing the pages,
    /// since snapshots do not save this metadata.
    fn compute_row_count(&mut self, blob_store: &dyn BlobStore) {
        let mut row_count = 0;
        let mut blob_store_bytes = 0;
        for row in self.scan_rows(blob_store) {
            row_count += 1;
            blob_store_bytes += row.blob_store_bytes();
        }
        self.row_count = row_count as u64;
        self.blob_store_bytes = blob_store_bytes.into();
    }
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;
    use crate::blob_store::HashMapBlobStore;
    use crate::page::tests::hash_unmodified_save_get;
    use crate::var_len::VarLenGranule;
    use proptest::prelude::*;
    use proptest::test_runner::TestCaseResult;
    use spacetimedb_lib::db::raw_def::v9::{RawIndexAlgorithm, RawModuleDefV9Builder};
    use spacetimedb_primitives::{col_list, TableId};
    use spacetimedb_sats::bsatn::to_vec;
    use spacetimedb_sats::proptest::generate_typed_row;
    use spacetimedb_sats::{product, AlgebraicType, ArrayValue};
    use spacetimedb_schema::def::ModuleDef;
    use spacetimedb_schema::schema::Schema as _;

    /// Create a `Table` from a `ProductType` without validation.
    pub(crate) fn table(ty: ProductType) -> Table {
        // Use a fast path here to avoid slowing down Miri in the proptests.
        // Does not perform validation.
        let schema = TableSchema::from_product_type(ty);
        Table::new(schema.into(), SquashedOffset::COMMITTED_STATE)
    }

    #[test]
    fn unique_violation_error() {
        let table_name = "UniqueIndexed";
        let index_name = "UniqueIndexed_unique_col_idx_btree";
        let mut builder = RawModuleDefV9Builder::new();
        builder
            .build_table_with_new_type(
                table_name,
                ProductType::from([("unique_col", AlgebraicType::I32), ("other_col", AlgebraicType::I32)]),
                true,
            )
            .with_unique_constraint(0)
            .with_index(
                RawIndexAlgorithm::BTree { columns: col_list![0] },
                "accessor_name_doesnt_matter",
            );

        let def: ModuleDef = builder.finish().try_into().expect("Failed to build schema");

        let schema = TableSchema::from_module_def(&def, def.table(table_name).unwrap(), (), TableId::SENTINEL);
        assert_eq!(schema.indexes.len(), 1);
        let index_schema = schema.indexes[0].clone();

        let mut table = Table::new(schema.into(), SquashedOffset::COMMITTED_STATE);
        let cols = ColList::new(0.into());

        let index = table.new_index(index_schema.index_id, &cols, true).unwrap();
        table.insert_index(&NullBlobStore, cols, index);

        // Reserve a page so that we can check the hash.
        let pi = table.inner.pages.reserve_empty_page(table.row_size()).unwrap();
        let hash_pre_ins = hash_unmodified_save_get(&mut table.inner.pages[pi]);

        // Insert the row (0, 0).
        table
            .insert(&mut NullBlobStore, &product![0i32, 0i32])
            .expect("Initial insert failed");

        // Inserting cleared the hash.
        let hash_post_ins = hash_unmodified_save_get(&mut table.inner.pages[pi]);
        assert_ne!(hash_pre_ins, hash_post_ins);

        // Try to insert the row (0, 1), and assert that we get the expected error.
        match table.insert(&mut NullBlobStore, &product![0i32, 1i32]) {
            Ok(_) => panic!("Second insert with same unique value succeeded"),
            Err(InsertError::IndexError(UniqueConstraintViolation {
                constraint_name,
                table_name,
                cols,
                value,
            })) => {
                assert_eq!(&*constraint_name, index_name);
                assert_eq!(&*table_name, "UniqueIndexed");
                assert_eq!(cols.iter().map(|c| c.to_string()).collect::<Vec<_>>(), &["unique_col"]);
                assert_eq!(value, AlgebraicValue::I32(0));
            }
            Err(e) => panic!("Expected UniqueConstraintViolation but found {:?}", e),
        }

        // Second insert did not clear the hash as we had a constraint violation.
        assert_eq!(hash_post_ins, *table.inner.pages[pi].unmodified_hash().unwrap());
    }

    fn insert_retrieve_body(ty: impl Into<ProductType>, val: impl Into<ProductValue>) -> TestCaseResult {
        let val = val.into();
        let mut blob_store = HashMapBlobStore::default();
        let mut table = table(ty.into());
        let (hash, row) = table.insert(&mut blob_store, &val).unwrap();
        prop_assert_eq!(row.row_hash(), hash);
        let ptr = row.pointer();
        prop_assert_eq!(table.pointer_map.pointers_for(hash), &[ptr]);

        prop_assert_eq!(table.inner.pages.len(), 1);
        prop_assert_eq!(table.inner.pages[PageIndex(0)].num_rows(), 1);

        let row_ref = table.get_row_ref(&blob_store, ptr).unwrap();
        prop_assert_eq!(row_ref.to_product_value(), val.clone());
        let bsatn_val = to_vec(&val).unwrap();
        prop_assert_eq!(&bsatn_val, &to_vec(&row_ref).unwrap());
        prop_assert_eq!(&bsatn_val, &row_ref.to_bsatn_vec().unwrap());

        prop_assert_eq!(
            &table.scan_rows(&blob_store).map(|r| r.pointer()).collect::<Vec<_>>(),
            &[ptr]
        );

        Ok(())
    }

    #[test]
    fn repro_serialize_bsatn_empty_array() {
        let ty = AlgebraicType::array(AlgebraicType::U64);
        let arr = ArrayValue::from(Vec::<u64>::new().into_boxed_slice());
        insert_retrieve_body(ty, AlgebraicValue::from(arr)).unwrap();
    }

    #[test]
    fn repro_serialize_bsatn_debug_assert() {
        let ty = AlgebraicType::array(AlgebraicType::U64);
        let arr = ArrayValue::from((0..130u64).collect::<Box<_>>());
        insert_retrieve_body(ty, AlgebraicValue::from(arr)).unwrap();
    }

    proptest! {
        #![proptest_config(ProptestConfig { max_shrink_iters: 0x10000000, ..Default::default() })]

        #[test]
        fn insert_retrieve((ty, val) in generate_typed_row()) {
            insert_retrieve_body(ty, val)?;
        }

        #[test]
        fn insert_delete_removed_from_pointer_map((ty, val) in generate_typed_row()) {
            let mut blob_store = HashMapBlobStore::default();
            let mut table = table(ty);
            let (hash, row) = table.insert(&mut blob_store, &val).unwrap();
            prop_assert_eq!(row.row_hash(), hash);
            let ptr = row.pointer();
            prop_assert_eq!(table.pointer_map.pointers_for(hash), &[ptr]);

            prop_assert_eq!(table.inner.pages.len(), 1);
            prop_assert_eq!(table.inner.pages[PageIndex(0)].num_rows(), 1);
            prop_assert_eq!(&table.scan_rows(&blob_store).map(|r| r.pointer()).collect::<Vec<_>>(), &[ptr]);
            prop_assert_eq!(table.row_count, 1);

            let hash_pre_del = hash_unmodified_save_get(&mut table.inner.pages[ptr.page_index()]);

            table.delete(&mut blob_store, ptr, |_| ());

            let hash_post_del = hash_unmodified_save_get(&mut table.inner.pages[ptr.page_index()]);
            assert_ne!(hash_pre_del, hash_post_del);

            prop_assert_eq!(table.pointer_map.pointers_for(hash), &[]);

            prop_assert_eq!(table.inner.pages.len(), 1);
            prop_assert_eq!(table.inner.pages[PageIndex(0)].num_rows(), 0);
            prop_assert_eq!(table.row_count, 0);

            prop_assert!(&table.scan_rows(&blob_store).next().is_none());
        }

        #[test]
        fn insert_duplicate_set_semantic((ty, val) in generate_typed_row()) {
            let mut blob_store = HashMapBlobStore::default();
            let mut table = table(ty);

            let (hash, row) = table.insert(&mut blob_store, &val).unwrap();
            prop_assert_eq!(row.row_hash(), hash);
            let ptr = row.pointer();
            prop_assert_eq!(table.inner.pages.len(), 1);
            prop_assert_eq!(table.pointer_map.pointers_for(hash), &[ptr]);
            prop_assert_eq!(table.row_count, 1);
            prop_assert_eq!(&table.scan_rows(&blob_store).map(|r| r.pointer()).collect::<Vec<_>>(), &[ptr]);

            let blob_uses = blob_store.usage_counter();

            let hash_pre_ins = hash_unmodified_save_get(&mut table.inner.pages[ptr.page_index()]);

            prop_assert!(table.insert(&mut blob_store, &val).is_err());

            // Hash was cleared and is different despite failure to insert.
            let hash_post_ins = hash_unmodified_save_get(&mut table.inner.pages[ptr.page_index()]);
            assert_ne!(hash_pre_ins, hash_post_ins);

            prop_assert_eq!(table.row_count, 1);
            prop_assert_eq!(table.inner.pages.len(), 1);
            prop_assert_eq!(table.pointer_map.pointers_for(hash), &[ptr]);

            let blob_uses_after = blob_store.usage_counter();

            prop_assert_eq!(blob_uses_after, blob_uses);
            prop_assert_eq!(table.inner.pages[PageIndex(0)].num_rows(), 1);
            prop_assert_eq!(&table.scan_rows(&blob_store).map(|r| r.pointer()).collect::<Vec<_>>(), &[ptr]);
        }
    }

    // Compare `scan_rows` against a simpler implementation.
    #[test]
    fn table_scan_iter_eq_flatmap() {
        let mut blob_store = HashMapBlobStore::default();
        let mut table = table(AlgebraicType::U64.into());
        for v in 0..2u64.pow(14) {
            table.insert(&mut blob_store, &product![v]).unwrap();
        }

        let complex = table.scan_rows(&blob_store).map(|r| r.pointer());
        let simple = table
            .inner
            .pages
            .iter()
            .zip((0..).map(PageIndex))
            .flat_map(|(page, pi)| {
                page.iter_fixed_len(table.row_size())
                    .map(move |po| RowPointer::new(false, pi, po, table.squashed_offset))
            });
        assert!(complex.eq(simple));
    }

    #[test]
    #[should_panic]
    fn read_row_unaligned_page_offset_soundness() {
        // Insert a `u64` into a table.
        let pt = AlgebraicType::U64.into();
        let pv = product![42u64];
        let mut table = table(pt);
        let blob_store = &mut NullBlobStore;
        let (_, row_ref) = table.insert(blob_store, &pv).unwrap();

        // Manipulate the page offset to 1 instead of 0.
        // This now points into the "middle" of a row.
        let ptr = row_ref.pointer().with_page_offset(PageOffset(1));

        // We expect this to panic.
        // Miri should not have any issue with this call either.
        table.get_row_ref(&NullBlobStore, ptr).unwrap().to_product_value();
    }

    #[test]
    fn test_blob_store_bytes() {
        let pt: ProductType = [AlgebraicType::String, AlgebraicType::I32].into();
        let blob_store = &mut HashMapBlobStore::default();
        let mut insert =
            |table: &mut Table, string, num| table.insert(blob_store, &product![string, num]).unwrap().1.pointer();
        let mut table1 = table(pt.clone());

        // Insert short string, `blob_store_bytes` should be 0.
        let short_str = std::str::from_utf8(&[98; 6]).unwrap();
        let short_row_ptr = insert(&mut table1, short_str, 0);
        assert_eq!(table1.blob_store_bytes.0, 0);

        // Insert long string, `blob_store_bytes` should be the length of the string.
        const BLOB_OBJ_LEN: BlobNumBytes = BlobNumBytes(VarLenGranule::OBJECT_SIZE_BLOB_THRESHOLD + 1);
        let long_str = std::str::from_utf8(&[98; BLOB_OBJ_LEN.0]).unwrap();
        let long_row_ptr = insert(&mut table1, long_str, 0);
        assert_eq!(table1.blob_store_bytes, BLOB_OBJ_LEN);

        // Insert previous long string in the same table,
        // `blob_store_bytes` should count the length twice,
        // even though `HashMapBlobStore` deduplicates it.
        let long_row_ptr2 = insert(&mut table1, long_str, 1);
        const BLOB_OBJ_LEN_2X: BlobNumBytes = BlobNumBytes(BLOB_OBJ_LEN.0 * 2);
        assert_eq!(table1.blob_store_bytes, BLOB_OBJ_LEN_2X);

        // Insert previous long string in a new table,
        // `blob_store_bytes` should show the length,
        // even though `HashMapBlobStore` deduplicates it.
        let mut table2 = table(pt);
        let _ = insert(&mut table2, long_str, 0);
        assert_eq!(table2.blob_store_bytes, BLOB_OBJ_LEN);

        // Delete `short_str` row. This should not affect the byte count.
        table1.delete(blob_store, short_row_ptr, |_| ()).unwrap();
        assert_eq!(table1.blob_store_bytes, BLOB_OBJ_LEN_2X);

        // Delete the first long string row. This gets us down to `BLOB_OBJ_LEN` (we had 2x before).
        table1.delete(blob_store, long_row_ptr, |_| ()).unwrap();
        assert_eq!(table1.blob_store_bytes, BLOB_OBJ_LEN);

        // Delete the first long string row. This gets us down to 0 (we've now deleted 2x).
        table1.delete(blob_store, long_row_ptr2, |_| ()).unwrap();
        assert_eq!(table1.blob_store_bytes, 0.into());
    }

    /// Assert that calling `get_row_ref` to get a row ref to a non-existent `RowPointer`
    /// does not panic.
    #[test]
    fn get_row_ref_no_panic() {
        let blob_store = &mut HashMapBlobStore::default();
        let table = table([AlgebraicType::String, AlgebraicType::I32].into());

        // This row pointer has an incorrect `SquashedOffset`, and so does not point into `table`.
        assert!(table
            .get_row_ref(
                blob_store,
                RowPointer::new(false, PageIndex(0), PageOffset(0), SquashedOffset::TX_STATE),
            )
            .is_none());

        // This row pointer has the correct `SquashedOffset`, but points out-of-bounds within `table`.
        assert!(table
            .get_row_ref(
                blob_store,
                RowPointer::new(false, PageIndex(0), PageOffset(0), SquashedOffset::COMMITTED_STATE),
            )
            .is_none());
    }
}
