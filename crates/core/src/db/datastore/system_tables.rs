use crate::error::{DBError, TableError};
use core::fmt;
use nonempty::NonEmpty;
use once_cell::sync::Lazy;

use spacetimedb_sats::db::auth::{StAccess, StTableType};
use spacetimedb_sats::db::def::*;
use spacetimedb_sats::hash::Hash;
use spacetimedb_sats::product_value::InvalidFieldError;
use spacetimedb_sats::{
    impl_deserialize, impl_serialize, product, AlgebraicType, AlgebraicValue, ArrayValue, ProductType, ProductValue,
};
use strum::Display;

/// The static ID of the table that defines tables
pub(crate) const ST_TABLES_ID: TableId = TableId(0);
/// The static ID of the table that defines columns
pub(crate) const ST_COLUMNS_ID: TableId = TableId(1);
/// The static ID of the table that defines sequences
pub(crate) const ST_SEQUENCES_ID: TableId = TableId(2);
/// The static ID of the table that defines indexes
pub(crate) const ST_INDEXES_ID: TableId = TableId(3);
/// The static ID of the table that defines constraints
pub(crate) const ST_CONSTRAINTS_ID: TableId = TableId(4);
/// The static ID of the table that defines the stdb module associated with
/// the database
pub(crate) const ST_MODULE_ID: TableId = TableId(5);

pub(crate) const ST_TABLES_NAME: &str = "st_table";
pub(crate) const ST_COLUMNS_NAME: &str = "st_columns";
pub(crate) const ST_SEQUENCES_NAME: &str = "st_sequence";
pub(crate) const ST_INDEXES_NAME: &str = "st_indexes";
pub(crate) const ST_CONSTRAINTS_NAME: &str = "st_constraints";
pub(crate) const ST_MODULE_NAME: &str = "st_module";

macro_rules! system_field {
    ($name:ident) => {
        impl SystemField for $name {
            fn to_field_id(&self) -> u32 {
                *self as u32
            }

            fn to_field_name(&self) -> String {
                self.name().to_string()
            }

            fn to_field_name_str(&self) -> &'static str {
                self.name()
            }
        }
    };
}

#[allow(non_camel_case_types)]
#[derive(Debug, Display)]
pub enum SystemTable {
    st_table,
    st_columns,
    st_sequence,
    st_indexes,
    st_constraints,
}
pub(crate) fn system_tables() -> [TableSchema; 6] {
    [
        st_table_schema(),
        st_columns_schema(),
        st_indexes_schema(),
        st_constraints_schema(),
        st_module_schema(),
        // Is important this is always last, so the starting sequence is correct
        st_sequences_schema(),
    ]
}

// WARNING: In order to keep a stable schema, don't change the discriminant of the fields
#[derive(Debug, Copy, Clone)]
pub enum StTableFields {
    TableId = 0,
    TableName = 1,
    TableType = 2,
    TablesAccess = 3,
}

impl StTableFields {
    pub fn name(&self) -> &'static str {
        // WARNING: Don't change the name of the fields
        match self {
            Self::TableId => "table_id",
            Self::TableName => "table_name",
            Self::TableType => "table_type",
            Self::TablesAccess => "table_access",
        }
    }
}

system_field!(StTableFields);

// WARNING: In order to keep a stable schema, don't change the discriminant of the fields
#[derive(Debug, Copy, Clone)]
pub enum StColumnFields {
    TableId = 0,
    ColPos = 1,
    ColType = 2,
    ColName = 3,
}

impl StColumnFields {
    pub fn name(&self) -> &'static str {
        // WARNING: Don't change the name of the fields
        match self {
            Self::TableId => "table_id",
            Self::ColPos => "col_pos",
            Self::ColType => "col_type",
            Self::ColName => "col_name",
        }
    }
}

system_field!(StColumnFields);

// WARNING: In order to keep a stable schema, don't change the discriminant of the fields
#[derive(Debug, Copy, Clone)]
pub enum StIndexFields {
    IndexId = 0,
    TableId = 1,
    IndexType = 2,
    IndexName = 3,
    Columns = 4,
    IsUnique = 5,
}

impl StIndexFields {
    pub fn name(&self) -> &'static str {
        // WARNING: Not change the field names
        match self {
            StIndexFields::IndexId => "index_id",
            StIndexFields::TableId => "table_id",
            StIndexFields::IndexType => "index_type",
            StIndexFields::IndexName => "index_name",
            StIndexFields::Columns => "columns",
            StIndexFields::IsUnique => "is_unique",
        }
    }
}

