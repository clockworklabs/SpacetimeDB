use super::mut_tx::{IndexScanPoint, IndexScanRanged, MutTxId};
use super::state_view::IterMutTx;
use core::cell::RefCell;
use core::ops::RangeBounds;
use spacetimedb_data_structures::map::{HashSet, IntMap, IntSet};
use spacetimedb_execution::{Datastore, DeltaStore, Row};
use spacetimedb_lib::query::Delta;
use spacetimedb_primitives::{IndexId, TableId};
use spacetimedb_sats::{AlgebraicValue, ProductValue};

/// The tables and index keys a view's query read.
#[derive(Default)]
pub(super) struct ViewReads {
    pub tables: IntSet<TableId>,
    pub points: IntMap<TableId, HashSet<(IndexId, AlgebraicValue)>>,
}

/// A [`Datastore`] that records the tables and index keys a view's query reads.
///
/// Reads are recorded when a scan starts, so a lookup that finds no rows is still recorded.
/// A table with more than `max_points_per_table` distinct keys is recorded as a full table scan.
pub struct ViewReadRecorder<'a> {
    tx: &'a MutTxId,
    max_points_per_table: usize,
    reads: RefCell<ViewReads>,
}

impl<'a> ViewReadRecorder<'a> {
    pub(super) fn new(tx: &'a MutTxId, max_points_per_table: usize) -> Self {
        Self {
            tx,
            max_points_per_table,
            reads: <_>::default(),
        }
    }

    pub(super) fn into_reads(self) -> ViewReads {
        self.reads.into_inner()
    }

    fn record_table(&self, table_id: TableId) {
        let mut reads = self.reads.borrow_mut();
        reads.points.remove(&table_id);
        reads.tables.insert(table_id);
    }

    fn record_point(&self, table_id: TableId, index_id: IndexId, point: &AlgebraicValue) {
        let mut reads = self.reads.borrow_mut();
        if reads.tables.contains(&table_id) {
            return;
        }
        let points = reads.points.entry(table_id).or_default();
        points.insert((index_id, point.clone()));
        if points.len() > self.max_points_per_table {
            reads.points.remove(&table_id);
            reads.tables.insert(table_id);
        }
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
        self.record_table(table_id);
        self.tx.row_count(table_id)
    }

    fn table_scan<'b>(&'b self, table_id: TableId) -> anyhow::Result<Self::TableIter<'b>> {
        self.record_table(table_id);
        self.tx.table_scan(table_id)
    }

    fn index_scan_range<'b>(
        &'b self,
        table_id: TableId,
        index_id: IndexId,
        range: &impl RangeBounds<AlgebraicValue>,
    ) -> anyhow::Result<Self::RangeIndexIter<'b>> {
        // The planner doesn't emit range scans yet, so record a full table scan.
        self.record_table(table_id);
        Datastore::index_scan_range(self.tx, table_id, index_id, range)
    }

    fn index_scan_point<'b>(
        &'b self,
        table_id: TableId,
        index_id: IndexId,
        point: &AlgebraicValue,
    ) -> anyhow::Result<Self::PointIndexIter<'b>> {
        self.record_point(table_id, index_id, point);
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
