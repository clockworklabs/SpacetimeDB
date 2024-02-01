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
use crate::static_assert_size;
use ahash::AHashMap;
use core::fmt;
use core::hash::{BuildHasher, Hasher};
use core::ops::RangeBounds;
use spacetimedb_primitives::ColList;
use spacetimedb_sats::{
    algebraic_value::ser::ValueSerializer,
    db::def::TableSchema,
    satn::Satn,
    ser::{Serialize, Serializer},
    AlgebraicValue, ProductType, ProductValue,
};
use thiserror::Error;

/// A database table containing the row schema, the rows, and indices.
///
/// The table stores the rows into a page manager
/// and uses an internal map to ensure that no identical row is stored more than once.
pub struct Table {
    /// The type of rows this table stores, with layout information included.
    row_layout: RowTypeLayout,
    /// The visitor program for `row_layout`.
    visitor_prog: VarLenVisitorProgram,
    /// The page manager that holds rows
    /// including both their fixed and variable components.
    pages: Pages,
    /// Maps `RowHash -> [RowPointer]` where a [`RowPointer`] points into `pages`.
    pointer_map: PointerMap,
    /// The indices associated with a set of columns of the table.
    pub indexes: AHashMap<ColList, BTreeIndex>,
    /// The schema of the table, from which the type, and other details are derived.
    pub schema: Box<TableSchema>,

    /// `SquashedOffset::TX_STATE` or `SquashedOffset::COMMITTED_STATE`
    /// depending on whether this is a tx scratchpad table
    /// or a committed table.
    squashed_offset: SquashedOffset,
}

