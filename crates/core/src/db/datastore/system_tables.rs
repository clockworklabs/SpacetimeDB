//! Schema definitions and accesses to the system tables,
//! which store metadata about a SpacetimeDB database.
//!
//! When defining a new system table, remember to:
//! - Define constants for its ID and name.
//! - Add it to [`system_tables`], and define a constant for its index there.
//! - Use [`st_fields_enum`] to define its column enum.
//! - Define a function that returns its schema.
//! - Add its schema to [`system_table_schema`].
//! - Define a Rust struct which holds its rows, and implement `TryFrom<RowRef<'_>>` for that struct.

use crate::db::relational_db::RelationalDB;
use crate::error::{DBError, TableError};
use crate::execution_context::ExecutionContext;
use derive_more::From;
use spacetimedb_lib::{Address, Identity, SumType};
use spacetimedb_primitives::*;
use spacetimedb_sats::algebraic_type::fmt::fmt_algebraic_type;
use spacetimedb_sats::db::auth::{StAccess, StTableType};
use spacetimedb_sats::db::def::*;
use spacetimedb_sats::hash::Hash;
use spacetimedb_sats::product_value::InvalidFieldError;
use spacetimedb_sats::{
    impl_deserialize, impl_serialize, product, AlgebraicType, AlgebraicValue, ArrayValue, ProductValue, SumTypeVariant,
    SumValue,
};
use spacetimedb_table::table::RowRef;
use spacetimedb_vm::errors::{ErrorType, ErrorVm};
use spacetimedb_vm::ops::parse;
use std::ops::Deref as _;
use std::str::FromStr;
use strum::Display;

use super::locking_tx_datastore::tx::TxId;
use super::locking_tx_datastore::MutTxId;

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
/// The static ID of the table that defines connected clients
pub(crate) const ST_CLIENTS_ID: TableId = TableId(6);
/// The static ID of the table that defines system variables
pub(crate) const ST_VAR_ID: TableId = TableId(7);

pub(crate) const ST_TABLES_NAME: &str = "st_table";
pub(crate) const ST_COLUMNS_NAME: &str = "st_columns";
pub(crate) const ST_SEQUENCES_NAME: &str = "st_sequence";
pub(crate) const ST_INDEXES_NAME: &str = "st_indexes";
pub(crate) const ST_CONSTRAINTS_NAME: &str = "st_constraints";
pub(crate) const ST_MODULE_NAME: &str = "st_module";
pub(crate) const ST_CLIENTS_NAME: &str = "st_clients";
pub(crate) const ST_VAR_NAME: &str = "st_var";

/// Reserved range of sequence values used for system tables.
///
/// Ids for user-created tables will start at `ST_RESERVED_SEQUENCE_RANGE + 1`.
///
/// The range applies to all sequences allocated by system tables, i.e. table-,
/// sequence-, index-, and constraint-ids.
/// > Note that column-ids are positional indices and not based on a sequence.
///
/// These ids can be referred to statically even for system tables introduced
/// after a database was created, so as long as the range is not exceeded.
///
/// However unlikely it may seem, it is advisable to check for overflow in the
/// test suite when adding sequences to system tables.
pub(crate) const ST_RESERVED_SEQUENCE_RANGE: u32 = 4096;

// This help to keep the correct order when bootstrapping
#[allow(non_camel_case_types)]
#[derive(Debug, Display)]
pub enum SystemTable {
    st_table,
    st_columns,
    st_sequence,
    st_indexes,
    st_constraints,
}
pub(crate) fn system_tables() -> [TableSchema; 8] {
    [
        st_table_schema(),
        st_columns_schema(),
        st_indexes_schema(),
        st_constraints_schema(),
        st_module_schema(),
        st_clients_schema(),
        st_var_schema(),
        // Is important this is always last, so the starting sequence for each
        // system table is correct.
        st_sequences_schema(),
    ]
}

