//! Schema definitions and accesses to the system tables,
//! which store metadata about a SpacetimeDB database.
//!
//! When defining a new system table, remember to:
//! - Define constants for its ID and name.
//! - Name it in singular (`st_column` not `st_columns`).
//! - Add a type `St(...)Row` to define its schema, deriving SpacetimeType.
//!     - You will probably need to add a new ID type in `spacetimedb_primitives`,
//!       with trait implementations in `spacetimedb_sats::{typespace, de::impl, ser::impl}`.
//! - Add it to [`system_tables`], and define a constant for its index there.
//! - Use [`st_fields_enum`] to define its column enum.
//! - Register its schema in [`system_module_def`], making sure to call `validate_system_table` at the end of the function.

use spacetimedb_lib::db::auth::{StAccess, StTableType};
use spacetimedb_lib::db::raw_def::v9::{btree, RawSql};
use spacetimedb_lib::db::raw_def::*;
use spacetimedb_lib::de::{Deserialize, DeserializeOwned, Error};
use spacetimedb_lib::ser::Serialize;
use spacetimedb_lib::st_var::StVarValue;
use spacetimedb_lib::{ConnectionId, Identity, ProductValue, SpacetimeType};
use spacetimedb_primitives::*;
use spacetimedb_sats::algebraic_value::ser::value_serialize;
use spacetimedb_sats::hash::Hash;
use spacetimedb_sats::product_value::InvalidFieldError;
use spacetimedb_sats::{impl_deserialize, impl_serialize, impl_st, u256, AlgebraicType, AlgebraicValue, ArrayValue};
use spacetimedb_schema::def::{
    BTreeAlgorithm, ConstraintData, DirectAlgorithm, IndexAlgorithm, ModuleDef, UniqueConstraintData,
};
use spacetimedb_schema::schema::{
    ColumnSchema, ConstraintSchema, IndexSchema, RowLevelSecuritySchema, ScheduleSchema, Schema, SequenceSchema,
    TableSchema,
};
use spacetimedb_table::table::RowRef;
use std::cell::RefCell;
use std::str::FromStr;
use strum::Display;
use v9::{RawModuleDefV9Builder, TableType};

use super::error::DatastoreError;

/// The static ID of the table that defines tables
pub(crate) const ST_TABLE_ID: TableId = TableId(1);
/// The static ID of the table that defines columns
pub(crate) const ST_COLUMN_ID: TableId = TableId(2);
/// The static ID of the table that defines sequences
pub(crate) const ST_SEQUENCE_ID: TableId = TableId(3);
/// The static ID of the table that defines indexes
pub(crate) const ST_INDEX_ID: TableId = TableId(4);
/// The static ID of the table that defines constraints
pub(crate) const ST_CONSTRAINT_ID: TableId = TableId(5);
/// The static ID of the table that defines the stdb module associated with
/// the database
pub(crate) const ST_MODULE_ID: TableId = TableId(6);
/// The static ID of the table that defines connected clients
pub(crate) const ST_CLIENT_ID: TableId = TableId(7);
/// The static ID of the table that defines system variables
pub(crate) const ST_VAR_ID: TableId = TableId(8);
/// The static ID of the table that defines scheduled tables
pub(crate) const ST_SCHEDULED_ID: TableId = TableId(9);

/// The static ID of the table that defines the row level security (RLS) policies
pub(crate) const ST_ROW_LEVEL_SECURITY_ID: TableId = TableId(10);
pub(crate) const ST_TABLE_NAME: &str = "st_table";
pub(crate) const ST_COLUMN_NAME: &str = "st_column";
pub(crate) const ST_SEQUENCE_NAME: &str = "st_sequence";
pub(crate) const ST_INDEX_NAME: &str = "st_index";
pub(crate) const ST_CONSTRAINT_NAME: &str = "st_constraint";
pub(crate) const ST_MODULE_NAME: &str = "st_module";
pub(crate) const ST_CLIENT_NAME: &str = "st_client";
pub(crate) const ST_SCHEDULED_NAME: &str = "st_scheduled";
pub(crate) const ST_VAR_NAME: &str = "st_var";
pub(crate) const ST_ROW_LEVEL_SECURITY_NAME: &str = "st_row_level_security";
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
    st_column,
    st_sequence,
    st_index,
    st_constraint,
    st_row_level_security,
}

pub(crate) fn system_tables() -> [TableSchema; 10] {
    [
        // The order should match the `id` of the system table, that start with [ST_TABLE_IDX].
        st_table_schema(),
        st_column_schema(),
        st_index_schema(),
        st_constraint_schema(),
        st_module_schema(),
        st_client_schema(),
        st_var_schema(),
        st_scheduled_schema(),
        st_row_level_security_schema(),
        // Is important this is always last, so the starting sequence for each
        // system table is correct.
        st_sequence_schema(),
    ]
}

