use super::traits::{ColumnSchema, IndexSchema, SequenceSchema, TableSchema};
use crate::db::datastore::traits::ConstraintSchema;
use crate::error::{DBError, TableError};
use core::fmt;
use nonempty::NonEmpty;
use once_cell::sync::Lazy;
use spacetimedb_lib::auth::{StAccess, StTableType};
use spacetimedb_lib::{ColumnIndexAttribute, Hash, IndexType};
use spacetimedb_primitives::{ColId, IndexId, SequenceId, TableId};
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

pub(crate) const ST_TABLE_ID_INDEX_ID: IndexId = IndexId(0);
pub(crate) const ST_TABLE_NAME_INDEX_ID: IndexId = IndexId(3);
pub(crate) const ST_INDEX_ID_INDEX_ID: IndexId = IndexId(1);
pub(crate) const ST_SEQUENCE_ID_INDEX_ID: IndexId = IndexId(2);
pub(crate) const ST_CONSTRAINT_ID_INDEX_ID: IndexId = IndexId(4);
pub(crate) const ST_CONSTRAINT_ID_INDEX_HACK: IndexId = IndexId(5);

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

macro_rules! st_fields_enum {
    ($(#[$attr:meta])* enum $ty_name:ident { $($name:expr, $var:ident = $discr:expr,)* }) => {
        #[derive(Copy, Clone, Debug)]
        $(#[$attr])*
        pub enum $ty_name {
            $($var = $discr,)*
        }

        impl $ty_name {
            pub fn col_id(self) -> ColId {
                ColId(self as u32)
            }

            pub fn col_name(self) -> String {
                self.name().into()
            }

            pub fn name(self) -> &'static str {
                match self {
                    $(Self::$var => $name,)*
                }
            }
        }

        impl From<$ty_name> for NonEmpty<ColId> {
            fn from(value: $ty_name) -> Self {
                NonEmpty::new(value.col_id())
            }
        }
    }
}

// WARNING: For a stable schema, don't change the field names and discriminants.
st_fields_enum!(enum StTableFields {
    "table_id", TableId = 0,
    "table_name", TableName = 1,
    "table_type", TableType = 2,
    "table_access", TablesAccess = 3,
});
// WARNING: For a stable schema, don't change the field names and discriminants.
st_fields_enum!(enum StColumnFields {
    "table_id", TableId = 0,
    "col_id", ColId = 1,
    "col_name", ColName = 2,
    "col_type", ColType = 3,
    "is_autoinc", IsAutoInc = 4,
});
// WARNING: For a stable schema, don't change the field names and discriminants.
st_fields_enum!(enum StIndexFields {
    "index_id", IndexId = 0,
    "table_id", TableId = 1,
    "index_name", IndexName = 2,
    "index_type", IndexType = 3,
    "columns", Columns = 4,
    "is_unique", IsUnique = 5,
});
// WARNING: For a stable schema, don't change the field names and discriminants.
st_fields_enum!(
    /// The fields that define the internal table [crate::db::relational_db::ST_SEQUENCES_NAME].
    enum StSequenceFields {
    "sequence_id", SequenceId = 0,
    "sequence_name", SequenceName = 1,
    "table_id", TableId = 2,
    "col_id", ColId = 3,
    "increment", Increment = 4,
    "start", Start = 5,
    "min_value", MinValue = 6,
    "max_value", MaxValue = 7,
    "allocated", Allocated = 8,
});
// WARNING: For a stable schema, don't change the field names and discriminants.
st_fields_enum!(enum StConstraintFields {
    "constraint_id", ConstraintId = 0,
    "constraint_name", ConstraintName = 1,
    "kind", Kind = 2,
    "table_id", TableId = 3,
    "columns", Columns = 4,
});
// WARNING: For a stable schema, don't change the field names and discriminants.
st_fields_enum!(enum StModuleFields {
    "program_hash", ProgramHash = 0,
    "kind", Kind = 1,
    "epoch", Epoch = 2,
});

/// System Table [ST_TABLES_NAME]
///
/// | table_id | table_name  | table_type | table_access |
/// |----------|-------------|----------- |------------- |
/// | 4        | "customers" | "user"     | "public"     |
pub fn st_table_schema() -> TableSchema {
    TableSchema {
        table_id: ST_TABLES_ID,
        table_name: ST_TABLES_NAME.into(),
        columns: vec![
            ColumnSchema {
                table_id: ST_TABLES_ID,
                col_id: StTableFields::TableId.col_id(),
                col_name: StTableFields::TableId.col_name(),
                col_type: AlgebraicType::U32,
                is_autoinc: true,
            },
            ColumnSchema {
                table_id: ST_TABLES_ID,
                col_id: StTableFields::TableName.col_id(),
                col_name: StTableFields::TableName.col_name(),
                col_type: AlgebraicType::String,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_TABLES_ID,
                col_id: StTableFields::TableType.col_id(),
                col_name: StTableFields::TableType.col_name(),
                col_type: AlgebraicType::String,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_TABLES_ID,
                col_id: StTableFields::TablesAccess.col_id(),
                col_name: StTableFields::TablesAccess.col_name(),
                col_type: AlgebraicType::String,
                is_autoinc: false,
            },
        ],
        constraints: vec![],
        indexes: vec![
            IndexSchema {
                index_id: ST_TABLE_ID_INDEX_ID,
                table_id: ST_TABLES_ID,
                cols: StTableFields::TableId.into(),
                index_name: "table_id_idx".into(),
                is_unique: true,
                index_type: IndexType::BTree,
            },
            IndexSchema {
                index_id: ST_TABLE_NAME_INDEX_ID,
                table_id: ST_TABLES_ID,
                cols: StTableFields::TableName.into(),
                index_name: "table_name_idx".into(),
                is_unique: true,
                index_type: IndexType::BTree,
            },
        ],
        table_type: StTableType::System,
        table_access: StAccess::Public,
    }
}

pub static ST_TABLE_ROW_TYPE: Lazy<ProductType> =
    Lazy::new(|| ProductType::from_iter(st_table_schema().columns.iter().map(|c| c.col_type.clone())));

/// System Table [ST_COLUMNS_NAME]
///
/// | table_id | col_id | col_name | col_type              | is_autoinc |
/// |----------|---------|----------|-----------------------|------------|
/// | 1        | 0       | "id"     | AlgebraicType::U32    | true       |
pub fn st_columns_schema() -> TableSchema {
    TableSchema {
        table_id: ST_COLUMNS_ID,
        table_name: ST_COLUMNS_NAME.into(),
        indexes: vec![],
        columns: vec![
            ColumnSchema {
                table_id: ST_COLUMNS_ID,
                col_id: StColumnFields::TableId.col_id(),
                col_name: StColumnFields::TableId.col_name(),
                col_type: AlgebraicType::U32,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_COLUMNS_ID,
                col_id: StColumnFields::ColId.col_id(),
                col_name: StColumnFields::ColId.col_name(),
                col_type: AlgebraicType::U32,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_COLUMNS_ID,
                col_id: StColumnFields::ColName.col_id(),
                col_name: StColumnFields::ColName.col_name(),
                col_type: AlgebraicType::String,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_COLUMNS_ID,
                col_id: StColumnFields::ColType.col_id(),
                col_name: StColumnFields::ColType.col_name(),
                col_type: AlgebraicType::bytes(),
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_COLUMNS_ID,
                col_id: StColumnFields::IsAutoInc.col_id(),
                col_name: StColumnFields::IsAutoInc.col_name(),
                col_type: AlgebraicType::Bool,
                is_autoinc: false,
            },
        ],
        // TODO(cloutiertyler): (table_id, col_id) should be have a unique constraint
        constraints: vec![ConstraintSchema {
            constraint_id: ST_CONSTRAINT_ID_INDEX_HACK.0.into(),
            constraint_name: "ct_columns_table_id".to_string(),
            kind: ColumnIndexAttribute::INDEXED,
            table_id: ST_COLUMNS_ID,
            //TODO: Change to multi-columns when PR for it land: StColumnFields::ColId as u32
            columns: StColumnFields::TableId.into(),
        }],
        table_type: StTableType::System,
        table_access: StAccess::Public,
    }
}

pub static ST_COLUMNS_ROW_TYPE: Lazy<ProductType> =
    Lazy::new(|| ProductType::from_iter(st_columns_schema().columns.iter().map(|c| c.col_type.clone())));

/// System Table [ST_INDEXES]
///
/// | index_id | table_id | index_name  | columns | is_unique | index_type |
/// |----------|----------|-------------|---------|-----------|------------|
/// | 1        |          | "ix_sample" | [1]     | false     | "btree"    |
pub fn st_indexes_schema() -> TableSchema {
    TableSchema {
        table_id: ST_INDEXES_ID,
        table_name: ST_INDEXES_NAME.into(),
        columns: vec![
            ColumnSchema {
                table_id: ST_INDEXES_ID,
                col_id: StIndexFields::IndexId.col_id(),
                col_name: StIndexFields::IndexId.col_name(),
                col_type: AlgebraicType::U32,
                is_autoinc: true,
            },
            ColumnSchema {
                table_id: ST_INDEXES_ID,
                col_id: StIndexFields::TableId.col_id(),
                col_name: StIndexFields::TableId.col_name(),
                col_type: AlgebraicType::U32,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_INDEXES_ID,
                col_id: StIndexFields::IndexName.col_id(),
                col_name: StIndexFields::IndexName.col_name(),
                col_type: AlgebraicType::String,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_INDEXES_ID,
                col_id: StIndexFields::Columns.col_id(),
                col_name: StIndexFields::Columns.col_name(),
                col_type: AlgebraicType::array(AlgebraicType::U32),
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_INDEXES_ID,
                col_id: StIndexFields::IsUnique.col_id(),
                col_name: StIndexFields::IsUnique.col_name(),
                col_type: AlgebraicType::Bool,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_INDEXES_ID,
                col_id: StIndexFields::IndexType.col_id(),
                col_name: StIndexFields::IndexType.col_name(),
                col_type: AlgebraicType::U8,
                is_autoinc: false,
            },
        ],
        // TODO: Unique constraint on index name?
        constraints: vec![],
        indexes: vec![IndexSchema {
            index_id: ST_INDEX_ID_INDEX_ID,
            table_id: ST_INDEXES_ID,
            cols: NonEmpty::new(StIndexFields::IndexId.col_id()),
            index_name: "index_id_idx".into(),
            is_unique: true,
            index_type: IndexType::BTree,
        }],
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
        table_id: ST_SEQUENCES_ID,
        table_name: ST_SEQUENCES_NAME.into(),
        columns: vec![
            ColumnSchema {
                table_id: ST_SEQUENCES_ID,
                col_id: StSequenceFields::SequenceId.col_id(),
                col_name: StSequenceFields::SequenceId.col_name(),
                col_type: AlgebraicType::U32,
                is_autoinc: true,
            },
            ColumnSchema {
                table_id: ST_SEQUENCES_ID,
                col_id: StSequenceFields::SequenceName.col_id(),
                col_name: StSequenceFields::SequenceName.col_name(),
                col_type: AlgebraicType::String,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_SEQUENCES_ID,
                col_id: StSequenceFields::TableId.col_id(),
                col_name: StSequenceFields::TableId.col_name(),
                col_type: AlgebraicType::U32,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_SEQUENCES_ID,
                col_id: StSequenceFields::ColId.col_id(),
                col_name: StSequenceFields::ColId.col_name(),
                col_type: AlgebraicType::U32,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_SEQUENCES_ID,
                col_id: StSequenceFields::Increment.col_id(),
                col_name: StSequenceFields::Increment.col_name(),
                col_type: AlgebraicType::I128,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_SEQUENCES_ID,
                col_id: StSequenceFields::Start.col_id(),
                col_name: StSequenceFields::Start.col_name(),
                col_type: AlgebraicType::I128,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_SEQUENCES_ID,
                col_id: StSequenceFields::MinValue.col_id(),
                col_name: StSequenceFields::MinValue.col_name(),
                col_type: AlgebraicType::I128,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_SEQUENCES_ID,
                col_id: StSequenceFields::MaxValue.col_id(),
                col_name: StSequenceFields::MaxValue.col_name(),
                col_type: AlgebraicType::I128,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_SEQUENCES_ID,
                col_id: StSequenceFields::Allocated.col_id(),
                col_name: StSequenceFields::Allocated.col_name(),
                col_type: AlgebraicType::I128,
                is_autoinc: false,
            },
        ],
        // TODO: Unique constraint on sequence name?
        constraints: vec![],
        indexes: vec![IndexSchema {
            index_id: ST_SEQUENCE_ID_INDEX_ID,
            table_id: ST_SEQUENCES_ID,
            cols: NonEmpty::new(StSequenceFields::SequenceId.col_id()),
            index_name: "sequences_id_idx".into(),
            is_unique: true,
            index_type: IndexType::BTree,
        }],
        table_type: StTableType::System,
        table_access: StAccess::Public,
    }
}

pub static ST_SEQUENCE_ROW_TYPE: Lazy<ProductType> =
    Lazy::new(|| ProductType::from_iter(st_sequences_schema().columns.iter().map(|c| c.col_type.clone())));

/// System Table [ST_CONSTRAINTS_NAME]
///
/// | constraint_id | constraint_name      | kind | table_id | columns |
/// |---------------|-------------------- -|------|-------|------------|
/// | 1             | "unique_customer_id" | 1    | 100   | [1, 4]     |
pub(crate) fn st_constraints_schema() -> TableSchema {
    TableSchema {
        table_id: ST_CONSTRAINTS_ID,
        table_name: ST_CONSTRAINTS_NAME.into(),
        columns: vec![
            ColumnSchema {
                table_id: ST_CONSTRAINTS_ID,
                col_id: StConstraintFields::ConstraintId.col_id(),
                col_name: StConstraintFields::ConstraintId.col_name(),
                col_type: AlgebraicType::U32,
                is_autoinc: true,
            },
            ColumnSchema {
                table_id: ST_CONSTRAINTS_ID,
                col_id: StConstraintFields::ConstraintName.col_id(),
                col_name: StConstraintFields::ConstraintName.col_name(),
                col_type: AlgebraicType::String,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_CONSTRAINTS_ID,
                col_id: StConstraintFields::Kind.col_id(),
                col_name: StConstraintFields::Kind.col_name(),
                col_type: AlgebraicType::U32,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_CONSTRAINTS_ID,
                col_id: StConstraintFields::TableId.col_id(),
                col_name: StConstraintFields::TableId.col_name(),
                col_type: AlgebraicType::U32,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_CONSTRAINTS_ID,
                col_id: StConstraintFields::Columns.col_id(),
                col_name: StConstraintFields::Columns.col_name(),
                col_type: AlgebraicType::array(AlgebraicType::U32),
                is_autoinc: false,
            },
        ],
        // TODO: Unique constraint on sequence name?
        constraints: vec![],
        indexes: vec![IndexSchema {
            index_id: ST_CONSTRAINT_ID_INDEX_ID,
            table_id: ST_CONSTRAINTS_ID,
            cols: StConstraintFields::ConstraintId.into(),
            index_name: "constraint_id_idx".into(),
            is_unique: true,
            index_type: IndexType::BTree,
        }],
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
        table_id: ST_MODULE_ID,
        table_name: ST_MODULE_NAME.into(),
        columns: vec![
            ColumnSchema {
                table_id: ST_MODULE_ID,
                col_id: StModuleFields::ProgramHash.col_id(),
                col_name: StModuleFields::ProgramHash.col_name(),
                col_type: AlgebraicType::array(AlgebraicType::U8),
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_MODULE_ID,
                col_id: StModuleFields::Kind.col_id(),
                col_name: StModuleFields::Kind.col_name(),
                col_type: AlgebraicType::U8,
                is_autoinc: false,
            },
            ColumnSchema {
                table_id: ST_MODULE_ID,
                col_id: StModuleFields::Epoch.col_id(),
                col_name: StModuleFields::Epoch.col_name(),
                col_type: AlgebraicType::U128,
                is_autoinc: false,
            },
        ],
        constraints: vec![],
        indexes: vec![],
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
                field: StTableFields::TableType.col_name(),
                expect: format!("`{}` or `{}`", StTableType::System.as_str(), StTableType::User.as_str()),
                found: x.to_string(),
            })?;

        let table_access = row
            .field_as_str(StTableFields::TablesAccess as usize, None)?
            .try_into()
            .map_err(|x: &str| TableError::DecodeField {
                table: ST_TABLES_NAME.into(),
                field: StTableFields::TablesAccess.col_name(),
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

impl From<StTableRow<String>> for ProductValue {
    fn from(x: StTableRow<String>) -> Self {
        product![
            x.table_id,
            x.table_name,
            x.table_type.as_str().to_owned(),
            x.table_access.as_str().to_owned()
        ]
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct StColumnRow<Name: AsRef<str>> {
    pub(crate) table_id: TableId,
    pub(crate) col_id: ColId,
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
        let table_id: TableId = row.field_as_u32(StColumnFields::TableId as usize, None)?.into();
        let col_id = row.field_as_u32(StColumnFields::ColId as usize, None)?.into();
        let col_name = row.field_as_str(StColumnFields::ColName as usize, None)?;
        let bytes = row.field_as_bytes(StColumnFields::ColType as usize, None)?;
        let col_type =
            AlgebraicType::decode(&mut &bytes[..]).map_err(|e| TableError::InvalidSchema(table_id, e.into()))?;
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

impl From<StColumnRow<String>> for ProductValue {
    fn from(x: StColumnRow<String>) -> Self {
        let mut bytes = Vec::new();
        x.col_type.encode(&mut bytes);
        product![
            x.table_id,
            x.col_id,
            x.col_name,
            AlgebraicValue::Bytes(bytes),
            x.is_autoinc,
        ]
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct StIndexRow<Name: AsRef<str>> {
    pub(crate) index_id: IndexId,
    pub(crate) table_id: TableId,
    pub(crate) index_name: Name,
    pub(crate) index_type: IndexType,
    pub(crate) cols: NonEmpty<ColId>,
    pub(crate) is_unique: bool,
}

impl StIndexRow<&str> {
    pub fn to_owned(&self) -> StIndexRow<String> {
        StIndexRow {
            index_id: self.index_id,
            table_id: self.table_id,
            index_name: self.index_name.to_owned(),
            cols: self.cols.clone(),
            is_unique: self.is_unique,
            index_type: self.index_type,
        }
    }
}

fn to_cols(row: &ProductValue, col_pos: ColId, col_name: &'static str) -> Result<NonEmpty<ColId>, DBError> {
    let index = col_pos.0 as usize;
    let cols = row.field_as_array(index, Some(col_name))?;
    if let ArrayValue::U32(x) = &cols {
        let x: Vec<_> = x.iter().map(|x| ColId::from(*x)).collect();
        Ok(NonEmpty::from_slice(&x).unwrap())
    } else {
        Err(InvalidFieldError {
            name: Some(col_name),
            col_pos,
        }
        .into())
    }
}

impl<'a> TryFrom<&'a ProductValue> for StIndexRow<&'a str> {
    type Error = DBError;
    fn try_from(row: &'a ProductValue) -> Result<StIndexRow<&'a str>, DBError> {
        let index_id = row.field_as_u32(StIndexFields::IndexId as usize, None)?.into();
        let table_id = row.field_as_u32(StIndexFields::TableId as usize, None)?.into();
        let index_name = row.field_as_str(StIndexFields::IndexName as usize, None)?;
        let index_type = row.field_as_u8(StIndexFields::IndexType as usize, None)?;
        let index_type = IndexType::try_from(index_type).map_err(|_| InvalidFieldError {
            col_pos: StIndexFields::IndexType.col_id(),
            name: Some(StIndexFields::IndexType.name()),
        })?;
        let columns = to_cols(row, StIndexFields::Columns.col_id(), StIndexFields::Columns.name())?;
        let is_unique = row.field_as_bool(StIndexFields::IsUnique as usize, None)?;
        Ok(StIndexRow {
            index_id,
            table_id,
            index_name,
            index_type,
            cols: columns,
            is_unique,
        })
    }
}

impl From<StIndexRow<String>> for ProductValue {
    fn from(x: StIndexRow<String>) -> Self {
        let cols: Vec<_> = x.cols.iter().map(|x| u32::from(*x)).collect();
        product![
            x.index_id,
            x.table_id,
            x.index_name,
            u8::from(x.index_type),
            ArrayValue::from(cols),
            x.is_unique,
        ]
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct StSequenceRow<Name: AsRef<str>> {
    pub(crate) sequence_id: SequenceId,
    pub(crate) sequence_name: Name,
    pub(crate) table_id: TableId,
    pub(crate) col_id: ColId,
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
        let sequence_id = row.field_as_u32(StSequenceFields::SequenceId as usize, None)?.into();
        let sequence_name = row.field_as_str(StSequenceFields::SequenceName as usize, None)?;
        let table_id = row.field_as_u32(StSequenceFields::TableId as usize, None)?.into();
        let col_id = row.field_as_u32(StSequenceFields::ColId as usize, None)?.into();
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

impl From<StSequenceRow<String>> for ProductValue {
    fn from(x: StSequenceRow<String>) -> Self {
        product![
            x.sequence_id,
            x.sequence_name,
            x.table_id,
            x.col_id,
            x.increment,
            x.start,
            x.min_value,
            x.max_value,
            x.allocated,
        ]
    }
}

impl From<StSequenceRow<String>> for SequenceSchema {
    fn from(sequence: StSequenceRow<String>) -> Self {
        Self {
            sequence_id: sequence.sequence_id,
            sequence_name: sequence.sequence_name,
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
    pub(crate) constraint_id: IndexId,
    pub(crate) constraint_name: Name,
    pub(crate) kind: ColumnIndexAttribute,
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
        let kind = ColumnIndexAttribute::try_from(kind).expect("Fail to decode ColumnIndexAttribute");
        let table_id = row.field_as_u32(StConstraintFields::TableId as usize, None)?.into();
        let columns = to_cols(
            row,
            StConstraintFields::Columns.col_id(),
            StConstraintFields::Columns.name(),
        )?;

        Ok(StConstraintRow {
            constraint_id,
            constraint_name,
            kind,
            table_id,
            columns,
        })
    }
}

impl From<StConstraintRow<String>> for ProductValue {
    fn from(x: StConstraintRow<String>) -> Self {
        let cols: Vec<_> = x.columns.iter().map(|x| u32::from(*x)).collect();

        product![
            x.constraint_id,
            x.constraint_name,
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