system_field!(StIndexFields);

// WARNING: In order to keep a stable schema, don't change the discriminant of the fields
/// The fields that define the internal table [crate::db::relational_db::ST_SEQUENCES_NAME].
#[derive(Debug, Copy, Clone)]
pub enum StSequenceFields {
    SequenceId = 0,
    SequenceName = 1,
    TableId = 2,
    ColId = 3,
    Increment = 4,
    Start = 5,
    MinValue = 6,
    MaxValue = 7,
    Allocated = 8,
}

impl StSequenceFields {
    pub fn name(&self) -> &'static str {
        match self {
            StSequenceFields::SequenceId => "sequence_id",
            StSequenceFields::SequenceName => "sequence_name",
            StSequenceFields::TableId => "table_id",
            StSequenceFields::ColId => "col_id",
            StSequenceFields::Increment => "increment",
            StSequenceFields::Start => "start",
            StSequenceFields::MinValue => "min_value",
            StSequenceFields::MaxValue => "max_value",
            StSequenceFields::Allocated => "allocated",
        }
    }
}

system_field!(StSequenceFields);

// WARNING: In order to keep a stable schema, don't change the discriminant of the fields
#[derive(Debug, Copy, Clone)]
pub enum StConstraintFields {
    ConstraintId = 0,
    ConstraintName = 1,
    Kind = 2,
    TableId = 3,
    Columns = 4,
}

impl StConstraintFields {
    pub fn name(&self) -> &'static str {
        // WARNING: Don't change the name of the fields
        match self {
            Self::ConstraintId => "constraint_id",
            Self::ConstraintName => "constraint_name",
            Self::Kind => "kind",
            Self::TableId => "table_id",
            Self::Columns => "columns",
        }
    }
}

system_field!(StConstraintFields);

// WARNING: In order to keep a stable schema, don't change the discriminant of the fields
#[derive(Debug, Copy, Clone)]
pub enum StModuleFields {
    ProgramHash = 0,
    Kind = 1,
    Epoch = 2,
}

impl StModuleFields {
    pub fn name(&self) -> &'static str {
        match self {
            Self::ProgramHash => "program_hash",
            Self::Kind => "kind",
            Self::Epoch => "epoch",
        }
    }
}

system_field!(StModuleFields);

/// System Table [ST_TABLES_NAME]
///
/// | table_id: u32 | table_name: String | table_type: String | table_access: String |
/// |---------------|--------------------| ------------------ | -------------------- |
/// | 4             | "customers"        | "user"             | "public"             |
pub fn st_table_schema() -> TableSchema {
    TableDef::new(
        ST_TABLES_NAME,
        &[
            ColumnDef::sys(StTableFields::TableId, AlgebraicType::U32),
            ColumnDef::sys(StTableFields::TableName, AlgebraicType::String),
            ColumnDef::sys(StTableFields::TableType, AlgebraicType::String),
            ColumnDef::sys(StTableFields::TablesAccess, AlgebraicType::String),
        ],
    )
    .with_type(StTableType::System)
    .with_constraints(&[ConstraintDef::for_sys_column(
        ST_TABLES_NAME,
        StTableFields::TableId,
        Constraints::primary_key_auto(),
    )])
    .with_indexes(&[IndexDef::for_sys_column(ST_TABLES_NAME, StTableFields::TableName, true)])
    .into_schema(ST_TABLES_ID)
}

pub static ST_TABLE_ROW_TYPE: Lazy<ProductType> =
    Lazy::new(|| ProductType::from_iter(st_table_schema().columns.iter().map(|c| c.col_type.clone())));

