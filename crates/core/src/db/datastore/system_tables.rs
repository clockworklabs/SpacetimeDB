use crate::error::{DBError, TableError};
use core::fmt;
use spacetimedb_primitives::*;
use spacetimedb_sats::db::auth::{StAccess, StTableType};
use spacetimedb_sats::db::def::*;
use spacetimedb_sats::hash::Hash;
use spacetimedb_sats::product_value::InvalidFieldError;
use spacetimedb_sats::{
    impl_deserialize, impl_serialize, product, AlgebraicType, AlgebraicValue, ArrayValue, ProductValue,
};
use spacetimedb_table::table::RowRef;
use std::ops::Deref as _;
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
pub(crate) fn system_tables() -> [TableSchema; 6] {
    [
        st_table_schema(),
        st_columns_schema(),
        st_indexes_schema(),
        st_constraints_schema(),
        st_module_schema(),
        // Is important this is always last, so the starting sequence for each
        // system table is correct.
        st_sequences_schema(),
    ]
}

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
pub fn st_columns_schema() -> TableSchema {
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
pub fn st_indexes_schema() -> TableSchema {
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
pub(crate) fn st_sequences_schema() -> TableSchema {
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
pub(crate) fn st_constraints_schema() -> TableSchema {
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
/// * `program_hash` is the [`Hash`] of the raw bytes of the (compiled) module.
/// * `constraints` is the [`ModuleKind`] (currently always [`WASM_MODULE`]).
/// * `epoch` is a _fencing token_ used to protect against concurrent updates.
///
/// | program_hash        | kind     | epoch |
/// |---------------------|----------|-------|
/// | [250, 207, 5, ...]  | 0        | 42    |
pub(crate) fn st_module_schema() -> TableSchema {
    TableDef::new(
        ST_MODULE_NAME.into(),
        vec![
            ColumnDef::sys(
                StModuleFields::ProgramHash.name(),
                AlgebraicType::array(AlgebraicType::U8),
            ),
            ColumnDef::sys(StModuleFields::Kind.name(), AlgebraicType::U8),
            ColumnDef::sys(StModuleFields::Epoch.name(), AlgebraicType::U128),
        ],
    )
    .with_type(StTableType::System)
    .into_schema(ST_MODULE_ID)
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

impl TryFrom<RowRef<'_>> for StModuleRow {
    type Error = DBError;

    fn try_from(row: RowRef<'_>) -> Result<Self, Self::Error> {
        let col_pos = StModuleFields::ProgramHash.col_id();
        let bytes = row.read_col::<ArrayValue>(col_pos)?;
        let ArrayValue::U8(bytes) = bytes else {
            let name = Some(StModuleFields::ProgramHash.name());
            return Err(InvalidFieldError { name, col_pos }.into());
        };
        let program_hash = Hash::from_slice(&bytes);

        Ok(Self {
            program_hash,
            kind: row.read_col::<u8>(StModuleFields::Kind).map(ModuleKind)?,
            epoch: row.read_col::<u128>(StModuleFields::Epoch).map(Epoch)?,
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
        product![AlgebraicValue::Bytes(program_hash.as_slice().into()), *kind, *epoch,]
    }
}