/// Types that represent the fields / columns of a system table.
pub trait StFields: Copy + Sized {
    /// Returns the column position of the system table field.
    fn col_id(self) -> ColId;

    /// Returns the column index of the system table field.
    #[inline]
    fn col_idx(self) -> usize {
        self.col_id().idx()
    }

    /// Returns the column name of the system table field a static string slice.
    fn name(self) -> &'static str;

    /// Returns the column name of the system table field as a boxed slice.
    #[inline]
    fn col_name(self) -> Box<str> {
        self.name().into()
    }

    /// Return all fields of this type, in order.
    fn fields() -> &'static [Self];
}

// The following are indices into the array returned by [`system_tables`].
pub(crate) const ST_TABLE_IDX: usize = 0;
pub(crate) const ST_COLUMN_IDX: usize = 1;
pub(crate) const ST_INDEX_IDX: usize = 2;
pub(crate) const ST_CONSTRAINT_IDX: usize = 3;
pub(crate) const ST_MODULE_IDX: usize = 4;
pub(crate) const ST_CLIENT_IDX: usize = 5;
pub(crate) const ST_VAR_IDX: usize = 6;
pub(crate) const ST_SCHEDULED_IDX: usize = 7;
pub(crate) const ST_ROW_LEVEL_SECURITY_IDX: usize = 8;
// Must be the last index in the array.
pub(crate) const ST_SEQUENCE_IDX: usize = 9;

