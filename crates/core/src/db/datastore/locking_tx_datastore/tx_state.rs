use core::ops::RangeBounds;
use spacetimedb_data_structures::map::{IntMap, IntSet};
use spacetimedb_primitives::{ColList, IndexId, TableId};
use spacetimedb_sats::{AlgebraicType, AlgebraicValue};
use spacetimedb_table::{
    blob_store::{BlobStore, HashMapBlobStore},
    indexes::{RowPointer, SquashedOffset},
    static_assert_size,
    table::{IndexScanIter, RowRef, Table},
};
use std::collections::{btree_map, BTreeMap, BTreeSet};

pub(super) type DeleteTable = BTreeSet<RowPointer>;

/// A mapping to find the actual index given an `IndexId`.
pub(super) type IndexIdMap = IntMap<IndexId, (TableId, ColList)>;
pub(super) type RemovedIndexIdSet = IntSet<IndexId>;

/// `TxState` tracks all of the modifications made during a particular transaction.
/// Rows inserted during a transaction will be added to insert_tables, and similarly,
/// rows deleted in the transaction will be added to delete_tables.
///
/// Note that the state of a row at the beginning of a transaction is not tracked here,
/// but rather in the `CommittedState` structure.
///
/// Note that because a transaction may have several operations performed on the same
/// row, it is not the case that a call to insert a row guarantees that the row
/// will be present in `insert_tables`. Rather, a row will be present in `insert_tables`
/// if the cumulative effect of all the calls results in the row being inserted during
/// this transaction. The same holds for delete tables.
///
/// For a concrete example, suppose a row is already present in a table at the start
/// of a transaction. A call to delete that row will enter it into `delete_tables`.
/// A subsequent call to reinsert that row will not put it into `insert_tables`, but
/// instead remove it from `delete_tables`, as the cumulative effect is to do nothing.
///
/// This data structure also tracks modifications beyond inserting and deleting rows.
/// In particular, creating indexes and sequences is tracked by `insert_tables`.
///
/// This means that we have the following invariants, within `TxState` and also
/// the corresponding `CommittedState`:
///   - any row in `insert_tables` must not be in the associated `CommittedState`
///   - any row in `delete_tables` must be in the associated `CommittedState`
///   - any row cannot be in both `insert_tables` and `delete_tables`
#[derive(Default)]
pub(super) struct TxState {
    //NOTE: Need to preserve order to correctly restore the db after reopen
    /// For any `TableId` that has had a row inserted into it in this TX
    /// (which may have since been deleted),
    /// a separate `Table` containing only the new insertions.
    ///
    /// `RowPointer`s into the `insert_tables` use `SquashedOffset::TX_STATE`.
    pub(super) insert_tables: BTreeMap<TableId, Table>,

    /// For any `TableId` that has had a previously-committed row deleted from it,
    /// a set of the deleted previously-committed rows.
    ///
    /// Any `RowPointer` in this set will have `SquashedOffset::COMMITTED_STATE`.
    pub(super) delete_tables: BTreeMap<TableId, DeleteTable>,

    /// A blob store for those blobs referred to by the `insert_tables`.
    ///
    /// When committing the TX, these blobs will be copied into the committed state blob store.
    /// Keeping the two separate makes rolling back a TX faster,
    /// as otherwise we'd have to either:
    /// - Maintain the set of newly-referenced blob hashes in the `TxState`,
    ///   and free each of them during rollback.
    /// - Traverse all rows in the `insert_tables` and free each of their blobs during rollback.
    pub(super) blob_store: HashMapBlobStore,

    /// Provides fast lookup for index id -> an index.
    pub(super) index_id_map: IndexIdMap,

    /// Lists all the `IndexId` that are to be removed from `CommittedState::index_id_map`.
    // This is in an `Option<Box<>>` to reduce the size of `TxState` - it's very uncommon
    // that this would be created.
    pub(super) index_id_map_removals: Option<Box<RemovedIndexIdSet>>,
}

static_assert_size!(TxState, 120);

