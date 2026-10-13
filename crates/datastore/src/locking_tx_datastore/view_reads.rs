use super::mut_tx::{IndexScanPoint, IndexScanRanged, MutTxId};
use super::state_view::IterMutTx;
use core::cell::RefCell;
use core::ops::RangeBounds;
use spacetimedb_data_structures::map::HashSet;
use spacetimedb_execution::{Datastore, DeltaStore, Row};
use spacetimedb_lib::query::Delta;
use spacetimedb_primitives::{IndexId, TableId};
use spacetimedb_sats::{AlgebraicValue, ProductValue};

/// A read made by a view's query, to be recorded in the view's read set.
#[derive(PartialEq, Eq, Hash)]
pub(super) enum ViewRead {
    Table(TableId),
    Point(TableId, IndexId, AlgebraicValue),
}

/// A [`Datastore`] over a [`MutTxId`] that remembers what the query executor reads,
/// so a view's read set can be derived from the reads its query actually made.
///
/// Reads are recorded when the scan starts, so lookups that find no rows are still recorded.
pub struct ViewReadRecorder<'a> {
    tx: &'a MutTxId,
    reads: RefCell<HashSet<ViewRead>>,
}

impl<'a> ViewReadRecorder<'a> {
    pub(super) fn new(tx: &'a MutTxId) -> Self {
        Self {
            tx,
            reads: <_>::default(),
        }
    }

    pub(super) fn into_reads(self) -> HashSet<ViewRead> {
        self.reads.into_inner()
    }

    fn record(&self, read: ViewRead) {
        self.reads.borrow_mut().insert(read);
    }
}

impl Datastore for ViewReadRecorder<'_> {
    type TableIter<'b>
        = IterMutTx<'b>
    where
        Self: 'b;

    type RangeIndexIter<'b>
        = IndexScanRanged<'b>
    where
        Self: 'b;

    type PointIndexIter<'b>
        = IndexScanPoint<'b>
    where
        Self: 'b;

    fn row_count(&self, table_id: TableId) -> u64 {
        self.record(ViewRead::Table(table_id));
        self.tx.row_count(table_id)
    }

    fn table_scan<'b>(&'b self, table_id: TableId) -> anyhow::Result<Self::TableIter<'b>> {
        self.record(ViewRead::Table(table_id));
        self.tx.table_scan(table_id)
    }

    fn index_scan_range<'b>(
        &'b self,
        table_id: TableId,
        index_id: IndexId,
        range: &impl RangeBounds<AlgebraicValue>,
    ) -> anyhow::Result<Self::RangeIndexIter<'b>> {
        // The planner never emits range scans yet (`IndexProbe::Range`),
        // so ranges are conservatively recorded as full table scans.
        self.record(ViewRead::Table(table_id));
        Datastore::index_scan_range(self.tx, table_id, index_id, range)
    }

    fn index_scan_point<'b>(
        &'b self,
        table_id: TableId,
        index_id: IndexId,
        point: &AlgebraicValue,
    ) -> anyhow::Result<Self::PointIndexIter<'b>> {
        self.record(ViewRead::Point(table_id, index_id, point.clone()));
        Datastore::index_scan_point(self.tx, table_id, index_id, point)
    }
}

/// Views are evaluated against a mutable transaction, which has no delta tables.
impl DeltaStore for ViewReadRecorder<'_> {
    fn num_inserts(&self, table_id: TableId) -> usize {
        self.tx.num_inserts(table_id)
    }

    fn num_deletes(&self, table_id: TableId) -> usize {
        self.tx.num_deletes(table_id)
    }

    fn inserts_for_table(&self, table_id: TableId) -> Option<std::slice::Iter<'_, ProductValue>> {
        self.tx.inserts_for_table(table_id)
    }

    fn deletes_for_table(&self, table_id: TableId) -> Option<std::slice::Iter<'_, ProductValue>> {
        self.tx.deletes_for_table(table_id)
    }

    fn index_scan_range_for_delta(
        &self,
        _: TableId,
        _: IndexId,
        _: Delta,
        _: impl RangeBounds<AlgebraicValue>,
    ) -> impl Iterator<Item = Row<'_>> {
        std::iter::empty()
    }

    fn index_scan_point_for_delta(
        &self,
        _: TableId,
        _: IndexId,
        _: Delta,
        _: &AlgebraicValue,
    ) -> impl Iterator<Item = Row<'_>> {
        std::iter::empty()
    }
}
