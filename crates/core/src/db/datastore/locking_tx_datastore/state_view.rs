use super::{
    committed_state::{CommittedIndexIter, CommittedState},
    datastore::Result,
    tx_state::{DeleteTable, TxState},
};
use crate::{
    db::datastore::system_tables::{
        StColumnFields, StColumnRow, StConstraintFields, StConstraintRow, StIndexFields, StIndexRow, StScheduledFields,
        StScheduledRow, StSequenceFields, StSequenceRow, StTableFields, StTableRow, SystemTable, ST_COLUMN_ID,
        ST_CONSTRAINT_ID, ST_INDEX_ID, ST_SCHEDULED_ID, ST_SEQUENCE_ID, ST_TABLE_ID,
    },
    error::TableError,
};
use core::ops::RangeBounds;
use spacetimedb_primitives::{ColList, TableId};
use spacetimedb_sats::AlgebraicValue;
use spacetimedb_schema::schema::{ColumnSchema, TableSchema};
use spacetimedb_table::{
    blob_store::HashMapBlobStore,
    table::{IndexScanIter, RowRef, Table, TableScanIter},
};
use std::sync::Arc;

// StateView trait, is designed to define the behavior of viewing internal datastore states.
// Currently, it applies to: CommittedState, MutTxId, and TxId.
pub trait StateView {
    fn get_schema(&self, table_id: TableId) -> Option<&Arc<TableSchema>>;

    fn table_id_from_name(&self, table_name: &str) -> Result<Option<TableId>> {
        let name = &<Box<str>>::from(table_name).into();
        let row = self.iter_by_col_eq(ST_TABLE_ID, StTableFields::TableName, name)?.next();
        Ok(row.map(|row| row.read_col(StTableFields::TableId).unwrap()))
    }

    /// Returns the number of rows in the table identified by `table_id`.
    fn table_row_count(&self, table_id: TableId) -> Option<u64>;

    fn iter(&self, table_id: TableId) -> Result<Iter<'_>>;

