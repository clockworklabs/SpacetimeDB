use super::traits::{ColId, ColumnSchema, IndexSchema, SequenceId, SequenceSchema, TableId, TableSchema};
use crate::db::datastore::traits::ConstraintSchema;
use crate::error::{DBError, TableError};

use once_cell::sync::Lazy;
use spacetimedb_lib::auth::{StAccess, StTableType};
use spacetimedb_lib::{ColumnIndexAttribute, Hash};
use spacetimedb_sats::slim_slice::{try_into, LenTooLong};
use spacetimedb_sats::{
    impl_deserialize, impl_serialize, product, product_value::InvalidFieldError, string, AlgebraicType, AlgebraicValue,
    ArrayValue, ProductType, ProductValue, SatsNonEmpty, SatsStr, SatsString, SatsVec,
};
use std::fmt;
use std::ops::Deref;

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

pub(crate) const TABLE_ID_SEQUENCE_ID: SequenceId = SequenceId(0);
pub(crate) const SEQUENCE_ID_SEQUENCE_ID: SequenceId = SequenceId(1);
pub(crate) const INDEX_ID_SEQUENCE_ID: SequenceId = SequenceId(2);
pub(crate) const CONSTRAINT_ID_SEQUENCE_ID: SequenceId = SequenceId(3);

pub(crate) const ST_TABLE_ID_INDEX_ID: u32 = 0;
pub(crate) const ST_TABLE_NAME_INDEX_ID: u32 = 3;
pub(crate) const ST_INDEX_ID_INDEX_ID: u32 = 1;
pub(crate) const ST_SEQUENCE_ID_INDEX_ID: u32 = 2;
pub(crate) const ST_CONSTRAINT_ID_INDEX_ID: u32 = 4;
pub(crate) const ST_CONSTRAINT_ID_INDEX_HACK: u32 = 5;
pub(crate) struct SystemTables {}

impl SystemTables {
    pub(crate) fn tables() -> [TableSchema; 6] {
        [
            st_table_schema(),
            st_columns_schema(),
            st_sequences_schema(),
            st_indexes_schema(),
            st_constraints_schema(),
            st_module_schema(),
        ]
    }

    pub(crate) fn total_tables() -> usize {
        Self::tables().len()
    }

    pub(crate) fn total_indexes() -> usize {
        Self::tables().iter().flat_map(|x| x.indexes.iter()).count()
    }

    pub(crate) fn total_constraints_indexes() -> usize {
        Self::tables()
            .iter()
            .flat_map(|x| x.constraints.iter().filter(|x| x.kind != ColumnIndexAttribute::UNSET))
            .count()
    }

    pub(crate) fn total_sequences() -> usize {
        Self::tables()
            .iter()
            .flat_map(|x| x.columns.iter().filter(|x| x.is_autoinc))
            .count()
    }

    pub(crate) fn total_constraints() -> usize {
        Self::tables().iter().flat_map(|x| x.constraints.iter()).count()
    }
}

// WARNING: In order to keep a stable schema, don't change the discriminant of the fields
#[derive(Copy, Clone, Debug)]
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

// WARNING: In order to keep a stable schema, don't change the discriminant of the fields
#[derive(Copy, Clone, Debug)]
pub enum StColumnFields {
    TableId = 0,
    ColId = 1,
    ColType = 2,
    ColName = 3,
    IsAutoInc = 4,
}

impl StColumnFields {
    pub fn name(&self) -> &'static str {
        // WARNING: Don't change the name of the fields
        match self {
            Self::TableId => "table_id",
            Self::ColId => "col_id",
            Self::ColType => "col_type",
            Self::ColName => "col_name",
            Self::IsAutoInc => "is_autoinc",
        }
    }
}

// WARNING: In order to keep a stable schema, don't change the discriminant of the fields
#[derive(Debug)]
pub enum StIndexFields {
    IndexId = 0,
    TableId = 1,
    Cols = 2,
    IndexName = 3,
    IsUnique = 4,
}

