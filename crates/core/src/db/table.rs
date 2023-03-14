//! Table Catalog
//!
//! Maintains a mirror of the tables stored in the database and their schema,
//! including the *internal system tables*.
//!
//! See [`Catalog`](crate::db::catalog::Catalog) documentation for more details.
//!
use std::collections::hash_map::Iter;
use std::collections::HashMap;

use crate::db::relational_db::{ST_COLUMNS_ID, ST_TABLES_ID};
use crate::db::TypeValue;
use crate::error::{DBError, TableError};
pub use spacetimedb_lib::ColumnIndexAttribute;
use spacetimedb_lib::{TupleValue, TypeDef};
use spacetimedb_sats::product_value::InvalidFieldError;
use spacetimedb_sats::relation::{DbTable, Header};
use spacetimedb_sats::{product, AlgebraicType, AlgebraicValue, ProductType, ProductTypeElement, ProductValue};
use spacetimedb_vm::dsl::scalar;

// WARNING: In order to keep a stable schema, don't change the discriminant of the fields
#[derive(Debug)]
pub enum TableFields {
    TableId = 0,
    TableName = 1,
}

impl TableFields {
    pub(crate) fn name(&self) -> &'static str {
        // WARNING: Don't change the name of the fields
        match self {
            Self::TableId => "table_id",
            Self::TableName => "table_name",
        }
    }
}

/// Extra fields generated for schema queries
#[derive(Debug)]
pub enum TableFieldsExtra {
    IsSystemTable,
}

impl TableFieldsExtra {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::IsSystemTable => "is_system_table",
        }
    }
}

// WARNING: In order to keep a stable schema, don't change the discriminant of the fields
#[derive(Debug)]
pub enum ColumnFields {
    TableId = 0,
    ColId = 1,
    ColType = 2,
    ColName = 3,
    ColIndexAttribute = 4,
}

impl ColumnFields {
    pub(crate) fn name(&self) -> &'static str {
        // WARNING: Don't change the name of the fields
        match self {
            Self::TableId => "table_id",
            Self::ColId => "col_id",
            Self::ColType => "col_type",
            Self::ColName => "col_name",
            Self::ColIndexAttribute => "col_idx_attr",
        }
    }
}

impl From<TableFields> for Option<&'static str> {
    fn from(x: TableFields) -> Self {
        Some(x.name())
    }
}

impl From<TableFields> for Option<String> {
    fn from(x: TableFields) -> Self {
        Some(x.name().into())
    }
}

impl From<ColumnFields> for Option<&'static str> {
    fn from(x: ColumnFields) -> Self {
        Some(x.name())
    }
}

impl From<ColumnFields> for Option<String> {
    fn from(x: ColumnFields) -> Self {
        Some(x.name().into())
    }
}

impl From<ColumnFields> for String {
    fn from(x: ColumnFields) -> Self {
        x.name().into()
    }
}

pub struct ColumnDef<'a> {
    pub column: &'a ProductTypeElement,
    pub attr: ColumnIndexAttribute,
}

/// Describe the column + meta attributes
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct ProductTypeMeta {
    pub columns: ProductType,
    pub attr: Vec<ColumnIndexAttribute>,
}

impl ProductTypeMeta {
    pub fn new(columns: ProductType) -> Self {
        Self {
            attr: vec![ColumnIndexAttribute::UnSet; columns.elements.len()],
            columns,
        }
    }

    pub fn push(&mut self, name: &str, ty: AlgebraicType, attr: ColumnIndexAttribute) {
        self.columns
            .elements
            .push(ProductTypeElement::new(ty, Some(name.to_string())));
        self.attr.push(attr);
    }

    pub fn with_attributes(iter: impl Iterator<Item = (ProductTypeElement, ColumnIndexAttribute)>) -> Self {
        let mut columns = Vec::new();
        let mut attrs = Vec::new();
        for (col, attr) in iter {
            columns.push(col);
            attrs.push(attr);
        }
        Self {
            attr: attrs,
            columns: ProductType::new(columns),
        }
    }

    pub fn len(&self) -> usize {
        self.columns.elements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.columns.elements.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = ColumnDef> {
        self.columns
            .elements
            .iter()
            .zip(self.attr.iter())
            .map(|(column, attr)| ColumnDef { column, attr: *attr })
    }

    pub fn with_defaults<'a>(
        &'a self,
        row: &'a mut ProductValue,
    ) -> impl Iterator<Item = (ColumnDef, &'a mut AlgebraicValue)> + 'a {
        self.iter()
            .zip(row.elements.iter_mut())
            .filter(|(col, _)| matches!(col.attr, ColumnIndexAttribute::Identity | ColumnIndexAttribute::AutoInc))
    }
}

impl From<ProductType> for ProductTypeMeta {
    fn from(value: ProductType) -> Self {
        ProductTypeMeta::new(value)
    }
}

impl From<ProductTypeMeta> for ProductType {
    fn from(value: ProductTypeMeta) -> Self {
        value.columns
    }
}

impl<'a> FromIterator<&'a (&'a str, AlgebraicType, ColumnIndexAttribute)> for ProductTypeMeta {
    fn from_iter<T: IntoIterator<Item = &'a (&'a str, AlgebraicType, ColumnIndexAttribute)>>(iter: T) -> Self {
        Self::with_attributes(
            iter.into_iter()
                .map(|(name, ty, attr)| (ProductTypeElement::new(ty.clone(), Some(name.to_string())), *attr)),
        )
    }
}

