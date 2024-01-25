use std::ops::RangeBounds;

use spacetimedb_primitives::{ColList, TableId};
use spacetimedb_sats::{AlgebraicValue, ProductValue};

use crate::{db::db_metrics::DB_METRICS, execution_context::ExecutionContext};

use super::{
    btree_index::BTreeIndexRangeIter,
    committed_state::{get_committed_row, CommittedIndexIter, CommittedState},
    mut_tx::StateView as _,
    tx_state::{RowState, TxState},
    DataRef, RowId,
};

pub struct Iter<'a> {
    ctx: &'a ExecutionContext<'a>,
    table_id: TableId,
    tx_state: Option<&'a TxState>,
    committed_state: &'a CommittedState,
    table_name: &'a str,
    stage: ScanStage<'a>,
    num_committed_rows_fetched: u64,
}

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
