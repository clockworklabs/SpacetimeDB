use super::{
    btree_index::BTreeIndexRangeIter,
    committed_state::{get_committed_row, CommittedIndexIter, CommittedState},
    tx_state::{RowState, TxState},
    DataRef, RowId,
};
use crate::db::datastore::Result;
use crate::{
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
use spacetimedb_lib::Address;
use spacetimedb_primitives::{ColList, TableId};
use spacetimedb_sats::{
    db::def::{ColumnSchema, ConstraintSchema, IndexSchema, SequenceSchema, TableSchema},
    AlgebraicValue, ProductValue,
};
use std::{borrow::Cow, ops::RangeBounds};

// StateView trait, is designed to define the behavior of viewing internal datastore states.
// Currently, it applies to: CommittedState, MutTxId, and TxId.
pub trait StateView {
    fn get_schema(&self, table_id: &TableId) -> Option<&TableSchema>;

    fn table_id_from_name(&self, table_name: &str, database_address: Address) -> Result<Option<TableId>> {
        self.iter_by_col_eq(
            &ExecutionContext::internal(database_address),
            &ST_TABLES_ID,
            ColList::new(StTableFields::TableName.col_id()),
            AlgebraicValue::String(table_name.to_owned()),
        )
        .map(|mut iter| {
            iter.next()
                .map(|row| TableId(*row.view().elements[0].as_u32().unwrap()))
        })
    }

    fn iter<'a>(&'a self, ctx: &'a ExecutionContext, table_id: &TableId) -> Result<Iter<'a>>;

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

    fn schema_for_table(&self, ctx: &ExecutionContext, table_id: TableId) -> Result<Cow<'_, TableSchema>> {
        if let Some(schema) = self.get_schema(&table_id) {
            return Ok(Cow::Borrowed(schema));
        }

        // Look up the table_name for the table in question.
        let table_id_col = ColList::new(StTableFields::TableId.col_id());

        // let table_id_col = ColList::new(.col_id());
        let value: AlgebraicValue = table_id.into();
        let rows = self
            .iter_by_col_eq(ctx, &ST_TABLES_ID, table_id_col, table_id.into())?
            .collect::<Vec<_>>();
        let row = rows
            .first()
            .ok_or_else(|| TableError::IdNotFound(SystemTable::st_table, table_id.into()))?;
        let el = StTableRow::try_from(row.view())?;
        let table_name = el.table_name.to_owned();
        let table_id = el.table_id;

        // Look up the columns for the table in question.
        let mut columns = self
            .iter_by_col_eq(
                ctx,
                &ST_COLUMNS_ID,
                ColList::new(StColumnFields::TableId.col_id()),
                value,
            )?
            .map(|row| {
                let el = StColumnRow::try_from(row.view())?;
                Ok(ColumnSchema {
                    table_id: el.table_id,
                    col_pos: el.col_pos,
                    col_name: el.col_name.into(),
                    col_type: el.col_type,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        columns.sort_by_key(|col| col.col_pos);

        // Look up the constraints for the table in question.
        let mut constraints = Vec::new();
        for data_ref in self.iter_by_col_eq(
            ctx,
            &ST_CONSTRAINTS_ID,
            ColList::new(StConstraintFields::TableId.col_id()),
            table_id.into(),
        )? {
            let row = data_ref.view();

            let el = StConstraintRow::try_from(row)?;
            let constraint_schema = ConstraintSchema {
                constraint_id: el.constraint_id,
                constraint_name: el.constraint_name.to_string(),
                constraints: el.constraints,
                table_id: el.table_id,
                columns: el.columns,
            };
            constraints.push(constraint_schema);
        }

        // Look up the sequences for the table in question.
        let mut sequences = Vec::new();
        for data_ref in self.iter_by_col_eq(
            ctx,
            &ST_SEQUENCES_ID,
            ColList::new(StSequenceFields::TableId.col_id()),
            AlgebraicValue::U32(table_id.into()),
        )? {
            let row = data_ref.view();

            let el = StSequenceRow::try_from(row)?;
            let sequence_schema = SequenceSchema {
                sequence_id: el.sequence_id,
                sequence_name: el.sequence_name.to_string(),
                table_id: el.table_id,
                col_pos: el.col_pos,
                increment: el.increment,
                start: el.start,
                min_value: el.min_value,
                max_value: el.max_value,
                allocated: el.allocated,
            };
            sequences.push(sequence_schema);
        }

        // Look up the indexes for the table in question.
        let mut indexes = Vec::new();
        for data_ref in self.iter_by_col_eq(
            ctx,
            &ST_INDEXES_ID,
            ColList::new(StIndexFields::TableId.col_id()),
            table_id.into(),
        )? {
            let row = data_ref.view();

            let el = StIndexRow::try_from(row)?;
            let index_schema = IndexSchema {
                table_id: el.table_id,
                columns: el.columns,
                index_name: el.index_name.into(),
                is_unique: el.is_unique,
                index_id: el.index_id,
                index_type: el.index_type,
            };
            indexes.push(index_schema);
        }

        Ok(Cow::Owned(TableSchema::new(
            table_id,
            table_name,
            columns,
            indexes,
            constraints,
            sequences,
            el.table_type,
            el.table_access,
        )))
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
        DB_METRICS
            .rdb_num_rows_fetched
            .with_label_values(
                &self.ctx.workload(),
                &self.ctx.database(),
                self.ctx.reducer_name(),
                &self.table_id.into(),
                self.table_name,
            )
            .inc_by(self.num_committed_rows_fetched);
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
            table_name,
            tx_state,
            committed_state,
            stage: ScanStage::Start,
            num_committed_rows_fetched: 0,
        }
    }
}

enum ScanStage<'a> {
    Start,
    CurrentTx {
        iter: indexmap::map::Iter<'a, RowId, ProductValue>,
    },
    Committed {
        iter: indexmap::map::Iter<'a, RowId, ProductValue>,
    },
}

impl<'a> Iterator for Iter<'a> {
    type Item = DataRef<'a>;

    #[tracing::instrument(skip_all)]
    fn next(&mut self) -> Option<Self::Item> {
        let table_id = self.table_id;

        // Moves the current scan stage to the current tx if rows were inserted in it.
        // Returns `None` otherwise.
        let maybe_stage_current_tx_inserts = |this: &mut Self| {
            let table = &this.tx_state?;
            let insert_table = table.insert_tables.get(&table_id)?;
            this.stage = ScanStage::CurrentTx {
                iter: insert_table.rows.iter(),
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
                    let _span = tracing::debug_span!("ScanStage::Start").entered();
                    if let Some(table) = self.committed_state.tables.get(&table_id) {
                        // The committed state has changes for this table.
                        // Go through them in (1).
                        self.stage = ScanStage::Committed {
                            iter: table.rows.iter(),
                        };
                    } else {
                        // No committed changes, so look for inserts in the current tx in (2).
                        maybe_stage_current_tx_inserts(self);
                    }
                }
                ScanStage::Committed { iter } => {
                    // (1) Go through the committed state for this table.
                    let _span = tracing::debug_span!("ScanStage::Committed").entered();
                    for (row_id, row) in iter {
                        // Increment metric for number of committed rows scanned.
                        self.num_committed_rows_fetched += 1;
                        match self.tx_state {
                            Some(tx_state) => {
                                // Check the committed row's state in the current tx.
                                match tx_state.get_row_op(&table_id, row_id) {
                                    RowState::Committed(_) => unreachable!("a row cannot be committed in a tx state"),
                                    // Do nothing, via (3), we'll get it in the next stage (2).
                                    RowState::Insert(_) |
                                    // Skip it, it's been deleted.
                                    RowState::Delete => {}
                                    // There either are no state changes for the current tx (`None`),
                                    // or there are, but `row_id` specifically has not been changed.
                                    // Either way, the row is in the committed state
                                    // and hasn't been removed in the current tx,
                                    // so it exists and can be returned.
                                    RowState::Absent => return Some(DataRef::new(row_id, row)),
                                }
                            }
                            None => return Some(DataRef::new(row_id, row)),
                        }
                    }
                    // (3) We got here, so we must've exhausted the committed changes.
                    // Start looking in the current tx for inserts, if any, in (2).
                    maybe_stage_current_tx_inserts(self)?
                }
                ScanStage::CurrentTx { iter } => {
                    // (2) look for inserts in the current tx.
                    let _span = tracing::debug_span!("ScanStage::CurrentTx").entered();
                    return iter.next().map(|(id, row)| DataRef::new(id, row));
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
    pub(crate) inserted_rows: BTreeIndexRangeIter<'a>,
    pub(crate) committed_rows: Option<BTreeIndexRangeIter<'a>>,
    pub(crate) num_committed_rows_fetched: u64,
}

#[cfg(feature = "metrics")]
impl Drop for IndexSeekIterMutTxId<'_> {
    fn drop(&mut self) {
        let table_name = self
            .committed_state
            .get_schema(&self.table_id)
            .map(|table| table.table_name.as_str())
            .unwrap_or_default();

        // Increment number of index seeks
        DB_METRICS
            .rdb_num_index_seeks
            .with_label_values(
                &self.ctx.workload(),
                &self.ctx.database(),
                self.ctx.reducer_name(),
                &self.table_id.0,
                table_name,
            )
            .inc();

        // Increment number of index keys scanned
        DB_METRICS
            .rdb_num_keys_scanned
            .with_label_values(
                &self.ctx.workload(),
                &self.ctx.database(),
                self.ctx.reducer_name(),
                &self.table_id.0,
                table_name,
            )
            .inc_by(self.committed_rows.as_ref().map_or(0, |iter| iter.keys_scanned()));

        // Increment number of rows fetched
        DB_METRICS
            .rdb_num_rows_fetched
            .with_label_values(
                &self.ctx.workload(),
                &self.ctx.database(),
                self.ctx.reducer_name(),
                &self.table_id.0,
                table_name,
            )
            .inc_by(self.num_committed_rows_fetched);
    }
}

impl<'a> Iterator for IndexSeekIterMutTxId<'a> {
    type Item = DataRef<'a>;

    #[tracing::instrument(skip_all)]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(row_id) = self.inserted_rows.next() {
            return Some(DataRef::new(
                row_id,
                self.tx_state.get_row(&self.table_id, row_id).unwrap(),
            ));
        }

        if let Some(row_id) = self.committed_rows.as_mut().and_then(|i| {
            i.find(|row_id| {
                !self
                    .tx_state
                    .delete_tables
                    .get(&self.table_id)
                    .map_or(false, |table| table.contains(row_id))
            })
        }) {
            self.num_committed_rows_fetched += 1;
            return Some(get_committed_row(self.committed_state, &self.table_id, row_id));
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
    type Item = DataRef<'a>;

    #[tracing::instrument(skip_all)]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            IterByColRange::Scan(range) => range.next(),
            IterByColRange::Index(range) => range.next(),
            IterByColRange::CommittedIndex(seek) => seek.next(),
        }
    }
}

pub struct ScanIterByColRange<'a, R: RangeBounds<AlgebraicValue>> {
    pub(crate) scan_iter: Iter<'a>,
    pub(crate) cols: ColList,
    pub(crate) range: R,
}

impl<'a, R: RangeBounds<AlgebraicValue>> Iterator for ScanIterByColRange<'a, R> {
    type Item = DataRef<'a>;

    #[tracing::instrument(skip_all)]
    fn next(&mut self) -> Option<Self::Item> {
        for data_ref in &mut self.scan_iter {
            let row = data_ref.view();
            let value = row.project_not_empty(&self.cols).unwrap();
            if self.range.contains(&value) {
                return Some(data_ref);
            }
        }
        None
    }
}
