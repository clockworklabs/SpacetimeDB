use super::{system_tables::StTableRow, Result};
use crate::db::datastore::system_tables::ST_TABLES_ID;
use crate::execution_context::ExecutionContext;
use nonempty::NonEmpty;
use spacetimedb_primitives::*;
use spacetimedb_sats::db::def::*;
use spacetimedb_sats::hash::Hash;
use spacetimedb_sats::DataKey;
use spacetimedb_sats::{AlgebraicValue, ProductType, ProductValue};
use std::borrow::Cow;
use std::{ops::RangeBounds, sync::Arc};

/// Operations in a transaction are either Inserts or Deletes.
/// Inserts report the byte objects they inserted, to be persisted
/// later in an object store.s
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

    type DataRef<'a>;

    fn view_product_value<'a>(&self, data_ref: Self::DataRef<'a>) -> &'a ProductValue;
}

pub trait Tx {
    type TxId;

    fn begin_tx(&self) -> Self::TxId;
    fn release_tx(&self, ctx: &ExecutionContext, tx: Self::TxId);
}

pub trait MutTx {
    type MutTxId;

    fn begin_mut_tx(&self) -> Self::MutTxId;
    fn commit_mut_tx(&self, ctx: &ExecutionContext, tx: Self::MutTxId) -> Result<Option<TxData>>;
    fn rollback_mut_tx(&self, ctx: &ExecutionContext, tx: Self::MutTxId);

    #[cfg(test)]
    fn commit_mut_tx_for_test(&self, tx: Self::MutTxId) -> Result<Option<TxData>>;

    #[cfg(test)]
    fn rollback_mut_tx_for_test(&self, tx: Self::MutTxId);
}

pub trait TxDatastore: DataRow + Tx {
    type Iter<'a>: Iterator<Item = Self::DataRef<'a>>
    where
        Self: 'a;

    type IterByColRange<'a, R: RangeBounds<AlgebraicValue>>: Iterator<Item = Self::DataRef<'a>>
    where
        Self: 'a;

    type IterByColEq<'a>: Iterator<Item = Self::DataRef<'a>>
    where
        Self: 'a;

    fn iter_tx<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::TxId,
        table_id: TableId,
    ) -> Result<Self::Iter<'a>>;

    fn iter_by_col_range_tx<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::TxId,
        table_id: TableId,
        cols: NonEmpty<ColId>,
        range: R,
    ) -> Result<Self::IterByColRange<'a, R>>;

    fn iter_by_col_eq_tx<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::TxId,
        table_id: TableId,
        cols: NonEmpty<ColId>,
        value: AlgebraicValue,
    ) -> Result<Self::IterByColEq<'a>>;

    fn get_tx<'a>(
        &self,
        tx: &'a Self::TxId,
        table_id: TableId,
        row_id: &'a Self::RowId,
    ) -> Result<Option<Self::DataRef<'a>>>;
}

pub trait MutTxDatastore: TxDatastore + MutTx {
    // Tables
    fn create_table_mut_tx(&self, tx: &mut Self::MutTxId, schema: TableDef) -> Result<TableId>;
    // In these methods, we use `'tx` because the return type must borrow data
    // from `Inner` in the `Locking` implementation,
    // and `Inner` lives in `tx: &MutTxId`.
    fn row_type_for_table_mut_tx<'tx>(
        &self,
        tx: &'tx Self::MutTxId,
        table_id: TableId,
    ) -> Result<Cow<'tx, ProductType>>;
    fn schema_for_table_mut_tx<'tx>(&self, tx: &'tx Self::MutTxId, table_id: TableId) -> Result<Cow<'tx, TableSchema>>;
    fn drop_table_mut_tx(&self, tx: &mut Self::MutTxId, table_id: TableId) -> Result<()>;
    fn rename_table_mut_tx(&self, tx: &mut Self::MutTxId, table_id: TableId, new_name: &str) -> Result<()>;
    fn table_id_exists(&self, tx: &Self::MutTxId, table_id: &TableId) -> bool;
    fn table_id_from_name_mut_tx(&self, tx: &Self::MutTxId, table_name: &str) -> Result<Option<TableId>>;
    fn table_name_from_id_mut_tx<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::MutTxId,
        table_id: TableId,
    ) -> Result<Option<&'a str>>;
    fn get_all_tables_mut_tx<'tx>(
        &self,
        ctx: &ExecutionContext,
        tx: &'tx Self::MutTxId,
    ) -> super::Result<Vec<Cow<'tx, TableSchema>>> {
        let mut tables = Vec::new();
        let table_rows = self.iter_mut_tx(ctx, tx, ST_TABLES_ID)?.collect::<Vec<_>>();
        for data_ref in table_rows {
            let data = self.view_product_value(data_ref);
            let row = StTableRow::try_from(data)?;
            tables.push(self.schema_for_table_mut_tx(tx, row.table_id)?);
        }
        Ok(tables)
    }

    // Indexes
    fn create_index_mut_tx(&self, tx: &mut Self::MutTxId, index: IndexDef) -> Result<IndexId>;
    fn drop_index_mut_tx(&self, tx: &mut Self::MutTxId, index_id: IndexId) -> Result<()>;
    fn index_id_from_name_mut_tx(&self, tx: &Self::MutTxId, index_name: &str) -> super::Result<Option<IndexId>>;

    // TODO: Index data
    // - index_scan_mut_tx
    // - index_range_scan_mut_tx
    // - index_seek_mut_tx

    // Sequences
    fn get_next_sequence_value_mut_tx(&self, tx: &mut Self::MutTxId, seq_id: SequenceId) -> Result<i128>;
    fn create_sequence_mut_tx(&self, tx: &mut Self::MutTxId, seq: SequenceDef) -> Result<SequenceId>;
    fn drop_sequence_mut_tx(&self, tx: &mut Self::MutTxId, seq_id: SequenceId) -> Result<()>;
    fn sequence_id_from_name_mut_tx(
        &self,
        tx: &Self::MutTxId,
        sequence_name: &str,
    ) -> super::Result<Option<SequenceId>>;

    // Data
    fn iter_mut_tx<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::MutTxId,
        table_id: TableId,
    ) -> Result<Self::Iter<'a>>;
    fn iter_by_col_range_mut_tx<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::MutTxId,
        table_id: TableId,
        cols: impl Into<NonEmpty<ColId>>,
        range: R,
    ) -> Result<Self::IterByColRange<'a, R>>;
    fn iter_by_col_eq_mut_tx<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::MutTxId,
        table_id: TableId,
        cols: impl Into<NonEmpty<ColId>>,
        value: AlgebraicValue,
    ) -> Result<Self::IterByColEq<'a>>;
    fn get_mut_tx<'a>(
        &self,
        tx: &'a Self::MutTxId,
        table_id: TableId,
        row_id: &'a Self::RowId,
    ) -> Result<Option<Self::DataRef<'a>>>;
    fn delete_mut_tx<'a>(
        &'a self,
        tx: &'a mut Self::MutTxId,
        table_id: TableId,
        row_ids: impl IntoIterator<Item = Self::RowId>,
    ) -> u32;
    fn delete_by_rel_mut_tx(
        &self,
        tx: &mut Self::MutTxId,
        table_id: TableId,
        relation: impl IntoIterator<Item = ProductValue>,
    ) -> u32;
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

