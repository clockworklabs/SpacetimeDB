use crate::db::relational_db::ST_TABLES_ID;
use core::fmt;
use spacetimedb_lib::auth::{StAccess, StTableType};
use spacetimedb_lib::relation::{DbTable, FieldName, FieldOnly, Header, TableField};
use spacetimedb_lib::{ColumnIndexAttribute, DataKey, Hash};
use spacetimedb_sats::product_value::InvalidFieldError;
use spacetimedb_sats::slim_slice::SlimSliceBoxCollected;
use spacetimedb_sats::{
    string, AlgebraicType, AlgebraicValue, ProductType, ProductTypeElement, ProductValue, SatsNonEmpty, SatsString,
    SatsVec,
};
use spacetimedb_vm::expr::SourceExpr;
use std::{ops::RangeBounds, sync::Arc};

use super::{system_tables::StTableRow, Result};

/// The `id` for [Sequence]
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct TableId(pub(crate) u32);
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[repr(transparent)]
pub struct ColId(pub u32);
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct IndexId(pub(crate) u32);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SequenceId(pub(crate) u32);

impl From<IndexId> for AlgebraicValue {
    fn from(value: IndexId) -> Self {
        value.0.into()
    }
}

impl From<SequenceId> for AlgebraicValue {
    fn from(value: SequenceId) -> Self {
        value.0.into()
    }
}

impl fmt::Display for SequenceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TableId {
    pub fn from_u32_for_testing(id: u32) -> Self {
        Self(id)
    }
}

impl From<TableId> for AlgebraicValue {
    fn from(value: TableId) -> Self {
        value.0.into()
    }
}

impl From<ColId> for AlgebraicValue {
    fn from(value: ColId) -> Self {
        value.0.into()
    }
}

