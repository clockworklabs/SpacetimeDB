use crate::db::datastore::system_tables::{StColumnRow, StConstraintRow, StIndexRow, StSequenceRow, ST_TABLES_ID};
use spacetimedb_sats::db::def::*;
use spacetimedb_sats::hash::Hash;
use spacetimedb_sats::DataKey;
use spacetimedb_sats::{AlgebraicValue, ProductType, ProductValue};
use std::{ops::RangeBounds, sync::Arc};

use super::{system_tables::StTableRow, Result};

/// Operations in a transaction are either Inserts or Deletes.
/// Inserts report the byte objects they inserted, to be persisted
/// later in an object store.
pub enum TxOp {
    Insert(Arc<Vec<u8>>),
    Delete,
}

/// A record of a single operation within a transaction.
pub struct TxRecord {
    /// Whether the operation was an insert or a delete.
    pub(crate) op: TxOp,
    /// The value of the modified row.
    pub(crate) product_value: ProductValue,
    /// The key of the modified row.
    pub(crate) key: DataKey,
    /// The table that was modified.
    pub(crate) table_id: TableId,
}

/// A record of all the operations within a transaction.
pub struct TxData {
    pub(crate) records: Vec<TxRecord>,
}

pub trait Data: Into<ProductValue> {
    fn view(&self) -> &ProductValue;
}

pub trait DataRow: Send + Sync {
    type RowId: Copy;

    type Data: Data;
    type DataRef: Clone;

    fn data_to_owned(&self, data_ref: Self::DataRef) -> Self::Data;
}

pub trait Tx {
    type TxId;

    fn begin_tx(&self) -> Self::TxId;
    fn release_tx(&self, tx: Self::TxId);
}

pub trait MutTx {
    type MutTxId;

    fn begin_mut_tx(&self) -> Self::MutTxId;
    fn rollback_mut_tx(&self, tx: Self::MutTxId);
    fn commit_mut_tx(&self, tx: Self::MutTxId) -> Result<Option<TxData>>;
}

pub trait TxDatastore: DataRow + Tx {
    type Iter<'a>: Iterator<Item = Self::DataRef>
    where
        Self: 'a;

    type IterByColRange<'a, R: RangeBounds<AlgebraicValue>>: Iterator<Item = Self::DataRef>
    where
        Self: 'a;

    type IterByColEq<'a>: Iterator<Item = Self::DataRef>
    where
        Self: 'a;

    fn iter_tx<'a>(&'a self, tx: &'a Self::TxId, table_id: TableId) -> Result<Self::Iter<'a>>;

    fn iter_by_col_range_tx<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        tx: &'a Self::TxId,
        table_id: TableId,
        col_id: ColId,
        range: R,
    ) -> Result<Self::IterByColRange<'a, R>>;

    fn iter_by_col_eq_tx<'a>(
        &'a self,
        tx: &'a Self::TxId,
        table_id: TableId,
        col_id: ColId,
        value: AlgebraicValue,
    ) -> Result<Self::IterByColEq<'a>>;

    fn get_tx<'a>(
        &'a self,
        tx: &'a Self::TxId,
        table_id: TableId,
        row_id: Self::RowId,
    ) -> Result<Option<Self::DataRef>>;

    fn scan_st_tables<'a>(&'a self, tx: &'a Self::TxId) -> Result<Vec<StTableRow<String>>>;

    fn scan_st_columns<'a>(&'a self, tx: &'a Self::TxId) -> Result<Vec<StColumnRow<String>>>;

    fn scan_st_constraints<'a>(&'a self, tx: &'a Self::TxId) -> Result<Vec<StConstraintRow<String>>>;

    fn scan_st_sequences<'a>(&'a self, tx: &'a Self::TxId) -> Result<Vec<StSequenceRow<String>>>;

    fn scan_st_indexes<'a>(&'a self, tx: &'a Self::TxId) -> Result<Vec<StIndexRow<String>>>;
}

pub trait MutTxDatastore: TxDatastore + MutTx {
    // Tables
    fn create_table_mut_tx(&self, tx: &mut Self::MutTxId, schema: TableDef) -> Result<TableId>;
    fn row_type_for_table_mut_tx(&self, tx: &Self::MutTxId, table_id: TableId) -> Result<ProductType>;
    fn schema_for_table_mut_tx(&self, tx: &Self::MutTxId, table_id: TableId) -> Result<TableSchema>;
    fn drop_table_mut_tx(&self, tx: &mut Self::MutTxId, table_id: TableId) -> Result<()>;
    fn rename_table_mut_tx(&self, tx: &mut Self::MutTxId, table_id: TableId, new_name: &str) -> Result<()>;
    fn table_id_exists(&self, tx: &Self::MutTxId, table_id: &TableId) -> bool;
    fn table_id_from_name_mut_tx(&self, tx: &Self::MutTxId, table_name: &str) -> Result<Option<TableId>>;
    fn table_name_from_id_mut_tx(&self, tx: &Self::MutTxId, table_id: TableId) -> Result<Option<String>>;
    fn get_all_tables_mut_tx(&self, tx: &Self::MutTxId) -> super::Result<Vec<TableSchema>> {
        let mut tables = Vec::new();
        let table_rows = self.iter_mut_tx(tx, ST_TABLES_ID)?.collect::<Vec<_>>();
        for data_ref in table_rows {
            let data = self.data_to_owned(data_ref);
            let row = StTableRow::try_from(data.view())?;
            let table_id = row.table_id;
            tables.push(self.schema_for_table_mut_tx(tx, table_id)?);
        }
        Ok(tables)
    }