/// System Table [ST_COLUMNS_NAME]
///
/// | table_id: u32 | col_id | col_type: Bytes       | col_name: String | is_autoinc: bool |
/// |---------------|--------|-----------------------|------------------|------------------|
/// | 1             | 0      | AlgebraicType->0b0101 | "id"             | true             |
pub fn st_columns_schema() -> TableSchema {
    TableDef::new(
        ST_COLUMNS_NAME,
        &[
            ColumnDef::sys(StColumnFields::TableId, AlgebraicType::U32),
            ColumnDef::sys(StColumnFields::ColPos, AlgebraicType::U32),
            ColumnDef::sys(StColumnFields::ColType, AlgebraicType::bytes()),
            ColumnDef::sys(StColumnFields::ColName, AlgebraicType::String),
        ],
    )
    .with_type(StTableType::System)
    .with_constraints(&[ConstraintDef::for_column(
        ST_COLUMNS_NAME,
        &format!("{}_{}", StColumnFields::TableId.name(), StColumnFields::ColPos.name()),
        Constraints::unique(),
        NonEmpty::from_slice(&[StColumnFields::TableId.into(), StColumnFields::ColPos.into()]).unwrap(),
    )])
    .into_schema(ST_COLUMNS_ID)
}

pub static ST_COLUMNS_ROW_TYPE: Lazy<ProductType> =
    Lazy::new(|| ProductType::from_iter(st_columns_schema().columns.iter().map(|c| c.col_type.clone())));

/// System Table [ST_INDEXES]
///
/// | index_id: u32 | table_id: u32 | index_name: String | index_type: String    | columns: NonEmpty<u32> | is_unique: bool      |
/// |---------------|---------------|--------------------|-----------------------|------------------------|----------------------|
/// | 1             | 1             | "ix_sample"        | "Btree"               | [1]                    | 0                    |
pub fn st_indexes_schema() -> TableSchema {
    TableDef::new(
        ST_INDEXES_NAME,
        &[
            ColumnDef::sys(StIndexFields::IndexId, AlgebraicType::U32),
            ColumnDef::sys(StIndexFields::TableId, AlgebraicType::U32),
            ColumnDef::sys(StIndexFields::IndexType, AlgebraicType::String),
            ColumnDef::sys(StIndexFields::IndexName, AlgebraicType::String),
            ColumnDef::sys(StIndexFields::Columns, AlgebraicType::array(AlgebraicType::U32)),
            ColumnDef::sys(StIndexFields::IsUnique, AlgebraicType::Bool),
        ],
    )
    .with_type(StTableType::System)
    // TODO: Unique constraint on index name?
    .with_constraints(&[ConstraintDef::for_sys_column(
        ST_INDEXES_NAME,
        StIndexFields::IndexId,
        Constraints::primary_key_auto(),
    )])
    .into_schema(ST_INDEXES_ID)
}

pub static ST_INDEX_ROW_TYPE: Lazy<ProductType> =
    Lazy::new(|| ProductType::from_iter(st_indexes_schema().columns.iter().map(|c| c.col_type.clone())));

/// System Table [ST_SEQUENCES]
///
/// | sequence_id | sequence_name     | increment | start | min_value | max_value | table_id | col_id | allocated |
/// |-------------|-------------------|-----------|-------|-----------|-----------|----------|--------|-----------|
/// | 1           | "seq_customer_id" | 1         | 100   | 10        | 1200      | 1        | 1      | 200       |
pub(crate) fn st_sequences_schema() -> TableSchema {
    TableDef::new(
        ST_SEQUENCES_NAME,
        &[
            ColumnDef::sys(StSequenceFields::SequenceId, AlgebraicType::U32),
            ColumnDef::sys(StSequenceFields::SequenceName, AlgebraicType::String),
            ColumnDef::sys(StSequenceFields::TableId, AlgebraicType::U32),
            ColumnDef::sys(StSequenceFields::ColId, AlgebraicType::U32),
            ColumnDef::sys(StSequenceFields::Increment, AlgebraicType::I128),
            ColumnDef::sys(StSequenceFields::Start, AlgebraicType::I128),
            ColumnDef::sys(StSequenceFields::MinValue, AlgebraicType::I128),
            ColumnDef::sys(StSequenceFields::MaxValue, AlgebraicType::I128),
            ColumnDef::sys(StSequenceFields::Allocated, AlgebraicType::I128),
        ],
    )
    .with_type(StTableType::System)
    // TODO: Unique constraint on sequence name?
    .with_constraints(&[ConstraintDef::for_sys_column(
        ST_SEQUENCES_NAME,
        StSequenceFields::SequenceId,
        Constraints::primary_key_auto(),
    )])
    .into_schema(ST_SEQUENCES_ID)
}

pub static ST_SEQUENCE_ROW_TYPE: Lazy<ProductType> =
    Lazy::new(|| ProductType::from_iter(st_sequences_schema().columns.iter().map(|c| c.col_type.clone())));