macro_rules! st_fields_enum {
    ($(#[$attr:meta])* enum $ty_name:ident { $($name:expr, $var:ident = $discr:expr,)* }) => {
        #[derive(Copy, Clone, Debug)]
        $(#[$attr])*
        pub enum $ty_name {
            $($var = $discr,)*
        }

        impl StFields for $ty_name {
            #[inline]
            fn col_id(self) -> ColId {
                ColId(self as _)
            }

            #[inline]
            fn name(self) -> &'static str {
                match self {
                    $(Self::$var => $name,)*
                }
            }

            fn fields() -> &'static [$ty_name] {
                &[$($ty_name::$var,)*]
            }
        }

        impl From<$ty_name> for ColId {
            fn from(value: $ty_name) -> Self {
                value.col_id()
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
    "table_primary_key", PrimaryKey = 4,
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
    "index_algorithm", IndexAlgorithm = 3,
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
    "table_id", TableId = 2,
    "constraint_data", ConstraintData = 3,
});
// WARNING: For a stable schema, don't change the field names and discriminants.
st_fields_enum!(enum StRowLevelSecurityFields {
    "table_id", TableId = 0,
    "sql", Sql = 1,
});
// WARNING: For a stable schema, don't change the field names and discriminants.
st_fields_enum!(enum StModuleFields {
    "database_identity", DatabaseIdentity = 0,
    "owner_identity", OwnerIdentity = 1,
    "program_kind", ProgramKind = 2,
    "program_hash", ProgramHash = 3,
    "program_bytes", ProgramBytes = 4,
    "module_version", ModuleVersion = 5,
});
// WARNING: For a stable schema, don't change the field names and discriminants.
st_fields_enum!(enum StClientFields {
    "identity", Identity = 0,
    "connection_id", ConnectionId = 1,
});
// WARNING: For a stable schema, don't change the field names and discriminants.
st_fields_enum!(enum StVarFields {
    "name", Name = 0,
    "value", Value = 1,
});

st_fields_enum!(enum StScheduledFields {
    "schedule_id", ScheduleId = 0,
    "table_id", TableId = 1,
    "reducer_name", ReducerName = 2,
    "schedule_name", ScheduleName = 3,
    "at_column", AtColumn = 4,
});

/// Helper method to check that a system table has the correct fields.
/// Does not check field types since those aren't included in `StFields` types.
/// If anything in here is not true, the system is completely broken, so it's fine to assert.
fn validate_system_table<T: StFields + 'static>(def: &ModuleDef, table_name: &str) {
    let table = def.table(table_name).expect("missing system table definition");
    let fields = T::fields();
    assert_eq!(table.columns.len(), fields.len());
    for field in T::fields() {
        let col = table
            .columns
            .get(field.col_id().idx())
            .expect("missing system table field");
        assert_eq!(&col.name[..], field.name());
    }
}

/// See the comment on [`SYSTEM_MODULE_DEF`].
fn system_module_def() -> ModuleDef {
    let mut builder = RawModuleDefV9Builder::new();

    let st_table_type = builder.add_type::<StTableRow>();
    builder
        .build_table(ST_TABLE_NAME, *st_table_type.as_ref().expect("should be ref"))
        .with_type(TableType::System)
        .with_auto_inc_primary_key(StTableFields::TableId)
        .with_index_no_accessor_name(btree(StTableFields::TableId))
        .with_unique_constraint(StTableFields::TableName)
        .with_index_no_accessor_name(btree(StTableFields::TableName));

    let st_raw_column_type = builder.add_type::<StColumnRow>();
    let st_col_row_unique_cols = [StColumnFields::TableId.col_id(), StColumnFields::ColPos.col_id()];
    builder
        .build_table(ST_COLUMN_NAME, *st_raw_column_type.as_ref().expect("should be ref"))
        .with_type(TableType::System)
        .with_unique_constraint(st_col_row_unique_cols)
        .with_index_no_accessor_name(btree(st_col_row_unique_cols));

    let st_index_type = builder.add_type::<StIndexRow>();
    builder
        .build_table(ST_INDEX_NAME, *st_index_type.as_ref().expect("should be ref"))
        .with_type(TableType::System)
        .with_auto_inc_primary_key(StIndexFields::IndexId)
        .with_index_no_accessor_name(btree(StIndexFields::IndexId));
    // TODO(1.0): unique constraint on name?

    let st_sequence_type = builder.add_type::<StSequenceRow>();
    builder
        .build_table(ST_SEQUENCE_NAME, *st_sequence_type.as_ref().expect("should be ref"))
        .with_type(TableType::System)
        .with_auto_inc_primary_key(StSequenceFields::SequenceId)
        .with_index_no_accessor_name(btree(StSequenceFields::SequenceId));
    // TODO(1.0): unique constraint on name?

    let st_constraint_type = builder.add_type::<StConstraintRow>();
    builder
        .build_table(ST_CONSTRAINT_NAME, *st_constraint_type.as_ref().expect("should be ref"))
        .with_type(TableType::System)
        .with_auto_inc_primary_key(StConstraintFields::ConstraintId)
        .with_index_no_accessor_name(btree(StConstraintFields::ConstraintId));
    // TODO(1.0): unique constraint on name?

    let st_row_level_security_type = builder.add_type::<StRowLevelSecurityRow>();
    builder
        .build_table(
            ST_ROW_LEVEL_SECURITY_NAME,
            *st_row_level_security_type.as_ref().expect("should be ref"),
        )
        .with_type(TableType::System)
        .with_primary_key(StRowLevelSecurityFields::Sql)
        .with_unique_constraint(StRowLevelSecurityFields::Sql)
        .with_index_no_accessor_name(btree(StRowLevelSecurityFields::Sql))
        .with_index_no_accessor_name(btree(StRowLevelSecurityFields::TableId));

    let st_module_type = builder.add_type::<StModuleRow>();
    builder
        .build_table(ST_MODULE_NAME, *st_module_type.as_ref().expect("should be ref"))
        .with_type(TableType::System);
    // TODO: add empty unique constraint here, once we've implemented those.

    let st_client_type = builder.add_type::<StClientRow>();
    let st_client_unique_cols = [StClientFields::Identity, StClientFields::ConnectionId];
    builder
        .build_table(ST_CLIENT_NAME, *st_client_type.as_ref().expect("should be ref"))
        .with_type(TableType::System)
        .with_unique_constraint(st_client_unique_cols) // FIXME: this is a noop?
        .with_index_no_accessor_name(btree(st_client_unique_cols));

    let st_schedule_type = builder.add_type::<StScheduledRow>();
    builder
        .build_table(ST_SCHEDULED_NAME, *st_schedule_type.as_ref().expect("should be ref"))
        .with_type(TableType::System)
        .with_unique_constraint(StScheduledFields::TableId) // FIXME: this is a noop?
        .with_index_no_accessor_name(btree(StScheduledFields::TableId))
        .with_auto_inc_primary_key(StScheduledFields::ScheduleId) // FIXME: this is a noop?
        .with_index_no_accessor_name(btree(StScheduledFields::ScheduleId));
    // TODO(1.0): unique constraint on name?

    let st_var_type = builder.add_type::<StVarRow>();
    builder
        .build_table(ST_VAR_NAME, *st_var_type.as_ref().expect("should be ref"))
        .with_type(TableType::System)
        .with_unique_constraint(StVarFields::Name) // FIXME: this is a noop?
        .with_index_no_accessor_name(btree(StVarFields::Name))
        .with_primary_key(StVarFields::Name);

    let result = builder
        .finish()
        .try_into()
        .expect("system table module is invalid, did you change it or add a validation rule it doesn't meet?");

    validate_system_table::<StTableFields>(&result, ST_TABLE_NAME);
    validate_system_table::<StColumnFields>(&result, ST_COLUMN_NAME);
    validate_system_table::<StIndexFields>(&result, ST_INDEX_NAME);
    validate_system_table::<StSequenceFields>(&result, ST_SEQUENCE_NAME);
    validate_system_table::<StConstraintFields>(&result, ST_CONSTRAINT_NAME);
    validate_system_table::<StRowLevelSecurityFields>(&result, ST_ROW_LEVEL_SECURITY_NAME);
    validate_system_table::<StModuleFields>(&result, ST_MODULE_NAME);
    validate_system_table::<StClientFields>(&result, ST_CLIENT_NAME);
    validate_system_table::<StVarFields>(&result, ST_VAR_NAME);
    validate_system_table::<StScheduledFields>(&result, ST_SCHEDULED_NAME);

    result
}

lazy_static::lazy_static! {
    /// The canonical definition of the system tables.
    ///
    /// It's important not to leak this `ModuleDef` or the `Def`s it contains outside this file.
    /// You should only return `Schema`s from this file, not `Def`s!
    ///
    /// This is because `SYSTEM_MODULE_DEF` has a `Typespace` that is DISTINCT from the typespace used in the client module.
    /// System `TableDef`s refer to this typespace, but client `TableDef`s refer to the client typespace.
    /// This could easily result in confusing errors!
    /// Fortunately, when converting from `TableDef` to `TableSchema`, all `AlgebraicType`s are resolved,
    /// so that they are self-contained and do not refer to any `Typespace`.
    static ref SYSTEM_MODULE_DEF: ModuleDef = system_module_def();
}

fn st_schema(name: &str, id: TableId) -> TableSchema {
    let result = TableSchema::from_module_def(
        &SYSTEM_MODULE_DEF,
        SYSTEM_MODULE_DEF.table(name).expect("missing system table definition"),
        (),
        id,
    );
    result
}

fn st_table_schema() -> TableSchema {
    st_schema(ST_TABLE_NAME, ST_TABLE_ID)
}

fn st_column_schema() -> TableSchema {
    st_schema(ST_COLUMN_NAME, ST_COLUMN_ID)
}

fn st_index_schema() -> TableSchema {
    st_schema(ST_INDEX_NAME, ST_INDEX_ID)
}

fn st_sequence_schema() -> TableSchema {
    st_schema(ST_SEQUENCE_NAME, ST_SEQUENCE_ID)
}

fn st_constraint_schema() -> TableSchema {
    st_schema(ST_CONSTRAINT_NAME, ST_CONSTRAINT_ID)
}

fn st_row_level_security_schema() -> TableSchema {
    st_schema(ST_ROW_LEVEL_SECURITY_NAME, ST_ROW_LEVEL_SECURITY_ID)
}

pub(crate) fn st_module_schema() -> TableSchema {
    st_schema(ST_MODULE_NAME, ST_MODULE_ID)
}

fn st_client_schema() -> TableSchema {
    st_schema(ST_CLIENT_NAME, ST_CLIENT_ID)
}

fn st_scheduled_schema() -> TableSchema {
    st_schema(ST_SCHEDULED_NAME, ST_SCHEDULED_ID)
}

pub fn st_var_schema() -> TableSchema {
    st_schema(ST_VAR_NAME, ST_VAR_ID)
}

/// If `table_id` refers to a known system table, return its schema.
///
/// Used when restoring from a snapshot; system tables are reinstantiated with this schema,
/// whereas user tables are reinstantiated with a schema computed from the snapshotted system tables.
///
/// This must be kept in sync with the set of system tables.
pub(crate) fn system_table_schema(table_id: TableId) -> Option<TableSchema> {
    match table_id {
        ST_TABLE_ID => Some(st_table_schema()),
        ST_COLUMN_ID => Some(st_column_schema()),
        ST_SEQUENCE_ID => Some(st_sequence_schema()),
        ST_INDEX_ID => Some(st_index_schema()),
        ST_CONSTRAINT_ID => Some(st_constraint_schema()),
        ST_ROW_LEVEL_SECURITY_ID => Some(st_row_level_security_schema()),
        ST_MODULE_ID => Some(st_module_schema()),
        ST_CLIENT_ID => Some(st_client_schema()),
        ST_VAR_ID => Some(st_var_schema()),
        ST_SCHEDULED_ID => Some(st_scheduled_schema()),
        _ => None,
    }
}

/// System Table [ST_TABLE_NAME]
///
/// | table_id | table_name  | table_type | table_access |
/// |----------|-------------|----------- |------------- |
/// | 4        | "customers" | "user"     | "public"     |
#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct StTableRow {
    pub(crate) table_id: TableId,
    pub(crate) table_name: Box<str>,
    pub(crate) table_type: StTableType,
    pub(crate) table_access: StAccess,
    /// The primary key of the table.
    /// This is a `ColId` everywhere else, but we make it a `ColList` here
    /// for future compatibility in case we ever have composite primary keys.
    pub(crate) table_primary_key: Option<ColList>,
}

