use super::mut_tx::FilterDeleted;
use super::{committed_state::CommittedState, datastore::Result, tx_state::TxState};
use crate::error::{DatastoreError, TableError};
use crate::locking_tx_datastore::mut_tx::{IndexScanPoint, IndexScanRanged};
use crate::system_tables::{
    ConnectionIdViaU128, StColumnFields, StColumnRow, StConnectionCredentialsFields, StConnectionCredentialsRow,
    StConstraintFields, StConstraintRow, StIndexFields, StIndexRow, StScheduledFields, StScheduledRow,
    StSequenceFields, StSequenceRow, StTableFields, StTableRow, StViewFields, StViewParamFields, StViewRow,
    SystemTable, ST_COLUMN_ID, ST_CONNECTION_CREDENTIALS_ID, ST_CONSTRAINT_ID, ST_INDEX_ID, ST_SCHEDULED_ID,
    ST_SEQUENCE_ID, ST_TABLE_ID, ST_VIEW_ID, ST_VIEW_PARAM_ID,
};
use anyhow::anyhow;
use core::ops::RangeBounds;
use spacetimedb_lib::ConnectionId;
use spacetimedb_primitives::{ColList, TableId};
use spacetimedb_sats::AlgebraicValue;
use spacetimedb_schema::schema::{ColumnSchema, TableSchema, ViewDefInfo};
use spacetimedb_table::table::IndexScanPointIter;
use spacetimedb_table::{
    blob_store::HashMapBlobStore,
    table::{IndexScanRangeIter, RowRef, Table, TableScanIter},
};
use std::sync::Arc;

// StateView trait, is designed to define the behavior of viewing internal datastore states.
// Currently, it applies to: CommittedState, MutTxId, and TxId.
pub trait StateView {
    type Iter<'a>: Iterator<Item = RowRef<'a>>
    where
        Self: 'a;
    type IterByColRange<'a, R: RangeBounds<AlgebraicValue>>: Iterator<Item = RowRef<'a>>
    where
        Self: 'a;
    type IterByColEq<'a, 'r>: Iterator<Item = RowRef<'a>>
    where
        Self: 'a;

    fn get_schema(&self, table_id: TableId) -> Option<&Arc<TableSchema>>;

    fn table_id_from_name(&self, table_name: &str) -> Result<Option<TableId>> {
        let name = &<Box<str>>::from(table_name).into();
        let row = self.iter_by_col_eq(ST_TABLE_ID, StTableFields::TableName, name)?.next();
        Ok(row.map(|row| row.read_col(StTableFields::TableId).unwrap()))
    }

    /// Returns the number of rows in the table identified by `table_id`.
    fn table_row_count(&self, table_id: TableId) -> Option<u64>;

    fn iter(&self, table_id: TableId) -> Result<Self::Iter<'_>>;

