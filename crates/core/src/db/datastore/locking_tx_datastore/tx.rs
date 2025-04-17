use super::{
    committed_state::CommittedState,
    datastore::{Result, TxMetrics},
    state_view::{IterByColRangeTx, StateView},
    IterByColEqTx, SharedReadGuard,
};
use crate::db::datastore::locking_tx_datastore::state_view::IterTx;
use crate::execution_context::ExecutionContext;
use spacetimedb_execution::Datastore;
use spacetimedb_lib::metrics::ExecutionMetrics;
use spacetimedb_primitives::{ColList, TableId};
use spacetimedb_sats::AlgebraicValue;
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_table::blob_store::BlobStore;
use spacetimedb_table::table::Table;
use std::num::NonZeroU64;
use std::sync::Arc;
use std::{
    ops::RangeBounds,
    time::{Duration, Instant},
};

/// A read-only transaction with a shared lock on the committed state.
pub struct TxId {
    pub(super) committed_state_shared_lock: SharedReadGuard<CommittedState>,
    pub(super) lock_wait_time: Duration,
    pub(super) timer: Instant,
    pub(crate) ctx: ExecutionContext,
    pub(crate) metrics: ExecutionMetrics,
}

impl Datastore for TxId {
    fn blob_store(&self) -> &dyn BlobStore {
        &self.committed_state_shared_lock.blob_store
    }

    fn table(&self, table_id: TableId) -> Option<&Table> {
        self.committed_state_shared_lock.get_table(table_id)
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
    /// - [`TxMetrics`], various measurements of the work performed by this transaction.
    /// - `String`, the name of the reducer which ran within this transaction.
    pub(super) fn release(self) -> (TxMetrics, String) {
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
        (tx_metrics, reducer)
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
    pub(crate) fn num_distinct_values(&self, table_id: TableId, cols: &ColList) -> Option<NonZeroU64> {
        let table = self.committed_state_shared_lock.get_table(table_id)?;
        let (_, index) = table.get_index_by_cols(cols)?;
        NonZeroU64::new(index.num_keys() as u64)
    }
}
