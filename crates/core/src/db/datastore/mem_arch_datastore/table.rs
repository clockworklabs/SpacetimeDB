use super::{
    blob_store::{BlobStore, NullBlobStore},
    btree_index::{BTreeIndex, BTreeIndexRangeIter},
    de::read_row_from_page,
    eq::eq_row_in_page,
    indexes::{Bytes, PageIndex, RowHash, RowPointer, Size, SquashedOffset},
    layout::RowTypeLayout,
    page::FixedLenRowsIter,
    pages::Pages,
    pointer_map::PointerMap,
    row_hash::hash_row_in_page,
    row_vars_simple::{row_type_visitor, VarLenVisitorProgram},
    ser::write_row_to_pages,
};
use crate::{error::IndexError, static_assert_size};
use core::hash::{BuildHasher, Hasher};
use nonempty::NonEmpty;
use spacetimedb_primitives::ColId;
use spacetimedb_sats::{db::def::TableSchema, AlgebraicValue, ProductType, ProductValue};
use std::{collections::HashMap, ops::RangeBounds};
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
    pub(crate) indexes: HashMap<NonEmpty<ColId>, BTreeIndex>,
    // TODO(integration): Add `ProductType`, indices, and other table info needed.
    pub(crate) row_type: ProductType,
    pub(crate) schema: Box<TableSchema>,

    /// `SquashedOffset::TX_STATE` or `SquashedOffset::COMMITTED_STATE`
    /// depending on whether this is a tx scratchpad table
    /// or a committed table.
    squashed_offset: SquashedOffset,
}

static_assert_size!(Table, 256);

#[derive(Error, Debug)]
pub enum InsertError {
    #[error("Duplicate insertion of row {0:?} violates set semantics")]
    Duplicate(RowPointer),

    #[error("TODO(error-handling): describe possible failures in `write_row_to_pages`")]
    WriteRowToPages,

    #[error(transparent)]
    IndexError(#[from] IndexError),
}

// Public API:
impl Table {
    /// Creates a new empty table that can store rows with the given `row_type`.
    pub fn new(schema: TableSchema, squashed_offset: SquashedOffset) -> Self {
        let row_type = schema.get_row_type().clone();
        let row_layout: RowTypeLayout = row_type.clone().into();
        let visitor_prog = row_type_visitor(&row_layout);
        Self {
            row_type,
            row_layout,
            visitor_prog,
            schema: Box::new(schema),
            indexes: HashMap::default(),
            pages: Pages::default(),
            pointer_map: PointerMap::default(),
            squashed_offset,
        }
    }

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
        .map_err(|_| InsertError::WriteRowToPages)
    }

    fn build_error_unique(&self, index: &BTreeIndex, value: AlgebraicValue) -> IndexError {
        IndexError::UniqueConstraintViolation {
            constraint_name: index.name.clone(),
            table_name: self.schema.table_name.clone(),
            cols: index
                .cols
                .iter()
                .map(|&x| self.schema.columns()[usize::from(x)].col_name.clone())
                .collect(),
            value,
        }
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
        row: ProductValue,
    ) -> Result<(RowHash, RowPointer), InsertError> {
        // Check unique constraints
        for index in self.indexes.values() {
            if index.violates_unique_constraint(&row) {
                let value = row.project_not_empty(&index.cols).unwrap();
                return Err(self.build_error_unique(index, value).into());
            }
        }

        // Optimistically insert the `row` before checking for set-semantic collisions,
        // under the assumption that set-semantic collisions are rare.

        let ptr = self.insert_internal_allow_duplicate(blob_store, &row)?;

        // Ensure row isn't already there.
        // SAFETY: We just inserted `ptr`, so we know it's valid.
        let hash = unsafe { self.row_hash_for(ptr) };
        // Safety:
        // We just inserted `ptr` and computed `hash`, so they're valid.
        // `self` trivially has the same `row_layout` as `self`.
        let existing_row = unsafe { Self::contains_same_row(self, self, ptr, hash) };

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
        for index in self.indexes.values_mut() {
            index.insert(&row, ptr).unwrap();
        }

        Ok((hash, ptr))
    }