/// System Table [ST_CONSTRAINTS_NAME]
///
/// | constraint_id | constraint_name      | kind | table_id | columns |
/// |---------------|-------------------- -|-----------|-------|-----------|
/// | 1             | "unique_customer_id" | 1         | 100   | [1, 4]        |
pub(crate) fn st_constraints_schema() -> TableSchema {
    TableDef::new(
        ST_CONSTRAINTS_NAME,
        &[
            ColumnDef::sys(StConstraintFields::ConstraintId, AlgebraicType::U32),
            ColumnDef::sys(StConstraintFields::ConstraintName, AlgebraicType::String),
            ColumnDef::sys(StConstraintFields::Kind, AlgebraicType::U32),
            ColumnDef::sys(StConstraintFields::TableId, AlgebraicType::U32),
            ColumnDef::sys(StConstraintFields::Columns, AlgebraicType::array(AlgebraicType::U32)),
        ],
    )
    .with_type(StTableType::System)
    .with_constraints(&[ConstraintDef::for_sys_column(
        ST_CONSTRAINTS_NAME,
        StConstraintFields::ConstraintId,
        Constraints::primary_key_auto(),
    )])
    .into_schema(ST_CONSTRAINTS_ID)
}

pub static ST_CONSTRAINT_ROW_TYPE: Lazy<ProductType> =
    Lazy::new(|| ProductType::from_iter(st_constraints_schema().columns.iter().map(|c| c.col_type.clone())));

/// System table [ST_MODULE_NAME]
///
/// This table holds exactly one row, describing the latest version of the
/// SpacetimeDB module associated with the database:
///
/// * `program_hash` is the [`Hash`] of the raw bytes of the (compiled) module.
/// * `kind` is the [`ModuleKind`] (currently always [`WASM_MODULE`]).
/// * `epoch` is a _fencing token_ used to protect against concurrent updates.
///
/// | program_hash        | kind     | epoch |
/// |---------------------|----------|-------|
/// | [250, 207, 5, ...]  | 0        | 42    |
pub(crate) fn st_module_schema() -> TableSchema {
    TableDef::new(
        ST_MODULE_NAME,
        &[
            ColumnDef::sys(StModuleFields::ProgramHash, AlgebraicType::array(AlgebraicType::U8)),
            ColumnDef::sys(StModuleFields::Kind, AlgebraicType::U8),
            ColumnDef::sys(StModuleFields::Epoch, AlgebraicType::U128),
        ],
    )
    .with_type(StTableType::System)
    .into_schema(ST_MODULE_ID)
}

pub static ST_MODULE_ROW_TYPE: Lazy<ProductType> =
    Lazy::new(|| ProductType::from_iter(st_module_schema().columns.iter().map(|c| c.col_type.clone())));