    fn table_name(&self, table_id: TableId) -> Option<&str> {
        self.get_schema(table_id).map(|s| &*s.table_name)
    }

    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`,
    /// where the values of `cols` are contained in `range`.
    fn iter_by_col_range<R: RangeBounds<AlgebraicValue>>(
        &self,
        table_id: TableId,
        cols: ColList,
        range: R,
    ) -> Result<IterByColRange<'_, R>>;

    fn iter_by_col_eq<'a, 'r>(
        &'a self,
        table_id: TableId,
        cols: impl Into<ColList>,
        value: &'r AlgebraicValue,
    ) -> Result<IterByColEq<'a, 'r>> {
        self.iter_by_col_range(table_id, cols.into(), value)
    }

    /// Reads the schema information for the specified `table_id` directly from the database.
    fn schema_for_table_raw(&self, table_id: TableId) -> Result<TableSchema> {
        // Look up the table_name for the table in question.
        let value_eq = &table_id.into();
        let row = self
            .iter_by_col_eq(ST_TABLE_ID, StTableFields::TableId, value_eq)?
            .next()
            .ok_or_else(|| TableError::IdNotFound(SystemTable::st_table, table_id.into()))?;
        let row = StTableRow::try_from(row)?;
        let table_name = row.table_name;
        let table_id: TableId = row.table_id;
        let table_type = row.table_type;
        let table_access = row.table_access;
        let table_primary_key = row.table_primary_key.as_ref().and_then(ColList::as_singleton);

        // Look up the columns for the table in question.
        let mut columns: Vec<ColumnSchema> = self
            .iter_by_col_eq(ST_COLUMN_ID, StColumnFields::TableId, value_eq)?
            .map(|row| {
                let row = StColumnRow::try_from(row)?;
                Ok(row.into())
            })
            .collect::<Result<Vec<_>>>()?;
        columns.sort_by_key(|col| col.col_pos);

        // Look up the constraints for the table in question.
        let constraints = self
            .iter_by_col_eq(ST_CONSTRAINT_ID, StConstraintFields::TableId, value_eq)?
            .map(|row| {
                let row = StConstraintRow::try_from(row)?;
                Ok(row.into())
            })
            .collect::<Result<Vec<_>>>()?;

        // Look up the sequences for the table in question.
        let sequences = self
            .iter_by_col_eq(ST_SEQUENCE_ID, StSequenceFields::TableId, value_eq)?
            .map(|row| {
                let row = StSequenceRow::try_from(row)?;
                Ok(row.into())
            })
            .collect::<Result<Vec<_>>>()?;

        // Look up the indexes for the table in question.
        let indexes = self
            .iter_by_col_eq(ST_INDEX_ID, StIndexFields::TableId, value_eq)?
            .map(|row| {
                let row = StIndexRow::try_from(row)?;
                Ok(row.into())
            })
            .collect::<Result<Vec<_>>>()?;

        let schedule = self
            .iter_by_col_eq(ST_SCHEDULED_ID, StScheduledFields::TableId, value_eq)?
            .next()
            .map(|row| -> Result<_> {
                let row = StScheduledRow::try_from(row)?;
                Ok(row.into())
            })
            .transpose()?;

        Ok(TableSchema::new(
            table_id,
            table_name,
            columns,
            indexes,
            constraints,
            sequences,
            table_type,
            table_access,
            schedule,
            table_primary_key,
        ))
    }

    /// Reads the schema information for the specified `table_id`, consulting the `cache` first.
    ///
    /// If the schema is not found in the cache, the method calls [Self::schema_for_table_raw].
    ///
    /// Note: The responsibility of populating the cache is left to the caller.
    fn schema_for_table(&self, table_id: TableId) -> Result<Arc<TableSchema>> {
        if let Some(schema) = self.get_schema(table_id) {
            return Ok(schema.clone());
        }

        self.schema_for_table_raw(table_id).map(Arc::new)
    }
}

pub struct Iter<'a> {
    table_id: TableId,
    tx_state_del: Option<&'a DeleteTable>,
    tx_state_ins: Option<(&'a Table, &'a HashMapBlobStore)>,
    committed_state: &'a CommittedState,
    stage: ScanStage<'a>,
}

impl<'a> Iter<'a> {
    pub(super) fn new(table_id: TableId, tx_state: Option<&'a TxState>, committed_state: &'a CommittedState) -> Self {
        let tx_state_ins = tx_state.and_then(|tx| {
            let ins = tx.insert_tables.get(&table_id)?;
            let bs = &tx.blob_store;
            Some((ins, bs))
        });
        let tx_state_del = tx_state.and_then(|tx| tx.delete_tables.get(&table_id));
        Self {
            table_id,
            tx_state_ins,
            tx_state_del,
            committed_state,
            stage: ScanStage::Start,
        }
    }
}

enum ScanStage<'a> {
    /// We haven't decided yet where to yield from.
    Start,
    /// Yielding rows from the current tx.
    CurrentTx { iter: TableScanIter<'a> },
    /// Yielding rows from the committed state
    /// without considering tx state deletes as there are none.
    CommittedNoTxDeletes { iter: TableScanIter<'a> },
    /// Yielding rows from the committed state
    /// but there are deleted rows in the tx state,
    /// so we must check against those.
    CommittedWithTxDeletes { iter: TableScanIter<'a> },
}

impl<'a> Iterator for Iter<'a> {
    type Item = RowRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let table_id = self.table_id;

        // The finite state machine goes:
        //
        //     Start
        //       |
        //       |--> CurrentTx -------------------------------\
        //       |        ^                                    |
        //       |        \--------------------\               |
        //       |                             ^               |
        //       |--> CommittedNoTxDeletes ----|---------------\
        //       |                             ^               v
        //       \--> CommittedWithTxDeletes --|------/----> Stop

        loop {
            match &mut self.stage {
                ScanStage::Start => {
                    if let Some(table) = self.committed_state.tables.get(&table_id) {
                        // The committed state has changes for this table.
                        let iter = table.scan_rows(&self.committed_state.blob_store);
                        self.stage = if self.tx_state_del.is_some() {
                            // There are no deletes in the tx state
                            // so we don't need to care about those (1a).
                            ScanStage::CommittedWithTxDeletes { iter }
                        } else {
                            // There are deletes in the tx state
                            // so we must exclude those (1b).
                            ScanStage::CommittedNoTxDeletes { iter }
                        };
                        continue;
                    }
                }
                ScanStage::CommittedNoTxDeletes { iter } => {
                    // (1a) Go through the committed state for this table
                    // but do not consider deleted rows.
                    if let next @ Some(_) = iter.next() {
                        return next;
                    }
                }
                ScanStage::CommittedWithTxDeletes { iter } => {
                    // (1b) Check the committed row's state in the current tx.
                    // If it's been deleted, skip it.
                    // If it's still present, yield it.
                    // Note that the committed state and the insert tables are disjoint sets,
                    // so at this point we know the row will not be yielded in (3).
                    //
                    // NOTE for future MVCC implementors:
                    // In MVCC, it is no longer valid to elide inserts in this way.
                    // When a transaction inserts a row, that row *must* appear in its insert tables,
                    // even if the row is already present in the committed state.
                    //
                    // Imagine a chain of committed but un-squashed transactions:
                    // `Committed 0: Insert Row A` - `Committed 1: Delete Row A`
                    // where `Committed 1` happens after `Committed 0`.
                    // Imagine a transaction `Running 2: Insert Row A`,
                    // which began before `Committed 1` was committed.
                    // Because `Committed 1` has since been committed,
                    // `Running 2` *must* happen after `Committed 1`.
                    // Therefore, the correct sequence of events is:
                    // - Insert Row A
                    // - Delete Row A
                    // - Insert Row A
                    // This is impossible to recover if `Running 2` elides its insert.
                    //
                    // As a result, in MVCC, this branch will need to check if the `row_ref`
                    // also exists in the `tx_state.insert_tables` and ensure it is yielded only once.
                    let del_tables = unsafe { self.tx_state_del.unwrap_unchecked() };
                    if let next @ Some(_) = iter.find(|row_ref| !del_tables.contains(&row_ref.pointer())) {
                        return next;
                    }
                }
                ScanStage::CurrentTx { iter } => {
                    // (3) look for inserts in the current tx.
                    return iter.next();
                }
            }

            // (2) We got here, so we must've exhausted the committed changes.
            // Start looking in the current tx for inserts, if any, in (3).
            let (insert_table, blob_store) = self.tx_state_ins?;
            let iter = insert_table.scan_rows(blob_store);
            self.stage = ScanStage::CurrentTx { iter };
        }
    }
}

pub struct IndexSeekIterMutTxId<'a> {
    pub(super) table_id: TableId,
    pub(super) tx_state: &'a TxState,
    pub(super) inserted_rows: IndexScanIter<'a>,
    pub(super) committed_rows: Option<IndexScanIter<'a>>,
    pub(super) num_committed_rows_fetched: u64,
}

impl<'a> Iterator for IndexSeekIterMutTxId<'a> {
    type Item = RowRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(row_ref) = self.inserted_rows.next() {
            return Some(row_ref);
        }

        if let Some(row_ref) = self
            .committed_rows
            .as_mut()
            // NOTE for future MVCC implementors:
            // In MVCC, it is no longer valid to elide inserts in this way.
            // When a transaction inserts a row, that row *must* appear in its insert tables,
            // even if the row is already present in the committed state.
            //
            // Imagine a chain of committed but un-squashed transactions:
            // `Committed 0: Insert Row A` - `Committed 1: Delete Row A`
            // where `Committed 1` happens after `Committed 0`.
            // Imagine a transaction `Running 2: Insert Row A`,
            // which began before `Committed 1` was committed.
            // Because `Committed 1` has since been committed,
            // `Running 2` *must* happen after `Committed 1`.
            // Therefore, the correct sequence of events is:
            // - Insert Row A
            // - Delete Row A
            // - Insert Row A
            // This is impossible to recover if `Running 2` elides its insert.
            //
            // As a result, in MVCC, this branch will need to check if the `row_ref`
            // also exists in the `tx_state.insert_tables` and ensure it is yielded only once.
            .and_then(|i| i.find(|row_ref| !self.tx_state.is_deleted(self.table_id, row_ref.pointer())))
        {
            // TODO(metrics): This doesn't actually fetch a row.
            // Move this counter to `RowRef::read_row`.
            self.num_committed_rows_fetched += 1;
            return Some(row_ref);
        }

        None
    }
}

/// An [IterByColRange] for an individual column value.
pub type IterByColEq<'a, 'r> = IterByColRange<'a, &'r AlgebraicValue>;

/// An iterator for a range of values in a column.
pub enum IterByColRange<'a, R: RangeBounds<AlgebraicValue>> {
    /// When the column in question does not have an index.
    Scan(ScanIterByColRange<'a, R>),

    /// When the column has an index, and the table
    /// has been modified this transaction.
    Index(IndexSeekIterMutTxId<'a>),

    /// When the column has an index, and the table
    /// has not been modified in this transaction.
    CommittedIndex(CommittedIndexIter<'a>),
}

impl<'a, R: RangeBounds<AlgebraicValue>> Iterator for IterByColRange<'a, R> {
    type Item = RowRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            IterByColRange::Scan(range) => range.next(),
            IterByColRange::Index(range) => range.next(),
            IterByColRange::CommittedIndex(seek) => seek.next(),
        }
    }
}

pub struct ScanIterByColRange<'a, R: RangeBounds<AlgebraicValue>> {
    scan_iter: Iter<'a>,
    cols: ColList,
    range: R,
}

impl<'a, R: RangeBounds<AlgebraicValue>> ScanIterByColRange<'a, R> {
    pub(super) fn new(scan_iter: Iter<'a>, cols: ColList, range: R) -> Self {
        Self { scan_iter, cols, range }
    }
}

impl<'a, R: RangeBounds<AlgebraicValue>> Iterator for ScanIterByColRange<'a, R> {
    type Item = RowRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        for row_ref in &mut self.scan_iter {
            let value = row_ref.project_not_empty(&self.cols).unwrap();
            if self.range.contains(&value) {
                return Some(row_ref);
            }
        }

        None
    }
}