    /// Checks if `committed_table` contains a row equal (by `eq_row_in_page`)
    /// to the row at `tx_ptr` within `tx_table`,
    /// and if so, returns the pointer to the row in `committed_table`.
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
    /// - `tx_ptr` must refer to a valid existant row in `tx_table`.
    /// - `row_hash` must be the hash of the row at `tx_ptr`,
    ///   as returned by `tx_table.insert`.
    pub(crate) unsafe fn contains_same_row(
        committed_table: &Table,
        tx_table: &Table,
        tx_ptr: RowPointer,
        row_hash: RowHash,
    ) -> Option<RowPointer> {
        committed_table
            .pointer_map
            .pointers_for(row_hash)
            .iter()
            .copied()
            .find(|committed_ptr| {
                let committed_page = &committed_table.pages[committed_ptr.page_index()];
                let tx_page = &tx_table.pages[tx_ptr.page_index()];
                let committed_offset = committed_ptr.page_offset();
                let tx_offset = tx_ptr.page_offset();

                // Safety:
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

    /// Construct a `ProductValue` containing the data stored in `row`.
    // TODO: Make this `unsafe` and trust `ptr`; remove `Option` from return.
    //       See TODO comment on `Table::is_row_present`.
    pub fn get_row(&self, blob_store: &dyn BlobStore, row: RowPointer) -> Option<ProductValue> {
        debug_assert_eq!(
            row.squashed_offset(),
            self.squashed_offset,
            "Cannot get row with mismatched SquashedOffset from Table",
        );
        if !self.is_row_present(row) {
            return None;
        }

        let page = &self.pages[row.page_index()];
        // SAFETY: `ptr` points to a valid row in this table per above check.
        let pv = unsafe { read_row_from_page(page, blob_store, row.page_offset(), &self.row_layout) };
        Some(pv)
    }

    /// # Safety
    ///
    /// `ptr` must point to a valid, live row in this table.
    unsafe fn delete_internal_skip_pointer_map(&mut self, blob_store: &mut dyn BlobStore, ptr: RowPointer) {
        // Delete the physical row.
        // SAFETY: `self.row_size` known to be consistent with `self.pages`,
        // as the two are tied together in `Table::new`.
        // SAFETY: `ptr` points to a valid row in this table as we extracted `row_value`.
        unsafe {
            self.pages
                .delete_row(&self.visitor_prog, self.row_size(), ptr, blob_store)
        };
    }

    /// Deletes the row identified by `ptr` from the table.
    // TODO: Make this `unsafe` and trust `ptr`; remove `Option` from return.
    //       See TODO comment on `Table::is_row_present`.
    pub fn delete(&mut self, blob_store: &mut dyn BlobStore, ptr: RowPointer) -> Option<ProductValue> {
        // TODO(bikeshedding,integration): Do we want to make this method safe?
        // We could use `ptr` to ask the page if `is_row_present`
        // and check that `ptr.page_offset() % self.row_size() == 0`
        // and then `row_hash_for` would be safe.
        // This has some minor costs though.
        //
        // Current theory is that there's no reason to make this method safe;
        // it will be used through higher-level safe methods, like `delete_by_col_eq`,
        // which discover a known-valid `RowPointer` and pass it to this method.
        //
        // But for now since we need to check whether the row is present,
        // the method can be safe.
        let row_value = self.get_row(blob_store, ptr)?;

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
    /// by [`Table::contains_same_row`],
    /// delete that row.
    ///
    /// If a matching row was found, returns the pointer to that row.
    /// The returned pointer is now invalid, as the row to which it referred has been deleted.
    ///
    /// This operation works by temporarily inserting the `row` into `self`,
    /// checking `contains_same_row` on the newly-inserted row,
    /// deleting the matching row if it exists,
    /// then deleting the temporary insertion.
    pub fn delete_equal_row(
        &mut self,
        blob_store: &mut dyn BlobStore,
        row: ProductValue,
    ) -> Result<Option<RowPointer>, InsertError> {
        let ptr = self.insert_internal_allow_duplicate(blob_store, &row)?;

        // SAFETY: We just inserted `ptr`, so we know it's valid.
        let hash = unsafe { self.row_hash_for(ptr) };

        // Safety:
        // We just inserted `ptr` and computed `hash`, so they're valid.
        // `self` trivially has the same `row_layout` as `self`.
        let existing_row = unsafe { Self::contains_same_row(self, self, ptr, hash) };

        if let Some(existing_row) = existing_row {
            // If an equal row was present, delete it.
            self.delete(blob_store, existing_row)
                .expect("Found a row by `Table::contains_same_row`, but then failed to delete it");
        }

        // Safety: `ptr` is valid as we just inserted it.
        unsafe {
            self.delete_internal_skip_pointer_map(blob_store, ptr);
        }

        Ok(existing_row)
    }

    pub(crate) fn get_row_type(&self) -> &ProductType {
        &self.row_type
    }

    pub(crate) fn get_schema(&self) -> &TableSchema {
        &self.schema
    }

    pub fn insert_index(&mut self, blob_store: &dyn BlobStore, mut index: BTreeIndex) {
        index.build_from_rows(self.scan_rows(blob_store)).unwrap();
        self.indexes.insert(index.cols.clone(), index);
    }

    pub(crate) fn scan_rows<'a>(&'a self, blob_store: &'a dyn BlobStore) -> TableScanIter<'a> {
        TableScanIter {
            current_page: None,
            current_page_idx: PageIndex(0),
            table: self,
            blob_store,
        }
    }

    /// When there's an index for `cols`,
    /// returns an iterator over the [`BTreeIndex`] that yields all the `RowId`s
    /// that match the specified `range` in the indexed column.
    ///
    /// Matching is defined by `Ord for AlgebraicValue`.
    pub fn index_seek<'a>(
        &'a self,
        blob_store: &'a dyn BlobStore,
        cols: &NonEmpty<ColId>,
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

    pub(crate) fn from_template(template: &Table, squashed_offset: SquashedOffset) -> Self {
        let mut new = Table::new(template.schema.as_ref().clone(), squashed_offset);
        for index in template.indexes.values() {
            // `new` is known to be empty (we just constructed it!),
            // so no need for an actual blob store here.
            new.insert_index(
                &NullBlobStore,
                BTreeIndex::new(
                    index.index_id,
                    index.table_id,
                    index.cols.clone(),
                    index.name.clone(),
                    index.is_unique,
                ),
            );
        }
        new
    }
}

#[derive(Copy, Clone)]
/// A reference to a single row within a table.
//
// TODO: When [`Table::read_row`] becomes `unsafe`,
//       make this struct a "proof" that `pointer` is valid within `table`,
//       and make `RowRef::new` `unsafe`.
//       Add the following to the docs:
//
// # Safety
//
// Outside of the module boundary, the existince of a `RowRef`
// is sufficient to prove that it refers to a valid row,
// so `RowRef::read_row` is safe, whereas `RowRef::new` is unsafe.
pub struct RowRef<'a> {
    table: &'a Table,
    blob_store: &'a dyn BlobStore,
    pointer: RowPointer,
}

impl<'a> std::fmt::Debug for RowRef<'a> {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fmt.debug_struct("RowRef")
            .field("pointer", &self.pointer)
            .finish_non_exhaustive()
    }
}

impl<'a> RowRef<'a> {
    /// Construct a `RowRef` to the row at `pointer` within `table`.
    //
    // TODO: When [`Table::read_row`] becomes `unsafe`,
    //       make this struct a "proof" that `pointer` is valid within `table`,
    //       and make `RowRef::new` `unsafe`.
    //       Add the following to the docs:
    //
    // # Safety
    //
    // `pointer` must refer to a row within `table`
    // which was previously inserted and has not been deleted since.
    //
    // This means:
    // - The `PageIndex` of `pointer` must be in-bounds for `table.pages`.
    // - The `PageOffset` of `pointer` must be properly aligned for the row type of `table`,
    //   and must refer to a valid, live row in that page.
    // - The `SquashedOffset` of `pointer` must match `table.squashed_offset`.
    //
    // Showing that `pointer` was the result of a call to `table.insert`
    // and has not been passed to `table.delete`
    // is sufficient to demonstrate all of these properties.
    pub fn new(table: &'a Table, blob_store: &'a dyn BlobStore, pointer: RowPointer) -> Self {
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
    pub fn read_row(&self) -> ProductValue {
        self.table.get_row(self.blob_store, self.pointer).unwrap()
    }

    pub fn pointer(&self) -> RowPointer {
        self.pointer
    }
}

pub struct TableScanIter<'table> {
    current_page: Option<FixedLenRowsIter<'table>>,
    current_page_idx: PageIndex,
    pub(crate) table: &'table Table,
    pub(crate) blob_store: &'table dyn BlobStore,
}

impl<'a> Iterator for TableScanIter<'a> {
    // TODO(perf): Rewrite this to `Item = RowRef<'a>`,
    //             rather than eagerly reading out the `ProductValue`.
    //             Possibly LLVM eliminates the `ProductValue` anyways,
    //             but that seems unlikely.
    type Item = RowRef<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match &mut self.current_page {
                // If we're currently visiting a page,
                Some(iter_fixed_len) => {
                    if let Some(page_offset) = iter_fixed_len.next() {
                        // If there's still at least one row in that page to visit,
                        // return a ref to that row.

                        let ptr =
                            RowPointer::new(false, self.current_page_idx, page_offset, self.table.squashed_offset);

                        // TODO(perf, deep-integration): Once `RowRef::new` is unsafe, justify:
                        //
                        // SAFETY: `offset` came from the `iter_fixed_len`, so it must point to a valid row.
                        return Some(RowRef::new(self.table, self.blob_store, ptr));
                    } else {
                        // If we've finished visiting that page, set `current_page` to `None`,
                        // increment `self.current_page_idx` to the index of the next page,
                        // and go to the `None` case in the match.
                        self.current_page = None;
                        self.current_page_idx.0 += 1;
                    }
                }

                // If we aren't currently visiting a page,
                None => {
                    // The `else` case in the `Some` match arm
                    // already incremented `self.current_page_idx`,
                    // or we're just beginning and so it was initialized as 0.
                    let next_idx = self.current_page_idx.idx();

                    if next_idx >= self.table.pages.len() {
                        // If there aren't any more pages, we're done.

                        return None;
                    } else {
                        // If there's another page, set `self.current_page` to it,
                        // and go to the `Some` case in the match.

                        let next_page = &self.table.pages[self.current_page_idx];
                        let iter = next_page.iter_fixed_len(self.table.row_size());
                        self.current_page = Some(iter);
                    }
                }
            }
        }
    }
}

