use super::{
    committed_state::CommittedIndexIter, committed_state::CommittedState, datastore::Result, tx_state::TxState,
};
use crate::{
    db::datastore::system_tables::{
        StColumnFields, StColumnRow, StConstraintFields, StConstraintRow, StIndexFields, StIndexRow, StSequenceFields,
        StSequenceRow, StTableFields, StTableRow, SystemTable, ST_COLUMNS_ID, ST_CONSTRAINTS_ID, ST_INDEXES_ID,
        ST_SEQUENCES_ID, ST_TABLES_ID,
    },
    error::TableError,
    execution_context::ExecutionContext,
};
use core::ops::RangeBounds;
use spacetimedb_lib::address::Address;
use spacetimedb_primitives::{ColList, TableId};
use spacetimedb_sats::{
    db::def::{ColumnSchema, ConstraintSchema, IndexSchema, SequenceSchema, TableSchema},
    AlgebraicValue,
};
use spacetimedb_table::table::{IndexScanIter, RowRef, TableScanIter};
use std::sync::Arc;

// StateView trait, is designed to define the behavior of viewing internal datastore states.
// Currently, it applies to: CommittedState, MutTxId, and TxId.
pub(crate) trait StateView {
    fn get_schema(&self, table_id: TableId) -> Option<&Arc<TableSchema>>;

    fn table_id_from_name(&self, table_name: &str, database_address: Address) -> Result<Option<TableId>> {
        let ctx = ExecutionContext::internal(database_address);
        let name = &<Box<str>>::from(table_name).into();
        let row = self
            .iter_by_col_eq(&ctx, ST_TABLES_ID, StTableFields::TableName, name)?
            .next();
        Ok(row.map(|row| row.read_col(StTableFields::TableId).unwrap()))
    }

    fn iter<'a>(&'a self, ctx: &'a ExecutionContext, table_id: TableId) -> Result<Iter<'a>>;

