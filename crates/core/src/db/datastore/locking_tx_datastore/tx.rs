use super::{
    committed_state::{CommittedIndexIter, CommittedState},
    datastore::Result,
    state_view::{Iter, IterByColRange, StateView},
    SharedReadGuard,
};
use crate::{db::db_metrics::DB_METRICS, execution_context::ExecutionContext};
use spacetimedb_primitives::{ColList, TableId};
use spacetimedb_sats::{db::def::TableSchema, AlgebraicValue};
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
    fn get_schema(&self, table_id: &TableId) -> Option<&TableSchema> {
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
        #[cfg(feature = "metrics")]
        {
            let workload = &ctx.workload();
            let db = &ctx.database();
            let reducer = ctx.reducer_name();
            let elapsed_time = self.timer.elapsed();
            let cpu_time = elapsed_time - self.lock_wait_time;
            DB_METRICS
                .rdb_num_txns
                .with_label_values(workload, db, reducer, &false)
                .inc();
            DB_METRICS
                .rdb_txn_cpu_time_sec
                .with_label_values(workload, db, reducer)
                .observe(cpu_time.as_secs_f64());
            DB_METRICS
                .rdb_txn_elapsed_time_sec
                .with_label_values(workload, db, reducer)
                .observe(elapsed_time.as_secs_f64());
        }
    }
}