pub struct IndexScanIter<'a> {
    table: &'a Table,
    blob_store: &'a dyn BlobStore,
    btree_index_iter: BTreeIndexRangeIter<'a>,
}

impl<'a> Iterator for IndexScanIter<'a> {
    type Item = RowRef<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        let ptr = self.btree_index_iter.next()?;
        // FIXME: Determine if this is correct.
        // Will a table's index necessarily hold only pointers into that index?
        // Edge case: if an index is added during a transaction which then scans that index,
        // it appears that the newly-created `TxState` index
        // will also hold pointers into the `CommittedState`.
        Some(RowRef::new(self.table, self.blob_store, ptr))
    }
}

impl IndexScanIter<'_> {
    pub fn keys_scanned(&self) -> u64 {
        self.btree_index_iter.keys_scanned()
    }
}

// Private API:
impl Table {
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
        let page = &self.pages[ptr.page_index()];
        page.has_row_offset(self.row_size(), ptr.page_offset())
    }

    /// Returns the row size for a row in the table.
    fn row_size(&self) -> Size {
        self.row_layout.size()
    }

    /// Returns the fixed-len portion of the row at `ptr`.
    #[allow(unused)]
    fn get_fixed_row(&self, ptr: RowPointer) -> &Bytes {
        let page = &self.pages[ptr.page_index()];
        page.get_row_data(ptr.page_offset(), self.row_size())
    }

    /// Returns the row hash for `ptr`.
    ///
    /// # Safety
    ///
    /// `ptr` must refer to a valid fixed row in this table,
    /// i.e. have been previously returned by [`Table::insert`],
    /// and not deleted since.
    unsafe fn row_hash_for(&self, ptr: RowPointer) -> RowHash {
        let mut hasher = RowHash::hasher_builder().build_hasher();
        let page = &self.pages[ptr.page_index()];
        // SAFETY: Caller promised that `ptr` refers to a live fixed row in this table, so:
        // 1. `ptr.page_offset()` points at a row in `page` lasting `self.row_fixed_size` bytes.
        // 2. the row must be valid for `self.row_layout`.
        // 3. for any `vlr: VarLenRef` stored in the row,
        //   `vlr.first_offset` is either `NULL` or points to a valid granule in `page`.
        unsafe { hash_row_in_page(&mut hasher, page, ptr.page_offset(), &self.row_layout) };
        RowHash(hasher.finish())
    }
}

