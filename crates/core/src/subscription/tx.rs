use std::ops::Deref;

use spacetimedb_execution::{Datastore, DeltaStore};
use spacetimedb_lib::ProductValue;
use spacetimedb_primitives::TableId;
use spacetimedb_table::{blob_store::BlobStore, table::Table};

use crate::db::datastore::{locking_tx_datastore::tx::TxId, traits::TxData};

/// A wrapper around a read only tx delta queries
pub struct DeltaTx<'a> {
    tx: &'a TxId,
    data: Option<&'a TxData>,
}

impl<'a> DeltaTx<'a> {
    pub fn new(tx: &'a TxId, data: &'a TxData) -> Self {
        Self { tx, data: Some(data) }
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
        Self { tx, data: None }
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
}
