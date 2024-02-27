use super::{
    committed_state::CommittedIndexIter, committed_state::CommittedState, datastore::Result, tx_state::TxState,
};
use crate::{
    address::Address,
    db::{
        datastore::system_tables::{
            StColumnFields, StColumnRow, StConstraintFields, StConstraintRow, StIndexFields, StIndexRow,
            StSequenceFields, StSequenceRow, StTableFields, StTableRow, SystemTable, ST_COLUMNS_ID, ST_CONSTRAINTS_ID,
            ST_INDEXES_ID, ST_SEQUENCES_ID, ST_TABLES_ID,
        },
        db_metrics::DB_METRICS,
    },
    error::TableError,
    execution_context::ExecutionContext,
};
use spacetimedb_primitives::{ColList, TableId};
use spacetimedb_sats::{
    db::def::{ColumnSchema, ConstraintSchema, IndexSchema, SequenceSchema, TableSchema},
    AlgebraicValue,
};
use spacetimedb_table::table::{IndexScanIter, RowRef, TableScanIter};
use std::{borrow::Cow, ops::RangeBounds};

// StateView trait, is designed to define the behavior of viewing internal datastore states.
// Currently, it applies to: CommittedState, MutTxId, and TxId.
pub(crate) trait StateView {
    fn get_schema(&self, table_id: &TableId) -> Option<&TableSchema>;

    fn table_id_from_name(&self, table_name: &str, database_address: Address) -> Result<Option<TableId>> {
        let ctx = ExecutionContext::internal(database_address);
        let name = table_name.to_owned().into();
        let row = self
            .iter_by_col_eq(&ctx, &ST_TABLES_ID, StTableFields::TableName.col_id().into(), name)?
            .next();
        Ok(row.map(|row| row.read_col(StTableFields::TableId).unwrap()))
    }

    fn iter<'a>(&'a self, ctx: &'a ExecutionContext, table_id: &TableId) -> Result<Iter<'a>>;

    // TODO(noa): rename to table_name, and TableId doesn't need to be a reference
    fn table_exists(&self, table_id: &TableId) -> Option<&str>;

    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`,
    /// where the values of `cols` are contained in `range`.
    fn iter_by_col_range<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        ctx: &'a ExecutionContext,
        table_id: &TableId,
        cols: ColList,
        range: R,
    ) -> Result<IterByColRange<'a, R>>;

    fn iter_by_col_eq<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        table_id: &TableId,
        cols: ColList,
        value: AlgebraicValue,
    ) -> Result<IterByColEq<'_>> {
        self.iter_by_col_range(ctx, table_id, cols, value)
    }

    /// Reads the schema information for the specified `table_id` directly from the database.
    fn schema_for_table_raw(&self, ctx: &ExecutionContext, table_id: TableId) -> Result<TableSchema> {
        // Look up the table_name for the table in question.
        let st_table_table_id_col = StTableFields::TableId.col_id().into();
        let value: AlgebraicValue = table_id.into();
        let row = self
            .iter_by_col_eq(ctx, &ST_TABLES_ID, st_table_table_id_col, table_id.into())?
            .next()
            .ok_or_else(|| TableError::IdNotFound(SystemTable::st_table, table_id.into()))?;
        let row = StTableRow::try_from(row)?;
        let table_name: String = row.table_name;
        let table_id: TableId = row.table_id;
        let table_type = row.table_type;
        let table_access = row.table_access;

        // Look up the columns for the table in question.
        let st_columns_table_id_col = StColumnFields::TableId.col_id().into();
        let mut columns = self
            .iter_by_col_eq(ctx, &ST_COLUMNS_ID, st_columns_table_id_col, value)?
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
        let st_constraints_table_id = StConstraintFields::TableId.col_id().into();
        let constraints = self
            .iter_by_col_eq(ctx, &ST_CONSTRAINTS_ID, st_constraints_table_id, table_id.into())?
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
        let st_seq_table_id = StSequenceFields::TableId.col_id().into();
        let sequences = self
            .iter_by_col_eq(ctx, &ST_SEQUENCES_ID, st_seq_table_id, table_id.into())?
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
        let st_idx_table_id = StIndexFields::TableId.col_id().into();
        let indexes = self
            .iter_by_col_eq(ctx, &ST_INDEXES_ID, st_idx_table_id, table_id.into())?
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
    fn schema_for_table(&self, ctx: &ExecutionContext, table_id: TableId) -> Result<Cow<'_, TableSchema>> {
        if let Some(schema) = self.get_schema(&table_id) {
            return Ok(Cow::Borrowed(schema));
        }

        self.schema_for_table_raw(ctx, table_id).map(Cow::Owned)
    }
}

pub struct Iter<'a> {
    ctx: &'a ExecutionContext<'a>,
    table_id: TableId,
    tx_state: Option<&'a TxState>,
    committed_state: &'a CommittedState,
    table_name: &'a str,
    stage: ScanStage<'a>,
    num_committed_rows_fetched: u64,
}

#[cfg(feature = "metrics")]
impl Drop for Iter<'_> {
    fn drop(&mut self) {
        let n = self.num_committed_rows_fetched;
        DB_METRICS.rdb_num_rows_fetched.with_label_values_async(
            &self.ctx.workload(),
            &self.ctx.database(),
            self.ctx.reducer_name(),
            &self.table_id.into(),
            self.table_name,
            move |met| {
                met.inc_by(n);
            },
        );
    }
}

impl<'a> Iter<'a> {
    pub(crate) fn new(
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
                        if !self
                            .tx_state
                            .map(|tx_state| tx_state.is_deleted(table_id, row_ref.pointer()))
                            .unwrap_or(false)
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
    pub(crate) ctx: &'a ExecutionContext<'a>,
    pub(crate) table_id: TableId,
    pub(crate) tx_state: &'a TxState,
    pub(crate) committed_state: &'a CommittedState,
    pub(crate) inserted_rows: IndexScanIter<'a>,
    pub(crate) committed_rows: Option<IndexScanIter<'a>>,
    pub(crate) num_committed_rows_fetched: u64,
}

#[cfg(feature = "metrics")]
impl Drop for IndexSeekIterMutTxId<'_> {
    fn drop(&mut self) {
        let workload = &self.ctx.workload();
        let db = &self.ctx.database();
        let reducer_name = self.ctx.reducer_name();
        let table_id = &self.table_id.0;
        let table_name = self
            .committed_state
            .get_schema(&self.table_id)
            .map(|table| table.table_name.as_str())
            .unwrap_or_default();

        // Increment number of index seeks
        DB_METRICS.rdb_num_index_seeks.with_label_values_async(
            workload,
            db,
            reducer_name,
            table_id,
            table_name,
            move |met| met.inc(),
        );

        let num_keys = self
            .committed_rows
            .as_ref()
            .map_or(0, |iter| iter.num_pointers_yielded());
        // Increment number of index keys scanned
        DB_METRICS.rdb_num_keys_scanned.with_label_values_async(
            workload,
            db,
            reducer_name,
            table_id,
            table_name,
            move |met| met.inc_by(num_keys),
        );

        // Increment number of rows fetched
        let n = self.num_committed_rows_fetched;
        DB_METRICS.rdb_num_rows_fetched.with_label_values_async(
            workload,
            db,
            reducer_name,
            table_id,
            table_name,
            move |met| {
                met.inc_by(n);
            },
        );
    }
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
pub type IterByColEq<'a> = IterByColRange<'a, AlgebraicValue>;

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
    pub(crate) fn new(scan_iter: Iter<'a>, cols: ColList, range: R) -> Self {
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