#[derive(Debug, Copy, Clone)]
pub struct TableRow<'a> {
    pub(crate) table_id: u32,
    pub(crate) table_name: &'a str,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct ColumnRow<'a> {
    pub(crate) table_id: u32,
    pub(crate) col_id: u32,
    pub(crate) col_name: &'a str,
    pub(crate) col_type: TypeDef,
    pub(crate) col_idx: ColumnIndexAttribute,
}

#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
#[allow(dead_code)]
pub struct TableDef {
    pub(crate) table_id: u32,
    pub(crate) name: String,
    pub(crate) columns: ProductTypeMeta,
    pub(crate) is_system_table: bool,
}

impl TableDef {
    pub fn new(table_id: u32, name: &str, columns: ProductTypeMeta, is_system_table: bool) -> Self {
        Self {
            table_id,
            name: name.into(),
            columns,
            is_system_table,
        }
    }
}

pub struct TableCatalog {
    tables: HashMap<u32, TableDef>,
}

impl TableCatalog {
    pub fn new() -> Self {
        Self {
            tables: Default::default(),
        }
    }

    /// Return the number of [TableDef] stored in the catalog
    pub fn len(&self) -> usize {
        self.tables.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tables.is_empty()
    }

    /// Return the number of columns stored in the catalog
    pub fn len_columns(&self) -> usize {
        self.tables.values().map(|x| x.columns.len()).sum()
    }

    pub fn insert(&mut self, schema: TableDef) {
        self.tables.insert(schema.table_id, schema);
    }

    /// Return the [TableDef] by their `table_id`
    pub fn get(&self, table_id: u32) -> Option<&TableDef> {
        self.tables.get(&table_id)
    }

    /// Return the [TableDef] by their `name`
    pub fn get_by_name(&self, named: &str) -> Option<&TableDef> {
        self.tables
            .iter()
            .find_map(|(_, table)| if table.name == named { Some(table) } else { None })
    }

    /// Return the `table_id` from the [TableDef]
    pub fn find_id_by_name(&mut self, named: &str) -> Option<u32> {
        self.tables
            .iter()
            .find_map(|(id, table)| if table.name == named { Some(*id) } else { None })
    }

    /// Return `true` if the [TableDef] is removed
    pub fn remove(&mut self, table_id: u32) -> bool {
        self.tables.remove(&table_id).is_some()
    }

    /// Returns the [TableDef] by their name
    pub fn find_name_for_id(&self, table_id: u32) -> Option<&str> {
        self.tables.get(&table_id).map(|x| x.name.as_str())
    }

    pub(crate) fn schema_table(&self) -> DbTable {
        DbTable::new(
            &Header::new(ProductType::from_iter([
                (TableFields::TableId.name(), TypeDef::U32),
                (TableFields::TableName.name(), TypeDef::String),
                (TableFieldsExtra::IsSystemTable.name(), TypeDef::Bool),
            ])),
            ST_TABLES_ID,
        )
    }

    pub(crate) fn schema_columns(&self) -> DbTable {
        DbTable::new(
            &Header::new(ProductType::from_iter([
                (ColumnFields::TableId.name(), TypeDef::U32),
                (ColumnFields::ColId.name(), TypeDef::U32),
                (ColumnFields::ColName.name(), TypeDef::String),
                (ColumnFields::ColType.name(), TypeDef::make_meta_type()),
                (ColumnFields::ColIndexAttribute.name(), TypeDef::String),
            ])),
            ST_COLUMNS_ID,
        )
    }