impl TryFrom<RowRef<'_>> for StTableRow {
    type Error = DatastoreError;
    fn try_from(row: RowRef<'_>) -> Result<Self, DatastoreError> {
        read_via_bsatn(row)
    }
}

impl From<StTableRow> for ProductValue {
    fn from(x: StTableRow) -> Self {
        to_product_value(&x)
    }
}

/// A wrapper around `AlgebraicType` that acts like `AlgegbraicType::bytes()` for serialization purposes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AlgebraicTypeViaBytes(pub AlgebraicType);
impl_st!([] AlgebraicTypeViaBytes, AlgebraicType::bytes());
impl<'de> Deserialize<'de> for AlgebraicTypeViaBytes {
    fn deserialize<D: spacetimedb_lib::de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let bytes = <&[u8]>::deserialize(deserializer)?;
        let ty = AlgebraicType::decode(&mut &*bytes).map_err(D::Error::custom)?;
        Ok(AlgebraicTypeViaBytes(ty))
    }
}
thread_local! {
    static ALGEBRAIC_TYPE_WRITE_BUF: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}
impl_serialize!([] AlgebraicTypeViaBytes, (self, ser) => {
    ALGEBRAIC_TYPE_WRITE_BUF.with_borrow_mut(|buf| {
        buf.clear();
        self.0.encode(buf);
        buf[..].serialize(ser)
    })
});
impl From<AlgebraicType> for AlgebraicTypeViaBytes {
    fn from(ty: AlgebraicType) -> Self {
        Self(ty)
    }
}

