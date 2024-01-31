use super::{btree_index::BTreeIndexRangeIter, table::Table, RowId};
use spacetimedb_primitives::{ColList, TableId};
use spacetimedb_sats::{AlgebraicValue, ProductValue};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::RangeBounds,
};

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
pub struct TxState {
    //NOTE: Need to preserve order to correctly restore the db after reopen
    /// For each table,  additions have
    pub(crate) insert_tables: BTreeMap<TableId, Table>,
    pub(crate) delete_tables: BTreeMap<TableId, BTreeSet<RowId>>,
}

/// Represents whether a row has been previously committed, inserted
/// or deleted this transaction, or simply not present at all.
pub enum RowState<'a> {
    /// The row is present in the table because it was inserted
    /// in a previously committed transaction.
    Committed(&'a ProductValue),
    /// The row is present because it has been inserted in the
    /// current transaction.
    Insert(&'a ProductValue),
    /// The row is absent because it has been deleted in the
    /// current transaction.
    Delete,
    /// The row is not present in the table.
    Absent,
}

impl TxState {
    /// Returns the state of `row_id` within the table identified by `table_id`.
    #[tracing::instrument(skip_all)]
    pub fn get_row_op(&self, table_id: &TableId, row_id: &RowId) -> RowState {
        if let Some(true) = self.delete_tables.get(table_id).map(|set| set.contains(row_id)) {
            return RowState::Delete;
        }
        let Some(table) = self.insert_tables.get(table_id) else {
            return RowState::Absent;
        };
        table.get_row(row_id).map(RowState::Insert).unwrap_or(RowState::Absent)
    }

    #[tracing::instrument(skip_all)]
    pub fn get_row(&self, table_id: &TableId, row_id: &RowId) -> Option<&ProductValue> {
        if Some(true) == self.delete_tables.get(table_id).map(|set| set.contains(row_id)) {
            return None;
        }
        let Some(table) = self.insert_tables.get(table_id) else {
            return None;
        };
        table.get_row(row_id)
    }

    pub fn get_insert_table_mut(&mut self, table_id: &TableId) -> Option<&mut Table> {
        self.insert_tables.get_mut(table_id)
    }

    pub fn get_insert_table(&self, table_id: &TableId) -> Option<&Table> {
        self.insert_tables.get(table_id)
    }

    pub fn get_or_create_delete_table(&mut self, table_id: TableId) -> &mut BTreeSet<RowId> {
        self.delete_tables.entry(table_id).or_default()
    }

    /// When there's an index on `cols`,
    /// returns an iterator over the [BTreeIndex] that yields all the `RowId`s
    /// that match the specified `value` in the indexed column.
    ///
    /// Matching is defined by `Ord for AlgebraicValue`.
    ///
    /// For a unique index this will always yield at most one `RowId`.
    /// When there is no index this returns `None`.
    pub fn index_seek<'a>(
        &'a self,
        table_id: &TableId,
        cols: &ColList,
        range: &impl RangeBounds<AlgebraicValue>,
    ) -> Option<BTreeIndexRangeIter<'a>> {
        self.insert_tables.get(table_id)?.index_seek(cols, range)
    }
}
