use super::datastore::record_metrics;
use super::mut_tx::elapsed_or_zero;
use super::{
    committed_state::{CommittedIndexIter, CommittedState},
    datastore::Result,
    state_view::{Iter, IterByColRange, StateView},
    SharedReadGuard,
};
use crate::energy::DatastoreComputeDuration;
use crate::execution_context::ExecutionContext;
use spacetimedb_primitives::{ColList, TableId};
use spacetimedb_sats::AlgebraicValue;
use spacetimedb_schema::schema::TableSchema;
use std::sync::atomic::{AtomicU64, Ordering};
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

    /// Time spent performing operations that access the database,
    /// for which energy will be charged.
    ///
    /// Will be used as a shared counter with [`Ordering::Relaxed`] atomics.
    /// Does not provide any synchronization.
    /// This means that accesses should mostly compile into relatively-efficient unfenced operations.
    /// These accesses may, however, be contended, as we execute some read-only transactions in parallel.
    ///
    /// Incremental evaluation
    pub(super) datastore_compute_time_microseconds: AtomicU64,
}

impl StateView for TxId {
    fn get_schema(&self, table_id: TableId) -> Option<&Arc<TableSchema>> {
        self.while_tracking_compute_time(|this| this.committed_state_shared_lock.get_schema(table_id))
    }

    fn table_row_count(&self, table_id: TableId) -> Option<u64> {
        self.while_tracking_compute_time(|this| this.committed_state_shared_lock.table_row_count(table_id))
    }

    fn iter(&self, table_id: TableId) -> Result<Iter<'_>> {
        // TODO(energy): track compute time spent in the iterator.
        // This is challenging to do while maintaining non-reentrancy,
        // and there are concerns about overhead - wrapping the body of `next`
        // in (the equivalent of) `while_tracking_compute_time` means two clock reads for every row,
        // which is potentially too much.

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
    ) -> Result<IterByColRange<'_, R>> {
        // TODO(energy): track compute time spent in the iterator.
        // This is challenging to do while maintaining non-reentrancy,
        // and there are concerns about overhead - wrapping the body of `next`
        // in (the equivalent of) `while_tracking_compute_time` means two clock reads for every row,
        // which is potentially too much.

        match self.committed_state_shared_lock.index_seek(table_id, &cols, &range) {
            Some(committed_rows) => Ok(IterByColRange::CommittedIndex(CommittedIndexIter::new(
                table_id,
                None,
                &self.committed_state_shared_lock,
                committed_rows,
            ))),
            None => self
                .committed_state_shared_lock
                .iter_by_col_range(table_id, cols, range),
        }
    }
}

impl TxId {
    /// Run `f` on `self` while tracking database compute time for `self`.
    ///
    /// Odd signature with explicit lifetimes here to allow `Res` to borrow from `self`.
    ///
    /// This method must never be called re-entrantly from the `f` of either this method
    /// or [`Self::while_tracking_compute_time_mut`].
    /// Doing so will double-count compute time in release builds.
    /// However, it is acceptable for multiple actors to call `while_tracking_compute_time` in parallel.
    /// In this case, the total tracked compute time will be the sum of all the callers' durations.
    ///
    /// Most, if not all, `pub` methods of `TxId` should have their bodies enclosed
    /// in either this or [`Self::while_tracking_compute_time`].
    fn while_tracking_compute_time<'a, Res>(&'a self, f: impl FnOnce(&'a Self) -> Res) -> Res {
        let before = Instant::now();
        let res = f(self);
        let elapsed = elapsed_or_zero(before);

        self.datastore_compute_time_microseconds
            // `self.datastore_compute_time_microseconds` is not used for any synchronization;
            // it is strictly a shared counter. As such, `Ordering::Relaxed` is sufficient.
            .fetch_add(elapsed.as_micros() as u64, Ordering::Relaxed);

        res
    }

    /// Returns a [`DatastoreComputeDuration`]
    /// representing the total time spent performing datastore operations during the transaction,
    /// for which energy should be charged.
    ///
    /// If this transaction was created by downgrading a [`super::mut_tx::MutTxId`],
    /// the returned time includes compute time spent in both the original mutable transaction
    /// and the subsequent immutable transaction.
    pub(super) fn release(self) -> DatastoreComputeDuration {
        record_metrics(&self.ctx, self.timer, self.lock_wait_time, true, None, None);

        DatastoreComputeDuration::from_micros(self.datastore_compute_time_microseconds.into_inner())
    }

    /// The Number of Distinct Values (NDV) for a column or list of columns,
    /// if there's an index available on `cols`.
    pub(crate) fn num_distinct_values(&self, table_id: TableId, cols: &ColList) -> Option<u64> {
        self.while_tracking_compute_time(|this| {
            this.committed_state_shared_lock
                .get_table(table_id)
                .and_then(|t| t.indexes.get(cols).map(|index| index.num_keys() as u64))
        })
    }
}
