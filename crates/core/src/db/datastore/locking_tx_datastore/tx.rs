use super::datastore::record_metrics;
use super::{
    committed_state::{CommittedIndexIter, CommittedState},
    datastore::Result,
    state_view::{Iter, IterByColRange, StateView},
    SharedReadGuard,
};
use crate::execution_context::ExecutionContext;
use spacetimedb_primitives::{ColList, TableId};
use spacetimedb_sats::{db::def::TableSchema, AlgebraicValue};
use std::sync::Arc;
use std::{
    ops::RangeBounds,
    time::{Duration, Instant},
};

pub struct TxId {
    pub(crate) committed_state_shared_lock: SharedReadGuard<CommittedState>,
    pub(crate) lock_wait_time: Duration,
    pub(crate) timer: Instant,
}

impl StateView for TxId {
    fn get_schema(&self, table_id: &TableId) -> Option<&Arc<TableSchema>> {
        self.committed_state_shared_lock.get_schema(table_id)
    }

    fn iter<'a>(&'a self, ctx: &'a ExecutionContext, table_id: &TableId) -> Result<Iter<'a>> {
        self.committed_state_shared_lock.iter(ctx, table_id)
    }

    fn table_exists(&self, table_id: &TableId) -> Option<&str> {
        self.committed_state_shared_lock.table_exists(table_id)
    }

    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`,
    /// where the values of `cols` are contained in `range`.
    fn iter_by_col_range<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        ctx: &'a ExecutionContext,
        table_id: &TableId,
        cols: ColList,
        range: R,
    ) -> Result<IterByColRange<'a, R>> {
        match self.committed_state_shared_lock.index_seek(*table_id, &cols, &range) {
            Some(committed_rows) => Ok(IterByColRange::CommittedIndex(CommittedIndexIter::new(
                ctx,
                *table_id,
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
    pub(crate) fn release(self, ctx: &ExecutionContext) {
        record_metrics(ctx, self.timer, self.lock_wait_time, true);
    }

    pub fn get_row_count(&self, table_id: TableId) -> Option<u64> {
        self.committed_state_shared_lock
            .get_table(table_id)
            .map(|table| table.row_count)
    }
}
