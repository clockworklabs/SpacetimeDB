//! Table Catalog
//!
//! Maintains a mirror of the tables stored in the database and their schema,
//! including the *internal system tables*.
//!
//! See [`Catalog`](crate::db::catalog::Catalog) documentation for more details.
//!
use crate::error::{DBError, TableError};
use spacetimedb_lib::{TupleDef, TupleValue, TypeDef};
use std::collections::hash_map::Iter;
use std::collections::HashMap;

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

// WARNING: In order to keep a stable schema, don't change the discriminant of the fields
#[derive(Debug)]
pub enum ColumnFields {
    TableId = 0,
    ColId = 1,
    ColType = 2,
    ColName = 3,
}

impl ColumnFields {
    pub(crate) fn name(&self) -> &'static str {
        // WARNING: Don't change the name of the fields
        match self {
            Self::TableId => "table_id",
            Self::ColId => "col_id",
            Self::ColType => "col_type",
            Self::ColName => "col_name",
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

#[derive(Debug, Copy, Clone)]
pub struct TableRow<'a> {
    pub(crate) table_id: u32,
    pub(crate) table_name: &'a str,
}

#[derive(Debug)]
pub struct ColumnRow<'a> {
    pub(crate) table_id: u32,
    pub(crate) col_id: u32,
    pub(crate) col_name: &'a str,
    pub(crate) col_type: TypeDef,
}

pub struct TableDef {
    pub(crate) table_id: u32,
    pub(crate) name: String,
    // TODO: This is required for the next pr that joins all the pieces of the catalog
    #[allow(dead_code)]
    pub(crate) schema: TupleDef,
    pub(crate) is_system_table: bool,
}

impl TableDef {
    pub fn new(table_id: u32, name: &str, schema: TupleDef, is_system_table: bool) -> Self {
        Self {
            table_id,
            name: name.into(),
            schema,
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

    pub fn insert(&mut self, schema: TableDef) {
        self.tables.insert(schema.table_id, schema);
    }

    /// Return the [TableDef] by their `table_id`
    pub fn get(&mut self, table_id: u32) -> Option<&TableDef> {
        self.tables.get(&table_id)
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

    /// Returns a [Iterator] across all the tables
    pub fn iter(&self) -> Iter<'_, u32, TableDef> {
        self.tables.iter()
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

/// System Table [ST_TABLES_NAME]
///
/// | table_id | table_name     |
/// |----------|----------------|
/// | 1        | "customers"    |
pub(crate) fn table_schema() -> TupleDef {
    TupleDef::from_iter([
        (TableFields::TableId.name(), TypeDef::U32),
        (TableFields::TableName.name(), TypeDef::String),
    ])
}

impl Default for TableCatalog {
    fn default() -> Self {
        Self::new()
    }
}

/// System Table [ST_COLUMNS_NAME]
///
/// | table_id: u32 | col_id | col_type: Bytes | col_name: String |
/// |---------------|--------|-----------------|------------------|
/// | 1             | 0      | TypeDef->0b0101 | "id"             |
pub(crate) fn columns_schema() -> TupleDef {
    TupleDef::from_iter([
        (ColumnFields::TableId.name(), TypeDef::U32),
        (ColumnFields::ColId.name(), TypeDef::U32),
        (ColumnFields::ColType.name(), TypeDef::bytes()),
        (ColumnFields::ColName.name(), TypeDef::String),
    ])
}

pub fn decode_table_schema(row: &TupleValue) -> Result<TableRow, DBError> {
    let table_id = row.field_as_u32(TableFields::TableId as usize, TableFields::TableId.into())?;
    let table_name = row.field_as_str(TableFields::TableName as usize, TableFields::TableName.into())?;

    Ok(TableRow { table_id, table_name })
}

pub fn decode_columns_schema(row: &TupleValue) -> Result<ColumnRow, DBError> {
    let table_id = row.field_as_u32(ColumnFields::TableId as usize, ColumnFields::TableId.into())?;
    let col_id = row.field_as_u32(ColumnFields::ColId as usize, ColumnFields::ColId.into())?;

    let bytes = row.field_as_bytes(ColumnFields::ColType as usize, ColumnFields::ColType.into())?;
    let col_type = TypeDef::decode(&mut &bytes[..]).map_err(|e| TableError::InvalidSchema(table_id, e.into()))?;

    let col_name = row.field_as_str(ColumnFields::ColName as usize, ColumnFields::ColName.into())?;

    Ok(ColumnRow {
        table_id,
        col_id,
        col_name,
        col_type,
    })
}