/// System Table [ST_COLUMN_NAME]
///
/// | table_id | col_id | col_name | col_type            |
/// |----------|---------|----------|--------------------|
/// | 1        | 0       | "id"     | AlgebraicType::U32 |
#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct StColumnRow {
    pub(crate) table_id: TableId,
    pub(crate) col_pos: ColId,
    pub(crate) col_name: Box<str>,
    pub(crate) col_type: AlgebraicTypeViaBytes,
}

impl TryFrom<RowRef<'_>> for StColumnRow {
    type Error = DatastoreError;
    fn try_from(row: RowRef<'_>) -> Result<Self, DatastoreError> {
        read_via_bsatn(row)
    }
}

impl From<StColumnRow> for ProductValue {
    fn from(x: StColumnRow) -> Self {
        to_product_value(&x)
    }
}

impl From<StColumnRow> for ColumnSchema {
    fn from(column: StColumnRow) -> Self {
        Self {
            table_id: column.table_id,
            col_pos: column.col_pos,
            col_name: column.col_name,
            col_type: column.col_type.0,
        }
    }
}

/// System Table [ST_INDEX_NAME]
///
/// | index_id | table_id | index_name  | index_algorithm            |
/// |----------|----------|-------------|----------------------------|
/// | 1        |          | "ix_sample" | btree({"columns": [1, 2]}) |
#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct StIndexRow {
    pub(crate) index_id: IndexId,
    pub(crate) table_id: TableId,
    pub(crate) index_name: Box<str>,
    pub(crate) index_algorithm: StIndexAlgorithm,
}

/// An index algorithm for storing in the system tables.
///
/// It is critical that this type never grow in layout, as it is stored in the system tables.
/// This is checked by (TODO(1.0): add a test!)
///
/// It is forbidden to add data to any of the variants of this type.
/// You have to add a NEW variant.
#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub enum StIndexAlgorithm {
    /// Unused variant to reserve space.
    Unused(u128),

    /// A BTree index.
    BTree { columns: ColList },

    /// A Direct index.
    Direct { column: ColId },
}

impl From<IndexAlgorithm> for StIndexAlgorithm {
    fn from(algorithm: IndexAlgorithm) -> Self {
        match algorithm {
            IndexAlgorithm::BTree(BTreeAlgorithm { columns }) => Self::BTree { columns },
            IndexAlgorithm::Direct(DirectAlgorithm { column }) => Self::Direct { column },
            algo => unreachable!("unexpected `{algo:?}`, did you add a new one?"),
        }
    }
}

impl From<StIndexAlgorithm> for IndexAlgorithm {
    fn from(algorithm: StIndexAlgorithm) -> Self {
        match algorithm {
            StIndexAlgorithm::BTree { columns } => Self::BTree(BTreeAlgorithm { columns }),
            StIndexAlgorithm::Direct { column } => Self::Direct(DirectAlgorithm { column }),
            algo => unreachable!("unexpected `{algo:?}` in system table `st_indexes`"),
        }
    }
}

