use super::{
    bflatn_from::serialize_row_from_page,
    bflatn_to::write_row_to_pages,
    blob_store::{BlobStore, NullBlobStore},
    btree_index::{BTreeIndex, BTreeIndexRangeIter},
    eq::eq_row_in_page,
    indexes::{Bytes, PageIndex, PageOffset, RowHash, RowPointer, Size, SquashedOffset},
    layout::RowTypeLayout,
    page::{FixedLenRowsIter, Page},
    pages::Pages,
    pointer_map::PointerMap,
    row_hash::hash_row_in_page,
    row_type_visitor::{row_type_visitor, VarLenVisitorProgram},
};
use crate::{
    bflatn_to_bsatn_fast_path::StaticBsatnLayout,
    read_column::{ReadColumn, TypeError},
    static_assert_size,
};
use core::fmt;
use core::hash::{BuildHasher, Hasher};
use core::ops::RangeBounds;
use spacetimedb_data_structures::map::HashMap;
use spacetimedb_primitives::{ColId, ColList};
use spacetimedb_sats::{
    algebraic_value::ser::ValueSerializer,
    bsatn::{self, ser::BsatnError},
    db::def::TableSchema,
    product_value::InvalidFieldError,
    satn::Satn,
    ser::{Serialize, Serializer},
    AlgebraicValue, ProductType, ProductValue,
};
use std::sync::Arc;
use thiserror::Error;

/// A database table containing the row schema, the rows, and indices.
///
/// The table stores the rows into a page manager
/// and uses an internal map to ensure that no identical row is stored more than once.
pub struct Table {
    /// Page manager and row layout grouped together, for `RowRef` purposes.
    inner: TableInner,
    /// The visitor program for `row_layout`.
    visitor_prog: VarLenVisitorProgram,
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
}

impl Table {
    pub fn row_layout(&self) -> &RowTypeLayout {
        &self.inner.row_layout
    }
    pub fn pages(&self) -> &Pages {
        &self.inner.pages
    }
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
    pub(crate) unsafe fn get_row_ref_unchecked<'a>(
        &'a self,
        blob_store: &'a dyn BlobStore,
        ptr: RowPointer,
    ) -> RowRef<'a> {
        // SAFETY: Forward caller requirements.
        unsafe { RowRef::new(self, blob_store, ptr) }
    }

    /// Returns the page and page offset that `ptr` points to.
    pub(crate) fn page_and_offset(&self, ptr: RowPointer) -> (&Page, PageOffset) {
        (&self.pages[ptr.page_index()], ptr.page_offset())
    }
}

