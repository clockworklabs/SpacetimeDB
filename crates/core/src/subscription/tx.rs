use std::{
    collections::BTreeMap,
    ops::{Deref, RangeBounds},
    sync::Arc,
};

use hashbrown::HashMap;
use itertools::Either;
use smallvec::SmallVec;
use spacetimedb_execution::{Datastore, DeltaStore, Row};
use spacetimedb_lib::{query::Delta, AlgebraicValue, ProductValue};
use spacetimedb_primitives::{IndexId, TableId};
use spacetimedb_table::{blob_store::BlobStore, table::Table};

use crate::db::datastore::{
    locking_tx_datastore::{state_view::StateView, tx::TxId},
    traits::TxData,
};

use super::module_subscription_manager::QueriedTableIndexIds;

/// If an index is defined on a set of columns,
/// and if that index is used in a subscription query,
/// we build the very same index for each delta table.
///
/// Here a column value maps to its position(s) in the delta table.
type DeltaTableIndex = BTreeMap<AlgebraicValue, SmallVec<[usize; 1]>>;

/// The set of indexes that have been built over a [TxData] delta table.
#[derive(Default, Debug)]
pub struct DeltaTableIndexes {
    inserts: HashMap<(TableId, IndexId), DeltaTableIndex>,
    deletes: HashMap<(TableId, IndexId), DeltaTableIndex>,
}

impl DeltaTableIndexes {
    /// Get the btree index corresponding to `index_id` for the inserts of this delta table.
    fn get_index_for_inserts(&self, table_id: TableId, index_id: IndexId) -> Option<&DeltaTableIndex> {
        self.inserts.get(&(table_id, index_id))
    }

    /// Get the btree index corresponding to `index_id` for the deletes of this delta table.
    fn get_index_for_deletes(&self, table_id: TableId, index_id: IndexId) -> Option<&DeltaTableIndex> {
        self.deletes.get(&(table_id, index_id))
    }

    /// Construct the btree indexes required by the subscription manager for this delta table.
    fn from_tx_data(tx: &TxId, data: &TxData, meta: &QueriedTableIndexIds) -> Self {
        fn build_indexes_for_rows<'a>(
            tx: &'a TxId,
            meta: &'a QueriedTableIndexIds,
            rows: impl Iterator<Item = (&'a TableId, &'a Arc<[ProductValue]>)>,
        ) -> HashMap<(TableId, IndexId), DeltaTableIndex> {
            let mut indexes: HashMap<(TableId, IndexId), DeltaTableIndex> = HashMap::new();
            for (table_id, rows) in rows {
                if let Some(schema) = tx.get_schema(*table_id) {
                    // Fetch the column ids for each index
                    let mut cols_for_index = vec![];
                    for index_id in meta.index_ids_for_table(*table_id) {
                        cols_for_index.push((index_id, schema.col_list_for_index_id(index_id)));
                    }
                    for (i, row) in rows.iter().enumerate() {
                        for (index_id, col_list) in &cols_for_index {
                            if !col_list.is_empty() {
                                indexes
                                    .entry((*table_id, *index_id))
                                    .or_default()
                                    .entry(row.project(col_list).unwrap())
                                    .or_default()
                                    .push(i);
                            }
                        }
                    }
                }
            }
            indexes
        }

        Self {
            inserts: build_indexes_for_rows(tx, meta, data.inserts()),
            deletes: build_indexes_for_rows(tx, meta, data.deletes()),
        }
    }
}

/// A wrapper around a read only tx delta queries
pub struct DeltaTx<'a> {
    tx: &'a TxId,
    data: Option<&'a TxData>,
    indexes: DeltaTableIndexes,
}

impl<'a> DeltaTx<'a> {
    pub fn new(tx: &'a TxId, data: &'a TxData, indexes: &QueriedTableIndexIds) -> Self {
        Self {
            tx,
            data: Some(data),
            indexes: DeltaTableIndexes::from_tx_data(tx, data, indexes),
        }
    }
}

impl Deref for DeltaTx<'_> {
    type Target = TxId;

    fn deref(&self) -> &Self::Target {
        self.tx
    }
}

impl<'a> From<&'a TxId> for DeltaTx<'a> {
    fn from(tx: &'a TxId) -> Self {
        Self {
            tx,
            data: None,
            indexes: DeltaTableIndexes::default(),
        }
    }
}

impl Datastore for DeltaTx<'_> {
    fn table(&self, table_id: TableId) -> Option<&Table> {
        self.tx.table(table_id)
    }

    fn blob_store(&self) -> &dyn BlobStore {
        self.tx.blob_store()
    }
}

impl DeltaStore for DeltaTx<'_> {
    fn num_inserts(&self, table_id: TableId) -> usize {
        self.data
            .and_then(|data| {
                data.inserts()
                    .find(|(id, _)| **id == table_id)
                    .map(|(_, rows)| rows.len())
            })
            .unwrap_or_default()
    }

    fn num_deletes(&self, table_id: TableId) -> usize {
        self.data
            .and_then(|data| {
                data.deletes()
                    .find(|(id, _)| **id == table_id)
                    .map(|(_, rows)| rows.len())
            })
            .unwrap_or_default()
    }

    fn inserts_for_table(&self, table_id: TableId) -> Option<std::slice::Iter<'_, ProductValue>> {
        self.data.and_then(|data| {
            data.inserts()
                .find(|(id, _)| **id == table_id)
                .map(|(_, rows)| rows.iter())
        })
    }

    fn deletes_for_table(&self, table_id: TableId) -> Option<std::slice::Iter<'_, ProductValue>> {
        self.data.and_then(|data| {
            data.deletes()
                .find(|(id, _)| **id == table_id)
                .map(|(_, rows)| rows.iter())
        })
    }

    fn index_scan_range_for_delta(
        &self,
        table_id: TableId,
        index_id: IndexId,
        delta: Delta,
        range: impl RangeBounds<AlgebraicValue>,
    ) -> impl Iterator<Item = Row> {
        fn scan_index<'a>(
            data: Option<&'a TxData>,
            indexes: &'a DeltaTableIndexes,
            table_id: TableId,
            index_id: IndexId,
            range: impl RangeBounds<AlgebraicValue>,
            get_index: impl Fn(&DeltaTableIndexes, TableId, IndexId) -> Option<&DeltaTableIndex>,
            get_ith_row: impl Fn(&TxData, TableId, usize) -> Option<&ProductValue>,
        ) -> impl Iterator<Item = Row<'a>> {
            data.and_then(move |data| {
                get_index(indexes, table_id, index_id).map(move |btree| {
                    btree
                        .range(range)
                        .flat_map(|(_, positions)| positions)
                        .filter_map(move |i| get_ith_row(data, table_id, *i))
                        .map(Row::Ref)
                })
            })
            .into_iter()
            .flatten()
        }
        match delta {
            Delta::Inserts => Either::Left(scan_index(
                self.data,
                &self.indexes,
                table_id,
                index_id,
                range,
                DeltaTableIndexes::get_index_for_inserts,
                TxData::get_ith_insert,
            )),
            Delta::Deletes => Either::Right(scan_index(
                self.data,
                &self.indexes,
                table_id,
                index_id,
                range,
                DeltaTableIndexes::get_index_for_deletes,
                TxData::get_ith_delete,
            )),
        }
    }

    fn index_scan_point_for_delta(
        &self,
        table_id: TableId,
        index_id: IndexId,
        delta: Delta,
        point: &AlgebraicValue,
    ) -> impl Iterator<Item = Row> {
        self.index_scan_range_for_delta(table_id, index_id, delta, point)
    }
}