impl TryFrom<RowRef<'_>> for StIndexRow {
    type Error = DatastoreError;
    fn try_from(row: RowRef<'_>) -> Result<Self, DatastoreError> {
        read_via_bsatn(row)
    }
}

impl From<StIndexRow> for ProductValue {
    fn from(x: StIndexRow) -> Self {
        to_product_value(&x)
    }
}

impl From<StIndexRow> for IndexSchema {
    fn from(x: StIndexRow) -> Self {
        Self {
            index_id: x.index_id,
            table_id: x.table_id,
            index_name: x.index_name,
            index_algorithm: x.index_algorithm.into(),
        }
    }
}

impl From<IndexSchema> for StIndexRow {
    fn from(x: IndexSchema) -> Self {
        Self {
            index_id: x.index_id,
            table_id: x.table_id,
            index_name: x.index_name,
            index_algorithm: x.index_algorithm.into(),
        }
    }
}

/// System Table [ST_SEQUENCE_NAME]
///
/// | sequence_id | sequence_name     | increment | start | min_value | max_value | table_id | col_pos| allocated |
/// |-------------|-------------------|-----------|-------|-----------|-----------|----------|--------|-----------|
/// | 1           | "seq_customer_id" | 1         | 100   | 10        | 1200      | 1        | 1      | 200       |
#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct StSequenceRow {
    pub(crate) sequence_id: SequenceId,
    pub(crate) sequence_name: Box<str>,
    pub(crate) table_id: TableId,
    pub(crate) col_pos: ColId,
    pub(crate) increment: i128,
    pub(crate) start: i128,
    pub(crate) min_value: i128,
    pub(crate) max_value: i128,
    pub(crate) allocated: i128,
}

impl TryFrom<RowRef<'_>> for StSequenceRow {
    type Error = DatastoreError;
    fn try_from(row: RowRef<'_>) -> Result<Self, DatastoreError> {
        read_via_bsatn(row)
    }
}

impl From<StSequenceRow> for ProductValue {
    fn from(x: StSequenceRow) -> Self {
        to_product_value(&x)
    }
}

impl From<StSequenceRow> for SequenceSchema {
    fn from(sequence: StSequenceRow) -> Self {
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

/// System Table [ST_CONSTRAINT_NAME]
///
/// | constraint_id | constraint_name      | table_id    | constraint_data    -------------|
/// |---------------|-------------------- -|-------------|---------------------------------|
/// | 1             | "unique_customer_id" | 1           | unique({"columns": [1, 2]})     |
#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct StConstraintRow {
    pub(crate) constraint_id: ConstraintId,
    pub(crate) constraint_name: Box<str>,
    pub(crate) table_id: TableId,
    pub(crate) constraint_data: StConstraintData,
}

/// Constraint data for storing in the system tables.
///
/// It is critical that this type never grow in layout, as it is stored in the system tables.
/// This is checked by (TODO: add a check in this PR!)
///
/// It is forbidden to add data to any of the variants of this type.
/// You have to add a NEW variant.
#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub enum StConstraintData {
    /// Unused variant to reserve space.
    Unused(u128),

    /// A BTree index.
    Unique { columns: ColSet },
}

impl From<ConstraintData> for StConstraintData {
    fn from(data: ConstraintData) -> Self {
        match data {
            ConstraintData::Unique(UniqueConstraintData { columns }) => StConstraintData::Unique { columns },
            _ => unimplemented!(),
        }
    }
}

impl TryFrom<RowRef<'_>> for StConstraintRow {
    type Error = DatastoreError;
    fn try_from(row: RowRef<'_>) -> Result<Self, DatastoreError> {
        read_via_bsatn(row)
    }
}

impl From<StConstraintRow> for ProductValue {
    fn from(x: StConstraintRow) -> Self {
        to_product_value(&x)
    }
}

impl From<StConstraintRow> for ConstraintSchema {
    fn from(x: StConstraintRow) -> Self {
        Self {
            constraint_id: x.constraint_id,
            constraint_name: x.constraint_name,
            table_id: x.table_id,
            data: match x.constraint_data {
                StConstraintData::Unique { columns } => ConstraintData::Unique(UniqueConstraintData { columns }),
                StConstraintData::Unused(_) => panic!("Someone put a forbidden variant in the system table!"),
            },
        }
    }
}

/// System Table [ST_ROW_LEVEL_SECURITY_NAME]
///
/// | table_id | sql          |
/// |----------|--------------|
/// | 1        | "SELECT ..." |
#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct StRowLevelSecurityRow {
    pub(crate) table_id: TableId,
    pub(crate) sql: RawSql,
}

