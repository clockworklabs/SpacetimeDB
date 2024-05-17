use crate::error::{DBError, TableError};
use core::fmt;
use spacetimedb_lib::{Address, Identity};
use spacetimedb_primitives::*;
use spacetimedb_sats::db::auth::{StAccess, StTableType};
use spacetimedb_sats::db::def::*;
use spacetimedb_sats::hash::Hash;
use spacetimedb_sats::product_value::InvalidFieldError;
use spacetimedb_sats::relation::FieldName;
use spacetimedb_sats::{
    impl_deserialize, impl_serialize, product, AlgebraicType, AlgebraicValue, ArrayValue, ProductValue,
};
use spacetimedb_table::table::RowRef;
use std::ops::Deref as _;

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

pub(crate) fn system_tables() -> [TableSchema; 7] {
    [
        StTable::schema(),
        StColumns::schema(),
        StIndexes::schema(),
        StConstraints::schema(),
        StModule::schema(),
        StClients::schema(),
        // Is important this is always last,
        // so the starting sequence for each system table is correct.
        StSequence::schema(),
    ]
}

macro_rules! system_table {
    ($(#[$attr:meta])* $ty_name:ident
        [id = $id:expr, idx = $idx:expr, name = $tname:expr],
        $table_def_ident:ident => $table_def:expr,
        columns = [$($name:expr, $var:ident @ $discr:literal : $ty:expr,)* ]
    ) => {
        #[derive(Copy, Clone, Debug)]
        $(#[$attr])*
        pub enum $ty_name {
            $($var = $discr,)*
        }

        impl $ty_name {
            /// The static ID of the system table.
            pub(crate) const ID: TableId = TableId($id);

            /// The index into the array returned by [`system_tables`].
            pub(crate) const IDX: usize = $idx;

            /// The name of the system table.
            pub(crate) const NAME: &'static str = &$tname;

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

            pub fn field_name(self) -> FieldName {
                FieldName::new(Self::ID, self.col_id())
            }

            #[inline]
            pub fn name(self) -> &'static str {
                match self {
                    $(Self::$var => $name,)*
                }
            }

            fn schema() -> TableSchema {
                let $table_def_ident = TableDef::new(
                    Self::NAME.into(),
                    [$( ColumnDef::sys($name, $ty) ),*].into(),
                )
                .with_type(StTableType::System);
                $table_def.into_schema(Self::ID)
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

// WARNING: For a stable schema, be careful when changing anything about the system tables!

system_table! (
    /// System Table 0, "st_table"
    ///
    /// | table_id | table_name  | table_type | table_access |
    /// |----------|-------------|----------- |------------- |
    /// | 4        | "customers" | "user"     | "public"     |
    StTable [id = 0, idx = 0, name = "st_table"],
    def => def
        .with_column_constraint(Constraints::primary_key_auto(), Self::TableId)
        .with_column_index(Self::TableName, true),
    columns = [
        "table_id", TableId @ 0 : AlgebraicType::U32,
        "table_name", TableName @ 1 : AlgebraicType::String,
        "table_type", TableType @ 2 : AlgebraicType::String,
        "table_access", TablesAccess @ 3 : AlgebraicType::String,
    ]
);
system_table!(
    /// System Table 1, "st_columns"
    ///
    /// | table_id | col_id | col_name | col_type            |
    /// |----------|---------|----------|--------------------|
    /// | 1        | 0       | "id"     | AlgebraicType::U32 |
    StColumns [id = 1, idx = 1, name = "st_columns"],
    def => def.with_column_constraint(
        Constraints::unique(),
        col_list![Self::TableId.col_id(), Self::ColPos.col_id()]
    ),
    columns = [
        "table_id", TableId @ 0 : AlgebraicType::U32,
        "col_pos", ColPos @ 1 : AlgebraicType::U32,
        "col_name", ColName @ 2 : AlgebraicType::String,
        "col_type", ColType @ 3 : AlgebraicType::bytes(),
    ]
);
system_table!(
    /// System Table 3, "st_indexes"
    ///
    /// | index_id | table_id | index_name  | columns | is_unique | index_type |
    /// |----------|----------|-------------|---------|-----------|------------|
    /// | 1        |          | "ix_sample" | [1]     | false     | "btree"    |
    StIndexes [id = 3, idx = 2, name = "st_indexes"],
    // TODO: Unique constraint on index name?
    def => def.with_column_constraint(Constraints::primary_key_auto(), Self::IndexId),
    columns = [
        "index_id", IndexId @ 0 : AlgebraicType::U32,
        "table_id", TableId @ 1 : AlgebraicType::U32,
        "index_name", IndexName @ 2 : AlgebraicType::String,
        "columns", Columns @ 3 : AlgebraicType::array(AlgebraicType::U32),
        "is_unique", IsUnique @ 4 : AlgebraicType::Bool,
        "index_type", IndexType @ 5 : AlgebraicType::U8,
    ]
);
system_table!(
    /// System Table 2, "st_sequence"
    ///
    /// | sequence_id | sequence_name     | increment | start | min_value | max_value | table_id | col_pos| allocated |
    /// |-------------|-------------------|-----------|-------|-----------|-----------|----------|--------|-----------|
    /// | 1           | "seq_customer_id" | 1         | 100   | 10        | 1200      | 1        | 1      | 200       |
    StSequence [id = 2, idx = 6, name = "st_sequence"],
    // TODO: Unique constraint on sequence name?
    def => def.with_column_constraint(Constraints::primary_key_auto(), Self::SequenceId),
    columns = [
        "sequence_id", SequenceId @ 0 : AlgebraicType::U32,
        "sequence_name", SequenceName @ 1 : AlgebraicType::String,
        "table_id", TableId @ 2 : AlgebraicType::U32,
        "col_pos", ColPos @ 3 : AlgebraicType::U32,
        "increment", Increment @ 4 : AlgebraicType::I128,
        "start", Start @ 5 : AlgebraicType::I128,
        "min_value", MinValue @ 6 : AlgebraicType::I128,
        "max_value", MaxValue @ 7 : AlgebraicType::I128,
        "allocated", Allocated @ 8 : AlgebraicType::I128,
    ]
);
system_table!(
    /// System Table 4, "st_constraints"
    ///
    /// | constraint_id | constraint_name      | constraints | table_id | columns |
    /// |---------------|-------------------- -|-------------|-------|------------|
    /// | 1             | "unique_customer_id" | 1           | 100   | [1, 4]     |
    StConstraints [id = 4, idx = 3, name = "st_constraints"],
    def => def.with_column_constraint(Constraints::primary_key_auto(), StConstraints::ConstraintId),
    columns = [
        "constraint_id", ConstraintId @ 0 : AlgebraicType::U32,
        "constraint_name", ConstraintName @ 1 : AlgebraicType::String,
        "constraints", Constraints @ 2 : AlgebraicType::U8,
        "table_id", TableId @ 3 : AlgebraicType::U32,
        "columns", Columns @ 4 : AlgebraicType::array(AlgebraicType::U32),
    ]
);
system_table!(
    /// System table 5, "st_module"
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
    StModule [id = 5, idx = 4, name = "st_module"],
    def => def,
    columns = [
        "program_hash", ProgramHash @ 0 : AlgebraicType::array(AlgebraicType::U8),
        "kind", Kind @ 1 : AlgebraicType::U8,
        "epoch", Epoch @ 2 : AlgebraicType::U128,
    ]
);
system_table!(
    /// System table 5, "st_clients"
    ///
    /// This table defines what clients are connected.
    ///
    /// identity                                                                                | address
    /// -----------------------------------------------------------------------------------------+--------------------------------------------------------
    ///  (__identity_bytes = 0x7452047061ea2502003412941d85a42f89b0702588b823ab55fc4f12e9ea8363) | (__address_bytes = 0x6bdea3ab517f5857dc9b1b5fe99e1b14)
    StClients [id = 6, idx = 5, name = "st_clients"],
    def => def.with_column_index(col_list![Self::Identity, Self::Address], true),
    columns = [
        "identity", Identity @ 0 : Identity::get_type(),
        "address", Address @ 1 : Address::get_type(),
    ]
);

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
            .read_col::<Box<str>>(StTable::TableType)?
            .deref()
            .try_into()
            .map_err(|x: &str| TableError::DecodeField {
                table: StTable::NAME.into(),
                field: StTable::TableType.col_name(),
                expect: format!("`{}` or `{}`", StTableType::System.as_str(), StTableType::User.as_str()),
                found: x.to_string(),
            })?;

        let table_access = row
            .read_col::<Box<str>>(StTable::TablesAccess)?
            .deref()
            .try_into()
            .map_err(|x: &str| TableError::DecodeField {
                table: StTable::NAME.into(),
                field: StTable::TablesAccess.col_name(),
                expect: format!("`{}` or `{}`", StAccess::Public.as_str(), StAccess::Private.as_str()),
                found: x.to_string(),
            })?;

        Ok(StTableRow {
            table_id: row.read_col(StTable::TableId)?,
            table_name: row.read_col(StTable::TableName)?,
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
        let table_id = row.read_col(StColumns::TableId)?;
        let bytes = row.read_col::<AlgebraicValue>(StColumns::ColType)?;
        let bytes = bytes.as_bytes().unwrap_or_default();
        let col_type =
            AlgebraicType::decode(&mut &*bytes).map_err(|e| TableError::InvalidSchema(table_id, e.into()))?;

        Ok(StColumnRow {
            col_pos: row.read_col(StColumns::ColPos)?,
            col_name: row.read_col(StColumns::ColName)?,
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
        let index_type = row.read_col::<u8>(StIndexes::IndexType)?;
        let index_type = IndexType::try_from(index_type).map_err(|_| InvalidFieldError {
            col_pos: StIndexes::IndexType.col_id(),
            name: Some(StIndexes::IndexType.name()),
        })?;
        Ok(StIndexRow {
            index_id: row.read_col(StIndexes::IndexId)?,
            table_id: row.read_col(StIndexes::TableId)?,
            index_name: row.read_col(StIndexes::IndexName)?,
            columns: to_cols(row, StIndexes::Columns, StIndexes::Columns.name())?,
            is_unique: row.read_col(StIndexes::IsUnique)?,
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
            sequence_id: row.read_col(StSequence::SequenceId)?,
            sequence_name: row.read_col(StSequence::SequenceName)?,
            table_id: row.read_col(StSequence::TableId)?,
            col_pos: row.read_col(StSequence::ColPos)?,
            increment: row.read_col(StSequence::Increment)?,
            start: row.read_col(StSequence::Start)?,
            min_value: row.read_col(StSequence::MinValue)?,
            max_value: row.read_col(StSequence::MaxValue)?,
            allocated: row.read_col(StSequence::Allocated)?,
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
        let constraints = row.read_col::<u8>(StConstraints::Constraints)?;
        let constraints = Constraints::try_from(constraints).expect("Fail to decode Constraints");
        let columns = to_cols(row, StConstraints::Columns, StConstraints::Columns.name())?;
        Ok(StConstraintRow {
            table_id: row.read_col(StConstraints::TableId)?,
            constraint_id: row.read_col(StConstraints::ConstraintId)?,
            constraint_name: row.read_col(StConstraints::ConstraintName)?,
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
        let col_pos = StModule::ProgramHash.col_id();
        let bytes = row.read_col::<ArrayValue>(col_pos)?;
        let ArrayValue::U8(bytes) = bytes else {
            let name = Some(StModule::ProgramHash.name());
            return Err(InvalidFieldError { name, col_pos }.into());
        };
        let program_hash = Hash::from_slice(&bytes);

        Ok(Self {
            program_hash,
            kind: row.read_col::<u8>(StModule::Kind).map(ModuleKind)?,
            epoch: row.read_col::<u128>(StModule::Epoch).map(Epoch)?,
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