static_assert_size!(Table, 248);

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
    /// Creates a new empty table with the given `schema`.
    pub fn new(schema: TableSchema, squashed_offset: SquashedOffset) -> Self {
        Self::new_with_indexes_capacity(schema, squashed_offset, 0)
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
                self.pages
                    .delete_row(&self.visitor_prog, self.row_size(), ptr, blob_store)
            };
            return Err(InsertError::Duplicate(existing_row));
        }

        // If the optimistic insertion was correct,
        // i.e. this is not a set-semantic duplicate,
        // add it to the `pointer_map`.
        self.pointer_map.insert(hash, ptr);

        // Insert row into indices.
        for (cols, index) in self.indexes.iter_mut() {
            index.insert(cols, row, ptr).unwrap();
        }

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
                &mut self.pages,
                &self.visitor_prog,
                blob_store,
                &self.row_layout,
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
                let (committed_page, committed_offset) = committed_table.page_and_offset(*committed_ptr);
                let (tx_page, tx_offset) = tx_table.page_and_offset(tx_ptr);

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
                        &committed_table.row_layout,
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
        unsafe { RowRef::new(self, blob_store, ptr) }
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
            self.pages
                .delete_row(&self.visitor_prog, self.row_size(), ptr, blob_store)
        };
    }

    /// Deletes the row identified by `ptr` from the table.
    // TODO: Make this `unsafe` and trust `ptr`; remove `Option` from return.
    //       See TODO comment on `Table::is_row_present`.
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

        // Delete row from indices.
        for (cols, index) in self.indexes.iter_mut() {
            let col_value = row_value.project_not_empty(cols).unwrap();
            let deleted = index.delete(&col_value, ptr);
            debug_assert!(deleted);
        }

        Some(row_value)
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
            // If an equal row was present, delete it.
            self.delete(blob_store, existing_row_ptr)
                .expect("Found a row by `Table::find_same_row`, but then failed to delete it");
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
    pub fn get_schema(&self) -> &TableSchema {
        &self.schema
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
        // TODO(perf): Consider `Arc`ing `self.schema`.
        // We'll still need to mutate the schema sometimes,
        // but those are rare, so we could use `ArcSwap` for that.
        let schema = self.schema.as_ref().clone();

        let mut new = Table::new_with_indexes_capacity(schema, squashed_offset, self.indexes.len());
        for (cols, index) in self.indexes.iter() {
            // `new` is known to be empty (we just constructed it!),
            // so no need for an actual blob store here.
            new.insert_index(
                &NullBlobStore,
                cols.clone(),
                BTreeIndex::new(index.index_id, index.is_unique, index.name.clone()),
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
        let (page, offset) = self.page_and_offset(ptr);
        // SAFETY: Caller promised that `ptr` refers to a live fixed row in this table, so:
        // 1. `offset` points at a row in `page` lasting `self.row_fixed_size` bytes.
        // 2. the row must be valid for `self.row_layout`.
        // 3. for any `vlr: VarLenRef` stored in the row,
        //    `vlr.first_offset` is either `NULL` or points to a valid granule in `page`.
        unsafe { hash_row_in_page(&mut hasher, page, offset, &self.row_layout) };
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
    table: &'a Table,
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
    unsafe fn new(table: &'a Table, blob_store: &'a dyn BlobStore, pointer: RowPointer) -> Self {
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

    /// Returns the raw row pointer for this row reference.
    pub fn pointer(&self) -> RowPointer {
        self.pointer
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
                    let next_page = self.table.pages.get(self.current_page_idx.idx())?;
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
    pub table_name: String,
    pub cols: Vec<String>,
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

    /// Returns a new empty table with the given `schema`
    /// and with a specified capacity for the `indexes` of the table.
    fn new_with_indexes_capacity(
        schema: TableSchema,
        squashed_offset: SquashedOffset,
        indexes_capacity: usize,
    ) -> Self {
        let row_layout: RowTypeLayout = schema.get_row_type().clone().into();
        let visitor_prog = row_type_visitor(&row_layout);
        Self {
            row_layout,
            visitor_prog,
            schema: Box::new(schema),
            indexes: AHashMap::with_capacity(indexes_capacity),
            pages: Pages::default(),
            pointer_map: PointerMap::default(),
            squashed_offset,
        }
    }

    /// Returns the page and page offset that `ptr` points to.
    fn page_and_offset(&self, ptr: RowPointer) -> (&Page, PageOffset) {
        (&self.pages[ptr.page_index()], ptr.page_offset())
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
        let (page, offset) = self.page_and_offset(ptr);
        self.squashed_offset == ptr.squashed_offset() && page.has_row_offset(self.row_size(), offset)
    }

    /// Returns the row size for a row in the table.
    fn row_size(&self) -> Size {
        self.row_layout.size()
    }

    /// Returns the fixed-len portion of the row at `ptr`.
    #[allow(unused)]
    fn get_fixed_row(&self, ptr: RowPointer) -> &Bytes {
        let (page, offset) = self.page_and_offset(ptr);
        page.get_row_data(offset, self.row_size())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::blob_store::HashMapBlobStore;
    use crate::indexes::{PageIndex, PageOffset};
    use crate::proptest_sats::generate_typed_row;
    use proptest::prelude::*;
    use proptest::test_runner::TestCaseResult;
    use spacetimedb_sats::bsatn::to_vec;
    use spacetimedb_sats::db::def::{ColumnDef, IndexDef, IndexType, TableDef};
    use spacetimedb_sats::{product, AlgebraicType, ArrayValue};

    fn table(ty: ProductType) -> Table {
        let def = TableDef::from_product("", ty);
        let schema = TableSchema::from_def(0.into(), def);
        Table::new(schema, SquashedOffset::COMMITTED_STATE)
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
        let index_schema = &schema.indexes[0];
        let index = BTreeIndex::new(index_schema.index_id, true, index_name);
        let mut table = Table::new(schema, SquashedOffset::COMMITTED_STATE);
        table.insert_index(&NullBlobStore, ColList::new(0.into()), index);

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
                assert_eq!(table_name, "UniqueIndexed");
                assert_eq!(cols, &["unique_col"]);
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

        prop_assert_eq!(table.pages.len(), 1);
        prop_assert_eq!(table.pages[PageIndex(0)].num_rows(), 1);

        prop_assert_eq!(unsafe { table.row_hash_for(ptr) }, hash);

        let row_ref = table.get_row_ref(&blob_store, ptr).unwrap();
        prop_assert_eq!(row_ref.to_product_value(), val.clone());
        prop_assert_eq!(to_vec(&val).unwrap(), to_vec(&row_ref).unwrap());

        prop_assert_eq!(
            &table.scan_rows(&blob_store).map(|r| r.pointer()).collect::<Vec<_>>(),
            &[ptr]
        );

        Ok(())
    }

    #[test]
    fn repro_serialize_bsatn_empty_array() {
        let ty = AlgebraicType::array(AlgebraicType::U64);
        let arr = ArrayValue::from(Vec::<u64>::new());
        insert_retrieve_body(ty, AlgebraicValue::from(arr)).unwrap();
    }

    #[test]
    fn repro_serialize_bsatn_debug_assert() {
        let ty = AlgebraicType::array(AlgebraicType::U64);
        let arr = ArrayValue::from((0..130u64).collect::<Vec<_>>());
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

            prop_assert_eq!(table.pages.len(), 1);
            prop_assert_eq!(table.pages[PageIndex(0)].num_rows(), 1);
            prop_assert_eq!(&table.scan_rows(&blob_store).map(|r| r.pointer()).collect::<Vec<_>>(), &[ptr]);

            table.delete(&mut blob_store, ptr);

            prop_assert_eq!(table.pointer_map.pointers_for(hash), &[]);

            prop_assert_eq!(table.pages.len(), 1);
            prop_assert_eq!(table.pages[PageIndex(0)].num_rows(), 0);

            prop_assert!(&table.scan_rows(&blob_store).next().is_none());
        }

        #[test]
        fn insert_duplicate_set_semantic((ty, val) in generate_typed_row()) {
            let mut blob_store = HashMapBlobStore::default();
            let mut table = table(ty);

            let (hash, ptr) = table.insert(&mut blob_store, &val).unwrap();
            prop_assert_eq!(table.pages.len(), 1);
            prop_assert_eq!(table.pointer_map.pointers_for(hash), &[ptr]);
            prop_assert_eq!(unsafe { table.row_hash_for(ptr) }, hash);
            prop_assert_eq!(&table.scan_rows(&blob_store).map(|r| r.pointer()).collect::<Vec<_>>(), &[ptr]);

            let blob_uses = blob_store.usage_counter();

            prop_assert!(table.insert(&mut blob_store, &val).is_err());
            prop_assert_eq!(table.pages.len(), 1);
            prop_assert_eq!(table.pointer_map.pointers_for(hash), &[ptr]);

            let blob_uses_after = blob_store.usage_counter();

            prop_assert_eq!(blob_uses_after, blob_uses);
            prop_assert_eq!(table.pages[PageIndex(0)].num_rows(), 1);
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
        let simple = table.pages.iter().zip((0..).map(PageIndex)).flat_map(|(page, pi)| {
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