#[cfg(test)]
mod tests {
    use crate::host::from_lib_tabledef;

    use super::*;
    use nonempty::NonEmpty;
    use spacetimedb_primitives::ColId;
    use spacetimedb_sats::db::attr::ColumnAttribute;
    use spacetimedb_sats::db::auth::{StAccess, StTableType};
    use spacetimedb_sats::db::def::{IndexDef, IndexType};
    use spacetimedb_sats::{AlgebraicType, AlgebraicTypeRef, ProductType, ProductTypeElement, Typespace};

    #[test]
    fn test_tabledef_from_lib_tabledef() -> anyhow::Result<()> {
        let id_and_name = IndexDef {
            table_id: TableId::AUTO_FOR_INDEX,
            cols: NonEmpty {
                head: ColId(0),
                tail: [ColId(1)].into(),
            },
            name: "id_and_name".into(),
            is_unique: false,
            index_type: IndexType::BTree,
        };
        let just_name = IndexDef {
            table_id: TableId::AUTO_FOR_INDEX,
            cols: ColId(1).into(),
            name: "just_name".into(),
            is_unique: false,
            index_type: IndexType::BTree,
        };

        let lib_table_def = spacetimedb_lib::TableDef {
            name: "Person".into(),
            data: AlgebraicTypeRef(0),
            column_attrs: vec![ColumnAttribute::IDENTITY, ColumnAttribute::UNSET],
            indexes: [id_and_name.clone(), just_name.clone()].into(),
            table_type: StTableType::User,
            table_access: StAccess::Public,
        };
        let row_type = ProductType::new(
            [
                ProductTypeElement {
                    name: Some("id".into()),
                    algebraic_type: AlgebraicType::U32,
                },
                ProductTypeElement {
                    name: Some("name".into()),
                    algebraic_type: AlgebraicType::String,
                },
            ]
            .into(),
        );

        let mut datastore_schema = from_lib_tabledef(Typespace::new(vec![row_type.into()]).with_type(&lib_table_def))?;
        let mut expected_schema = TableDef {
            table_name: "Person".into(),
            columns: [
                ColumnDef {
                    col_name: "id".into(),
                    col_type: AlgebraicType::U32,
                    is_autoinc: true,
                },
                ColumnDef {
                    col_name: "name".into(),
                    col_type: AlgebraicType::String,
                    is_autoinc: false,
                },
            ]
            .into(),
            indexes: [
                IndexDef {
                    table_id: TableId::AUTO_FOR_INDEX,
                    cols: ColId(0).into(),
                    name: "Person_id_unique".into(),
                    is_unique: true,
                    index_type: IndexType::BTree,
                },
                id_and_name,
                just_name,
            ]
            .into(),
            table_type: StTableType::User,
            table_access: StAccess::Public,
        };

        for schema in [&mut datastore_schema, &mut expected_schema] {
            schema.columns.sort_by(|a, b| a.col_name.cmp(&b.col_name));
            schema.indexes.sort_by(|a, b| a.name.cmp(&b.name));
        }

        assert_eq!(expected_schema, datastore_schema);

        Ok(())
    }
}
