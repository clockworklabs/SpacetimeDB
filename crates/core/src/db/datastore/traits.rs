use crate::db::relational_db::ST_TABLES_ID;
use crate::execution_context::ExecutionContext;
use anyhow::Context;
use nonempty::NonEmpty;
use spacetimedb_lib::auth::{StAccess, StTableType};
use spacetimedb_lib::relation::{Column, DbTable, FieldName, FieldOnly, Header, TableField};
use spacetimedb_lib::{ColumnIndexAttribute, DataKey, Hash, IndexType};
use spacetimedb_primitives::{ColId, IndexId, SequenceId, TableId};
use spacetimedb_sats::product_value::InvalidFieldError;
use spacetimedb_sats::{AlgebraicType, AlgebraicValue, ProductType, ProductTypeElement, ProductValue, WithTypespace};
use spacetimedb_vm::expr::SourceExpr;
use std::iter;
use std::{borrow::Cow, ops::RangeBounds, sync::Arc};

use super::{system_tables::StTableRow, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceSchema {
    pub(crate) sequence_id: SequenceId,
    pub(crate) sequence_name: String,
    pub(crate) table_id: TableId,
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
    pub(crate) sequence_name: String,
    pub(crate) table_id: TableId,
    pub(crate) col_id: ColId,
    pub(crate) increment: i128,
    pub(crate) start: Option<i128>,
    pub(crate) min_value: Option<i128>,
    pub(crate) max_value: Option<i128>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexSchema {
    pub(crate) index_id: IndexId,
    pub(crate) table_id: TableId,
    pub(crate) index_name: String,
    pub(crate) is_unique: bool,
    pub(crate) cols: NonEmpty<ColId>,
    pub(crate) index_type: IndexType,
}

/// This type is just the [IndexSchema] without the autoinc fields
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexDef {
    pub(crate) table_id: TableId,
    pub(crate) cols: NonEmpty<ColId>,
    pub(crate) name: String,
    pub(crate) is_unique: bool,
    pub(crate) index_type: IndexType,
}

impl IndexDef {
    pub fn new(name: String, table_id: TableId, col_id: ColId, is_unique: bool) -> Self {
        Self {
            cols: NonEmpty::new(col_id),
            name,
            is_unique,
            table_id,
            index_type: IndexType::BTree,
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
            index_type: value.index_type,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnSchema {
    pub table_id: TableId,
    pub col_id: ColId,
    pub col_name: String,
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
    pub(crate) col_name: String,
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
    pub(crate) constraint_id: IndexId,
    pub(crate) constraint_name: String,
    pub(crate) kind: ColumnIndexAttribute,
    pub(crate) table_id: TableId,
    pub(crate) columns: NonEmpty<ColId>,
}

/// This type is just the [ConstraintSchema] without the autoinc fields
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstraintDef {
    pub(crate) constraint_name: String,
    pub(crate) kind: ColumnIndexAttribute,
    pub(crate) table_id: TableId,
    pub(crate) columns: Vec<ColId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSchema {
    pub table_id: TableId,
    pub table_name: String,
    pub columns: Vec<ColumnSchema>,
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
        self.indexes.iter().find(
            |IndexSchema {
                 cols: NonEmpty { head: index_col, tail },
                 ..
             }| tail.is_empty() && index_col == col_id,
        )
    }

    pub fn get_column(&self, pos: usize) -> Option<&ColumnSchema> {
        self.columns.get(pos)
    }

    /// Check if the `col_name` exist on this [TableSchema]
    ///
    /// Warning: It ignores the `table_name`
    pub fn get_column_by_name(&self, col_name: &str) -> Option<&ColumnSchema> {
        self.columns.iter().find(|x| x.col_name == col_name)
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
                        col_pos: pos.into(),
                        name: None,
                    }
                    .into(),
                )
            })
            .collect()
    }

    /// Utility for project the fields from the supplied `columns` that is a [NonEmpty<u32>],
    /// used for when the list of field columns have at least one value.
    pub fn project_not_empty(&self, columns: &NonEmpty<ColId>) -> Result<Vec<&ColumnSchema>> {
        self.project(columns.iter().map(|&x| x.idx()))
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
                .collect(),
        )
    }
}