impl TxState {
    /// Returns the row count in insert tables
    /// and the number of rows deleted from committed state.
    pub(super) fn table_row_count(&self, table_id: TableId) -> (Option<u64>, u64) {
        let del_count = self.delete_tables.get(&table_id).map(|dt| dt.len() as u64).unwrap_or(0);
        let ins_count = self.insert_tables.get(&table_id).map(|it| it.row_count);
        (ins_count, del_count)
    }

    /// When there's an index on `cols`,
    /// returns an iterator over the [BTreeIndex] that yields all the `RowId`s
    /// that match the specified `value` in the indexed column.
    ///
    /// Matching is defined by `Ord for AlgebraicValue`.
    ///
    /// For a unique index this will always yield at most one `RowId`.
    /// When there is no index this returns `None`.
    pub(super) fn index_seek<'a>(
        &'a self,
        table_id: TableId,
        cols: &ColList,
        range: &impl RangeBounds<AlgebraicValue>,
    ) -> Option<IndexScanIter<'a>> {
        self.insert_tables
            .get(&table_id)?
            .index_seek(&self.blob_store, cols, range)
    }

    // TODO(perf, deep-integration): Make this unsafe. Add the following to the docs:
    //
    // # Safety
    //
    // `pointer` must refer to a row within the table at `table_id`
    // which was previously inserted and has not been deleted since.
    //
    // See [`RowRef::new`] for more detailed requirements.
    //
    // Showing that `pointer` was the result of a call to `self.insert`
    // with `table_id`
    // and has not been passed to `self.delete`
    // is sufficient to demonstrate that a call to `self.get` is safe.
    pub(super) fn get(&self, table_id: TableId, row_ptr: RowPointer) -> RowRef<'_> {
        debug_assert!(
            row_ptr.squashed_offset().is_tx_state(),
            "Cannot get COMMITTED_STATE row_ptr from TxState.",
        );
        let table = self
            .insert_tables
            .get(&table_id)
            .expect("Attempt to get TX_STATE row from table not present in insert_tables.");

        // TODO(perf, deep-integration): Use `get_row_ref_unchecked`.
        table.get_row_ref(&self.blob_store, row_ptr).unwrap()
    }

    pub(super) fn is_deleted(&self, table_id: TableId, row_ptr: RowPointer) -> bool {
        debug_assert!(
            row_ptr.squashed_offset().is_committed_state(),
            "Not meaningful to have a deleted TX_STATE row; it would just be removed from the insert_tables.",
        );
        self.delete_tables
            .get(&table_id)
            .map(|tbl| tbl.contains(&row_ptr))
            .unwrap_or(false)
    }

    pub(super) fn get_delete_table_mut(&mut self, table_id: TableId) -> &mut DeleteTable {
        self.delete_tables.entry(table_id).or_default()
    }

    pub(super) fn get_table_and_blob_store(&mut self, table_id: TableId) -> Option<(&mut Table, &mut dyn BlobStore)> {
        let table = self.insert_tables.get_mut(&table_id)?;
        let blob_store = &mut self.blob_store;
        Some((table, blob_store))
    }

    pub(super) fn get_table_and_blob_store_or_maybe_create_from<'this>(
        &'this mut self,
        table_id: TableId,
        template: Option<&Table>,
    ) -> Option<(
        &'this mut Table,
        &'this mut dyn BlobStore,
        &'this mut IndexIdMap,
        &'this mut DeleteTable,
    )> {
        let insert_tables = &mut self.insert_tables;
        let delete_tables = &mut self.delete_tables;
        let blob_store = &mut self.blob_store;
        let idx_map = &mut self.index_id_map;
        let tbl = match insert_tables.entry(table_id) {
            btree_map::Entry::Vacant(e) => {
                let new_table = template?.clone_structure(SquashedOffset::TX_STATE);
                e.insert(new_table)
            }
            btree_map::Entry::Occupied(e) => e.into_mut(),
        };
        Some((tbl, blob_store, idx_map, delete_tables.entry(table_id).or_default()))
    }

    /// Returns the table and index associated with the given `table_id` and `col_list`, if any.
    pub(super) fn get_table_and_index_type(&self, table_id: TableId, col_list: &ColList) -> Option<&AlgebraicType> {
        let table = self.insert_tables.get(&table_id)?;
        let index = table.indexes.get(col_list)?;
        Some(&index.key_type)
    }
}
