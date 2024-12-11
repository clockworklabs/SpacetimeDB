use super::datastore::record_metrics;
use super::{
    committed_state::CommittedState,
    datastore::Result,
    state_view::{IterTxByColRange, StateView},
    IterByColEq, SharedReadGuard,
};
use crate::db::datastore::locking_tx_datastore::committed_state::CommittedIndexIterTx;
use crate::db::datastore::locking_tx_datastore::state_view::IterTx;
use crate::execution_context::ExecutionContext;
use spacetimedb_primitives::{ColList, TableId};
use spacetimedb_sats::AlgebraicValue;
use spacetimedb_schema::schema::TableSchema;
use std::num::NonZeroU64;
use std::sync::Arc;
use std::{
    ops::RangeBounds,
    time::{Duration, Instant},
};

pub struct TxId {
    pub(super) committed_state_shared_lock: SharedReadGuard<CommittedState>,
    pub(super) lock_wait_time: Duration,
    pub(super) timer: Instant,
    pub(crate) ctx: ExecutionContext,
}

impl StateView for TxId {
    type Iter<'a> = IterTx<'a>;
    type IterByColRange<'a, R: RangeBounds<AlgebraicValue>> = IterTxByColRange<'a, R>;
    type IterByColEq<'a, 'r> = IterByColEq<'a, 'r>
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
        match self.committed_state_shared_lock.index_seek(table_id, &cols, &range) {
            Some(committed_rows) => Ok(IterTxByColRange::CommittedIndex(CommittedIndexIterTx::new(
                committed_rows,
            ))),
            None => self
                .committed_state_shared_lock
                .iter_by_col_range(table_id, cols, range),
        }
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
    pub(super) fn release(self) {
        record_metrics(&self.ctx, self.timer, self.lock_wait_time, true, None, None);
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
        let index = table.indexes.get(cols)?;
        NonZeroU64::new(index.num_keys() as u64)
    }
}
