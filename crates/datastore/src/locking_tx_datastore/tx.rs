use super::{
    committed_state::CommittedState,
    datastore::{Result, TxMetrics},
    state_view::{IterByColRangeTx, StateView},
    IterByColEqTx, SharedReadGuard,
};
use crate::execution_context::ExecutionContext;
use crate::locking_tx_datastore::state_view::IterTx;
use spacetimedb_durability::TxOffset;
use spacetimedb_execution::Datastore;
use spacetimedb_lib::metrics::ExecutionMetrics;
use spacetimedb_primitives::{ColList, IndexId, TableId};
use spacetimedb_sats::AlgebraicValue;
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_table::table::{IndexScanRangeIter, TableScanIter};
use std::sync::Arc;
use std::{future, num::NonZeroU64};
use std::{
    ops::RangeBounds,
    time::{Duration, Instant},
};

/// A read-only transaction with a shared lock on the committed state.
pub struct TxId {
    pub(super) committed_state_shared_lock: SharedReadGuard<CommittedState>,
    pub(super) lock_wait_time: Duration,
    pub(super) timer: Instant,
    // TODO(cloutiertyler): The below were made `pub` for the datastore split. We should
    // make these private again.
    pub ctx: ExecutionContext,
    pub metrics: ExecutionMetrics,
}

impl Datastore for TxId {
    type TableIter<'a>
        = TableScanIter<'a>
    where
        Self: 'a;

    type IndexIter<'a>
        = IndexScanRangeIter<'a>
    where
        Self: 'a;

    fn row_count(&self, table_id: TableId) -> u64 {
        self.committed_state_shared_lock
            .table_row_count(table_id)
            .unwrap_or_default()
    }

    fn table_scan<'a>(&'a self, table_id: TableId) -> anyhow::Result<Self::TableIter<'a>> {
        self.committed_state_shared_lock
            .get_table(table_id)
            .map(|table| table.scan_rows(&self.committed_state_shared_lock.blob_store))
            .ok_or_else(|| anyhow::anyhow!("TableId `{table_id}` does not exist"))
    }

    fn index_scan<'a>(
        &'a self,
        table_id: TableId,
        index_id: IndexId,
        range: &impl RangeBounds<AlgebraicValue>,
    ) -> anyhow::Result<Self::IndexIter<'a>> {
        self.committed_state_shared_lock
            .get_table(table_id)
            .ok_or_else(|| anyhow::anyhow!("TableId `{table_id}` does not exist"))
            .and_then(|table| {
                table
                    .get_index_by_id_with_table(&self.committed_state_shared_lock.blob_store, index_id)
                    .map(|i| i.seek_range(range))
                    .ok_or_else(|| anyhow::anyhow!("IndexId `{index_id}` does not exist"))
            })
    }
}

impl StateView for TxId {
    type Iter<'a> = IterTx<'a>;
    type IterByColRange<'a, R: RangeBounds<AlgebraicValue>> = IterByColRangeTx<'a, R>;
    type IterByColEq<'a, 'r>
        = IterByColEqTx<'a, 'r>
    where
        Self: 'a;

    fn get_schema(&self, table_id: TableId) -> Option<&Arc<TableSchema>> {
        self.committed_state_shared_lock.get_schema(table_id)
    }

    fn table_row_count(&self, table_id: TableId) -> Option<u64> {
        self.committed_state_shared_lock.table_row_count(table_id)
    }

    fn iter(&self, table_id: TableId) -> Result<Self::Iter<'_>> {
        self.committed_state_shared_lock.iter(table_id)
    }

    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`,
    /// where the values of `cols` are contained in `range`.
    fn iter_by_col_range<R: RangeBounds<AlgebraicValue>>(
        &self,
        table_id: TableId,
        cols: ColList,
        range: R,
    ) -> Result<Self::IterByColRange<'_, R>> {
        self.committed_state_shared_lock
            .iter_by_col_range(table_id, cols, range)
    }

    fn iter_by_col_eq<'a, 'r>(
        &'a self,
        table_id: TableId,
        cols: impl Into<ColList>,
        value: &'r AlgebraicValue,
    ) -> Result<Self::IterByColEq<'a, 'r>> {
        self.iter_by_col_range(table_id, cols.into(), value)
    }
}

impl TxId {
    /// Release this read-only transaction,
    /// allowing new mutable transactions to start if this was the last read-only transaction.
    ///
    /// Returns:
    /// - [`TxOffset`], the smallest transaction offset visible to this transaction.
    /// - [`TxMetrics`], various measurements of the work performed by this transaction.
    /// - `String`, the name of the reducer which ran within this transaction.
    pub(super) fn release(self) -> (TxOffset, TxMetrics, String) {
        // A read tx doesn't consume `next_tx_offset`, so subtract one to obtain
        // the offset that was visible to the transaction.
        //
        // Note that technically the tx could have run against an empty database,
        // in which case we'd wrongly return zero (a non-existent transaction).
        // This doesn not happen in practice, however, as [RelationalDB::set_initialized]
        // creates a transaction.
        let tx_offset = self.committed_state_shared_lock.next_tx_offset.saturating_sub(1);
        let tx_metrics = TxMetrics::new(
            &self.ctx,
            self.timer,
            self.lock_wait_time,
            self.metrics,
            true,
            None,
            &self.committed_state_shared_lock,
        );
        let reducer = self.ctx.into_reducer_name();
        (tx_offset, tx_metrics, reducer)
    }

    /// The Number of Distinct Values (NDV) for a column or list of columns,
    /// if there's an index available on `cols`.
    ///
    /// Returns `None` if:
    /// - No such table as `table_id` exists.
    /// - The table `table_id` does not have an index on exactly the `cols`.
    /// - The table `table_id` contains zero rows (i.e. the index is empty).
    //
    // This method must never return 0, as it's used as the divisor in quotients.
    // Do not change its return type to a bare `u64`.
    pub fn num_distinct_values(&self, table_id: TableId, cols: &ColList) -> Option<NonZeroU64> {
        let table = self.committed_state_shared_lock.get_table(table_id)?;
        let (_, index) = table.get_index_by_cols(cols)?;
        NonZeroU64::new(index.num_keys() as u64)
    }

    pub fn tx_offset(&self) -> future::Ready<TxOffset> {
        future::ready(self.committed_state_shared_lock.next_tx_offset)
    }
}