static_assert_size!(Table, 240);

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
            let value = row.project_not_empty(cols).unwrap();
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
    /// and a `RowPointer` which identifies it.
    ///
    /// When a row equal to `row` already exists in `self`,
    /// returns `InsertError::Duplicate(existing_row_pointer)`,
    /// where `existing_row_pointer` is a `RowPointer` which identifies the existing row.
    /// In this case, the duplicate is not inserted,
    /// but internal data structures may be altered in ways that affect performance and fragmentation.
    ///
    /// TODO(error-handling): describe errors from `write_row_to_pages` and return meaningful errors.
    pub fn insert(
        &mut self,
        blob_store: &mut dyn BlobStore,
        row: &ProductValue,
    ) -> Result<(RowHash, RowPointer), InsertError> {
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

        Ok((hash, ptr))
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
        let ptr = self.insert_internal_allow_duplicate(blob_store, row)?;

        // Ensure row isn't already there.
        // SAFETY: We just inserted `ptr`, so we know it's valid.
        let hash = unsafe { self.row_hash_for(ptr) };
        // Safety:
        // We just inserted `ptr` and computed `hash`, so they're valid.
        // `self` trivially has the same `row_layout` as `self`.
        let existing_row = unsafe { Self::find_same_row(self, self, ptr, hash) };

        if let Some(existing_row) = existing_row {
            // If an equal row was already present,
            // roll back our optimistic insert to avoid violating set semantics.

            // SAFETY: we just inserted `ptr`, so it must be valid.
            unsafe {
                self.inner
                    .pages
                    .delete_row(&self.visitor_prog, self.row_size(), ptr, blob_store)
            };
            return Err(InsertError::Duplicate(existing_row));
        }

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
    pub fn insert_internal_allow_duplicate(
        &mut self,
        blob_store: &mut dyn BlobStore,
        row: &ProductValue,
    ) -> Result<RowPointer, InsertError> {
        // SAFETY: `self.pages` is known to be specialized for `self.row_layout`,
        // as `self.pages` was constructed from `self.row_layout` in `Table::new`.
        unsafe {
            write_row_to_pages(
                &mut self.inner.pages,
                &self.visitor_prog,
                blob_store,
                &self.inner.row_layout,
                row,
                self.squashed_offset,
            )
        }
        .map_err(Into::into)
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
    /// - `row_hash` must be the hash of the row at `tx_ptr`,
    ///   as returned by `tx_table.insert`.
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
    pub unsafe fn delete_internal_skip_pointer_map(&mut self, blob_store: &mut dyn BlobStore, ptr: RowPointer) {
        // Delete the physical row.
        //
        // SAFETY:
        // - `ptr` points to a valid row in this table, per our invariants.
        // - `self.row_size` known to be consistent with `self.pages`,
        //    as the two are tied together in `Table::new`.
        unsafe {
            self.inner
                .pages
                .delete_row(&self.visitor_prog, self.row_size(), ptr, blob_store)
        };
    }

    /// Deletes the row identified by `ptr` from the table.
    /// NOTE: This method skips updating indexes. Use `delete` to delete a row with index updating.
    pub fn delete_internal(&mut self, blob_store: &mut dyn BlobStore, ptr: RowPointer) -> Option<ProductValue> {
        let row_value = self.get_row_ref(blob_store, ptr)?.to_product_value();

        // Remove the set semantic association.
        // SAFETY: `ptr` points to a valid row in this table as we extracted `row_value`.
        let hash = unsafe { self.row_hash_for(ptr) };
        let _remove_result = self.pointer_map.remove(hash, ptr);
        debug_assert!(_remove_result);

        // Delete the physical row.
        // SAFETY: `ptr` points to a valid row in this table as we extracted `row_value`.
        unsafe {
            self.delete_internal_skip_pointer_map(blob_store, ptr);
        };

        Some(row_value)
    }

    /// Deletes the row identified by `ptr` from the table.
    // TODO(perf,bikeshedding): Make this `unsafe` and trust `ptr`; remove `Option` from return.
    //     See TODO comment on `Table::is_row_present`.
    // TODO(perf): Remove returned `ProductValue`.
    //     Require callers who want the row to read it out explicitly before deleting.
    pub fn delete(&mut self, blob_store: &mut dyn BlobStore, ptr: RowPointer) -> Option<ProductValue> {
        // TODO(bikeshedding,integration): Do we want to make this method unsafe?
        // We currently use `ptr` to ask the page if `is_row_present` which checks alignment.
        // Based on this, we can input `ptr` to `row_hash_for`.
        // This has some minor costs though.
        //
        // Current theory is that there's no reason to make this method safe;
        // it will be used through higher-level safe methods, like `delete_by_col_eq`,
        // which discover a known-valid `RowPointer` and pass it to this method.
        //
        // But for now since we need to check whether the row is present,
        // the method can be safe.

        // SAFETY: `ptr` points to a valid row in this table as we extracted `row_value`.
        let row_ref = unsafe { self.inner.get_row_ref_unchecked(blob_store, ptr) };

        // Delete row from indices.
        // Do this before the actual deletion, as `index.delete` needs a `RowRef`
        // so it can extract the appropriate value.
        for (cols, index) in self.indexes.iter_mut() {
            let deleted = index.delete(cols, row_ref).unwrap();
            debug_assert!(deleted);
        }

        self.delete_internal(blob_store, ptr)
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
        let temp_ptr = self.insert_internal_allow_duplicate(blob_store, row)?;

        // SAFETY: We just inserted `ptr`, so we know it's valid.
        let hash = unsafe { self.row_hash_for(temp_ptr) };

        // Find the row equal to the passed-in `row`.
        // SAFETY:
        // - `self` trivially has the same `row_layout` as `self`.
        // - We just inserted `temp_ptr` and computed `hash`, so they're valid.
        let existing_row_ptr = unsafe { Self::find_same_row(self, self, temp_ptr, hash) };

        if let Some(existing_row_ptr) = existing_row_ptr {
            if skip_index_update {
                self.delete_internal(blob_store, existing_row_ptr)
                    .expect("Found a row by `Table::find_same_row`, but then failed to delete it");
            } else {
                // If an equal row was present, delete it.
                self.delete(blob_store, existing_row_ptr)
                    .expect("Found a row by `Table::find_same_row`, but then failed to delete it");
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
        let mut schema = self.schema.clone();
        with(Arc::make_mut(&mut schema));
        self.schema = schema;
    }

    /// Inserts a new `index` into the table.
    /// The index will be populated using the rows of the table.
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
        let sbl = self.inner.static_bsatn_layout.clone();
        let visitor = self.visitor_prog.clone();
        let mut new =
            Table::new_with_indexes_capacity(schema, layout, sbl, visitor, squashed_offset, self.indexes.len());

        for (cols, index) in self.indexes.iter() {
            // `new` is known to be empty (we just constructed it!),
            // so no need for an actual blob store here.
            new.insert_index(
                &NullBlobStore,
                cols.clone(),
                BTreeIndex::new(
                    index.index_id,
                    &self.inner.row_layout,
                    cols,
                    index.is_unique,
                    index.name.clone(),
                )
                .unwrap(),
            );
        }
        new
    }

    /// Returns the row hash for `ptr`.
    ///
    /// # Safety
    ///
    /// `ptr` must refer to a valid fixed row in this table,
    /// i.e. have been previously returned by [`Table::insert`] or [`Table::insert_internal_allow_duplicates`],
    /// and not deleted since.
    pub unsafe fn row_hash_for(&self, ptr: RowPointer) -> RowHash {
        let mut hasher = RowHash::hasher_builder().build_hasher();
        let (page, offset) = self.inner.page_and_offset(ptr);
        // SAFETY: Caller promised that `ptr` refers to a live fixed row in this table, so:
        // 1. `offset` points at a row in `page` lasting `self.row_fixed_size` bytes.
        // 2. the row must be valid for `self.row_layout`.
        // 3. for any `vlr: VarLenRef` stored in the row,
        //    `vlr.first_offset` is either `NULL` or points to a valid granule in `page`.
        unsafe { hash_row_in_page(&mut hasher, page, offset, &self.inner.row_layout) };
        RowHash(hasher.finish())
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
    #[doc(alias = "read_row")]
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
    /// If `cols` contains more than one column, the values of the projected columns are wrapped in a [`ProductValue`].
    /// If `cols` is a single column, the value of that column is returned without wrapping in a `ProductValue`.
    pub fn project_not_empty(self, cols: &ColList) -> Result<AlgebraicValue, InvalidFieldError> {
        let len = match cols.len() {
            0 => unreachable!("A `ColList` can never be empty"),
            1 => return self.read_col(cols.head()).map_err(|_| cols.head().into()),
            len => len,
        };
        let mut elements = Vec::with_capacity(len as usize);
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

    pub(crate) fn blob_store(&self) -> &dyn BlobStore {
        self.blob_store
    }

    pub fn row_layout(&self) -> &RowTypeLayout {
        &self.table.row_layout
    }

    pub fn page_and_offset(&self) -> (&Page, PageOffset) {
        self.table.page_and_offset(self.pointer())
    }

    /// The length of this row when BSATN-encoded.
    ///
    /// Only available for rows whose types have a static BSATN layout.
    /// Returns `None` for rows of other types, e.g. rows containing strings.
    pub fn bsatn_length(&self) -> Option<usize> {
        self.table.static_bsatn_layout.as_ref().map(|s| s.bsatn_length as usize)
    }

    /// BSATN-encode the row referred to by `self` into a freshly-allocated `Vec<u8>`.
    ///
    /// This method will use a [`StaticBsatnLayout`] if one is available,
    /// and may therefore be faster than calling [`bsatn::to_vec`].
    pub fn to_bsatn_vec(&self) -> Result<Vec<u8>, BsatnError> {
        if let Some(static_bsatn_layout) = &self.table.static_bsatn_layout {
            let mut vec = vec![0; static_bsatn_layout.bsatn_length as usize];
            let (page, offset) = self.page_and_offset();
            let row = page.get_row_data(offset, self.table.row_layout.size());
            // Safety:
            // - Existence of a `RowRef` treated as proof
            //   of row's validity and type information's correctness.
            // - `vec` constructed with exactly correct length above.
            unsafe {
                static_bsatn_layout.serialize_row_into(&mut vec, row);
            }
            Ok(vec)
        } else {
            bsatn::to_vec(self)
        }
    }

    /// BSATN-encode the row referred to by `self` into `buf`,
    /// pushing `self`'s bytes onto the end of `buf`, similar to [`Vec::extend`].
    ///
    /// This method will use a [`StaticBsatnLayout`] if one is available,
    /// and may therefore be faster than calling [`bsatn::to_writer`].
    pub fn to_bsatn_extend(&self, buf: &mut Vec<u8>) -> Result<(), BsatnError> {
        if let Some(static_bsatn_layout) = &self.table.static_bsatn_layout {
            // Get an initially-zeroed slice within `buf` of the correct length.
            let start = buf.len();
            let len = static_bsatn_layout.bsatn_length as usize;
            buf.reserve(len);
            buf.extend(std::iter::repeat(0).take(len));
            let buf = &mut buf[start..start + len];

            // Find the row referred to by `self`.
            let (page, offset) = self.page_and_offset();
            let row = page.get_row_data(offset, self.table.row_layout.size());

            // Write the row into the slice using a series of `memcpy`s.
            // Safety:
            // - Existence of a `RowRef` treated as proof
            //   of row's validity and type information's correctness.
            // - `buf` constructed with exactly correct length above.
            unsafe {
                static_bsatn_layout.serialize_row_into(buf, row);
            }

            Ok(())
        } else {
            // Use the slower, but more general, `bsatn_from` serializer to write the row.
            bsatn::to_writer(buf, self)
        }
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
    pub constraint_name: String,
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

        let cols = cols
            .iter()
            .map(|x| schema.columns()[x.idx()].col_name.clone())
            .collect();

        UniqueConstraintViolation {
            constraint_name: index.name.clone().into(),
            table_name: schema.table_name.clone(),
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
                pages: Pages::default(),
            },
            visitor_prog,
            schema,
            indexes: HashMap::with_capacity(indexes_capacity),
            pointer_map: PointerMap::default(),
            squashed_offset,
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
        let (page, offset) = self.inner.page_and_offset(ptr);
        self.squashed_offset == ptr.squashed_offset() && page.has_row_offset(self.row_size(), offset)
    }

    /// Returns the row size for a row in the table.
    fn row_size(&self) -> Size {
        self.inner.row_layout.size()
    }

    /// Returns the fixed-len portion of the row at `ptr`.
    #[allow(unused)]
    fn get_fixed_row(&self, ptr: RowPointer) -> &Bytes {
        let (page, offset) = self.inner.page_and_offset(ptr);
        page.get_row_data(offset, self.row_size())
    }
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;
    use crate::blob_store::HashMapBlobStore;
    use crate::indexes::{PageIndex, PageOffset};
    use proptest::prelude::*;
    use proptest::test_runner::TestCaseResult;
    use spacetimedb_sats::bsatn::to_vec;
    use spacetimedb_sats::db::def::{ColumnDef, IndexDef, IndexType, TableDef};
    use spacetimedb_sats::proptest::generate_typed_row;
    use spacetimedb_sats::{product, AlgebraicType, ArrayValue};

    pub(crate) fn table(ty: ProductType) -> Table {
        let def = TableDef::from_product("", ty);
        let schema = TableSchema::from_def(0.into(), def);
        Table::new(schema.into(), SquashedOffset::COMMITTED_STATE)
    }

    #[test]
    fn unique_violation_error() {
        let index_name = "my_unique_constraint";
        // Build a table for (I32, I32) with a unique index on the 0th column.
        let table_def = TableDef::new(
            "UniqueIndexed".into(),
            ["unique_col", "other_col"]
                .map(|c| ColumnDef {
                    col_name: c.into(),
                    col_type: AlgebraicType::I32,
                })
                .into(),
        )
        .with_indexes(vec![IndexDef {
            columns: 0.into(),
            index_name: index_name.into(),
            is_unique: true,
            index_type: IndexType::BTree,
        }]);
        let schema = TableSchema::from_def(0.into(), table_def);
        let index_schema = schema.indexes[0].clone();
        let mut table = Table::new(schema.into(), SquashedOffset::COMMITTED_STATE);
        let cols = ColList::new(0.into());

        let index = BTreeIndex::new(index_schema.index_id, &table.inner.row_layout, &cols, true, index_name).unwrap();
        table.insert_index(&NullBlobStore, cols, index);

        // Insert the row (0, 0).
        table
            .insert(&mut NullBlobStore, &product![0i32, 0i32])
            .expect("Initial insert failed");

        // Try to insert the row (0, 1), and assert that we get the expected error.
        match table.insert(&mut NullBlobStore, &product![0i32, 1i32]) {
            Ok(_) => panic!("Second insert with same unique value succeeded"),
            Err(InsertError::IndexError(UniqueConstraintViolation {
                constraint_name,
                table_name,
                cols,
                value,
            })) => {
                assert_eq!(constraint_name, index_name);
                assert_eq!(&*table_name, "UniqueIndexed");
                assert_eq!(cols.iter().map(|c| c.to_string()).collect::<Vec<_>>(), &["unique_col"]);
                assert_eq!(value, AlgebraicValue::I32(0));
            }
            Err(e) => panic!("Expected UniqueConstraintViolation but found {:?}", e),
        }
    }

    fn insert_retrieve_body(ty: impl Into<ProductType>, val: impl Into<ProductValue>) -> TestCaseResult {
        let val = val.into();
        let mut blob_store = HashMapBlobStore::default();
        let mut table = table(ty.into());
        let (hash, ptr) = table.insert(&mut blob_store, &val).unwrap();
        prop_assert_eq!(table.pointer_map.pointers_for(hash), &[ptr]);

        prop_assert_eq!(table.inner.pages.len(), 1);
        prop_assert_eq!(table.inner.pages[PageIndex(0)].num_rows(), 1);

        prop_assert_eq!(unsafe { table.row_hash_for(ptr) }, hash);

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
            let (hash, ptr) = table.insert(&mut blob_store, &val).unwrap();
            prop_assert_eq!(table.pointer_map.pointers_for(hash), &[ptr]);

            prop_assert_eq!(unsafe { table.row_hash_for(ptr) }, hash);

            prop_assert_eq!(table.inner.pages.len(), 1);
            prop_assert_eq!(table.inner.pages[PageIndex(0)].num_rows(), 1);
            prop_assert_eq!(&table.scan_rows(&blob_store).map(|r| r.pointer()).collect::<Vec<_>>(), &[ptr]);

            table.delete(&mut blob_store, ptr);

            prop_assert_eq!(table.pointer_map.pointers_for(hash), &[]);

            prop_assert_eq!(table.inner.pages.len(), 1);
            prop_assert_eq!(table.inner.pages[PageIndex(0)].num_rows(), 0);

            prop_assert!(&table.scan_rows(&blob_store).next().is_none());
        }

        #[test]
        fn insert_duplicate_set_semantic((ty, val) in generate_typed_row()) {
            let mut blob_store = HashMapBlobStore::default();
            let mut table = table(ty);

            let (hash, ptr) = table.insert(&mut blob_store, &val).unwrap();
            prop_assert_eq!(table.inner.pages.len(), 1);
            prop_assert_eq!(table.pointer_map.pointers_for(hash), &[ptr]);
            prop_assert_eq!(unsafe { table.row_hash_for(ptr) }, hash);
            prop_assert_eq!(&table.scan_rows(&blob_store).map(|r| r.pointer()).collect::<Vec<_>>(), &[ptr]);

            let blob_uses = blob_store.usage_counter();

            prop_assert!(table.insert(&mut blob_store, &val).is_err());
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
        let (_, ptr) = table.insert(&mut NullBlobStore, &pv).unwrap();

        // Manipulate the page offset to 1 instead of 0.
        // This now points into the "middle" of a row.
        let ptr = ptr.with_page_offset(PageOffset(1));

        // We expect this to panic.
        // Miri should not have any issue with this call either.
        table.get_row_ref(&NullBlobStore, ptr).unwrap().to_product_value();
    }
}