// The following are indices into the array returned by [`system_tables`].
pub(crate) const ST_TABLES_IDX: usize = 0;
pub(crate) const ST_COLUMNS_IDX: usize = 1;
pub(crate) const ST_INDEXES_IDX: usize = 2;
pub(crate) const ST_CONSTRAINTS_IDX: usize = 3;
pub(crate) const ST_MODULE_IDX: usize = 4;
pub(crate) const ST_CLIENT_IDX: usize = 5;
pub(crate) const ST_VAR_IDX: usize = 6;
pub(crate) const ST_SEQUENCES_IDX: usize = 7;
macro_rules! st_fields_enum {
    ($(#[$attr:meta])* enum $ty_name:ident { $($name:expr, $var:ident = $discr:expr,)* }) => {
        #[derive(Copy, Clone, Debug)]
        $(#[$attr])*
        pub enum $ty_name {
            $($var = $discr,)*
        }

        impl $ty_name {
            #[inline]
            pub fn col_id(self) -> ColId {
                ColId(self as u32)
            }

            #[inline]
            pub fn col_idx(self) -> usize {
                self.col_id().idx()
            }

            #[inline]
            pub fn col_name(self) -> Box<str> {
                self.name().into()
            }

            #[inline]
            pub fn name(self) -> &'static str {
                match self {
                    $(Self::$var => $name,)*
                }
            }
        }

        impl From<$ty_name> for ColId {
            fn from(value: $ty_name) -> Self {
                value.col_id()
            }
        }

        impl From<$ty_name> for ColList {
            fn from(value: $ty_name) -> Self {
                ColList::new(value.col_id())
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
    "col_pos", ColPos = 1,
    "col_name", ColName = 2,
    "col_type", ColType = 3,
});
// WARNING: For a stable schema, don't change the field names and discriminants.
st_fields_enum!(enum StIndexFields {
    "index_id", IndexId = 0,
    "table_id", TableId = 1,
    "index_name", IndexName = 2,
    "columns", Columns = 3,
    "is_unique", IsUnique = 4,
    "index_type", IndexType = 5,
});
// WARNING: For a stable schema, don't change the field names and discriminants.
st_fields_enum!(
    /// The fields that define the internal table [crate::db::relational_db::ST_SEQUENCES_NAME].
    enum StSequenceFields {
    "sequence_id", SequenceId = 0,
    "sequence_name", SequenceName = 1,
    "table_id", TableId = 2,
    "col_pos", ColPos = 3,
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
    "constraints", Constraints = 2,
    "table_id", TableId = 3,
    "columns", Columns = 4,
});
// WARNING: For a stable schema, don't change the field names and discriminants.
st_fields_enum!(enum StModuleFields {
    "database_address", DatabaseAddress = 0,
    "owner_identity", OwnerIdentity = 1,
    "program_kind", ProgramKind = 2,
    "program_hash", ProgramHash = 3,
    "program_bytes", ProgramBytes = 4,
});
// WARNING: For a stable schema, don't change the field names and discriminants.
st_fields_enum!(enum StClientsFields {
    "identity", Identity = 0,
    "address", Address = 1,
});
// WARNING: For a stable schema, don't change the field names and discriminants.
st_fields_enum!(enum StVarFields {
    "name", Name = 0,
    "value", Value = 1,
});

/// System Table [ST_TABLES_NAME]
///
/// | table_id | table_name  | table_type | table_access |
/// |----------|-------------|----------- |------------- |
/// | 4        | "customers" | "user"     | "public"     |
fn st_table_schema() -> TableSchema {
    TableDef::new(
        ST_TABLES_NAME.into(),
        vec![
            ColumnDef::sys(StTableFields::TableId.name(), AlgebraicType::U32),
            ColumnDef::sys(StTableFields::TableName.name(), AlgebraicType::String),
            ColumnDef::sys(StTableFields::TableType.name(), AlgebraicType::String),
            ColumnDef::sys(StTableFields::TablesAccess.name(), AlgebraicType::String),
        ],
    )
    .with_type(StTableType::System)
    .with_column_constraint(Constraints::primary_key_auto(), StTableFields::TableId)
    .with_column_index(StTableFields::TableName, true)
    .into_schema(ST_TABLES_ID)
}

/// System Table [ST_COLUMNS_NAME]
///
/// | table_id | col_id | col_name | col_type            |
/// |----------|---------|----------|--------------------|
/// | 1        | 0       | "id"     | AlgebraicType::U32 |
fn st_columns_schema() -> TableSchema {
    TableDef::new(
        ST_COLUMNS_NAME.into(),
        vec![
            ColumnDef::sys(StColumnFields::TableId.name(), AlgebraicType::U32),
            ColumnDef::sys(StColumnFields::ColPos.name(), AlgebraicType::U32),
            ColumnDef::sys(StColumnFields::ColName.name(), AlgebraicType::String),
            ColumnDef::sys(StColumnFields::ColType.name(), AlgebraicType::bytes()),
        ],
    )
    .with_type(StTableType::System)
    .with_column_constraint(Constraints::unique(), {
        let mut cols = ColList::new(StColumnFields::TableId.col_id());
        cols.push(StColumnFields::ColPos.col_id());
        cols
    })
    .into_schema(ST_COLUMNS_ID)
}

/// System Table [ST_INDEXES]
///
/// | index_id | table_id | index_name  | columns | is_unique | index_type |
/// |----------|----------|-------------|---------|-----------|------------|
/// | 1        |          | "ix_sample" | [1]     | false     | "btree"    |
fn st_indexes_schema() -> TableSchema {
    TableDef::new(
        ST_INDEXES_NAME.into(),
        vec![
            ColumnDef::sys(StIndexFields::IndexId.name(), AlgebraicType::U32),
            ColumnDef::sys(StIndexFields::TableId.name(), AlgebraicType::U32),
            ColumnDef::sys(StIndexFields::IndexName.name(), AlgebraicType::String),
            ColumnDef::sys(StIndexFields::Columns.name(), AlgebraicType::array(AlgebraicType::U32)),
            ColumnDef::sys(StIndexFields::IsUnique.name(), AlgebraicType::Bool),
            ColumnDef::sys(StIndexFields::IndexType.name(), AlgebraicType::U8),
        ],
    )
    .with_type(StTableType::System)
    // TODO: Unique constraint on index name?
    .with_column_constraint(Constraints::primary_key_auto(), StIndexFields::IndexId)
    .into_schema(ST_INDEXES_ID)
}

/// System Table [ST_SEQUENCES]
///
/// | sequence_id | sequence_name     | increment | start | min_value | max_value | table_id | col_pos| allocated |
/// |-------------|-------------------|-----------|-------|-----------|-----------|----------|--------|-----------|
/// | 1           | "seq_customer_id" | 1         | 100   | 10        | 1200      | 1        | 1      | 200       |
fn st_sequences_schema() -> TableSchema {
    TableDef::new(
        ST_SEQUENCES_NAME.into(),
        vec![
            ColumnDef::sys(StSequenceFields::SequenceId.name(), AlgebraicType::U32),
            ColumnDef::sys(StSequenceFields::SequenceName.name(), AlgebraicType::String),
            ColumnDef::sys(StSequenceFields::TableId.name(), AlgebraicType::U32),
            ColumnDef::sys(StSequenceFields::ColPos.name(), AlgebraicType::U32),
            ColumnDef::sys(StSequenceFields::Increment.name(), AlgebraicType::I128),
            ColumnDef::sys(StSequenceFields::Start.name(), AlgebraicType::I128),
            ColumnDef::sys(StSequenceFields::MinValue.name(), AlgebraicType::I128),
            ColumnDef::sys(StSequenceFields::MaxValue.name(), AlgebraicType::I128),
            ColumnDef::sys(StSequenceFields::Allocated.name(), AlgebraicType::I128),
        ],
    )
    .with_type(StTableType::System)
    // TODO: Unique constraint on sequence name?
    .with_column_constraint(Constraints::primary_key_auto(), StSequenceFields::SequenceId)
    .into_schema(ST_SEQUENCES_ID)
}

/// System Table [ST_CONSTRAINTS_NAME]
///
/// | constraint_id | constraint_name      | constraints | table_id | columns |
/// |---------------|-------------------- -|-------------|-------|------------|
/// | 1             | "unique_customer_id" | 1           | 100   | [1, 4]     |
fn st_constraints_schema() -> TableSchema {
    TableDef::new(
        ST_CONSTRAINTS_NAME.into(),
        vec![
            ColumnDef::sys(StConstraintFields::ConstraintId.name(), AlgebraicType::U32),
            ColumnDef::sys(StConstraintFields::ConstraintName.name(), AlgebraicType::String),
            ColumnDef::sys(StConstraintFields::Constraints.name(), AlgebraicType::U8),
            ColumnDef::sys(StConstraintFields::TableId.name(), AlgebraicType::U32),
            ColumnDef::sys(
                StConstraintFields::Columns.name(),
                AlgebraicType::array(AlgebraicType::U32),
            ),
        ],
    )
    .with_type(StTableType::System)
    .with_column_constraint(Constraints::primary_key_auto(), StConstraintFields::ConstraintId)
    .into_schema(ST_CONSTRAINTS_ID)
}

/// System table [ST_MODULE_NAME]
///
/// This table holds exactly one row, describing the latest version of the
/// SpacetimeDB module associated with the database:
///
/// * `database_address` is the [`Address`] of the database.
/// * `owner_identity` is the [`Identity`] of the owner of the database.
/// * `program_kind` is the [`ModuleKind`] (currently always [`WASM_MODULE`]).
/// * `program_hash` is the [`Hash`] of the raw bytes of the (compiled) module.
/// * `program_bytes` are the raw bytes of the (compiled) module.
///
/// | database_address | owner_identity |  program_kind | program_bytes | program_hash        |
/// |------------------|----------------|---------------|---------------|---------------------|
/// | <bytes>          | <bytes>        |  0            | <bytes>       | <bytes>             |
pub(crate) fn st_module_schema() -> TableSchema {
    TableDef::new(
        ST_MODULE_NAME.into(),
        vec![
            ColumnDef::sys(StModuleFields::DatabaseAddress.name(), AlgebraicType::bytes()),
            ColumnDef::sys(StModuleFields::OwnerIdentity.name(), AlgebraicType::bytes()),
            ColumnDef::sys(StModuleFields::ProgramKind.name(), AlgebraicType::U8),
            ColumnDef::sys(StModuleFields::ProgramHash.name(), AlgebraicType::bytes()),
            ColumnDef::sys(StModuleFields::ProgramBytes.name(), AlgebraicType::bytes()),
        ],
    )
    .with_type(StTableType::System)
    .into_schema(ST_MODULE_ID)
}

/// System table [ST_CLIENTS_NAME]
///
// identity                                                                                | address
// -----------------------------------------------------------------------------------------+--------------------------------------------------------
//  (__identity_bytes = 0x7452047061ea2502003412941d85a42f89b0702588b823ab55fc4f12e9ea8363) | (__address_bytes = 0x6bdea3ab517f5857dc9b1b5fe99e1b14)
fn st_clients_schema() -> TableSchema {
    TableDef::new(
        ST_CLIENTS_NAME.into(),
        vec![
            ColumnDef::sys(StClientsFields::Identity.name(), Identity::get_type()),
            ColumnDef::sys(StClientsFields::Address.name(), Address::get_type()),
        ],
    )
    .with_type(StTableType::System)
    .with_column_index(col_list![StClientsFields::Identity, StClientsFields::Address], true)
    .into_schema(ST_CLIENTS_ID)
}

/// System Table [ST_VAR_NAME]
///
/// | name        | value     |
/// |-------------|-----------|
/// | "row_limit" | (U64 = 5) |
pub fn st_var_schema() -> TableSchema {
    TableDef::new(
        ST_VAR_NAME.into(),
        vec![
            ColumnDef::sys(StVarFields::Name.name(), AlgebraicType::String),
            ColumnDef::sys(StVarFields::Value.name(), StVarValue::type_of()),
        ],
    )
    .with_type(StTableType::System)
    .with_column_constraint(Constraints::primary_key(), StVarFields::Name)
    .into_schema(ST_VAR_ID)
}

/// If `table_id` refers to a known system table, return its schema.
///
/// Used when restoring from a snapshot; system tables are reinstantiated with this schema,
/// whereas user tables are reinstantiated with a schema computed from the snapshotted system tables.
///
/// This must be kept in sync with the set of system tables.
pub(crate) fn system_table_schema(table_id: TableId) -> Option<TableSchema> {
    match table_id {
        ST_TABLES_ID => Some(st_table_schema()),
        ST_COLUMNS_ID => Some(st_columns_schema()),
        ST_SEQUENCES_ID => Some(st_sequences_schema()),
        ST_INDEXES_ID => Some(st_indexes_schema()),
        ST_CONSTRAINTS_ID => Some(st_constraints_schema()),
        ST_MODULE_ID => Some(st_module_schema()),
        ST_CLIENTS_ID => Some(st_clients_schema()),
        ST_VAR_ID => Some(st_var_schema()),
        _ => None,
    }
}

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

impl TryFrom<RowRef<'_>> for StTableRow<Box<str>> {
    type Error = DBError;
    // TODO(cloutiertyler): Noa, can we just decorate `StTableRow` with Deserialize or something instead?
    fn try_from(row: RowRef<'_>) -> Result<Self, DBError> {
        let table_type = row
            .read_col::<Box<str>>(StTableFields::TableType)?
            .deref()
            .try_into()
            .map_err(|x: &str| TableError::DecodeField {
                table: ST_TABLES_NAME.into(),
                field: StTableFields::TableType.col_name(),
                expect: format!("`{}` or `{}`", StTableType::System.as_str(), StTableType::User.as_str()),
                found: x.to_string(),
            })?;

        let table_access = row
            .read_col::<Box<str>>(StTableFields::TablesAccess)?
            .deref()
            .try_into()
            .map_err(|x: &str| TableError::DecodeField {
                table: ST_TABLES_NAME.into(),
                field: StTableFields::TablesAccess.col_name(),
                expect: format!("`{}` or `{}`", StAccess::Public.as_str(), StAccess::Private.as_str()),
                found: x.to_string(),
            })?;

        Ok(StTableRow {
            table_id: row.read_col(StTableFields::TableId)?,
            table_name: row.read_col(StTableFields::TableName)?,
            table_type,
            table_access,
        })
    }
}

impl From<StTableRow<Box<str>>> for ProductValue {
    fn from(x: StTableRow<Box<str>>) -> Self {
        product![
            x.table_id,
            x.table_name,
            <Box<str>>::from(x.table_type.as_str()),
            <Box<str>>::from(x.table_access.as_str()),
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StColumnRow<Name: AsRef<str>> {
    pub(crate) table_id: TableId,
    pub(crate) col_pos: ColId,
    pub(crate) col_name: Name,
    pub(crate) col_type: AlgebraicType,
}

impl TryFrom<RowRef<'_>> for StColumnRow<Box<str>> {
    type Error = DBError;
    fn try_from(row: RowRef<'_>) -> Result<Self, DBError> {
        let table_id = row.read_col(StColumnFields::TableId)?;
        let bytes = row.read_col::<AlgebraicValue>(StColumnFields::ColType)?;
        let bytes = bytes.as_bytes().unwrap_or_default();
        let col_type =
            AlgebraicType::decode(&mut &*bytes).map_err(|e| TableError::InvalidSchema(table_id, e.into()))?;

        Ok(StColumnRow {
            col_pos: row.read_col(StColumnFields::ColPos)?,
            col_name: row.read_col(StColumnFields::ColName)?,
            table_id,
            col_type,
        })
    }
}

impl From<StColumnRow<Box<str>>> for ProductValue {
    fn from(x: StColumnRow<Box<str>>) -> Self {
        let mut bytes = Vec::new();
        x.col_type.encode(&mut bytes);
        product![x.table_id, x.col_pos, x.col_name, AlgebraicValue::Bytes(bytes.into())]
    }
}

impl From<StColumnRow<Box<str>>> for ColumnSchema {
    fn from(column: StColumnRow<Box<str>>) -> Self {
        Self {
            table_id: column.table_id,
            col_pos: column.col_pos,
            col_name: column.col_name,
            col_type: column.col_type,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StIndexRow<Name: AsRef<str>> {
    pub(crate) index_id: IndexId,
    pub(crate) table_id: TableId,
    pub(crate) index_name: Name,
    pub(crate) columns: ColList,
    pub(crate) is_unique: bool,
    pub(crate) index_type: IndexType,
}

fn to_cols(row: RowRef<'_>, col_pos: impl Into<ColId>, col_name: &'static str) -> Result<ColList, DBError> {
    let col_pos = col_pos.into();
    let name = Some(col_name);
    let cols = row.read_col(col_pos)?;
    if let ArrayValue::U32(x) = &cols {
        Ok(x.iter()
            .map(|x| ColId::from(*x))
            .collect::<ColListBuilder>()
            .build()
            .expect("empty ColList"))
    } else {
        Err(InvalidFieldError { name, col_pos }.into())
    }
}

impl TryFrom<RowRef<'_>> for StIndexRow<Box<str>> {
    type Error = DBError;
    fn try_from(row: RowRef<'_>) -> Result<Self, DBError> {
        let index_type = row.read_col::<u8>(StIndexFields::IndexType)?;
        let index_type = IndexType::try_from(index_type).map_err(|_| InvalidFieldError {
            col_pos: StIndexFields::IndexType.col_id(),
            name: Some(StIndexFields::IndexType.name()),
        })?;
        Ok(StIndexRow {
            index_id: row.read_col(StIndexFields::IndexId)?,
            table_id: row.read_col(StIndexFields::TableId)?,
            index_name: row.read_col(StIndexFields::IndexName)?,
            columns: to_cols(row, StIndexFields::Columns, StIndexFields::Columns.name())?,
            is_unique: row.read_col(StIndexFields::IsUnique)?,
            index_type,
        })
    }
}

impl From<StIndexRow<Box<str>>> for ProductValue {
    fn from(x: StIndexRow<Box<str>>) -> Self {
        product![
            x.index_id,
            x.table_id,
            x.index_name,
            ArrayValue::from(x.columns.to_u32_vec()),
            x.is_unique,
            u8::from(x.index_type),
        ]
    }
}

impl From<StIndexRow<Box<str>>> for IndexSchema {
    fn from(x: StIndexRow<Box<str>>) -> Self {
        Self {
            index_id: x.index_id,
            table_id: x.table_id,
            index_type: x.index_type,
            index_name: x.index_name,
            is_unique: x.is_unique,
            columns: x.columns,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

impl TryFrom<RowRef<'_>> for StSequenceRow<Box<str>> {
    type Error = DBError;
    fn try_from(row: RowRef<'_>) -> Result<Self, DBError> {
        Ok(StSequenceRow {
            sequence_id: row.read_col(StSequenceFields::SequenceId)?,
            sequence_name: row.read_col(StSequenceFields::SequenceName)?,
            table_id: row.read_col(StSequenceFields::TableId)?,
            col_pos: row.read_col(StSequenceFields::ColPos)?,
            increment: row.read_col(StSequenceFields::Increment)?,
            start: row.read_col(StSequenceFields::Start)?,
            min_value: row.read_col(StSequenceFields::MinValue)?,
            max_value: row.read_col(StSequenceFields::MaxValue)?,
            allocated: row.read_col(StSequenceFields::Allocated)?,
        })
    }
}

impl From<StSequenceRow<Box<str>>> for ProductValue {
    fn from(x: StSequenceRow<Box<str>>) -> Self {
        product![
            x.sequence_id,
            x.sequence_name,
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

impl From<StSequenceRow<Box<str>>> for SequenceSchema {
    fn from(sequence: StSequenceRow<Box<str>>) -> Self {
        Self {
            sequence_id: sequence.sequence_id,
            sequence_name: sequence.sequence_name,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StConstraintRow<Name: AsRef<str>> {
    pub(crate) constraint_id: ConstraintId,
    pub(crate) constraint_name: Name,
    pub(crate) constraints: Constraints,
    pub(crate) table_id: TableId,
    pub(crate) columns: ColList,
}

impl TryFrom<RowRef<'_>> for StConstraintRow<Box<str>> {
    type Error = DBError;
    fn try_from(row: RowRef<'_>) -> Result<Self, DBError> {
        let constraints = row.read_col::<u8>(StConstraintFields::Constraints)?;
        let constraints = Constraints::try_from(constraints).expect("Fail to decode Constraints");
        let columns = to_cols(row, StConstraintFields::Columns, StConstraintFields::Columns.name())?;
        Ok(StConstraintRow {
            table_id: row.read_col(StConstraintFields::TableId)?,
            constraint_id: row.read_col(StConstraintFields::ConstraintId)?,
            constraint_name: row.read_col(StConstraintFields::ConstraintName)?,
            constraints,
            columns,
        })
    }
}

impl From<StConstraintRow<Box<str>>> for ProductValue {
    fn from(x: StConstraintRow<Box<str>>) -> Self {
        product![
            x.constraint_id,
            x.constraint_name,
            x.constraints.bits(),
            x.table_id,
            ArrayValue::from(x.columns.to_u32_vec())
        ]
    }
}

impl From<StConstraintRow<Box<str>>> for ConstraintSchema {
    fn from(x: StConstraintRow<Box<str>>) -> Self {
        Self {
            constraint_id: x.constraint_id,
            constraint_name: x.constraint_name,
            constraints: x.constraints,
            table_id: x.table_id,
            columns: x.columns,
        }
    }
}

/// Indicates the kind of module the `program_bytes` of a [`StModuleRow`]
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StModuleRow {
    pub(crate) database_address: Address,
    pub(crate) owner_identity: Identity,
    pub(crate) program_kind: ModuleKind,
    pub(crate) program_hash: Hash,
    pub(crate) program_bytes: Box<[u8]>,
}

pub fn read_st_module_bytes_col(row: RowRef<'_>, col: StModuleFields) -> Result<Box<[u8]>, DBError> {
    let bytes = row.read_col::<ArrayValue>(col.col_id())?;
    if let ArrayValue::U8(bytes) = bytes {
        Ok(bytes)
    } else {
        Err(InvalidFieldError {
            name: Some(col.name()),
            col_pos: col.col_id(),
        }
        .into())
    }
}

impl TryFrom<RowRef<'_>> for StModuleRow {
    type Error = DBError;

    fn try_from(row: RowRef<'_>) -> Result<Self, Self::Error> {
        let database_address =
            read_st_module_bytes_col(row, StModuleFields::DatabaseAddress).map(Address::from_slice)?;
        let owner_identity =
            read_st_module_bytes_col(row, StModuleFields::OwnerIdentity).map(|bytes| Identity::from_slice(&bytes))?;
        let program_kind = row.read_col::<u8>(StModuleFields::ProgramKind).map(ModuleKind)?;
        let program_hash =
            read_st_module_bytes_col(row, StModuleFields::ProgramHash).map(|bytes| Hash::from_slice(&bytes))?;
        let program_bytes = read_st_module_bytes_col(row, StModuleFields::ProgramBytes)?;

        Ok(Self {
            owner_identity,
            database_address,
            program_kind,
            program_hash,
            program_bytes,
        })
    }
}

impl From<StModuleRow> for ProductValue {
    fn from(
        StModuleRow {
            owner_identity,
            database_address,
            program_kind: ModuleKind(program_kind),
            program_hash,
            program_bytes,
        }: StModuleRow,
    ) -> Self {
        product![
            AlgebraicValue::Bytes((*database_address.as_slice()).into()),
            AlgebraicValue::Bytes((*owner_identity.as_bytes()).into()),
            program_kind,
            AlgebraicValue::Bytes(program_hash.as_slice().into()),
            AlgebraicValue::Bytes(program_bytes)
        ]
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StClientsRow {
    pub(crate) identity: Identity,
    pub(crate) address: Address,
}

impl From<&StClientsRow> for ProductValue {
    fn from(x: &StClientsRow) -> Self {
        product![x.identity, x.address]
    }
}

/// A handle for reading system variables from `st_var`
pub struct StVarTable;

impl StVarTable {
    /// Read the value of [ST_VARNAME_ROW_LIMIT] from `st_var`
    pub fn row_limit(ctx: &ExecutionContext, db: &RelationalDB, tx: &TxId) -> Result<Option<u64>, DBError> {
        if let Some(StVarValue::U64(limit)) = Self::read_var(ctx, db, tx, StVarName::RowLimit)? {
            return Ok(Some(limit));
        }
        Ok(None)
    }

    /// Read the value of [ST_VARNAME_SLOW_QRY] from `st_var`
    pub fn query_limit(ctx: &ExecutionContext, db: &RelationalDB, tx: &TxId) -> Result<Option<u64>, DBError> {
        if let Some(StVarValue::U64(ms)) = Self::read_var(ctx, db, tx, StVarName::SlowQryThreshold)? {
            return Ok(Some(ms));
        }
        Ok(None)
    }

    /// Read the value of [ST_VARNAME_SLOW_SUB] from `st_var`
    pub fn sub_limit(ctx: &ExecutionContext, db: &RelationalDB, tx: &TxId) -> Result<Option<u64>, DBError> {
        if let Some(StVarValue::U64(ms)) = Self::read_var(ctx, db, tx, StVarName::SlowSubThreshold)? {
            return Ok(Some(ms));
        }
        Ok(None)
    }

    /// Read the value of [ST_VARNAME_SLOW_INC] from `st_var`
    pub fn incr_limit(ctx: &ExecutionContext, db: &RelationalDB, tx: &TxId) -> Result<Option<u64>, DBError> {
        if let Some(StVarValue::U64(ms)) = Self::read_var(ctx, db, tx, StVarName::SlowIncThreshold)? {
            return Ok(Some(ms));
        }
        Ok(None)
    }

    /// Read the value of a system variable from `st_var`
    pub fn read_var(
        ctx: &ExecutionContext,
        db: &RelationalDB,
        tx: &TxId,
        name: StVarName,
    ) -> Result<Option<StVarValue>, DBError> {
        if let Some(row_ref) = db
            .iter_by_col_eq(ctx, tx, ST_VAR_ID, StVarFields::Name.col_id(), &name.into())?
            .next()
        {
            return Ok(Some(StVarRow::try_from(row_ref)?.value));
        }
        Ok(None)
    }

    /// Update the value of a system variable in `st_var`
    pub fn write_var(
        ctx: &ExecutionContext,
        db: &RelationalDB,
        tx: &mut MutTxId,
        name: StVarName,
        literal: &str,
    ) -> Result<(), DBError> {
        let value = Self::parse_var(name, literal)?;
        if let Some(row_ref) = db
            .iter_by_col_eq_mut(ctx, tx, ST_VAR_ID, StVarFields::Name.col_id(), &name.into())?
            .next()
        {
            db.delete(tx, ST_VAR_ID, [row_ref.pointer()]);
        }
        db.insert(tx, ST_VAR_ID, ProductValue::from(StVarRow { name, value }))?;
        Ok(())
    }

    /// Parse the literal representation of a system variable
    fn parse_var(name: StVarName, literal: &str) -> Result<StVarValue, DBError> {
        StVarValue::try_from_primitive(parse::parse(literal, &name.type_of())?).map_err(|v| {
            ErrorVm::Type(ErrorType::Parse {
                value: literal.to_string(),
                ty: fmt_algebraic_type(&name.type_of()).to_string(),
                err: format!("error parsing value: {:?}", v),
            })
            .into()
        })
    }
}

/// A row in the system table `st_var`
pub struct StVarRow {
    pub name: StVarName,
    pub value: StVarValue,
}

impl StVarRow {
    pub fn type_of() -> AlgebraicType {
        AlgebraicType::product([("name", AlgebraicType::String), ("value", StVarValue::type_of())])
    }
}

impl From<StVarRow> for ProductValue {
    fn from(var: StVarRow) -> Self {
        product!(var.name, var.value)
    }
}

impl From<StVarRow> for AlgebraicValue {
    fn from(row: StVarRow) -> Self {
        AlgebraicValue::Product(row.into())
    }
}

/// A system variable that defines a row limit for queries and subscriptions.
/// If the cardinality of a query is estimated to exceed this limit,
/// it will be rejected before being executed.
pub const ST_VARNAME_ROW_LIMIT: &str = "row_limit";
/// A system variable that defines a threshold for logging slow queries.
pub const ST_VARNAME_SLOW_QRY: &str = "slow_ad_hoc_query_ms";
/// A system variable that defines a threshold for logging slow subscriptions.
pub const ST_VARNAME_SLOW_SUB: &str = "slow_subscription_query_ms";
/// A system variable that defines a threshold for logging slow tx updates.
pub const ST_VARNAME_SLOW_INC: &str = "slow_tx_update_ms";

/// The name of a system variable in `st_var`
#[derive(Debug, Clone, Copy)]
pub enum StVarName {
    RowLimit,
    SlowQryThreshold,
    SlowSubThreshold,
    SlowIncThreshold,
}

impl From<StVarName> for AlgebraicValue {
    fn from(value: StVarName) -> Self {
        match value {
            StVarName::RowLimit => AlgebraicValue::String(ST_VARNAME_ROW_LIMIT.to_string().into_boxed_str()),
            StVarName::SlowQryThreshold => AlgebraicValue::String(ST_VARNAME_SLOW_QRY.to_string().into_boxed_str()),
            StVarName::SlowSubThreshold => AlgebraicValue::String(ST_VARNAME_SLOW_SUB.to_string().into_boxed_str()),
            StVarName::SlowIncThreshold => AlgebraicValue::String(ST_VARNAME_SLOW_INC.to_string().into_boxed_str()),
        }
    }
}

impl FromStr for StVarName {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            ST_VARNAME_ROW_LIMIT => Ok(StVarName::RowLimit),
            ST_VARNAME_SLOW_QRY => Ok(StVarName::SlowQryThreshold),
            ST_VARNAME_SLOW_SUB => Ok(StVarName::SlowSubThreshold),
            ST_VARNAME_SLOW_INC => Ok(StVarName::SlowIncThreshold),
            _ => Err(anyhow::anyhow!("Invalid system variable {}", s)),
        }
    }
}

impl StVarName {
    pub fn type_of(&self) -> AlgebraicType {
        match self {
            StVarName::RowLimit
            | StVarName::SlowQryThreshold
            | StVarName::SlowSubThreshold
            | StVarName::SlowIncThreshold => AlgebraicType::U64,
        }
    }
}

/// The value of a system variable in `st_var`
#[derive(Debug, Clone, From)]
pub enum StVarValue {
    Bool(bool),
    I8(i8),
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    I128(i128),
    U128(u128),
    F32(f32),
    F64(f64),
    String(Box<str>),
}

impl StVarValue {
    pub fn type_of() -> AlgebraicType {
        AlgebraicType::Sum(SumType::new(Box::new([
            SumTypeVariant::new_named(AlgebraicType::Bool, "Bool"),
            SumTypeVariant::new_named(AlgebraicType::I8, "I8"),
            SumTypeVariant::new_named(AlgebraicType::U8, "U8"),
            SumTypeVariant::new_named(AlgebraicType::I16, "I16"),
            SumTypeVariant::new_named(AlgebraicType::U16, "U16"),
            SumTypeVariant::new_named(AlgebraicType::I32, "I32"),
            SumTypeVariant::new_named(AlgebraicType::U32, "U32"),
            SumTypeVariant::new_named(AlgebraicType::I64, "I64"),
            SumTypeVariant::new_named(AlgebraicType::U64, "U64"),
            SumTypeVariant::new_named(AlgebraicType::I128, "I128"),
            SumTypeVariant::new_named(AlgebraicType::U128, "U128"),
            SumTypeVariant::new_named(AlgebraicType::F32, "F32"),
            SumTypeVariant::new_named(AlgebraicType::F64, "F64"),
            SumTypeVariant::new_named(AlgebraicType::String, "String"),
        ])))
    }

    pub fn try_from_primitive(value: AlgebraicValue) -> Result<Self, AlgebraicValue> {
        match value {
            AlgebraicValue::Bool(v) => Ok(StVarValue::Bool(v)),
            AlgebraicValue::I8(v) => Ok(StVarValue::I8(v)),
            AlgebraicValue::U8(v) => Ok(StVarValue::U8(v)),
            AlgebraicValue::I16(v) => Ok(StVarValue::I16(v)),
            AlgebraicValue::U16(v) => Ok(StVarValue::U16(v)),
            AlgebraicValue::I32(v) => Ok(StVarValue::I32(v)),
            AlgebraicValue::U32(v) => Ok(StVarValue::U32(v)),
            AlgebraicValue::I64(v) => Ok(StVarValue::I64(v)),
            AlgebraicValue::U64(v) => Ok(StVarValue::U64(v)),
            AlgebraicValue::I128(v) => Ok(StVarValue::I128(v.0)),
            AlgebraicValue::U128(v) => Ok(StVarValue::U128(v.0)),
            AlgebraicValue::F32(v) => Ok(StVarValue::F32(v.into_inner())),
            AlgebraicValue::F64(v) => Ok(StVarValue::F64(v.into_inner())),
            AlgebraicValue::String(v) => Ok(StVarValue::String(v)),
            _ => Err(value),
        }
    }

    pub fn try_from_sum(value: AlgebraicValue) -> Result<Self, AlgebraicValue> {
        value.into_sum()?.try_into()
    }
}

impl TryFrom<SumValue> for StVarValue {
    type Error = AlgebraicValue;

    fn try_from(sum: SumValue) -> Result<Self, Self::Error> {
        match sum.tag {
            0 => Ok(StVarValue::Bool(sum.value.into_bool()?)),
            1 => Ok(StVarValue::I8(sum.value.into_i8()?)),
            2 => Ok(StVarValue::U8(sum.value.into_u8()?)),
            3 => Ok(StVarValue::I16(sum.value.into_i16()?)),
            4 => Ok(StVarValue::U16(sum.value.into_u16()?)),
            5 => Ok(StVarValue::I32(sum.value.into_i32()?)),
            6 => Ok(StVarValue::U32(sum.value.into_u32()?)),
            7 => Ok(StVarValue::I64(sum.value.into_i64()?)),
            8 => Ok(StVarValue::U64(sum.value.into_u64()?)),
            9 => Ok(StVarValue::I128(sum.value.into_i128()?.0)),
            10 => Ok(StVarValue::U128(sum.value.into_u128()?.0)),
            11 => Ok(StVarValue::F32(sum.value.into_f32()?.into_inner())),
            12 => Ok(StVarValue::F64(sum.value.into_f64()?.into_inner())),
            13 => Ok(StVarValue::String(sum.value.into_string()?)),
            _ => Err(*sum.value),
        }
    }
}

impl From<StVarValue> for AlgebraicValue {
    fn from(value: StVarValue) -> Self {
        AlgebraicValue::Sum(value.into())
    }
}

impl From<StVarValue> for SumValue {
    fn from(value: StVarValue) -> Self {
        match value {
            StVarValue::Bool(v) => SumValue {
                tag: 0,
                value: Box::new(AlgebraicValue::Bool(v)),
            },
            StVarValue::I8(v) => SumValue {
                tag: 1,
                value: Box::new(AlgebraicValue::I8(v)),
            },
            StVarValue::U8(v) => SumValue {
                tag: 2,
                value: Box::new(AlgebraicValue::U8(v)),
            },
            StVarValue::I16(v) => SumValue {
                tag: 3,
                value: Box::new(AlgebraicValue::I16(v)),
            },
            StVarValue::U16(v) => SumValue {
                tag: 4,
                value: Box::new(AlgebraicValue::U16(v)),
            },
            StVarValue::I32(v) => SumValue {
                tag: 5,
                value: Box::new(AlgebraicValue::I32(v)),
            },
            StVarValue::U32(v) => SumValue {
                tag: 6,
                value: Box::new(AlgebraicValue::U32(v)),
            },
            StVarValue::I64(v) => SumValue {
                tag: 7,
                value: Box::new(AlgebraicValue::I64(v)),
            },
            StVarValue::U64(v) => SumValue {
                tag: 8,
                value: Box::new(AlgebraicValue::U64(v)),
            },
            StVarValue::I128(v) => SumValue {
                tag: 9,
                value: Box::new(AlgebraicValue::I128(v.into())),
            },
            StVarValue::U128(v) => SumValue {
                tag: 10,
                value: Box::new(AlgebraicValue::U128(v.into())),
            },
            StVarValue::F32(v) => SumValue {
                tag: 11,
                value: Box::new(AlgebraicValue::F32(v.into())),
            },
            StVarValue::F64(v) => SumValue {
                tag: 12,
                value: Box::new(AlgebraicValue::F64(v.into())),
            },
            StVarValue::String(v) => SumValue {
                tag: 13,
                value: Box::new(AlgebraicValue::String(v)),
            },
        }
    }
}

impl TryFrom<RowRef<'_>> for StVarRow {
    type Error = DBError;

    fn try_from(row: RowRef<'_>) -> Result<Self, Self::Error> {
        // The position of the `value` column in `st_var`
        let col_pos = StVarFields::Value.col_id();

        // An error when reading the `value` column in `st_var`
        let invalid_value = InvalidFieldError {
            col_pos,
            name: Some(StVarFields::Value.name()),
        };

        let name = row.read_col::<Box<str>>(StVarFields::Name.col_id())?;
        let name = StVarName::from_str(&name)?;
        match row.read_col::<AlgebraicValue>(col_pos)? {
            AlgebraicValue::Sum(sum) => Ok(StVarRow {
                name,
                value: sum.try_into().map_err(|_| invalid_value)?,
            }),
            _ => Err(invalid_value.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::db::relational_db::tests_utils::TestDB;

    use super::*;

    #[test]
    fn test_system_variables() {
        let db = TestDB::durable().expect("failed to create db");
        let ctx = ExecutionContext::default();
        let _ = db.with_auto_commit(&ctx, |tx| {
            StVarTable::write_var(&ctx, &db, tx, StVarName::RowLimit, "5")
        });
        assert_eq!(
            5,
            db.with_read_only(&ctx, |tx| StVarTable::row_limit(&ctx, &db, tx))
                .expect("failed to read from st_var")
                .expect("row_limit does not exist")
        );
    }

    #[test]
    fn test_sequences_within_reserved_range() {
        let mut num_tables = 0;
        let mut num_indexes = 0;
        let mut num_constraints = 0;
        let mut num_sequences = 0;

        for table in system_tables() {
            num_tables += 1;
            num_indexes += table.indexes.len();
            num_constraints += table.constraints.len();
            num_sequences += table.sequences.len();
        }

        assert!(
            num_tables <= ST_RESERVED_SEQUENCE_RANGE,
            "number of system tables exceeds reserved sequence range"
        );
        assert!(
            num_indexes <= ST_RESERVED_SEQUENCE_RANGE as usize,
            "number of system indexes exceeds reserved sequence range"
        );
        assert!(
            num_constraints <= ST_RESERVED_SEQUENCE_RANGE as usize,
            "number of system constraints exceeds reserved sequence range"
        );
        assert!(
            num_sequences <= ST_RESERVED_SEQUENCE_RANGE as usize,
            "number of system sequences exceeds reserved sequence range"
        );
    }
}