impl StIndexFields {
    pub fn name(&self) -> &'static str {
        // WARNING: Not change the field names
        match self {
            StIndexFields::IndexId => "index_id",
            StIndexFields::TableId => "table_id",
            StIndexFields::Cols => "cols",
            StIndexFields::IndexName => "index_name",
            StIndexFields::IsUnique => "is_unique",
        }
    }
}

// WARNING: In order to keep a stable schema, don't change the discriminant of the fields
/// The fields that define the internal table [crate::db::relational_db::ST_SEQUENCES_NAME].
#[derive(Debug)]
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
            StSequenceFields::Start => "increment",
            StSequenceFields::Increment => "start",
            StSequenceFields::MinValue => "min_value",
            StSequenceFields::MaxValue => "max_value",
            StSequenceFields::Allocated => "allocated",
        }
    }
}

// WARNING: In order to keep a stable schema, don't change the discriminant of the fields
#[derive(Copy, Clone, Debug)]
pub enum StConstraintFields {
    ConstraintId = 0,
    ConstraintName = 1,
    Kind = 2,
    TableId = 3,
    Columns = 4,
}

impl StConstraintFields {
    /// Return the column index for this constraint field.
    pub fn col_id(&self) -> ColId {
        ColId(*self as u32)
    }

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

// WARNING: In order to keep a stable schema, don't change the discriminant of the fields
#[derive(Copy, Clone, Debug)]
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

fn col_schema(table_id: TableId, id: u32, name: &str, col_type: AlgebraicType, is_autoinc: bool) -> ColumnSchema {
    ColumnSchema {
        table_id: table_id.0,
        col_id: ColId(id),
        col_name: string(name),
        col_type,
        is_autoinc,
    }
}

/// System Table [ST_TABLES_NAME]
///
/// | table_id: u32 | table_name: String | table_type: String | table_access: String |
/// |---------------|--------------------| ------------------ | -------------------- |
/// | 4             | "customers"        | "user"             | "public"             |
pub fn st_table_schema() -> TableSchema {
    let col_schema = |tf: StTableFields, ty, auto_inc| col_schema(ST_TABLES_ID, tf as u32, tf.name(), ty, auto_inc);

    TableSchema {
        table_id: ST_TABLES_ID.0,
        table_name: string(ST_TABLES_NAME),
        indexes: vec![
            IndexSchema {
                index_id: ST_TABLE_ID_INDEX_ID,
                table_id: ST_TABLES_ID.0,
                cols: SatsNonEmpty::new(ColId(StTableFields::TableId as u32)),
                index_name: string("table_id_idx"),
                is_unique: true,
            },
            IndexSchema {
                index_id: ST_TABLE_NAME_INDEX_ID,
                table_id: ST_TABLES_ID.0,
                cols: SatsNonEmpty::new(ColId(StTableFields::TableName as u32)),
                index_name: string("table_name_idx"),
                is_unique: true,
            },
        ],
        columns: [
            col_schema(StTableFields::TableId, AlgebraicType::U32, true),
            col_schema(StTableFields::TableName, AlgebraicType::String, false),
            col_schema(StTableFields::TableType, AlgebraicType::String, false),
            col_schema(StTableFields::TablesAccess, AlgebraicType::String, false),
        ]
        .into(),
        constraints: vec![],
        table_type: StTableType::System,
        table_access: StAccess::Public,
    }
}

pub static ST_TABLE_ROW_TYPE: Lazy<ProductType> =
    Lazy::new(|| ProductType::from_iter(st_table_schema().columns.iter().map(|c| c.col_type.clone())));

/// System Table [ST_COLUMNS_NAME]
///
/// | table_id: u32 | col_id | col_type: Bytes       | col_name: String | is_autoinc: bool |
/// |---------------|--------|-----------------------|------------------|------------------|
/// | 1             | 0      | AlgebraicType->0b0101 | "id"             | true             |
pub fn st_columns_schema() -> TableSchema {
    let col_schema = |cf: StColumnFields, ty, auto_inc| col_schema(ST_COLUMNS_ID, cf as u32, cf.name(), ty, auto_inc);

    TableSchema {
        table_id: ST_COLUMNS_ID.0,
        table_name: string(ST_COLUMNS_NAME),
        indexes: vec![],
        columns: [
            // TODO(cloutiertyler): (table_id, col_id) should be have a unique constraint
            col_schema(StColumnFields::TableId, AlgebraicType::U32, false),
            col_schema(StColumnFields::ColId, AlgebraicType::U32, false),
            col_schema(StColumnFields::ColType, AlgebraicType::bytes(), false),
            col_schema(StColumnFields::ColName, AlgebraicType::String, false),
            col_schema(StColumnFields::IsAutoInc, AlgebraicType::Bool, false),
        ]
        .into(),
        constraints: vec![ConstraintSchema {
            constraint_id: ST_CONSTRAINT_ID_INDEX_HACK,
            constraint_name: string("ct_columns_table_id"),
            kind: ColumnIndexAttribute::INDEXED,
            table_id: ST_COLUMNS_ID.0,
            //TODO: Change to multi-columns when PR for it land: StColumnFields::ColId as u32
            columns: [StColumnFields::TableId as u32].into(),
        }],
        table_type: StTableType::System,
        table_access: StAccess::Public,
    }
}

pub static ST_COLUMNS_ROW_TYPE: Lazy<ProductType> =
    Lazy::new(|| ProductType::from_iter(st_columns_schema().columns.iter().map(|c| c.col_type.clone())));

/// System Table [ST_INDEXES]
///
/// | index_id: u32 | table_id: u32 | cols: NonEmpty<u32> | index_name: String | is_unique: bool      |
/// |---------------|---------------|---------------------|--------------------|----------------------|
/// | 1             | 1             | [1]                 | "ix_sample"        | 0                    |
pub fn st_indexes_schema() -> TableSchema {
    let col_schema = |id, name, ty, auto_inc| col_schema(ST_INDEXES_ID, id, name, ty, auto_inc);

    TableSchema {
        table_id: ST_INDEXES_ID.0,
        table_name: string(ST_INDEXES_NAME),
        // TODO: Unique constraint on index name?
        indexes: vec![IndexSchema {
            index_id: ST_INDEX_ID_INDEX_ID,
            table_id: ST_INDEXES_ID.0,
            cols: SatsNonEmpty::new(ColId(0)),
            index_name: string("index_id_idx"),
            is_unique: true,
        }],
        columns: [
            col_schema(0, "index_id", AlgebraicType::U32, true),
            col_schema(1, "table_id", AlgebraicType::U32, false),
            col_schema(2, "cols", AlgebraicType::array(AlgebraicType::U32), false),
            col_schema(3, "index_name", AlgebraicType::String, false),
            col_schema(4, "is_unique", AlgebraicType::Bool, false),
        ]
        .into(),
        constraints: vec![],
        table_type: StTableType::System,
        table_access: StAccess::Public,
    }
}

pub static ST_INDEX_ROW_TYPE: Lazy<ProductType> =
    Lazy::new(|| ProductType::from_iter(st_indexes_schema().columns.iter().map(|c| c.col_type.clone())));

/// System Table [ST_SEQUENCES]
///
/// | sequence_id | sequence_name     | increment | start | min_value | max_value | table_id | col_id | allocated |
/// |-------------|-------------------|-----------|-------|-----------|-----------|----------|--------|-----------|
/// | 1           | "seq_customer_id" | 1         | 100   | 10        | 1200      | 1        | 1      | 200       |
pub(crate) fn st_sequences_schema() -> TableSchema {
    let col_schema = |id, name, ty, autoinc| col_schema(ST_SEQUENCES_ID, id, name, ty, autoinc);

    TableSchema {
        table_id: ST_SEQUENCES_ID.0,
        table_name: string(ST_SEQUENCES_NAME),
        // TODO: Unique constraint on sequence name?
        indexes: vec![IndexSchema {
            index_id: ST_SEQUENCE_ID_INDEX_ID,
            table_id: ST_SEQUENCES_ID.0,
            cols: SatsNonEmpty::new(ColId(0)),
            index_name: string("sequences_id_idx"),
            is_unique: true,
        }],
        columns: [
            col_schema(0, "sequence_id", AlgebraicType::U32, true),
            col_schema(1, "sequence_name", AlgebraicType::String, false),
            col_schema(2, "table_id", AlgebraicType::U32, false),
            col_schema(3, "col_id", AlgebraicType::U32, false),
            col_schema(4, "increment", AlgebraicType::I128, false),
            col_schema(5, "start", AlgebraicType::I128, false),
            col_schema(6, "min_value", AlgebraicType::I128, false),
            col_schema(7, "max_value", AlgebraicType::I128, false),
            col_schema(8, "allocated", AlgebraicType::I128, false),
        ]
        .into(),
        constraints: vec![],
        table_type: StTableType::System,
        table_access: StAccess::Public,
    }
}

pub static ST_SEQUENCE_ROW_TYPE: Lazy<ProductType> =
    Lazy::new(|| ProductType::from_iter(st_sequences_schema().columns.iter().map(|c| c.col_type.clone())));

/// System Table [ST_CONSTRAINTS_NAME]
///
/// | constraint_id | constraint_name      | kind | table_id | columns |
/// |---------------|-------------------- -|-----------|-------|-----------|
/// | 1             | "unique_customer_id" | 1         | 100   | [1, 4]        |
pub(crate) fn st_constraints_schema() -> TableSchema {
    let col_schema =
        |cf: StConstraintFields, ty, autoinc| col_schema(ST_CONSTRAINTS_ID, cf as u32, cf.name(), ty, autoinc);

    TableSchema {
        table_id: ST_CONSTRAINTS_ID.0,
        table_name: string(ST_CONSTRAINTS_NAME),
        // TODO: Unique constraint on sequence name?
        indexes: vec![IndexSchema {
            index_id: ST_CONSTRAINT_ID_INDEX_ID,
            table_id: ST_CONSTRAINTS_ID.0,
            cols: SatsNonEmpty::new(ColId(0)),
            index_name: string("constraint_id_idx"),
            is_unique: true,
        }],
        columns: [
            col_schema(StConstraintFields::ConstraintId, AlgebraicType::U32, true),
            col_schema(StConstraintFields::ConstraintName, AlgebraicType::String, false),
            col_schema(StConstraintFields::Kind, AlgebraicType::U32, false),
            col_schema(StConstraintFields::TableId, AlgebraicType::U32, false),
            col_schema(
                StConstraintFields::Columns,
                AlgebraicType::array(AlgebraicType::U32),
                false,
            ),
        ]
        .into(),
        constraints: vec![],
        table_type: StTableType::System,
        table_access: StAccess::Public,
    }
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
    let col_schema = |mf: StModuleFields, ty, autoinc| col_schema(ST_MODULE_ID, mf as u32, mf.name(), ty, autoinc);

    TableSchema {
        table_id: ST_MODULE_ID.0,
        table_name: string(ST_MODULE_NAME),
        indexes: vec![],
        columns: [
            col_schema(
                StModuleFields::ProgramHash,
                AlgebraicType::array(AlgebraicType::U8),
                false,
            ),
            col_schema(StModuleFields::Kind, AlgebraicType::U8, false),
            col_schema(StModuleFields::Epoch, AlgebraicType::U128, false),
        ]
        .into(),
        constraints: vec![],
        table_type: StTableType::System,
        table_access: StAccess::Public,
    }
}

pub static ST_MODULE_ROW_TYPE: Lazy<ProductType> =
    Lazy::new(|| ProductType::from_iter(st_module_schema().columns.iter().map(|c| c.col_type.clone())));

pub(crate) fn table_name_is_system(table_name: &str) -> bool {
    table_name.starts_with("st_")
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct StTableRow<Name> {
    pub(crate) table_id: u32,
    pub(crate) table_name: Name,
    pub(crate) table_type: StTableType,
    pub(crate) table_access: StAccess,
}

impl<'a> TryFrom<&'a ProductValue> for StTableRow<&'a SatsStr<'a>> {
    type Error = DBError;
    // TODO(cloutiertyler): Noa, can we just decorate `StTableRow` with Deserialize or something instead?
    fn try_from(row: &'a ProductValue) -> Result<StTableRow<&'a SatsStr<'a>>, DBError> {
        let table_id = row.field_as_u32(StTableFields::TableId as usize, None)?;
        let table_name = row.field_as_str(StTableFields::TableName as usize, None)?;
        let table_type = row
            .field_as_str(StTableFields::TableType as usize, None)?
            .deref()
            .try_into()
            .map_err(|x: &str| TableError::DecodeField {
                table: ST_TABLES_NAME.into(),
                field: StTableFields::TableType.name().into(),
                expect: format!("`{}` or `{}`", StTableType::System.as_str(), StTableType::User.as_str()),
                found: x.to_string(),
            })?;

        let table_access = row
            .field_as_str(StTableFields::TablesAccess as usize, None)?
            .deref()
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

impl StTableRow<&SatsStr<'_>> {
    pub fn to_owned(&self) -> StTableRow<SatsString> {
        StTableRow {
            table_id: self.table_id,
            table_name: self.table_name.into(),
            table_type: self.table_type,
            table_access: self.table_access,
        }
    }
}

impl<N: Into<SatsString>> From<StTableRow<N>> for ProductValue {
    fn from(x: StTableRow<N>) -> Self {
        product![
            x.table_id,
            x.table_name.into(),
            string(x.table_type.as_str()),
            string(x.table_access.as_str()),
        ]
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct StColumnRow<Name> {
    pub(crate) table_id: u32,
    pub(crate) col_id: ColId,
    pub(crate) col_name: Name,
    pub(crate) col_type: AlgebraicType,
    pub(crate) is_autoinc: bool,
}

impl StColumnRow<&SatsStr<'_>> {
    pub fn to_owned(&self) -> StColumnRow<SatsString> {
        StColumnRow {
            table_id: self.table_id,
            col_id: self.col_id,
            col_name: self.col_name.into(),
            col_type: self.col_type.clone(),
            is_autoinc: self.is_autoinc,
        }
    }
}

impl<'a> TryFrom<&'a ProductValue> for StColumnRow<&'a SatsStr<'a>> {
    type Error = DBError;
    fn try_from(row: &'a ProductValue) -> Result<StColumnRow<&'a SatsStr<'a>>, DBError> {
        let table_id = row.field_as_u32(StColumnFields::TableId as usize, None)?;
        let col_id = ColId(row.field_as_u32(StColumnFields::ColId as usize, None)?);

        let bytes = row.field_as_bytes(StColumnFields::ColType as usize, None)?;
        let col_type =
            AlgebraicType::decode(&mut &bytes[..]).map_err(|e| TableError::InvalidSchema(table_id, e.into()))?;

        let col_name = row.field_as_str(StColumnFields::ColName as usize, None)?;
        let is_autoinc = row.field_as_bool(StColumnFields::IsAutoInc as usize, None)?;

        Ok(StColumnRow {
            table_id,
            col_id,
            col_name,
            col_type,
            is_autoinc,
        })
    }
}

impl<N: Into<SatsString>> TryFrom<StColumnRow<N>> for ProductValue {
    type Error = LenTooLong;
    fn try_from(x: StColumnRow<N>) -> Result<Self, Self::Error> {
        let mut bytes = Vec::new();
        x.col_type.encode(&mut bytes);
        let bytes: SatsVec<u8> = try_into(bytes)?;
        Ok(product![
            AlgebraicValue::U32(x.table_id),
            x.col_id,
            AlgebraicValue::Bytes(bytes),
            AlgebraicValue::String(x.col_name.into()),
            AlgebraicValue::Bool(x.is_autoinc),
        ])
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct StIndexRow<Name> {
    pub(crate) index_id: u32,
    pub(crate) table_id: u32,
    pub(crate) cols: SatsNonEmpty<ColId>,
    pub(crate) index_name: Name,
    pub(crate) is_unique: bool,
}

impl StIndexRow<&SatsStr<'_>> {
    pub fn to_owned(&self) -> StIndexRow<SatsString> {
        StIndexRow {
            index_id: self.index_id,
            table_id: self.table_id,
            cols: self.cols.clone(),
            index_name: self.index_name.into(),
            is_unique: self.is_unique,
        }
    }
}

impl<'a> TryFrom<&'a ProductValue> for StIndexRow<&'a SatsStr<'a>> {
    type Error = DBError;
    fn try_from(row: &'a ProductValue) -> Result<StIndexRow<&'a SatsStr<'a>>, DBError> {
        let index_id = row.field_as_u32(StIndexFields::IndexId as usize, None)?;
        let table_id = row.field_as_u32(StIndexFields::TableId as usize, None)?;
        let cols = row.field_as_array(StIndexFields::Cols as usize, None)?;
        let cols = if let ArrayValue::U32(x) = cols {
            SatsNonEmpty::map_slice(x.shared_ref(), |&x| ColId(x)).unwrap()
        } else {
            return Err(InvalidFieldError {
                col_pos: StIndexFields::Cols as usize,
                name: StIndexFields::Cols.name().into(),
            }
            .into());
        };

        let index_name = row.field_as_str(StIndexFields::IndexName as usize, None)?;
        let is_unique = row.field_as_bool(StIndexFields::IsUnique as usize, None)?;
        Ok(StIndexRow {
            index_id,
            table_id,
            cols,
            index_name,
            is_unique,
        })
    }
}

impl<N: Into<SatsString>> From<StIndexRow<N>> for ProductValue {
    fn from(x: StIndexRow<N>) -> Self {
        product![
            AlgebraicValue::U32(x.index_id),
            AlgebraicValue::U32(x.table_id),
            AlgebraicValue::Array(x.cols.map(|x| x.0).into()),
            AlgebraicValue::String(x.index_name.into()),
            AlgebraicValue::Bool(x.is_unique)
        ]
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct StSequenceRow<Name> {
    pub(crate) sequence_id: u32,
    pub(crate) sequence_name: Name,
    pub(crate) table_id: u32,
    pub(crate) col_id: ColId,
    pub(crate) increment: i128,
    pub(crate) start: i128,
    pub(crate) min_value: i128,
    pub(crate) max_value: i128,
    pub(crate) allocated: i128,
}

impl StSequenceRow<&SatsStr<'_>> {
    pub fn to_owned(&self) -> StSequenceRow<SatsString> {
        StSequenceRow {
            sequence_id: self.sequence_id,
            sequence_name: self.sequence_name.into(),
            table_id: self.table_id,
            col_id: self.col_id,
            increment: self.increment,
            start: self.start,
            min_value: self.min_value,
            max_value: self.max_value,
            allocated: self.allocated,
        }
    }
}

impl<'a> TryFrom<&'a ProductValue> for StSequenceRow<&'a SatsStr<'a>> {
    type Error = DBError;
    fn try_from(row: &'a ProductValue) -> Result<StSequenceRow<&'a SatsStr<'a>>, DBError> {
        let sequence_id = row.field_as_u32(StSequenceFields::SequenceId as usize, None)?;
        let sequence_name = row.field_as_str(StSequenceFields::SequenceName as usize, None)?;
        let table_id = row.field_as_u32(StSequenceFields::TableId as usize, None)?;
        let col_id = ColId(row.field_as_u32(StSequenceFields::ColId as usize, None)?);
        let increment = row.field_as_i128(StSequenceFields::Increment as usize, None)?;
        let start = row.field_as_i128(StSequenceFields::Start as usize, None)?;
        let min_value = row.field_as_i128(StSequenceFields::MinValue as usize, None)?;
        let max_value = row.field_as_i128(StSequenceFields::MaxValue as usize, None)?;
        let allocated = row.field_as_i128(StSequenceFields::Allocated as usize, None)?;
        Ok(StSequenceRow {
            sequence_id,
            sequence_name,
            table_id,
            col_id,
            increment,
            start,
            min_value,
            max_value,
            allocated,
        })
    }
}

impl<N: Into<SatsString>> From<StSequenceRow<N>> for ProductValue {
    fn from(x: StSequenceRow<N>) -> Self {
        product![
            AlgebraicValue::U32(x.sequence_id),
            AlgebraicValue::String(x.sequence_name.into()),
            x.table_id,
            x.col_id,
            AlgebraicValue::I128(Box::new(x.increment)),
            AlgebraicValue::I128(Box::new(x.start)),
            AlgebraicValue::I128(Box::new(x.min_value)),
            AlgebraicValue::I128(Box::new(x.max_value)),
            AlgebraicValue::I128(Box::new(x.allocated)),
        ]
    }
}

impl<'a> From<&StSequenceRow<&'a SatsStr<'a>>> for SequenceSchema {
    fn from(sequence: &StSequenceRow<&'a SatsStr<'a>>) -> Self {
        Self {
            sequence_id: sequence.sequence_id,
            sequence_name: sequence.sequence_name.into(),
            table_id: sequence.table_id,
            col_id: sequence.col_id,
            start: sequence.start,
            increment: sequence.increment,
            min_value: sequence.min_value,
            max_value: sequence.max_value,
            allocated: sequence.allocated,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct StConstraintRow<Name> {
    pub(crate) constraint_id: u32,
    pub(crate) constraint_name: Name,
    pub(crate) kind: ColumnIndexAttribute,
    pub(crate) table_id: u32,
    pub(crate) columns: SatsVec<u32>,
}

impl StConstraintRow<&SatsStr<'_>> {
    pub fn to_owned(&self) -> StConstraintRow<SatsString> {
        StConstraintRow {
            constraint_id: self.constraint_id,
            constraint_name: self.constraint_name.into(),
            kind: self.kind,
            table_id: self.table_id,
            columns: self.columns.clone(),
        }
    }
}

impl<'a> TryFrom<&'a ProductValue> for StConstraintRow<&'a SatsStr<'a>> {
    type Error = DBError;
    fn try_from(row: &'a ProductValue) -> Result<StConstraintRow<&'a SatsStr<'a>>, DBError> {
        let constraint_id = row.field_as_u32(StConstraintFields::ConstraintId as usize, None)?;
        let constraint_name = row.field_as_str(StConstraintFields::ConstraintName as usize, None)?;
        let kind = row.field_as_u8(StConstraintFields::Kind as usize, None)?;
        let kind = ColumnIndexAttribute::try_from(kind).expect("Fail to decode ColumnIndexAttribute");
        let table_id = row.field_as_u32(StConstraintFields::TableId as usize, None)?;
        let columns = row.field_as_array(StConstraintFields::Columns as usize, None)?;
        let columns = if let ArrayValue::U32(x) = columns {
            x.clone()
        } else {
            panic!()
        };

        Ok(StConstraintRow {
            constraint_id,
            constraint_name,
            kind,
            table_id,
            columns,
        })
    }
}

impl<N: Into<SatsString>> From<StConstraintRow<N>> for ProductValue {
    fn from(x: StConstraintRow<N>) -> Self {
        product![
            AlgebraicValue::U32(x.constraint_id),
            AlgebraicValue::String(x.constraint_name.into()),
            AlgebraicValue::U8(x.kind.bits()),
            AlgebraicValue::U32(x.table_id),
            AlgebraicValue::Array(x.columns.clone().into())
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
        product![<SatsVec<_>>::from(program_hash.data), *kind, *epoch]
    }
}