impl TryFrom<RowRef<'_>> for StRowLevelSecurityRow {
    type Error = DatastoreError;
    fn try_from(row: RowRef<'_>) -> Result<Self, DatastoreError> {
        read_via_bsatn(row)
    }
}

impl From<StRowLevelSecurityRow> for ProductValue {
    fn from(x: StRowLevelSecurityRow) -> Self {
        to_product_value(&x)
    }
}

impl From<StRowLevelSecurityRow> for RowLevelSecuritySchema {
    fn from(x: StRowLevelSecurityRow) -> Self {
        Self {
            table_id: x.table_id,
            sql: x.sql,
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
impl_st!([] ModuleKind, AlgebraicType::U8);

/// A wrapper for [`ConnectionId`] that acts like [`AlgebraicType::U128`] for serialization purposes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConnectionIdViaU128(pub ConnectionId);
impl_serialize!([] ConnectionIdViaU128, (self, ser) => self.0.to_u128().serialize(ser));
impl_deserialize!([] ConnectionIdViaU128, de => <u128>::deserialize(de).map(ConnectionId::from_u128).map(ConnectionIdViaU128));
impl_st!([] ConnectionIdViaU128, AlgebraicType::U128);
impl From<ConnectionId> for ConnectionIdViaU128 {
    fn from(id: ConnectionId) -> Self {
        Self(id)
    }
}

/// A wrapper for [`Identity`] that acts like [`AlgebraicType::U256`] for serialization purposes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IdentityViaU256(pub Identity);
impl_serialize!([] IdentityViaU256, (self, ser) => self.0.to_u256().serialize(ser));
impl_deserialize!([] IdentityViaU256, de => <u256>::deserialize(de).map(Identity::from_u256).map(IdentityViaU256));
impl_st!([] IdentityViaU256, AlgebraicType::U256);
impl From<Identity> for IdentityViaU256 {
    fn from(id: Identity) -> Self {
        Self(id)
    }
}

/// System table [ST_MODULE_NAME]
/// This table holds exactly one row, describing the latest version of the
/// SpacetimeDB module associated with the database:
///
/// * `database_identity` is the [`Identity`] of the database.
/// * `owner_identity` is the [`Identity`] of the owner of the database.
/// * `program_kind` is the [`ModuleKind`] (currently always [`WASM_MODULE`]).
/// * `program_hash` is the [`Hash`] of the raw bytes of the (compiled) module.
/// * `program_bytes` are the raw bytes of the (compiled) module.
/// * `module_version` is the version of the module.
///
/// | identity | owner_identity |  program_kind | program_bytes | program_hash        | module_version |
/// |------------------|----------------|---------------|---------------|---------------------|----------------|
/// | <bytes>          | <bytes>        |  0            | <bytes>       | <bytes>             | <string>       |
#[derive(Clone, Debug, Eq, PartialEq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct StModuleRow {
    pub(crate) database_identity: IdentityViaU256,
    pub(crate) owner_identity: IdentityViaU256,
    pub(crate) program_kind: ModuleKind,
    pub(crate) program_hash: Hash,
    pub(crate) program_bytes: Box<[u8]>,
    pub(crate) module_version: Box<str>,
}

/// Read bytes directly from the column `col` in `row`.
pub fn read_bytes_from_col(row: RowRef<'_>, col: impl StFields) -> Result<Box<[u8]>, DatastoreError> {
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

/// Read an [`Identity`] directly from the column `col` in `row`.
///
/// The [`Identity`] is assumed to be stored as a flat byte array.
pub fn read_identity_from_col(row: RowRef<'_>, col: impl StFields) -> Result<Identity, DatastoreError> {
    Ok(Identity::from_u256(row.read_col(col.col_id())?))
}

/// Read a [`Hash`] directly from the column `col` in `row`.
///
/// The [`Hash`] is assumed to be stored as a flat byte array.
pub fn read_hash_from_col(row: RowRef<'_>, col: impl StFields) -> Result<Hash, DatastoreError> {
    Ok(Hash::from_u256(row.read_col(col.col_id())?))
}

impl TryFrom<RowRef<'_>> for StModuleRow {
    type Error = DatastoreError;

    fn try_from(row: RowRef<'_>) -> Result<Self, Self::Error> {
        read_via_bsatn(row)
    }
}

impl From<StModuleRow> for ProductValue {
    fn from(row: StModuleRow) -> Self {
        to_product_value(&row)
    }
}

/// System table [ST_CLIENT_NAME]
///
/// | identity                                                           | connection_id                      |
/// |--------------------------------------------------------------------+------------------------------------|
/// | 0x7452047061ea2502003412941d85a42f89b0702588b823ab55fc4f12e9ea8363 | 0x6bdea3ab517f5857dc9b1b5fe99e1b14 |
#[derive(Clone, Copy, Debug, Eq, PartialEq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct StClientRow {
    pub(crate) identity: IdentityViaU256,
    pub(crate) connection_id: ConnectionIdViaU128,
}

impl From<StClientRow> for ProductValue {
    fn from(var: StClientRow) -> Self {
        to_product_value(&var)
    }
}
impl From<&StClientRow> for ProductValue {
    fn from(var: &StClientRow) -> Self {
        to_product_value(var)
    }
}

impl TryFrom<RowRef<'_>> for StClientRow {
    type Error = DatastoreError;