#[cfg(test)]
mod test {
    use super::super::blob_store::HashMapBlobStore;
    use super::super::indexes::PageIndex;
    use super::super::ser::test::generate_typed_row;
    use super::*;
    use proptest::prelude::*;
    use spacetimedb_sats::db::def::TableDef;

    fn table(ty: ProductType) -> Table {
        let def = TableDef::from_product("", ty);
        let schema = TableSchema::from_def(0.into(), def);
        Table::new(schema, SquashedOffset::COMMITTED_STATE)
    }

    proptest! {
        #![proptest_config(ProptestConfig { max_shrink_iters: 0x1_0000, ..Default::default() })]

        #[test]
        fn insert_retrieve((ty, val) in generate_typed_row()) {
            let mut blob_store = HashMapBlobStore::default();
            let mut table = table(ty);
            let (hash, ptr) = table.insert(&mut blob_store, val.clone()).unwrap();
            assert_eq!(table.pointer_map.pointers_for(hash), &[ptr]);

            assert_eq!(table.pages.len(), 1);
            assert_eq!(table.pages[PageIndex(0)].num_rows(), 1);

            assert_eq!(unsafe { table.row_hash_for(ptr) }, hash);

            let val_after = table.get_row(&blob_store, ptr);
            assert_eq!(val_after, Some(val.clone()));

            assert_eq!(&table.scan_rows(&blob_store).map(|r| r.pointer()).collect::<Vec<_>>(), &[ptr]);
        }

        #[test]
        fn insert_delete_removed_from_pointer_map((ty, val) in generate_typed_row()) {
            let mut blob_store = HashMapBlobStore::default();
            let mut table = table(ty);
            let (hash, ptr) = table.insert(&mut blob_store, val.clone()).unwrap();
            assert_eq!(table.pointer_map.pointers_for(hash), &[ptr]);

            assert_eq!(unsafe { table.row_hash_for(ptr) }, hash);

            assert_eq!(table.pages.len(), 1);
            assert_eq!(table.pages[PageIndex(0)].num_rows(), 1);
            assert_eq!(&table.scan_rows(&blob_store).map(|r| r.pointer()).collect::<Vec<_>>(), &[ptr]);

            table.delete(&mut blob_store, ptr);

            assert_eq!(table.pointer_map.pointers_for(hash), &[]);

            assert_eq!(table.pages.len(), 1);
            assert_eq!(table.pages[PageIndex(0)].num_rows(), 0);

            assert!(&table.scan_rows(&blob_store).next().is_none());
        }

        #[test]
        fn insert_duplicate_set_semantic((ty, val) in generate_typed_row()) {
            let mut blob_store = HashMapBlobStore::default();
            let mut table = table(ty);

            let (hash, ptr) = table.insert(&mut blob_store, val.clone()).unwrap();
            assert_eq!(table.pages.len(), 1);
            assert_eq!(table.pointer_map.pointers_for(hash), &[ptr]);
            assert_eq!(unsafe { table.row_hash_for(ptr) }, hash);
            assert_eq!(&table.scan_rows(&blob_store).map(|r| r.pointer()).collect::<Vec<_>>(), &[ptr]);

            let blob_uses = blob_store.useage_counter();

            assert!(table.insert(&mut blob_store, val.clone()).is_err());
            assert_eq!(table.pages.len(), 1);
            assert_eq!(table.pointer_map.pointers_for(hash), &[ptr]);

            let blob_uses_after = blob_store.useage_counter();

            // Can't `assert_eq` because `BlobHash: !Debug`.
            assert!(blob_uses_after == blob_uses);
            assert_eq!(table.pages[PageIndex(0)].num_rows(), 1);
            assert_eq!(&table.scan_rows(&blob_store).map(|r| r.pointer()).collect::<Vec<_>>(), &[ptr]);
        }
    }
}