    fn table_name(&self, table_id: TableId) -> Option<&str> {
        self.get_schema(table_id).map(|s| &*s.table_name)
    }

    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`,
    /// where the values of `cols` are contained in `range`.
    fn iter_by_col_range<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        ctx: &'a ExecutionContext,
        table_id: TableId,
        cols: ColList,
        range: R,
    ) -> Result<IterByColRange<'a, R>>;

    fn iter_by_col_eq<'a, 'r>(
        &'a self,
        ctx: &'a ExecutionContext,
        table_id: TableId,
        cols: impl Into<ColList>,
        value: &'r AlgebraicValue,
    ) -> Result<IterByColEq<'a, 'r>> {
        self.iter_by_col_range(ctx, table_id, cols.into(), value)
    }

    /// Reads the schema information for the specified `table_id` directly from the database.
    fn schema_for_table_raw(&self, ctx: &ExecutionContext, table_id: TableId) -> Result<TableSchema> {
        // Look up the table_name for the table in question.
        let value_eq = &table_id.into();
        let row = self
            .iter_by_col_eq(ctx, ST_TABLES_ID, StTableFields::TableId, value_eq)?
            .next()
            .ok_or_else(|| TableError::IdNotFound(SystemTable::st_table, table_id.into()))?;
        let row = StTableRow::try_from(row)?;
        let table_name = row.table_name;
        let table_id: TableId = row.table_id;
        let table_type = row.table_type;
        let table_access = row.table_access;

        // Look up the columns for the table in question.
        let mut columns = self
            .iter_by_col_eq(ctx, ST_COLUMNS_ID, StColumnFields::TableId, value_eq)?
            .map(|row| {
                let row = StColumnRow::try_from(row)?;
                Ok(ColumnSchema {
                    table_id: row.table_id,
                    col_pos: row.col_pos,
                    col_name: row.col_name,
                    col_type: row.col_type,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        columns.sort_by_key(|col| col.col_pos);

        // Look up the constraints for the table in question.
        let constraints = self
            .iter_by_col_eq(ctx, ST_CONSTRAINTS_ID, StConstraintFields::TableId, value_eq)?
            .map(|row| {
                let row = StConstraintRow::try_from(row)?;
                Ok(ConstraintSchema {
                    constraint_id: row.constraint_id,
                    constraint_name: row.constraint_name,
                    constraints: row.constraints,
                    table_id: row.table_id,
                    columns: row.columns,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        // Look up the sequences for the table in question.
        let sequences = self
            .iter_by_col_eq(ctx, ST_SEQUENCES_ID, StSequenceFields::TableId, value_eq)?
            .map(|row| {
                let row = StSequenceRow::try_from(row)?;
                Ok(SequenceSchema {
                    sequence_id: row.sequence_id,
                    sequence_name: row.sequence_name,
                    table_id: row.table_id,
                    col_pos: row.col_pos,
                    increment: row.increment,
                    start: row.start,
                    min_value: row.min_value,
                    max_value: row.max_value,
                    allocated: row.allocated,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        // Look up the indexes for the table in question.
        let indexes = self
            .iter_by_col_eq(ctx, ST_INDEXES_ID, StIndexFields::TableId, value_eq)?
            .map(|row| {
                let row = StIndexRow::try_from(row)?;
                Ok(IndexSchema {
                    table_id: row.table_id,
                    columns: row.columns,
                    index_name: row.index_name,
                    is_unique: row.is_unique,
                    index_id: row.index_id,
                    index_type: row.index_type,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(TableSchema::new(
            table_id,
            table_name,
            columns,
            indexes,
            constraints,
            sequences,
            table_type,
            table_access,
        ))
    }

    /// Reads the schema information for the specified `table_id`, consulting the `cache` first.
    ///
    /// If the schema is not found in the cache, the method calls [Self::schema_for_table_raw].
    ///
    /// Note: The responsibility of populating the cache is left to the caller.
    fn schema_for_table(&self, ctx: &ExecutionContext, table_id: TableId) -> Result<Arc<TableSchema>> {
        if let Some(schema) = self.get_schema(table_id) {
            return Ok(schema.clone());
        }

        self.schema_for_table_raw(ctx, table_id).map(Arc::new)
    }
}

pub struct Iter<'a> {
    #[allow(dead_code)]
    ctx: &'a ExecutionContext,
    table_id: TableId,
    tx_state: Option<&'a TxState>,
    committed_state: &'a CommittedState,
    #[allow(dead_code)]
    table_name: &'a str,
    stage: ScanStage<'a>,
    num_committed_rows_fetched: u64,
}

// impl Drop for Iter<'_> {
//     fn drop(&mut self) {
//         let mut metrics = self.ctx.metrics.write();
//         // Increment number of rows fetched
//         metrics.inc_by(
//             self.table_id,
//             MetricType::RowsFetched,
//             self.num_committed_rows_fetched,
//             || self.table_name.to_string(),
//         );
//     }
// }

impl<'a> Iter<'a> {
    pub(super) fn new(
        ctx: &'a ExecutionContext,
        table_id: TableId,
        table_name: &'a str,
        tx_state: Option<&'a TxState>,
        committed_state: &'a CommittedState,
    ) -> Self {
        Self {
            ctx,
            table_id,
            tx_state,
            committed_state,
            table_name,
            stage: ScanStage::Start,
            num_committed_rows_fetched: 0,
        }
    }
}

enum ScanStage<'a> {
    Start,
    CurrentTx { iter: TableScanIter<'a> },
    Committed { iter: TableScanIter<'a> },
}

impl<'a> Iterator for Iter<'a> {
    type Item = RowRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let table_id = self.table_id;

        // Moves the current scan stage to the current tx if rows were inserted in it.
        // Returns `None` otherwise.
        // NOTE(pgoldman 2024-01-05): above comment appears to not describe the behavior of this function.
        let maybe_stage_current_tx_inserts = |this: &mut Self| {
            let table = &this.tx_state?;
            let insert_table = table.insert_tables.get(&table_id)?;
            this.stage = ScanStage::CurrentTx {
                iter: insert_table.scan_rows(&table.blob_store),
            };
            Some(())
        };

        // The finite state machine goes:
        //      Start --> CurrentTx ---\
        //        |         ^          |
        //        v         |          v
        //     Committed ---/------> Stop

        loop {
            match &mut self.stage {
                ScanStage::Start => {
                    if let Some(table) = self.committed_state.tables.get(&table_id) {
                        // The committed state has changes for this table.
                        // Go through them in (1).
                        self.stage = ScanStage::Committed {
                            iter: table.scan_rows(&self.committed_state.blob_store),
                        };
                    } else {
                        // No committed changes, so look for inserts in the current tx in (2).
                        maybe_stage_current_tx_inserts(self);
                    }
                }
                ScanStage::Committed { iter } => {
                    // (1) Go through the committed state for this table.
                    for row_ref in iter {
                        // Increment metric for number of committed rows scanned.
                        self.num_committed_rows_fetched += 1;
                        // Check the committed row's state in the current tx.
                        // If it's been deleted, skip it.
                        // If it's still present, yield it.
                        // Note that the committed state and the insert tables are disjoint sets,
                        // so at this point we know the row will not be yielded in (2).
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
                        if self
                            .tx_state
                            .filter(|tx_state| tx_state.is_deleted(table_id, row_ref.pointer()))
                            .is_none()
                        {
                            // There either are no state changes for the current tx (`None`),
                            // or there are, but `row_id` specifically has not been changed.
                            // Either way, the row is in the committed state
                            // and hasn't been removed in the current tx,
                            // so it exists and can be returned.
                            return Some(row_ref);
                        }
                    }
                    // (3) We got here, so we must've exhausted the committed changes.
                    // Start looking in the current tx for inserts, if any, in (2).
                    maybe_stage_current_tx_inserts(self)?;
                }
                ScanStage::CurrentTx { iter } => {
                    // (2) look for inserts in the current tx.
                    return iter.next();
                }
            }
        }
    }
}

pub struct IndexSeekIterMutTxId<'a> {
    #[allow(dead_code)]
    pub(super) ctx: &'a ExecutionContext,
    pub(super) table_id: TableId,
    pub(super) tx_state: &'a TxState,
    #[allow(dead_code)]
    pub(super) committed_state: &'a CommittedState,
    pub(super) inserted_rows: IndexScanIter<'a>,
    pub(super) committed_rows: Option<IndexScanIter<'a>>,
    pub(super) num_committed_rows_fetched: u64,
}

// impl Drop for IndexSeekIterMutTxId<'_> {
//     fn drop(&mut self) {
//         let mut metrics = self.ctx.metrics.write();
//         let get_table_name = || {
//             self.committed_state
//                 .get_schema(&self.table_id)
//                 .map(|table| &*table.table_name)
//                 .unwrap_or_default()
//                 .to_string()
//         };

//         let num_pointers_yielded = self
//             .committed_rows
//             .as_ref()
//             .map_or(0, |iter| iter.num_pointers_yielded());

//         // Increment number of index seeks
//         metrics.inc_by(self.table_id, MetricType::IndexSeeks, 1, get_table_name);
//         // Increment number of index keys scanned
//         metrics.inc_by(
//             self.table_id,
//             MetricType::KeysScanned,
//             num_pointers_yielded,
//             get_table_name,
//         );
//         // Increment number of rows fetched
//         metrics.inc_by(
//             self.table_id,
//             MetricType::RowsFetched,
//             self.num_committed_rows_fetched,
//             get_table_name,
//         );
//     }
// }

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