    fn try_from(row: RowRef<'_>) -> Result<Self, Self::Error> {
        read_via_bsatn(row)
    }
}

/// System table [ST_VAR_NAME]
///
/// | name        | value     |
/// |-------------|-----------|
/// | "row_limit" | (U64 = 5) |
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct StVarRow {
    pub name: StVarName,
    pub value: StVarValue,
}

impl From<StVarRow> for ProductValue {
    fn from(var: StVarRow) -> Self {
        to_product_value(&var)
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StVarName {
    RowLimit,
    SlowQryThreshold,
    SlowSubThreshold,
    SlowIncThreshold,
}
impl From<StVarName> for &'static str {
    fn from(value: StVarName) -> Self {
        match value {
            StVarName::RowLimit => ST_VARNAME_ROW_LIMIT,
            StVarName::SlowQryThreshold => ST_VARNAME_SLOW_QRY,
            StVarName::SlowSubThreshold => ST_VARNAME_SLOW_SUB,
            StVarName::SlowIncThreshold => ST_VARNAME_SLOW_INC,
        }
    }
}
impl From<StVarName> for AlgebraicValue {
    fn from(value: StVarName) -> Self {
        let value: &'static str = value.into();
        AlgebraicValue::String(value.into())
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
impl_st!([] StVarName, AlgebraicType::String);
impl_serialize!([] StVarName, (self, ser) => <&'static str>::from(*self).serialize(ser));
impl<'de> Deserialize<'de> for StVarName {
    fn deserialize<D: spacetimedb_lib::de::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let s = <&str>::deserialize(de)?;
        s.parse().map_err(D::Error::custom)
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

impl TryFrom<RowRef<'_>> for StVarRow {
    type Error = DatastoreError;

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

/// System table [ST_SCHEDULED_NAME]
/// | schedule_id | table_id | reducer_name | schedule_name |
#[derive(Clone, Debug, Eq, PartialEq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
pub struct StScheduledRow {
    pub(crate) schedule_id: ScheduleId,
    pub(crate) table_id: TableId,
    pub(crate) reducer_name: Box<str>,
    pub(crate) schedule_name: Box<str>,
    pub(crate) at_column: ColId,
}

impl TryFrom<RowRef<'_>> for StScheduledRow {
    type Error = DatastoreError;
    fn try_from(row: RowRef<'_>) -> Result<Self, DatastoreError> {
        read_via_bsatn(row)
    }
}

impl From<StScheduledRow> for ProductValue {
    fn from(x: StScheduledRow) -> Self {
        to_product_value(&x)
    }
}

impl From<StScheduledRow> for ScheduleSchema {
    fn from(row: StScheduledRow) -> Self {
        Self {
            table_id: row.table_id,
            reducer_name: row.reducer_name,
            schedule_id: row.schedule_id,
            schedule_name: row.schedule_name,
            at_column: row.at_column,
        }
    }
}

thread_local! {
    static READ_BUF: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Provides access to a buffer to which bytes can be written.
pub(crate) fn with_sys_table_buf<R>(run: impl FnOnce(&mut Vec<u8>) -> R) -> R {
    READ_BUF.with_borrow_mut(|buf| {
        buf.clear();
        run(buf)
    })
}

/// Read a value from a system table via BSATN.
fn read_via_bsatn<T: DeserializeOwned>(row: RowRef<'_>) -> Result<T, DatastoreError> {
    with_sys_table_buf(|buf| Ok(row.read_via_bsatn::<T>(buf)?))
}

/// Convert a value to a product value.
/// Panics if the value does not serialize to a product value.
/// It's fine to call this on system table types, because `validate_system_table` checks that
/// they are `ProductType`s.
///
/// TODO: this performs some unnecessary allocation. We may want to reimplement the conversions manually for
/// performance eventually.
fn to_product_value<T: Serialize>(value: &T) -> ProductValue {
    value_serialize(&value).into_product().expect("should be product")
}

#[cfg(test)]
mod tests {
    use super::*;

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