pub(crate) fn table_name_is_system(table_name: &str) -> bool {
    table_name.starts_with("st_")
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct StTableRow<Name: AsRef<str>> {
    pub(crate) table_id: TableId,
    pub(crate) table_name: Name,
    pub(crate) table_type: StTableType,
    pub(crate) table_access: StAccess,
}

impl<'a> TryFrom<&'a ProductValue> for StTableRow<&'a str> {
    type Error = DBError;
    // TODO(cloutiertyler): Noa, can we just decorate `StTableRow` with Deserialize or something instead?
    fn try_from(row: &'a ProductValue) -> Result<StTableRow<&'a str>, DBError> {
        let table_id = row.field_as_u32(StTableFields::TableId as usize, None)?.into();
        let table_name = row.field_as_str(StTableFields::TableName as usize, None)?;
        let table_type = row
            .field_as_str(StTableFields::TableType as usize, None)?
            .try_into()
            .map_err(|x: &str| TableError::DecodeField {
                table: ST_TABLES_NAME.into(),
                field: StTableFields::TableType.name().into(),
                expect: format!("`{}` or `{}`", StTableType::System.as_str(), StTableType::User.as_str()),
                found: x.to_string(),
            })?;

        let table_access = row
            .field_as_str(StTableFields::TablesAccess as usize, None)?
            .try_into()
            .map_err(|x: &str| TableError::DecodeField {
                table: ST_TABLES_NAME.into(),
                field: StTableFields::TablesAccess.name().into(),
                expect: format!("`{}` or `{}`", StAccess::Public.as_str(), StAccess::Private.as_str()),
                found: x.to_string(),
            })?;

        Ok(StTableRow {
            table_id,
            table_name,
            table_type,
            table_access,
        })
    }
}

impl StTableRow<&str> {
    pub fn to_owned(&self) -> StTableRow<String> {
        StTableRow {
            table_id: self.table_id,
            table_name: self.table_name.to_owned(),
            table_type: self.table_type,
            table_access: self.table_access,
        }
    }
}

impl<Name: AsRef<str>> From<&StTableRow<Name>> for ProductValue {
    fn from(x: &StTableRow<Name>) -> Self {
        product![
            x.table_id,
            AlgebraicValue::String(x.table_name.as_ref().to_owned()),
            AlgebraicValue::String(x.table_type.as_str().into()),
            AlgebraicValue::String(x.table_access.as_str().into())
        ]
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct StColumnRow<Name: AsRef<str>> {
    pub(crate) table_id: TableId,
    pub(crate) col_pos: ColId,
    pub(crate) col_name: Name,
    pub(crate) col_type: AlgebraicType,
}

impl StColumnRow<&str> {
    pub fn to_owned(&self) -> StColumnRow<String> {
        StColumnRow {
            table_id: self.table_id,
            col_pos: self.col_pos,
            col_name: self.col_name.to_owned(),
            col_type: self.col_type.clone(),
        }
    }
}

impl<'a> TryFrom<&'a ProductValue> for StColumnRow<&'a str> {
    type Error = DBError;
    fn try_from(row: &'a ProductValue) -> Result<StColumnRow<&'a str>, DBError> {
        let table_id: TableId = row.field_as_u32(StColumnFields::TableId as usize, None)?.into();
        let col_pos = row.field_as_u32(StColumnFields::ColPos as usize, None)?.into();

        let bytes = row.field_as_bytes(StColumnFields::ColType as usize, None)?;
        let col_type =
            AlgebraicType::decode(&mut &bytes[..]).map_err(|e| TableError::InvalidSchema(table_id.into(), e.into()))?;

        let col_name = row.field_as_str(StColumnFields::ColName as usize, None)?;

        Ok(StColumnRow {
            table_id,
            col_pos,
            col_name,
            col_type,
        })
    }
}

impl<Name: AsRef<str>> From<&StColumnRow<Name>> for ProductValue {
    fn from(x: &StColumnRow<Name>) -> Self {
        let mut bytes = Vec::new();
        x.col_type.encode(&mut bytes);
        product![
            x.table_id,
            x.col_pos,
            AlgebraicValue::Bytes(bytes),
            AlgebraicValue::String(x.col_name.as_ref().to_owned()),
        ]
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct StIndexRow<Name: AsRef<str>> {
    pub(crate) index_id: IndexId,
    pub(crate) table_id: TableId,
    pub(crate) index_type: IndexType,
    pub(crate) index_name: Name,
    pub(crate) columns: NonEmpty<ColId>,
    pub(crate) is_unique: bool,
}

impl StIndexRow<&str> {
    pub fn to_owned(&self) -> StIndexRow<String> {
        StIndexRow {
            index_id: self.index_id,
            table_id: self.table_id,
            index_type: self.index_type,
            columns: self.columns.clone(),
            index_name: self.index_name.to_owned(),
            is_unique: self.is_unique,
        }
    }
}

fn to_cols<F: SystemField>(row: &ProductValue, field: F) -> Result<NonEmpty<ColId>, DBError> {
    let index = field.to_field_id() as usize;
    let cols = row.field_as_array(index, Some(field.to_field_name_str()))?;
    if let ArrayValue::U32(x) = &cols {
        let x: Vec<_> = x.iter().map(|x| ColId::from(*x)).collect();
        Ok(NonEmpty::from_slice(&x).unwrap())
    } else {
        return Err(InvalidFieldError {
            name: Some(field.to_field_name_str()),
            col_pos: field.into(),
        }
        .into());
    }
}

impl<'a> TryFrom<&'a ProductValue> for StIndexRow<&'a str> {
    type Error = DBError;
    fn try_from(row: &'a ProductValue) -> Result<StIndexRow<&'a str>, DBError> {
        let index_id = row.field_as_u32(StIndexFields::IndexId as usize, None)?.into();
        let table_id = row.field_as_u32(StIndexFields::TableId as usize, None)?.into();
        let index_type = row.field_as_str(StIndexFields::IndexType as usize, None)?;
        let index_type = IndexType::try_from(index_type).map_err(|_| InvalidFieldError {
            col_pos: StIndexFields::IndexType.into(),
            name: StIndexFields::IndexType.name().into(),
        })?;
        let cols = to_cols(row, StIndexFields::Columns)?;
        let index_name = row.field_as_str(StIndexFields::IndexName as usize, None)?;
        let is_unique = row.field_as_bool(StIndexFields::IsUnique as usize, None)?;
        Ok(StIndexRow {
            index_id,
            table_id,
            index_type,
            columns: cols,
            index_name,
            is_unique,
        })
    }
}

impl<Name: AsRef<str>> From<&StIndexRow<Name>> for ProductValue {
    fn from(x: &StIndexRow<Name>) -> Self {
        let cols: Vec<_> = x.columns.iter().map(|x| u32::from(*x)).collect();
        product![
            x.index_id,
            x.table_id,
            AlgebraicValue::String(x.index_type.to_string()),
            AlgebraicValue::String(x.index_name.as_ref().to_string()),
            ArrayValue::from(cols),
            x.is_unique
        ]
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct StSequenceRow<Name: AsRef<str>> {
    pub(crate) sequence_id: SequenceId,
    pub(crate) sequence_name: Name,
    pub(crate) table_id: TableId,
    pub(crate) col_pos: ColId,
    pub(crate) increment: i128,
    pub(crate) start: i128,
    pub(crate) min_value: i128,
    pub(crate) max_value: i128,
    pub(crate) allocated: i128,
}

impl<Name: AsRef<str>> StSequenceRow<Name> {
    pub fn to_owned(&self) -> StSequenceRow<String> {
        StSequenceRow {
            sequence_id: self.sequence_id,
            sequence_name: self.sequence_name.as_ref().to_owned(),
            table_id: self.table_id,
            col_pos: self.col_pos,
            increment: self.increment,
            start: self.start,
            min_value: self.min_value,
            max_value: self.max_value,
            allocated: self.allocated,
        }
    }
}

impl<'a> TryFrom<&'a ProductValue> for StSequenceRow<&'a str> {
    type Error = DBError;
    fn try_from(row: &'a ProductValue) -> Result<StSequenceRow<&'a str>, DBError> {
        let sequence_id = row.field_as_u32(StSequenceFields::SequenceId as usize, None)?.into();
        let sequence_name = row.field_as_str(StSequenceFields::SequenceName as usize, None)?;
        let table_id = row.field_as_u32(StSequenceFields::TableId as usize, None)?.into();
        let col_pos = row.field_as_u32(StSequenceFields::ColId as usize, None)?.into();
        let increment = row.field_as_i128(StSequenceFields::Increment as usize, None)?;
        let start = row.field_as_i128(StSequenceFields::Start as usize, None)?;
        let min_value = row.field_as_i128(StSequenceFields::MinValue as usize, None)?;
        let max_value = row.field_as_i128(StSequenceFields::MaxValue as usize, None)?;
        let allocated = row.field_as_i128(StSequenceFields::Allocated as usize, None)?;
        Ok(StSequenceRow {
            sequence_id,
            sequence_name,
            table_id,
            col_pos,
            increment,
            start,
            min_value,
            max_value,
            allocated,
        })
    }
}

impl<Name: AsRef<str>> From<&StSequenceRow<Name>> for ProductValue {
    fn from(x: &StSequenceRow<Name>) -> Self {
        product![
            x.sequence_id,
            AlgebraicValue::String(x.sequence_name.as_ref().to_string()),
            x.table_id,
            x.col_pos,
            x.increment,
            x.start,
            x.min_value,
            x.max_value,
            x.allocated,
        ]
    }
}

impl<'a> From<&StSequenceRow<&'a str>> for SequenceSchema {
    fn from(sequence: &StSequenceRow<&'a str>) -> Self {
        Self {
            sequence_id: sequence.sequence_id,
            sequence_name: sequence.sequence_name.into(),
            table_id: sequence.table_id,
            col_pos: sequence.col_pos,
            start: sequence.start,
            increment: sequence.increment,
            min_value: sequence.min_value,
            max_value: sequence.max_value,
            allocated: sequence.allocated,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct StConstraintRow<Name: AsRef<str>> {
    pub(crate) constraint_id: ConstraintId,
    pub(crate) constraint_name: Name,
    pub(crate) kind: Constraints,
    pub(crate) table_id: TableId,
    pub(crate) columns: NonEmpty<ColId>,
}

impl StConstraintRow<&str> {
    pub fn to_owned(&self) -> StConstraintRow<String> {
        StConstraintRow {
            constraint_id: self.constraint_id,
            constraint_name: self.constraint_name.to_string(),
            kind: self.kind,
            table_id: self.table_id,
            columns: self.columns.clone(),
        }
    }
}

impl<'a> TryFrom<&'a ProductValue> for StConstraintRow<&'a str> {
    type Error = DBError;
    fn try_from(row: &'a ProductValue) -> Result<StConstraintRow<&'a str>, DBError> {
        let constraint_id = row
            .field_as_u32(StConstraintFields::ConstraintId as usize, None)?
            .into();
        let constraint_name = row.field_as_str(StConstraintFields::ConstraintName as usize, None)?;
        let kind = row.field_as_u8(StConstraintFields::Kind as usize, None)?;
        let kind = Constraints::try_from(kind).expect("Fail to decode Constraint");
        let table_id = row.field_as_u32(StConstraintFields::TableId as usize, None)?.into();
        let columns = to_cols(row, StConstraintFields::Columns)?;

        Ok(StConstraintRow {
            constraint_id,
            constraint_name,
            kind,
            table_id,
            columns,
        })
    }
}

impl<Name: AsRef<str>> From<&StConstraintRow<Name>> for ProductValue {
    fn from(x: &StConstraintRow<Name>) -> Self {
        let cols: Vec<_> = x.columns.iter().map(|x| u32::from(*x)).collect();

        product![
            x.constraint_id,
            AlgebraicValue::String(x.constraint_name.as_ref().to_string()),
            x.kind.bits(),
            x.table_id,
            ArrayValue::from(cols)
        ]
    }
}

/// Indicates the kind of module the `program_hash` of a [`StModuleRow`]
/// describes.
///
/// More or less a placeholder to allow for future non-WASM hosts without
/// having to change the [`st_module_schema`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModuleKind(u8);

/// The [`ModuleKind`] of WASM-based modules.
///
/// This is currently the only known kind.
pub const WASM_MODULE: ModuleKind = ModuleKind(0);

impl_serialize!([] ModuleKind, (self, ser) => self.0.serialize(ser));
impl_deserialize!([] ModuleKind, de => u8::deserialize(de).map(Self));

/// A monotonically increasing "epoch" value.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Epoch(pub(crate) u128);

impl_serialize!([] Epoch, (self, ser) => self.0.serialize(ser));
impl_deserialize!([] Epoch, de => u128::deserialize(de).map(Self));

impl fmt::Display for Epoch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StModuleRow {
    pub(crate) program_hash: Hash,
    pub(crate) kind: ModuleKind,
    pub(crate) epoch: Epoch,
}

impl StModuleRow {
    pub fn to_owned(&self) -> StModuleRow {
        self.clone()
    }
}

impl TryFrom<&ProductValue> for StModuleRow {
    type Error = DBError;

    fn try_from(row: &ProductValue) -> Result<Self, Self::Error> {
        let program_hash = row
            .field_as_bytes(
                StModuleFields::ProgramHash as usize,
                Some(StModuleFields::ProgramHash.name()),
            )
            .map(Hash::from_slice)?;
        let kind = row
            .field_as_u8(StModuleFields::Kind as usize, Some(StModuleFields::Kind.name()))
            .map(ModuleKind)?;
        let epoch = row
            .field_as_u128(StModuleFields::Epoch as usize, Some(StModuleFields::Epoch.name()))
            .map(Epoch)?;

        Ok(Self {
            program_hash,
            kind,
            epoch,
        })
    }
}

impl From<&StModuleRow> for ProductValue {
    fn from(
        StModuleRow {
            program_hash,
            kind: ModuleKind(kind),
            epoch: Epoch(epoch),
        }: &StModuleRow,
    ) -> Self {
        product![
            AlgebraicValue::Bytes(program_hash.as_slice().to_owned()),
            AlgebraicValue::U8(*kind),
            AlgebraicValue::U128(*epoch),
        ]
    }
}