    // Indexes
    fn create_index_mut_tx(&self, tx: &mut Self::MutTxId, table_id: TableId, index: IndexDef) -> Result<IndexId>;
    fn drop_index_mut_tx(&self, tx: &mut Self::MutTxId, index_id: IndexId) -> Result<()>;
    fn index_id_from_name_mut_tx(&self, tx: &Self::MutTxId, index_name: &str) -> super::Result<Option<IndexId>>;

    // TODO: Index data
    // - index_scan_mut_tx
    // - index_range_scan_mut_tx
    // - index_seek_mut_tx

    // Sequences
    fn get_next_sequence_value_mut_tx(&self, tx: &mut Self::MutTxId, seq_id: SequenceId) -> Result<i128>;
    fn create_sequence_mut_tx(&self, tx: &mut Self::MutTxId, table_id: TableId, seq: SequenceDef)
        -> Result<SequenceId>;
    fn drop_sequence_mut_tx(&self, tx: &mut Self::MutTxId, seq_id: SequenceId) -> Result<()>;
    fn sequence_id_from_name_mut_tx(
        &self,
        tx: &Self::MutTxId,
        sequence_name: &str,
    ) -> super::Result<Option<SequenceId>>;

    // Constraints
    fn drop_constraint_mut_tx(&self, tx: &mut Self::MutTxId, constraint_id: ConstraintId) -> super::Result<()>;
    fn constraint_id_from_name(&self, tx: &Self::MutTxId, constraint_name: &str)
        -> super::Result<Option<ConstraintId>>;

    // Data
    fn iter_mut_tx<'a>(&'a self, tx: &'a Self::MutTxId, table_id: TableId) -> Result<Self::Iter<'a>>;
    fn iter_by_col_range_mut_tx<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        tx: &'a Self::MutTxId,
        table_id: TableId,
        col_id: ColId,
        range: R,
    ) -> Result<Self::IterByColRange<'a, R>>;
    fn iter_by_col_eq_mut_tx<'a>(
        &'a self,
        tx: &'a Self::MutTxId,
        table_id: TableId,
        col_id: ColId,
        value: AlgebraicValue,
    ) -> Result<Self::IterByColEq<'a>>;
    fn get_mut_tx<'a>(
        &'a self,
        tx: &'a Self::MutTxId,
        table_id: TableId,
        row_id: Self::RowId,
    ) -> Result<Option<Self::DataRef>>;
    fn delete_mut_tx<'a>(&'a self, tx: &'a mut Self::MutTxId, table_id: TableId, row_id: Self::RowId) -> Result<bool>;
    fn delete_by_rel_mut_tx<R: IntoIterator<Item = ProductValue>>(
        &self,
        tx: &mut Self::MutTxId,
        table_id: TableId,
        relation: R,
    ) -> Result<Option<u32>>;
    fn insert_mut_tx<'a>(
        &'a self,
        tx: &'a mut Self::MutTxId,
        table_id: TableId,
        row: ProductValue,
    ) -> Result<ProductValue>;
}

/// Describes a programmable [`TxDatastore`].
///
/// A programmable datastore is one which has a program of some kind associated
/// with it.
pub trait Programmable: TxDatastore {
    /// Retrieve the [`Hash`] of the program currently associated with the
    /// datastore.
    ///
    /// A `None` result means that no program is currently associated, e.g.
    /// because the datastore has not been fully initialized yet.
    fn program_hash(&self, tx: &Self::TxId) -> Result<Option<Hash>>;
}

/// Describes a [`Programmable`] datastore which allows to update the program
/// associated with it.
pub trait MutProgrammable: MutTxDatastore {
    /// A fencing token (usually a monotonic counter) which allows to order
    /// `set_module_hash` with respect to a distributed locking service.
    type FencingToken: Eq + Ord;

    /// Update the [`Hash`] of the program currently associated with the
    /// datastore.
    ///
    /// The operation runs within the transactional context `tx`. The fencing
    /// token `fence` must be verified to be greater than in any previous
    /// invocations of this method.
    fn set_program_hash(&self, tx: &mut Self::MutTxId, fence: Self::FencingToken, hash: Hash) -> Result<()>;
}