    /// Returns a [Iterator] across all the tables
    pub fn iter(&self) -> Iter<'_, u32, TableDef> {
        self.tables.iter()
    }

    pub fn make_row_table(table_id: u32, table_name: &str, is_system_table: bool) -> ProductValue {
        product!(scalar(table_id), scalar(table_name), scalar(is_system_table))
    }

    pub fn make_row_column(
        table_id: u32,
        col_id: u32,
        col_name: &str,
        col_type: &AlgebraicType,
        index_attr: ColumnIndexAttribute,
    ) -> ProductValue {
        product!(
            scalar(table_id),
            scalar(col_id),
            scalar(col_name),
            //AlgebraicType::bytes().as_value(),
            col_type.as_value(),
            index_attr as u8,
        )
    }

    /// Returns a [Iterator] than map [TableDef] to [ProductValue]
    pub fn iter_row(&self) -> impl Iterator<Item = ProductValue> + '_ {
        self.tables
            .values()
            .map(|row| Self::make_row_table(row.table_id, &row.name, row.is_system_table))
    }

    /// Returns a [Iterator] than map [ProductTypeMeta] (aka: table columns) to [ProductValue]
    pub fn iter_columns_row(&self) -> impl Iterator<Item = ProductValue> + '_ {
        self.tables.values().flat_map(|row| {
            let mut cols = Vec::with_capacity(row.columns.len());
            for (pos, col) in row.columns.iter().enumerate() {
                cols.push(Self::make_row_column(
                    row.table_id,
                    pos as u32,
                    col.column.name.as_deref().unwrap_or_default(),
                    &col.column.algebraic_type,
                    col.attr,
                ));
            }
            cols
        })
    }

    /// Returns a [Iterator] for the system tables
    pub fn iter_system_tables(&self) -> impl Iterator<Item = (u32, &TableDef)> {
        self.iter().filter_map(|(table_id, schema)| {
            if schema.is_system_table {
                Some((*table_id, schema))
            } else {
                None
            }
        })
    }

    /// Returns a [Iterator] for the user tables
    pub fn iter_user_tables(&self) -> impl Iterator<Item = (u32, &TableDef)> {
        self.iter().filter_map(|(table_id, schema)| {
            if !schema.is_system_table {
                Some((*table_id, schema))
            } else {
                None
            }
        })
    }
}

impl Default for TableCatalog {
    fn default() -> Self {
        Self::new()
    }
}

pub fn decode_st_table_schema(row: &TupleValue) -> Result<TableRow, DBError> {
    let table_id = row.field_as_u32(TableFields::TableId as usize, TableFields::TableId.into())?;
    let table_name = row.field_as_str(TableFields::TableName as usize, TableFields::TableName.into())?;

    Ok(TableRow { table_id, table_name })
}

pub fn decode_st_columns_schema(row: &TupleValue) -> Result<ColumnRow, DBError> {
    let table_id = row.field_as_u32(ColumnFields::TableId as usize, ColumnFields::TableId.into())?;
    let col_id = row.field_as_u32(ColumnFields::ColId as usize, ColumnFields::ColId.into())?;

    let bytes = row.field_as_bytes(ColumnFields::ColType as usize, ColumnFields::ColType.into())?;
    let col_type = TypeDef::decode(&mut &bytes[..]).map_err(|e| TableError::InvalidSchema(table_id, e.into()))?;

    let col_name = row.field_as_str(ColumnFields::ColName as usize, ColumnFields::ColName.into())?;
    let col_idx = row
        .field_as_u8(
            ColumnFields::ColIndexAttribute as usize,
            ColumnFields::ColIndexAttribute.into(),
        )?
        .try_into()
        .map_err(|()| {
            InvalidFieldError(
                ColumnFields::ColIndexAttribute as usize,
                ColumnFields::ColIndexAttribute.into(),
            )
        })?;

    Ok(ColumnRow {
        table_id,
        col_id,
        col_name,
        col_type,
        col_idx,
    })
}

/// System Table [ST_COLUMNS_NAME]
///
/// | table_id: u32 | col_id | col_type: Bytes | col_name: String | col_idx_attr: u8     |
/// |---------------|--------|-----------------|------------------|----------------------|
/// | 1             | 0      | TypeDef->0b0101 | "id"             | 0                    |
pub(crate) fn st_columns_schema() -> ProductTypeMeta {
    ProductTypeMeta::new(ProductType::from_iter([
        (ColumnFields::TableId.name(), TypeDef::U32),
        (ColumnFields::ColId.name(), TypeDef::U32),
        (ColumnFields::ColType.name(), TypeDef::bytes()),
        (ColumnFields::ColName.name(), TypeDef::String),
        (ColumnFields::ColIndexAttribute.name(), TypeDef::U8),
    ]))
}

/// System Table [ST_TABLES_NAME]
///
/// | table_id | table_name     |
/// |----------|----------------|
/// | 1        | "customers"    |
pub(crate) fn st_table_schema() -> ProductTypeMeta {
    ProductTypeMeta::new(ProductType::from_iter([
        (TableFields::TableId.name(), TypeDef::U32),
        (TableFields::TableName.name(), TypeDef::String),
    ]))
}

pub(crate) fn row_st_table(table_id: u32, table_name: &str) -> ProductValue {
    product![TypeValue::U32(table_id), TypeValue::String(table_name.to_string()),]
}

pub(crate) fn row_st_columns(
    table_id: u32,
    pos: u32,
    col_name: &str,
    ty: &AlgebraicType,
    attribute: ColumnIndexAttribute,
) -> ProductValue {
    let mut bytes = Vec::new();
    ty.encode(&mut bytes);

    product![
        TypeValue::U32(table_id),
        TypeValue::U32(pos),
        TypeValue::Bytes(bytes),
        TypeValue::String(col_name.into()),
        TypeValue::U8(attribute as u8),
    ]
}
