use super::traits::{ColumnSchema, IndexSchema, SequenceId, SequenceSchema, TableId, TableSchema};
use crate::db::datastore::traits::ConstraintSchema;
use crate::error::{DBError, TableError};
use core::fmt;
use nonempty::NonEmpty;
use once_cell::sync::Lazy;
use spacetimedb_lib::auth::{StAccess, StTableType};
use spacetimedb_lib::{ColumnIndexAttribute, Hash};
use spacetimedb_sats::{
    impl_deserialize, impl_serialize, product, product_value::InvalidFieldError, AlgebraicType, AlgebraicValue,
    ArrayValue, ProductType, ProductValue,
};

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
#[derive(Debug)]
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
#[derive(Debug)]
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
#[derive(Debug)]
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

// WARNING: In order to keep a stable schema, don't change the discriminant of the fields
#[derive(Debug)]
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

/// System Table [ST_TABLES_NAME]
///
/// | table_id: u32 | table_name: String | table_type: String | table_access: String |
/// |---------------|--------------------| ------------------ | -------------------- |
/// | 4             | "customers"        | "user"             | "public"             |
pub fn st_table_schema() -> TableSchema {
    TableSchema {
        table_id: ST_TABLES_ID.0,
        table_name: ST_TABLES_NAME.into(),
        indexes: vec![
            IndexSchema {
                index_id: ST_TABLE_ID_INDEX_ID,
                table_id: ST_TABLES_ID.0,
                cols: NonEmpty::new(StTableFields::TableId as u32),
                index_name: "table_id_idx".into(),
                is_unique: true,
            },
            IndexSchema {
                index_id: ST_TABLE_NAME_INDEX_ID,
                table_id: ST_TABLES_ID.0,
                cols: NonEmpty::new(StTableFields::TableName as u32),
                index_name: "table_name_idx".into(),
                is_unique: true,
            },
        ],
        columns: vec![
            ColumnSchema {
                table_id: ST_TABLES_ID.0,
                col_id: StTableFields::TableId as u32,
                col_name: StTableFields::TableId.name().into(),
                col_type: AlgebraicType::U32,
                is_autoinc: true,
            },
            ColumnSchema {
                table_id: ST_TABLES_ID.0,
                col_id: StTableFields::TableName as u32,
                col_name: StTableFields::TableName.name().into(),
                col_type: AlgebraicType::String,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_TABLES_ID.0,
                col_id: StTableFields::TableType as u32,
                col_name: StTableFields::TableType.name().into(),
                col_type: AlgebraicType::String,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_TABLES_ID.0,
                col_id: StTableFields::TablesAccess as u32,
                col_name: StTableFields::TablesAccess.name().into(),
                col_type: AlgebraicType::String,
                is_autoinc: false,
            },
        ],
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
    TableSchema {
        table_id: ST_COLUMNS_ID.0,
        table_name: ST_COLUMNS_NAME.into(),
        indexes: vec![],
        columns: vec![
            // TODO(cloutiertyler): (table_id, col_id) should be have a unique constraint
            ColumnSchema {
                table_id: ST_COLUMNS_ID.0,
                col_id: StColumnFields::TableId as u32,
                col_name: StColumnFields::TableId.name().to_string(),
                col_type: AlgebraicType::U32,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_COLUMNS_ID.0,
                col_id: StColumnFields::ColId as u32,
                col_name: StColumnFields::ColId.name().to_string(),
                col_type: AlgebraicType::U32,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_COLUMNS_ID.0,
                col_id: StColumnFields::ColType as u32,
                col_name: StColumnFields::ColType.name().to_string(),
                col_type: AlgebraicType::bytes(),
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_COLUMNS_ID.0,
                col_id: StColumnFields::ColName as u32,
                col_name: StColumnFields::ColName.name().to_string(),
                col_type: AlgebraicType::String,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_COLUMNS_ID.0,
                col_id: StColumnFields::IsAutoInc as u32,
                col_name: StColumnFields::IsAutoInc.name().to_string(),
                col_type: AlgebraicType::Bool,
                is_autoinc: false,
            },
        ],
        constraints: vec![ConstraintSchema {
            constraint_id: ST_CONSTRAINT_ID_INDEX_HACK,
            constraint_name: "ct_columns_table_id".to_string(),
            kind: ColumnIndexAttribute::INDEXED,
            table_id: ST_COLUMNS_ID.0,
            //TODO: Change to multi-columns when PR for it land: StColumnFields::ColId as u32
            columns: vec![StColumnFields::TableId as u32],
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
    TableSchema {
        table_id: ST_INDEXES_ID.0,
        table_name: ST_INDEXES_NAME.into(),
        // TODO: Unique constraint on index name?
        indexes: vec![IndexSchema {
            index_id: ST_INDEX_ID_INDEX_ID,
            table_id: ST_INDEXES_ID.0,
            cols: NonEmpty::new(0),
            index_name: "index_id_idx".into(),
            is_unique: true,
        }],
        columns: vec![
            ColumnSchema {
                table_id: ST_INDEXES_ID.0,
                col_id: 0,
                col_name: "index_id".into(),
                col_type: AlgebraicType::U32,
                is_autoinc: true,
            },
            ColumnSchema {
                table_id: ST_INDEXES_ID.0,
                col_id: 1,
                col_name: "table_id".into(),
                col_type: AlgebraicType::U32,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_INDEXES_ID.0,
                col_id: 2,
                col_name: "cols".into(),
                col_type: AlgebraicType::array(AlgebraicType::U32),
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_INDEXES_ID.0,
                col_id: 3,
                col_name: "index_name".into(),
                col_type: AlgebraicType::String,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_INDEXES_ID.0,
                col_id: 4,
                col_name: "is_unique".into(),
                col_type: AlgebraicType::Bool,
                is_autoinc: false,
            },
        ],
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
    TableSchema {
        table_id: ST_SEQUENCES_ID.0,
        table_name: ST_SEQUENCES_NAME.into(),
        // TODO: Unique constraint on sequence name?
        indexes: vec![IndexSchema {
            index_id: ST_SEQUENCE_ID_INDEX_ID,
            table_id: ST_SEQUENCES_ID.0,
            cols: NonEmpty::new(0),
            index_name: "sequences_id_idx".into(),
            is_unique: true,
        }],
        columns: vec![
            ColumnSchema {
                table_id: ST_SEQUENCES_ID.0,
                col_id: 0,
                col_name: "sequence_id".into(),
                col_type: AlgebraicType::U32,
                is_autoinc: true,
            },
            ColumnSchema {
                table_id: ST_SEQUENCES_ID.0,
                col_id: 1,
                col_name: "sequence_name".into(),
                col_type: AlgebraicType::String,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_SEQUENCES_ID.0,
                col_id: 2,
                col_name: "table_id".into(),
                col_type: AlgebraicType::U32,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_SEQUENCES_ID.0,
                col_id: 3,
                col_name: "col_id".into(),
                col_type: AlgebraicType::U32,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_SEQUENCES_ID.0,
                col_id: 4,
                col_name: "increment".into(),
                col_type: AlgebraicType::I128,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_SEQUENCES_ID.0,
                col_id: 5,
                col_name: "start".into(),
                col_type: AlgebraicType::I128,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_SEQUENCES_ID.0,
                col_id: 6,
                col_name: "min_value".into(),
                col_type: AlgebraicType::I128,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_SEQUENCES_ID.0,
                col_id: 7,
                col_name: "max_malue".into(),
                col_type: AlgebraicType::I128,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_SEQUENCES_ID.0,
                col_id: 8,
                col_name: "allocated".into(),
                col_type: AlgebraicType::I128,
                is_autoinc: false,
            },
        ],
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
    TableSchema {
        table_id: ST_CONSTRAINTS_ID.0,
        table_name: ST_CONSTRAINTS_NAME.into(),
        // TODO: Unique constraint on sequence name?
        indexes: vec![IndexSchema {
            index_id: ST_CONSTRAINT_ID_INDEX_ID,
            table_id: ST_CONSTRAINTS_ID.0,
            cols: NonEmpty::new(0),
            index_name: "constraint_id_idx".into(),
            is_unique: true,
        }],
        columns: vec![
            ColumnSchema {
                table_id: ST_CONSTRAINTS_ID.0,
                col_id: StConstraintFields::ConstraintId as u32,
                col_name: StConstraintFields::ConstraintId.name().into(),
                col_type: AlgebraicType::U32,
                is_autoinc: true,
            },
            ColumnSchema {
                table_id: ST_CONSTRAINTS_ID.0,
                col_id: StConstraintFields::ConstraintName as u32,
                col_name: StConstraintFields::ConstraintName.name().into(),
                col_type: AlgebraicType::String,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_CONSTRAINTS_ID.0,
                col_id: StConstraintFields::Kind as u32,
                col_name: StConstraintFields::Kind.name().into(),
                col_type: AlgebraicType::U32,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_CONSTRAINTS_ID.0,
                col_id: StConstraintFields::TableId as u32,
                col_name: StConstraintFields::TableId.name().into(),
                col_type: AlgebraicType::U32,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_CONSTRAINTS_ID.0,
                col_id: StConstraintFields::Columns as u32,
                col_name: StConstraintFields::Columns.name().into(),
                col_type: AlgebraicType::array(AlgebraicType::U32),
                is_autoinc: false,
            },
        ],
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
    TableSchema {
        table_id: ST_MODULE_ID.0,
        table_name: ST_MODULE_NAME.into(),
        indexes: vec![],
        columns: vec![
            ColumnSchema {
                table_id: ST_MODULE_ID.0,
                col_id: StModuleFields::ProgramHash as u32,
                col_name: StModuleFields::ProgramHash.name().into(),
                col_type: AlgebraicType::array(AlgebraicType::U8),
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_MODULE_ID.0,
                col_id: StModuleFields::Kind as u32,
                col_name: StModuleFields::Kind.name().into(),
                col_type: AlgebraicType::U8,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_MODULE_ID.0,
                col_id: StModuleFields::Epoch as u32,
                col_name: StModuleFields::Epoch.name().into(),
                col_type: AlgebraicType::U128,
                is_autoinc: false,
            },
        ],
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
pub struct StTableRow<Name: AsRef<str>> {
    pub(crate) table_id: u32,
    pub(crate) table_name: Name,
    pub(crate) table_type: StTableType,
    pub(crate) table_access: StAccess,
}

impl<'a> TryFrom<&'a ProductValue> for StTableRow<&'a str> {
    type Error = DBError;
    // TODO(cloutiertyler): Noa, can we just decorate `StTableRow` with Deserialize or something instead?
    fn try_from(row: &'a ProductValue) -> Result<StTableRow<&'a str>, DBError> {
        let table_id = row.field_as_u32(StTableFields::TableId as usize, None)?;
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
            AlgebraicValue::U32(x.table_id),
            AlgebraicValue::String(x.table_name.as_ref().to_owned()),
            AlgebraicValue::String(x.table_type.as_str().into()),
            AlgebraicValue::String(x.table_access.as_str().into())
        ]
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct StColumnRow<Name: AsRef<str>> {
    pub(crate) table_id: u32,
    pub(crate) col_id: u32,
    pub(crate) col_name: Name,
    pub(crate) col_type: AlgebraicType,
    pub(crate) is_autoinc: bool,
}

impl StColumnRow<&str> {
    pub fn to_owned(&self) -> StColumnRow<String> {
        StColumnRow {
            table_id: self.table_id,
            col_id: self.col_id,
            col_name: self.col_name.to_owned(),
            col_type: self.col_type.clone(),
            is_autoinc: self.is_autoinc,
        }
    }
}

impl<'a> TryFrom<&'a ProductValue> for StColumnRow<&'a str> {
    type Error = DBError;
    fn try_from(row: &'a ProductValue) -> Result<StColumnRow<&'a str>, DBError> {
        let table_id = row.field_as_u32(StColumnFields::TableId as usize, None)?;
        let col_id = row.field_as_u32(StColumnFields::ColId as usize, None)?;

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

impl<Name: AsRef<str>> From<&StColumnRow<Name>> for ProductValue {
    fn from(x: &StColumnRow<Name>) -> Self {
        let mut bytes = Vec::new();
        x.col_type.encode(&mut bytes);
        product![
            AlgebraicValue::U32(x.table_id),
            AlgebraicValue::U32(x.col_id),
            AlgebraicValue::Bytes(bytes),
            AlgebraicValue::String(x.col_name.as_ref().to_owned()),
            AlgebraicValue::Bool(x.is_autoinc),
        ]
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct StIndexRow<Name: AsRef<str>> {
    pub(crate) index_id: u32,
    pub(crate) table_id: u32,
    pub(crate) cols: NonEmpty<u32>,
    pub(crate) index_name: Name,
    pub(crate) is_unique: bool,
}

impl StIndexRow<&str> {
    pub fn to_owned(&self) -> StIndexRow<String> {
        StIndexRow {
            index_id: self.index_id,
            table_id: self.table_id,
            cols: self.cols.clone(),
            index_name: self.index_name.to_owned(),
            is_unique: self.is_unique,
        }
    }
}

impl<'a> TryFrom<&'a ProductValue> for StIndexRow<&'a str> {
    type Error = DBError;
    fn try_from(row: &'a ProductValue) -> Result<StIndexRow<&'a str>, DBError> {
        let index_id = row.field_as_u32(StIndexFields::IndexId as usize, None)?;
        let table_id = row.field_as_u32(StIndexFields::TableId as usize, None)?;
        let cols = row.field_as_array(StIndexFields::Cols as usize, None)?;
        let cols = if let ArrayValue::U32(x) = cols {
            NonEmpty::from_slice(x).unwrap()
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

impl<Name: AsRef<str>> From<&StIndexRow<Name>> for ProductValue {
    fn from(x: &StIndexRow<Name>) -> Self {
        product![
            AlgebraicValue::U32(x.index_id),
            AlgebraicValue::U32(x.table_id),
            AlgebraicValue::ArrayOf(x.cols.clone()),
            AlgebraicValue::String(x.index_name.as_ref().to_string()),
            AlgebraicValue::Bool(x.is_unique)
        ]
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct StSequenceRow<Name: AsRef<str>> {
    pub(crate) sequence_id: u32,
    pub(crate) sequence_name: Name,
    pub(crate) table_id: u32,
    pub(crate) col_id: u32,
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
            col_id: self.col_id,
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
        let sequence_id = row.field_as_u32(StSequenceFields::SequenceId as usize, None)?;
        let sequence_name = row.field_as_str(StSequenceFields::SequenceName as usize, None)?;
        let table_id = row.field_as_u32(StSequenceFields::TableId as usize, None)?;
        let col_id = row.field_as_u32(StSequenceFields::ColId as usize, None)?;
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

impl<Name: AsRef<str>> From<&StSequenceRow<Name>> for ProductValue {
    fn from(x: &StSequenceRow<Name>) -> Self {
        product![
            AlgebraicValue::U32(x.sequence_id),
            AlgebraicValue::String(x.sequence_name.as_ref().to_string()),
            AlgebraicValue::U32(x.table_id),
            AlgebraicValue::U32(x.col_id),
            AlgebraicValue::I128(x.increment),
            AlgebraicValue::I128(x.start),
            AlgebraicValue::I128(x.min_value),
            AlgebraicValue::I128(x.max_value),
            AlgebraicValue::I128(x.allocated),
        ]
    }
}

impl<'a> From<&StSequenceRow<&'a str>> for SequenceSchema {
    fn from(sequence: &StSequenceRow<&'a str>) -> Self {
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
pub struct StConstraintRow<Name: AsRef<str>> {
    pub(crate) constraint_id: u32,
    pub(crate) constraint_name: Name,
    pub(crate) kind: ColumnIndexAttribute,
    pub(crate) table_id: u32,
    pub(crate) columns: Vec<u32>,
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

impl<Name: AsRef<str>> From<&StConstraintRow<Name>> for ProductValue {
    fn from(x: &StConstraintRow<Name>) -> Self {
        product![
            AlgebraicValue::U32(x.constraint_id),
            AlgebraicValue::String(x.constraint_name.as_ref().to_string()),
            AlgebraicValue::U8(x.kind.bits()),
            AlgebraicValue::U32(x.table_id),
            AlgebraicValue::ArrayOf(x.columns.clone())
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