    fn table_name(&self, table_id: TableId) -> Option<&str> {
        self.get_schema(table_id).map(|s| &*s.table_name)
    }

    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`,
    /// where the values of `cols` are contained in `range`.
    fn iter_by_col_range<R: RangeBounds<AlgebraicValue>>(
        &self,
        table_id: TableId,
        cols: ColList,
        range: R,
    ) -> Result<Self::IterByColRange<'_, R>>;

    fn iter_by_col_eq<'a, 'r>(
        &'a self,
        table_id: TableId,
        cols: impl Into<ColList>,
        value: &'r AlgebraicValue,
    ) -> Result<Self::IterByColEq<'a, 'r>>;

    /// Reads the schema information for the specified `table_id` directly from the database.
    fn schema_for_table_raw(&self, table_id: TableId) -> Result<TableSchema> {
        // Look up the table_name for the table in question.
        let value_eq = &table_id.into();
        let row = self
            .iter_by_col_eq(ST_TABLE_ID, StTableFields::TableId, value_eq)?
            .next()
            .ok_or_else(|| TableError::IdNotFound(SystemTable::st_table, table_id.into()))?;
        let row = StTableRow::try_from(row)?;
        let table_name = row.table_name;
        let table_id: TableId = row.table_id;
        let table_type = row.table_type;
        let table_access = row.table_access;
        let table_primary_key = row.table_primary_key.as_ref().and_then(ColList::as_singleton);

        // Look up the columns for the table in question.
        let mut columns: Vec<ColumnSchema> = iter_st_column_for_table(self, &table_id.into())?
            .map(|row| Ok(StColumnRow::try_from(row)?.into()))
            .collect::<Result<Vec<_>>>()?;
        columns.sort_by_key(|col| col.col_pos);

        // Look up the constraints for the table in question.
        let constraints = self
            .iter_by_col_eq(ST_CONSTRAINT_ID, StConstraintFields::TableId, value_eq)?
            .map(|row| {
                let row = StConstraintRow::try_from(row)?;
                Ok(row.into())
            })
            .collect::<Result<Vec<_>>>()?;

        // Look up the sequences for the table in question.
        let sequences = self
            .iter_by_col_eq(ST_SEQUENCE_ID, StSequenceFields::TableId, value_eq)?
            .map(|row| {
                let row = StSequenceRow::try_from(row)?;
                Ok(row.into())
            })
            .collect::<Result<Vec<_>>>()?;

        // Look up the indexes for the table in question.
        let indexes = self
            .iter_by_col_eq(ST_INDEX_ID, StIndexFields::TableId, value_eq)?
            .map(|row| StIndexRow::try_from(row).map(Into::into))
            .collect::<Result<Vec<_>>>()?;

        let schedule = self
            .iter_by_col_eq(ST_SCHEDULED_ID, StScheduledFields::TableId, value_eq)?
            .next()
            .map(|row| -> Result<_> {
                let row = StScheduledRow::try_from(row)?;
                Ok(row.into())
            })
            .transpose()?;

        // Look up the view info for the table in question, if any.
        let view_info: Option<ViewDefInfo> = self
            .iter_by_col_eq(
                ST_VIEW_ID,
                StViewFields::TableId,
                &AlgebraicValue::OptionSome(value_eq.clone()),
            )
            .map(|mut iter| {
                iter.next().map(|row| -> Result<_> {
                    let row = StViewRow::try_from(row)?;
                    let has_args = self
                        .iter_by_col_eq(ST_VIEW_PARAM_ID, StViewParamFields::ViewId, &row.view_id.into())?
                        .next()
                        .is_some();

                    Ok(ViewDefInfo {
                        view_id: row.view_id,
                        has_args,
                        is_anonymous: row.is_anonymous,
                    })
                })
            })
            .unwrap_or(None)
            .transpose()?;

        Ok(TableSchema::new(
            table_id,
            table_name,
            view_info,
            columns,
            indexes,
            constraints,
            sequences,
            table_type,
            table_access,
            schedule,
            table_primary_key,
        ))
    }

    /// Reads the schema information for the specified `table_id`, consulting the `cache` first.
    ///
    /// If the schema is not found in the cache, the method calls [Self::schema_for_table_raw].
    ///
    /// Note: The responsibility of populating the cache is left to the caller.
    fn schema_for_table(&self, table_id: TableId) -> Result<Arc<TableSchema>> {
        if let Some(schema) = self.get_schema(table_id) {
            return Ok(schema.clone());
        }

        self.schema_for_table_raw(table_id).map(Arc::new)
    }

    fn get_jwt_payload(&self, connection_id: ConnectionId) -> Result<Option<String>> {
        log::info!("Getting JWT payload for connection id: {}", connection_id.to_hex());
        let mut buf: Vec<u8> = Vec::new();
        self.iter_by_col_eq(
            ST_CONNECTION_CREDENTIALS_ID,
            StConnectionCredentialsFields::ConnectionId,
            &ConnectionIdViaU128::from(connection_id).into(),
        )?
            .next()
            .map(|row| row.read_via_bsatn::<StConnectionCredentialsRow>(&mut buf).map(|r| r.jwt_payload))
            .transpose()
            .map_err(|e| {
                log::error!(
                    "[{connection_id}]: get_jwt_payload: failed to get JWT payload for connection id ({connection_id}), error: {e}"
                );
                DatastoreError::Other(
                    anyhow!(
                        "Failed to get JWT payload for connection id ({connection_id}): {e}"
                    )
                )
            })
    }
}

/// Returns an iterator over all `st_column` rows for `table_id`.
pub(crate) fn iter_st_column_for_table<'a>(
    this: &'a (impl StateView + ?Sized),
    table_id: &'a AlgebraicValue,
) -> Result<impl 'a + Iterator<Item = RowRef<'a>>> {
    this.iter_by_col_eq(ST_COLUMN_ID, StColumnFields::TableId, table_id)
}

pub struct IterMutTx<'a> {
    tx_state_ins: Option<(&'a Table, &'a HashMapBlobStore)>,
    stage: ScanStage<'a>,
}

impl<'a> IterMutTx<'a> {
    pub(super) fn new(table_id: TableId, tx_state: &'a TxState, committed_state: &'a CommittedState) -> Result<Self> {
        // If the table exist, the committed state has it as we apply schema changes immediately.
        let Some(commit_table) = committed_state.get_table(table_id) else {
            return Err(TableError::IdNotFound(SystemTable::st_table, table_id.0).into());
        };

        // I can neither confirm nor deny that we have a tx insert table.
        let tx_state_ins = tx_state
            .insert_tables
            .get(&table_id)
            .map(|table| (table, &tx_state.blob_store));

        let iter = commit_table.scan_rows(&committed_state.blob_store);
        let stage = if let Some(deletes) = tx_state.get_delete_table(table_id) {
            // There are deletes in the tx state
            // so we must exclude those (1b).
            let iter = FilterDeleted { iter, deletes };
            ScanStage::CommittedWithTxDeletes { iter }
        } else {
            // There are no deletes in the tx state
            // so we don't need to care about those (1a).
            ScanStage::CommittedNoTxDeletes { iter }
        };

        Ok(Self { tx_state_ins, stage })
    }
}

enum ScanStage<'a> {
    /// Yielding rows from the current tx.
    CurrentTx { iter: TableScanIter<'a> },
    /// Yielding rows from the committed state
    /// without considering tx state deletes as there are none.
    CommittedNoTxDeletes { iter: TableScanIter<'a> },
    /// Yielding rows from the committed state
    /// but there are deleted rows in the tx state,
    /// so we must check against those.
    CommittedWithTxDeletes { iter: FilterDeleted<'a, TableScanIter<'a>> },
}

impl<'a> Iterator for IterMutTx<'a> {
    type Item = RowRef<'a>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        // The finite state machine goes:
        //
        //  CommittedNoTxDeletes ------\
        //                             |----> CurrentTx ---> STOP
        //  CommittedWithTxDeletes ----/
        loop {
            match &mut self.stage {
                ScanStage::CommittedNoTxDeletes { iter } => {
                    // (1a) Go through the committed state for this table
                    // but do not consider deleted rows.
                    if let next @ Some(_) = iter.next() {
                        return next;
                    }
                }
                ScanStage::CommittedWithTxDeletes { iter } => {
                    // (1b) Check the committed row's state in the current tx.
                    // If it's been deleted, skip it.
                    // If it's still present, yield it.
                    // Note that the committed state and the insert tables are disjoint sets,
                    // so at this point we know the row will not be yielded in (3).
                    //
                    // NOTE for future MVCC implementors:
                    // In MVCC, it is no longer valid to elide inserts in this way.
                    // When a transaction inserts a row, that row *must* appear in its insert tables,
                    // even if the row is already present in the committed state.
                    //
                    // Imagine a chain of committed but un-squashed transactions:
                    // `Committed 0: Insert Row A` - `Committed 1: Delete Row A`
                    // where `Committed 1` happens after `Committed 0`.
                    // Imagine a transaction `Running 2: Insert Row A`,
                    // which began before `Committed 1` was committed.
                    // Because `Committed 1` has since been committed,
                    // `Running 2` *must* happen after `Committed 1`.
                    // Therefore, the correct sequence of events is:
                    // - Insert Row A
                    // - Delete Row A
                    // - Insert Row A
                    // This is impossible to recover if `Running 2` elides its insert.
                    //
                    // As a result, in MVCC, this branch will need to check if the `row_ref`
                    // also exists in the `tx_state.insert_tables` and ensure it is yielded only once.
                    if let next @ Some(_) = iter.next() {
                        return next;
                    }
                }
                ScanStage::CurrentTx { iter } => {
                    // (3) look for inserts in the current tx.
                    return iter.next();
                }
            }

            // (2) We got here, so we must've exhausted the committed changes.
            // Start looking in the current tx for inserts, if any, in (3).
            let (insert_table, blob_store) = self.tx_state_ins?;
            let iter = insert_table.scan_rows(blob_store);
            self.stage = ScanStage::CurrentTx { iter };
        }
    }
}

/// A filter on a row.
pub trait RowFilter {
    /// Does this filter include `row`?
    fn filter<'a>(&self, row: RowRef<'a>) -> bool;
}

/// A row filter that matches `range` for the given `cols` of rows.
pub struct RangeOnColumn<R> {
    pub cols: ColList,
    pub range: R,
}

impl<R: RangeBounds<AlgebraicValue>> RowFilter for RangeOnColumn<R> {
    fn filter<'a>(&self, row: RowRef<'a>) -> bool {
        self.range.contains(&row.project(&self.cols).unwrap())
    }
}

/// A row filter that matches `val` for the given `cols` of rows.
pub struct EqOnColumn<'r> {
    pub cols: ColList,
    pub val: &'r AlgebraicValue,
}

impl RowFilter for EqOnColumn<'_> {
    fn filter<'a>(&self, row: RowRef<'a>) -> bool {
        self.val == &row.project(&self.cols).unwrap()
    }
}

/// Applies filter `F` to `I`, producing another iterator.
pub struct ApplyFilter<F, I> {
    iter: I,
    filter: F,
}

impl<F, I> ApplyFilter<F, I> {
    /// Returns an iterator that applies `filer` to `iter`.
    pub(super) fn new(filter: F, iter: I) -> Self {
        Self { iter, filter }
    }
}

impl<'a, F: RowFilter, I: Iterator<Item = RowRef<'a>>> Iterator for ApplyFilter<F, I> {
    type Item = RowRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.find(|row| self.filter.filter(*row))
    }
}

type ScanFilterTx<'a, F> = ApplyFilter<F, TableScanIter<'a>>;
pub type IterByColRangeTx<'a, R> = ScanOrIndex<ScanFilterTx<'a, RangeOnColumn<R>>, IndexScanRangeIter<'a>>;
pub type IterByColEqTx<'a, 'r> = ScanOrIndex<ScanFilterTx<'a, EqOnColumn<'r>>, IndexScanPointIter<'a>>;

type ScanFilterMutTx<'a, F> = ApplyFilter<F, IterMutTx<'a>>;
pub type IterByColRangeMutTx<'a, R> = ScanOrIndex<ScanFilterMutTx<'a, RangeOnColumn<R>>, IndexScanRanged<'a>>;
pub type IterByColEqMutTx<'a, 'r> = ScanOrIndex<ScanFilterMutTx<'a, EqOnColumn<'r>>, IndexScanPoint<'a>>;

/// An iterator that either scans or index scans.
pub enum ScanOrIndex<S, I> {
    /// When the column in question does not have an index.
    Scan(S),

    /// When the column has an index.
    Index(I),
}

impl<'a, S, I> Iterator for ScanOrIndex<S, I>
where
    S: Iterator<Item = RowRef<'a>>,
    I: Iterator<Item = RowRef<'a>>,
{
    type Item = RowRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Scan(iter) => iter.next(),
            Self::Index(iter) => iter.next(),
        }
    }
}
