use super::datastore::record_metrics;
use super::{
    committed_state::{CommittedIndexIter, CommittedState},
    datastore::Result,
    state_view::{Iter, IterByColRange, StateView},
    SharedReadGuard,
};
use crate::execution_context::ExecutionContext;
use spacetimedb_primitives::{ColList, TableId};
use spacetimedb_sats::AlgebraicValue;
use spacetimedb_schema::schema::TableSchema;
use std::sync::Arc;
use std::{
    ops::RangeBounds,
    time::{Duration, Instant},
};

pub struct TxId {
    pub(super) committed_state_shared_lock: SharedReadGuard<CommittedState>,
    pub(super) lock_wait_time: Duration,
    pub(super) timer: Instant,
}

impl StateView for TxId {
    fn get_schema(&self, table_id: TableId) -> Option<&Arc<TableSchema>> {
        self.committed_state_shared_lock.get_schema(table_id)
    }

    fn table_row_count(&self, table_id: TableId) -> Option<u64> {
        self.committed_state_shared_lock.table_row_count(table_id)
    }

    fn iter<'a>(&'a self, ctx: &'a ExecutionContext, table_id: TableId) -> Result<Iter<'a>> {
        self.committed_state_shared_lock.iter(ctx, table_id)
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
    ) -> Result<IterByColRange<'a, R>> {
        match self.committed_state_shared_lock.index_seek(table_id, &cols, &range) {
            Some(committed_rows) => Ok(IterByColRange::CommittedIndex(CommittedIndexIter::new(
                ctx,
                table_id,
                None,
                &self.committed_state_shared_lock,
                committed_rows,
            ))),
            None => self
                .committed_state_shared_lock
                .iter_by_col_range(ctx, table_id, cols, range),
        }
    }
}

impl TxId {
    pub(super) fn release(self, ctx: &ExecutionContext) {
        record_metrics(ctx, self.timer, self.lock_wait_time, true, None, None);
    }

    /// The Number of Distinct Values (NDV) for a column or list of columns,
    /// if there's an index available on `cols`.
    pub(crate) fn num_distinct_values(&self, table_id: TableId, cols: &ColList) -> Option<u64> {
        self.committed_state_shared_lock
            .get_table(table_id)
            .and_then(|t| t.indexes.get(cols).map(|index| index.num_keys() as u64))
    }
}