impl From<&TableSchema> for SourceExpr {
    fn from(value: &TableSchema) -> Self {
        SourceExpr::DbTable(DbTable::new(
            value.into(),
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
        Header::new(
            value.table_name.clone(),
            value
                .columns
                .iter()
                .map(|x| {
                    let field = FieldName::named(&value.table_name, &x.col_name);
                    let is_indexed = value.get_index_by_field(&field).is_some();

                    Column::new(field, x.col_type.clone(), x.col_id, is_indexed)
                })
                .collect(),
        )
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
                .collect(),
        )
    }
}

/// The magic table id zero, for use in [`IndexDef`]s.
///
/// The actual table id is usually not yet known when constructing an
/// [`IndexDef`]. [`AUTO_TABLE_ID`] can be used instead, which the storage
/// engine will replace with the actual table id upon creation of the table
/// respectively index.
pub const AUTO_TABLE_ID: TableId = TableId(0);

/// This type is just the [TableSchema] without the autoinc fields
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableDef {
    pub(crate) table_name: String,
    pub(crate) columns: Vec<ColumnDef>,
    pub(crate) indexes: Vec<IndexDef>,
    pub(crate) table_type: StTableType,
    pub(crate) table_access: StAccess,
}

impl TableDef {
    pub fn from_lib_tabledef(table: WithTypespace<'_, spacetimedb_lib::TableDef>) -> anyhow::Result<Self> {
        let schema = table
            .map(|t| &t.data)
            .resolve_refs()
            .context("recursive types not yet supported")?;
        let schema = schema.into_product().ok().context("table not a product type?")?;
        let table = table.ty();
        anyhow::ensure!(
            table.column_attrs.len() == schema.elements.len(),
            "mismatched number of columns"
        );

        // Build single-column index definitions, determining `is_unique` from
        // their respective column attributes.
        let mut columns = Vec::with_capacity(schema.elements.len());
        let mut indexes = Vec::new();
        for (col_id, (ty, col_attr)) in std::iter::zip(&schema.elements, &table.column_attrs).enumerate() {
            let col = ColumnDef {
                col_name: ty.name.clone().context("column without name")?,
                col_type: ty.algebraic_type.clone(),
                is_autoinc: col_attr.has_autoinc(),
            };

            let index_for_column = table.indexes.iter().find(|index| {
                // Ignore multi-column indexes
                matches!(*index.cols, [index_col_id] if index_col_id as usize == col_id)
            });

            // If there's an index defined for this column already, use it,
            // making sure that it is unique if the column has a unique constraint
            let index_info = if let Some(index) = index_for_column {
                Some((index.name.clone(), index.index_type))
            } else if col_attr.has_unique() {
                // If you didn't find an index, but the column is unique then create a unique btree index
                // anyway.
                Some((
                    format!("{}_{}_unique", table.name, col.col_name),
                    spacetimedb_lib::IndexType::BTree,
                ))
            } else {
                None
            };
            if let Some((name, ty)) = index_info {
                match ty {
                    spacetimedb_lib::IndexType::BTree => {}
                    // TODO
                    spacetimedb_lib::IndexType::Hash => anyhow::bail!("hash indexes not yet supported"),
                }
                indexes.push(IndexDef::new(
                    name,
                    AUTO_TABLE_ID,
                    ColId(col_id as u32),
                    col_attr.has_unique(),
                ))
            }
            columns.push(col);
        }

        // Multi-column indexes cannot be unique (yet), so just add them.
        let multi_col_indexes = table.indexes.iter().filter_map(|index| {
            if let [a, b, rest @ ..] = &*index.cols {
                Some(IndexDef {
                    table_id: AUTO_TABLE_ID,
                    cols: NonEmpty {
                        head: ColId::from(*a),
                        tail: iter::once(ColId::from(*b))
                            .chain(rest.iter().copied().map(Into::into))
                            .collect(),
                    },
                    name: index.name.clone(),
                    is_unique: false,
                    index_type: IndexType::BTree,
                })
            } else {
                None
            }
        });
        indexes.extend(multi_col_indexes);

        Ok(TableDef {
            table_name: table.name.clone(),
            columns,
            indexes,
            table_type: table.table_type,
            table_access: table.table_access,
        })
    }
}

impl From<ProductType> for TableDef {
    fn from(value: ProductType) -> Self {
        Self {
            table_name: "".to_string(),
            columns: value
                .elements
                .iter()
                .enumerate()
                .map(|(i, e)| ColumnDef {
                    col_name: e.name.to_owned().unwrap_or_else(|| i.to_string()),
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

impl From<&TableSchema> for TableDef {
    fn from(value: &TableSchema) -> Self {
        Self {
            table_name: value.table_name.clone(),
            columns: value.columns.iter().cloned().map(Into::into).collect(),
            indexes: value.indexes.iter().cloned().map(Into::into).collect(),
            table_type: value.table_type,
            table_access: value.table_access,
        }
    }
}

impl From<TableSchema> for TableDef {
    fn from(value: TableSchema) -> Self {
        Self {
            table_name: value.table_name,
            columns: value.columns.into_iter().map(Into::into).collect(),
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
    use nonempty::NonEmpty;
    use spacetimedb_lib::{
        auth::{StAccess, StTableType},
        ColumnIndexAttribute, IndexType,
    };
    use spacetimedb_primitives::ColId;
    use spacetimedb_sats::{AlgebraicType, AlgebraicTypeRef, ProductType, ProductTypeElement, Typespace};

    use super::{ColumnDef, IndexDef, TableDef, AUTO_TABLE_ID};

    #[test]
    fn test_tabledef_from_lib_tabledef() -> anyhow::Result<()> {
        let lib_table_def = spacetimedb_lib::TableDef {
            name: "Person".into(),
            data: AlgebraicTypeRef(0),
            column_attrs: vec![ColumnIndexAttribute::IDENTITY, ColumnIndexAttribute::UNSET],
            indexes: vec![
                spacetimedb_lib::IndexDef {
                    name: "id_and_name".into(),
                    index_type: spacetimedb_lib::IndexType::BTree,
                    cols: [0, 1].into(),
                },
                spacetimedb_lib::IndexDef {
                    name: "just_name".into(),
                    index_type: spacetimedb_lib::IndexType::BTree,
                    cols: [1].into(),
                },
            ],
            table_type: StTableType::User,
            table_access: StAccess::Public,
        };
        let row_type = ProductType::new(vec![
            ProductTypeElement {
                name: Some("id".into()),
                algebraic_type: AlgebraicType::U32,
            },
            ProductTypeElement {
                name: Some("name".into()),
                algebraic_type: AlgebraicType::String,
            },
        ]);

        let mut datastore_schema =
            TableDef::from_lib_tabledef(Typespace::new(vec![row_type.into()]).with_type(&lib_table_def))?;
        let mut expected_schema = TableDef {
            table_name: "Person".into(),
            columns: vec![
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
            ],
            indexes: vec![
                IndexDef {
                    table_id: AUTO_TABLE_ID,
                    cols: ColId(0).into(),
                    name: "Person_id_unique".into(),
                    is_unique: true,
                    index_type: IndexType::BTree,
                },
                IndexDef {
                    table_id: AUTO_TABLE_ID,
                    cols: NonEmpty {
                        head: ColId(0),
                        tail: [ColId(1)].into(),
                    },
                    name: "id_and_name".into(),
                    is_unique: false,
                    index_type: IndexType::BTree,
                },
                IndexDef {
                    table_id: AUTO_TABLE_ID,
                    cols: ColId(1).into(),
                    name: "just_name".into(),
                    is_unique: false,
                    index_type: IndexType::BTree,
                },
            ],
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