impl ColId {
    /// Returns this column "id" as an index.
    pub fn idx(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceSchema {
    pub(crate) sequence_id: u32,
    pub(crate) sequence_name: SatsString,
    pub(crate) table_id: u32,
    pub(crate) col_id: ColId,
    pub(crate) increment: i128,
    pub(crate) start: i128,
    pub(crate) min_value: i128,
    pub(crate) max_value: i128,
    pub(crate) allocated: i128,
}

/// This type is just the [SequenceSchema] without the autoinc fields
/// It's also adjusted to be convenient for specifying a new sequence
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceDef {
    pub(crate) sequence_name: SatsString,
    pub(crate) table_id: u32,
    pub(crate) col_id: ColId,
    pub(crate) increment: i128,
    pub(crate) start: Option<i128>,
    pub(crate) min_value: Option<i128>,
    pub(crate) max_value: Option<i128>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexSchema {
    pub(crate) index_id: u32,
    pub(crate) table_id: u32,
    pub(crate) index_name: SatsString,
    pub(crate) is_unique: bool,
    pub(crate) cols: SatsNonEmpty<ColId>,
}

/// This type is just the [IndexSchema] without the autoinc fields
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexDef {
    pub(crate) table_id: u32,
    pub(crate) cols: SatsNonEmpty<ColId>,
    pub(crate) name: SatsString,
    pub(crate) is_unique: bool,
}

impl IndexDef {
    pub fn new(name: SatsString, table_id: u32, col_id: ColId, is_unique: bool) -> Self {
        Self {
            cols: SatsNonEmpty::new(col_id),
            name,
            is_unique,
            table_id,
        }
    }
}

impl From<IndexSchema> for IndexDef {
    fn from(value: IndexSchema) -> Self {
        Self {
            table_id: value.table_id,
            cols: value.cols,
            name: value.index_name,
            is_unique: value.is_unique,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnSchema {
    pub table_id: u32,
    pub col_id: ColId,
    pub col_name: SatsString,
    pub col_type: AlgebraicType,
    pub is_autoinc: bool,
}

impl From<&ColumnSchema> for spacetimedb_lib::table::ColumnDef {
    fn from(value: &ColumnSchema) -> Self {
        Self {
            column: ProductTypeElement::from(value),
            // TODO(cloutiertyler): !!! This is not correct !!! We do not have the information regarding constraints here.
            // We should remove this field from the ColumnDef struct.
            attr: if value.is_autoinc {
                spacetimedb_lib::ColumnIndexAttribute::AUTO_INC
            } else {
                spacetimedb_lib::ColumnIndexAttribute::UNSET
            },
            // if value.is_autoinc && value.is_unique {
            //     spacetimedb_lib::ColumnIndexAttribute::Identity
            // } else if value.is_autoinc {
            //     spacetimedb_lib::ColumnIndexAttribute::AutoInc
            // } else if value.is_unique {
            //     spacetimedb_lib::ColumnIndexAttribute::Unique
            // } else {
            //     spacetimedb_lib::ColumnIndexAttribute::UnSet
            // },
            pos: value.col_id.idx(),
        }
    }
}

impl From<&ColumnSchema> for ProductTypeElement {
    fn from(value: &ColumnSchema) -> Self {
        Self {
            name: Some(value.col_name.clone()),
            algebraic_type: value.col_type.clone(),
        }
    }
}

/// This type is just the [ColumnSchema] without the autoinc fields
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnDef {
    pub(crate) col_name: SatsString,
    pub(crate) col_type: AlgebraicType,
    pub(crate) is_autoinc: bool,
}

impl From<ColumnSchema> for ColumnDef {
    fn from(value: ColumnSchema) -> Self {
        Self {
            col_name: value.col_name,
            col_type: value.col_type,
            is_autoinc: value.is_autoinc,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstraintSchema {
    pub(crate) constraint_id: u32,
    pub(crate) constraint_name: SatsString,
    pub(crate) kind: ColumnIndexAttribute,
    pub(crate) table_id: u32,
    pub(crate) columns: SatsVec<u32>,
}

/// This type is just the [ConstraintSchema] without the autoinc fields
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstraintDef {
    pub(crate) constraint_name: SatsString,
    pub(crate) kind: ColumnIndexAttribute,
    pub(crate) table_id: u32,
    pub(crate) columns: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSchema {
    pub table_id: u32,
    pub table_name: SatsString,
    pub columns: Box<[ColumnSchema]>,
    pub indexes: Vec<IndexSchema>,
    pub constraints: Vec<ConstraintSchema>,
    pub table_type: StTableType,
    pub table_access: StAccess,
}

impl TableSchema {
    /// Check if the `name` of the [FieldName] exist on this [TableSchema]
    ///
    /// Warning: It ignores the `table_name`
    pub fn get_column_by_field(&self, field: &FieldName) -> Option<&ColumnSchema> {
        match field.field() {
            FieldOnly::Name(x) => self.get_column_by_name(x),
            FieldOnly::Pos(x) => self.get_column(x),
        }
    }

    /// Check if there is an index for this [FieldName]
    ///
    /// Warning: It ignores the `table_name`
    pub fn get_index_by_field(&self, field: &FieldName) -> Option<&IndexSchema> {
        let ColumnSchema { col_id, .. } = self.get_column_by_field(field)?;
        self.indexes
            .iter()
            .find(|IndexSchema { cols, .. }| cols.len() == 1 && cols.head == *col_id)
    }

    pub fn get_column(&self, pos: usize) -> Option<&ColumnSchema> {
        self.columns.get(pos)
    }

    /// Check if the `col_name` exist on this [TableSchema]
    ///
    /// Warning: It ignores the `table_name`
    pub fn get_column_by_name(&self, col_name: &str) -> Option<&ColumnSchema> {
        self.columns.iter().find(|x| &*x.col_name == col_name)
    }

    /// Turn a [TableField] that could be an unqualified field `id` into `table.id`
    pub fn normalize_field(&self, or_use: &TableField) -> FieldName {
        FieldName::named(or_use.table.unwrap_or(&self.table_name), or_use.field)
    }

    /// Project the fields from the supplied `columns`.
    pub fn project(&self, columns: impl Iterator<Item = usize>) -> Result<Vec<&ColumnSchema>> {
        columns
            .map(|pos| {
                self.get_column(pos).ok_or(
                    InvalidFieldError {
                        col_pos: pos,
                        name: None,
                    }
                    .into(),
                )
            })
            .collect()
    }

    /// Utility for project the fields from the supplied `columns` that is a [NonEmpty<u32>],
    /// used for when the list of field columns have at least one value.
    pub fn project_not_empty(&self, columns: &SatsNonEmpty<ColId>) -> Result<Vec<&ColumnSchema>> {
        self.project(columns.iter().map(|x| x.idx()))
    }
}

impl From<&TableSchema> for ProductType {
    fn from(value: &TableSchema) -> Self {
        ProductType::new(
            value
                .columns
                .iter()
                .map(|c| ProductTypeElement {
                    name: Some(c.col_name.clone()),
                    algebraic_type: c.col_type.clone(),
                })
                .collect::<SlimSliceBoxCollected<_>>()
                .unwrap(),
        )
    }
}

impl From<&TableSchema> for SourceExpr {
    fn from(value: &TableSchema) -> Self {
        SourceExpr::DbTable(DbTable::new(
            Header::from_product_type(value.table_name.clone(), value.into()),
            value.table_id,
            value.table_type,
            value.table_access,
        ))
    }
}

impl From<&TableSchema> for DbTable {
    fn from(value: &TableSchema) -> Self {
        DbTable::new(value.into(), value.table_id, value.table_type, value.table_access)
    }
}

impl From<&TableSchema> for Header {
    fn from(value: &TableSchema) -> Self {
        Header::from_product_type(value.table_name.clone(), value.into())
    }
}

impl TableDef {
    pub fn get_row_type(&self) -> ProductType {
        ProductType::new(
            self.columns
                .iter()
                .map(|c| ProductTypeElement {
                    name: None,
                    algebraic_type: c.col_type.clone(),
                })
                .collect::<SlimSliceBoxCollected<_>>()
                .unwrap(),
        )
    }
}

/// This type is just the [TableSchema] without the autoinc fields
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableDef {
    pub(crate) table_name: SatsString,
    pub(crate) columns: Box<[ColumnDef]>,
    pub(crate) indexes: Vec<IndexDef>,
    pub(crate) table_type: StTableType,
    pub(crate) table_access: StAccess,
}

impl From<ProductType> for TableDef {
    fn from(value: ProductType) -> Self {
        Self {
            table_name: string(""),
            columns: value
                .elements
                .iter()
                .enumerate()
                .map(|(i, e)| ColumnDef {
                    col_name: e
                        .name
                        .to_owned()
                        .unwrap_or_else(|| SatsString::from_string(i.to_string())),
                    col_type: e.algebraic_type.clone(),
                    is_autoinc: false,
                })
                .collect(),
            indexes: vec![],
            table_type: StTableType::User,
            table_access: StAccess::Public,
        }
    }
}

impl From<TableSchema> for TableDef {
    fn from(value: TableSchema) -> Self {
        Self {
            table_name: value.table_name,
            columns: value.columns.into_vec().into_iter().map(Into::into).collect(),
            indexes: value.indexes.into_iter().map(Into::into).collect(),
            table_type: value.table_type,
            table_access: value.table_access,
        }
    }
}

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
}

pub trait MutTxDatastore: TxDatastore + MutTx {
    // Tables
    fn create_table_mut_tx(&self, tx: &mut Self::MutTxId, schema: TableDef) -> Result<TableId>;
    fn row_type_for_table_mut_tx(&self, tx: &Self::MutTxId, table_id: TableId) -> Result<ProductType>;
    fn schema_for_table_mut_tx(&self, tx: &Self::MutTxId, table_id: TableId) -> Result<TableSchema>;
    fn drop_table_mut_tx(&self, tx: &mut Self::MutTxId, table_id: TableId) -> Result<()>;
    fn rename_table_mut_tx(&self, tx: &mut Self::MutTxId, table_id: TableId, new_name: SatsString) -> Result<()>;
    fn table_id_exists(&self, tx: &Self::MutTxId, table_id: &TableId) -> bool;
    fn table_id_from_name_mut_tx(&self, tx: &Self::MutTxId, table_name: SatsString) -> Result<Option<TableId>>;
    fn table_name_from_id_mut_tx(&self, tx: &Self::MutTxId, table_id: TableId) -> Result<Option<SatsString>>;
    fn get_all_tables_mut_tx(&self, tx: &Self::MutTxId) -> super::Result<Vec<TableSchema>> {
        let mut tables = Vec::new();
        let table_rows = self.iter_mut_tx(tx, TableId(ST_TABLES_ID))?.collect::<Vec<_>>();
        for data_ref in table_rows {
            let data = self.data_to_owned(data_ref);
            let row = StTableRow::try_from(data.view())?;
            let table_id = TableId(row.table_id);
            tables.push(self.schema_for_table_mut_tx(tx, table_id)?);
        }
        Ok(tables)
    }

    // Indexes
    fn create_index_mut_tx(&self, tx: &mut Self::MutTxId, index: IndexDef) -> Result<IndexId>;
    fn drop_index_mut_tx(&self, tx: &mut Self::MutTxId, index_id: IndexId) -> Result<()>;
    fn index_id_from_name_mut_tx(&self, tx: &Self::MutTxId, index_name: SatsString) -> super::Result<Option<IndexId>>;

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
        sequence_name: SatsString,
    ) -> super::Result<Option<SequenceId>>;

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
