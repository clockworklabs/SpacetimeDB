use crate::error::SchemaErrors;
use crate::identifier::Identifier;
use spacetimedb_data_structures::map::HashMap;
use spacetimedb_primitives::{ColId, ColList, ColListBuilder};
use spacetimedb_sats::db::auth::{StAccess, StTableType};
use spacetimedb_sats::db::raw_def::*;
use spacetimedb_sats::relation::FieldName;
use spacetimedb_sats::AlgebraicType;
use spacetimedb_sats::{AlgebraicTypeRef, Typespace};

/// The default preallocation amount for sequences.
pub const SEQUENCE_PREALLOCATION_AMOUNT: i128 = 4_096;

/// A validated, canonicalized, immutable database definition.
///
/// Cannot be created directly. Instead, create a [spacetimedb_sats::db::raw_def::DatabaseDef] and call [spacetimedb_sats::db::raw_def::RawDatabaseDef::validate].
#[derive(Debug, Clone)]
pub struct DatabaseDef {
    /// The tables of the database def.
    pub(crate) tables: HashMap<Identifier, TableDef>,
    /// The typespace of the database def.
    pub(crate) typespace: Typespace,
}

impl DatabaseDef {
    /// Validate a RawDatabaseDef into a DatabaseDef.
    /// This also performs some canonicalization (identifier canonicalization, sorting lists of constraints, etc.)
    pub fn validate(raw_def: &RawDatabaseDef) -> Result<DatabaseDef, SchemaErrors> {
        crate::validate::validate_database(raw_def)
    }

    /// Access the tables of the database definition.
    pub fn tables(&self) -> &HashMap<Identifier, TableDef> {
        &self.tables
    }

    /// Access the typespace of the database definition.
    pub fn typespace(&self) -> &Typespace {
        &self.typespace
    }
}

impl TryFrom<RawDatabaseDef> for DatabaseDef {
    type Error = SchemaErrors;

    fn try_from(value: RawDatabaseDef) -> Result<Self, Self::Error> {
        DatabaseDef::validate(&value)
    }
}

/// Represents a sequence definition for a database table column.
///
/// Cannot be created directly. Instead, add a [spacetimedb_sats::db::raw_def::RawTableDef] to a [spacetimedb_sats::db::raw_def::RawDatabaseDef] and call [spacetimedb_sats::db::raw_def::RawDatabaseDef::validate].
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct SequenceDef {
    /// The name of the column associated with this sequence.
    pub column_name: Identifier,
    pub start: Option<i128>,
    pub min_value: Option<i128>,
    pub max_value: Option<i128>,
}

/// A struct representing the validated definition of a database index.
///
/// Cannot be created directly. Instead, add a [spacetimedb_sats::db::raw_def::RawIndexDef] to a [spacetimedb_sats::db::raw_def::RawDatabaseDef] and call [spacetimedb_sats::db::raw_def::RawDatabaseDef::validate].
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct IndexDef {
    /// The type of the index.
    pub index_type: IndexType,
    /// List of column positions that compose the index.
    pub column_names: Vec<Identifier>,
    pub(crate) _private: (),
}

/// A struct representing the validated definition of a database column.
///
/// Cannot be created directly. Instead, add a [spacetimedb_sats::db::raw_def::RawColumnDef] to a [spacetimedb_sats::db::raw_def::RawDatabaseDef] and call [spacetimedb_sats::db::raw_def::RawDatabaseDef::validate].
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct ColumnDef {
    /// The name of the column.
    pub col_name: Identifier,
    /// The type of the column.
    pub col_type: AlgebraicType,
    pub(crate) _private: (),
}

/// Requires that the projection of the table onto these columns is an bijection.
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct UniqueConstraintDef {
    pub column_names: Vec<Identifier>,
    pub(crate) _private: (),
}

/// A data structure representing the validated definition of a database table.
///
/// Cannot be created directly. Instead, add a [spacetimedb_sats::db::raw_def::RawTableDef] to a [spacetimedb_sats::db::raw_def::RawDatabaseDef] and call [spacetimedb_sats::db::raw_def::RawDatabaseDef::validate].
///
/// This struct holds information about the table, including its name, columns, indexes,
/// constraints, sequences, type, and access rights.
///
/// Validation rules:
/// - The table name must be a valid identifier.
/// - The table's columns must be sorted according to [crate::db::ordering::canonical_ordering].
/// - The table's indexes, constraints, and sequences must be sorted by their keys.
/// - The table's column types may refer only to types in the containing DatabaseDef's typespace.
/// - The table's column names must be unique.
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct TableDef {
    pub table_name: Identifier,
    pub columns: Vec<ColumnDef>,
    // TODO(jgilles): these may want to be changed to hash maps on the keys...
    pub indexes: Vec<IndexDef>,
    pub unique_constraints: Vec<UniqueConstraintDef>,
    pub sequences: Vec<SequenceDef>,
    pub schedule: Option<ScheduleDef>,
    pub table_type: StTableType,
    pub table_access: StAccess,
    /// The product type corresponding to a row of this table, stored in the DatabaseDef's typespace.
    pub product_type_ref: AlgebraicTypeRef,
    pub(crate) _private: (),
}

impl TableDef {
    /// Check if the `name` of the [FieldName] exist on this [TableDef]
    ///
    /// Warning: It ignores the `table_id`
    pub fn get_column_by_field(&self, field: FieldName) -> Option<&ColumnDef> {
        self.get_column(field.col.idx())
    }

    pub fn get_column(&self, pos: usize) -> Option<&ColumnDef> {
        self.columns.get(pos)
    }

    /// Check if the `col_name` exist on this [TableSchema]
    ///
    /// Warning: It ignores the `table_name`
    pub fn get_column_by_name(&self, col_name: &str) -> Option<&ColumnDef> {
        self.columns.iter().find(|x| &*x.col_name == col_name)
    }

    /// Get the `ColId` corresponding to a particular column name.
    ///
    /// This column ID is not stable across migrations and should be used carefully in migration code.
    pub fn get_column_id(&self, col_name: &Identifier) -> Option<ColId> {
        self.columns
            .iter()
            .position(|x| &x.col_name == col_name)
            .map(|id| ColId(id as u32))
    }

    pub fn get_column_list(&self, columns: &[Identifier]) -> Option<ColList> {
        if columns.is_empty() {
            return None;
        }

        let mut col_list = ColListBuilder::new();
        for col in columns {
            let col_id = self.get_column_id(col)?;
            // INCORRECT(jgilles): this erases ordering information!!
            col_list.push(col_id);
        }
        Some(col_list.build().expect("non-empty by previous check"))
    }
}

/// Marks a table as a timer table for a scheduled reducer.
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct ScheduleDef {
    /// The name of the column that stores the desired invocation time.
    pub at_column: Identifier,
    /// The name of the reducer to call. Not yet an `Identifier` because
    /// reducer names are not currently validated.
    pub reducer_name: Box<str>,
    pub(crate) _private: (),
}

/// An entity that can be looked up in a DatabaseDef.
pub trait DefLookup {
    type Parent: DefLookup;
    type Key: Ord + Clone;

    /// Extract a key for this entity.
    /// This should be stable across migrations: it should not include any positional information.
    fn key(&self, parent_key: &<Self::Parent as DefLookup>::Key) -> Self::Key;

    /// Look up this entity in a DatabaseDef.
    /// These accessors are typically somewhat slow, so they should be used sparingly.
    fn lookup<'def>(def: &'def DatabaseDef, key: &'_ Self::Key) -> Option<&'def Self>;
}

impl DefLookup for SequenceDef {
    type Parent = TableDef;
    type Key = (Identifier, Identifier); // table name, column name

    fn key(&self, table_name: &Identifier) -> Self::Key {
        (table_name.clone(), self.column_name.clone())
    }

    fn lookup<'def>(def: &'def DatabaseDef, key: &'_ Self::Key) -> Option<&'def Self> {
        let table = TableDef::lookup(def, &key.0)?;
        table.sequences.iter().find(|x| x.column_name == key.1)
    }
}

impl DefLookup for IndexDef {
    type Parent = TableDef;
    type Key = (Identifier, Vec<Identifier>, IndexType); // table name, column name list, index type.

    fn key(&self, table_name: &Identifier) -> Self::Key {
        (table_name.clone(), self.column_names.clone(), self.index_type)
    }

    fn lookup<'def>(def: &'def DatabaseDef, key: &'_ Self::Key) -> Option<&'def Self> {
        let table = TableDef::lookup(def, &key.0)?;
        table
            .indexes
            .iter()
            .find(|x| x.column_names == key.1 && x.index_type == key.2)
    }
}

impl DefLookup for UniqueConstraintDef {
    type Parent = TableDef;
    type Key = (Identifier, Vec<Identifier>); // table name, column name list

    fn key(&self, table_name: &Identifier) -> Self::Key {
        (table_name.clone(), self.column_names.clone())
    }

    fn lookup<'def>(def: &'def DatabaseDef, key: &'_ Self::Key) -> Option<&'def Self> {
        let table = TableDef::lookup(def, &key.0)?;
        table.unique_constraints.iter().find(|x| x.column_names == key.1)
    }
}

impl DefLookup for ColumnDef {
    type Parent = TableDef;
    type Key = (Identifier, Identifier); // table name, column name

    fn key(&self, table_name: &Identifier) -> Self::Key {
        (table_name.clone(), self.col_name.clone())
    }

    fn lookup<'def>(def: &'def DatabaseDef, key: &'_ Self::Key) -> Option<&'def Self> {
        let table = TableDef::lookup(def, &key.0)?;
        table.columns.iter().find(|x| x.col_name == key.1)
    }
}

impl DefLookup for ScheduleDef {
    type Parent = TableDef;
    type Key = Identifier; // table name

    fn key(&self, table_name: &Identifier) -> Self::Key {
        table_name.clone()
    }

    fn lookup<'def>(def: &'def DatabaseDef, key: &'_ Self::Key) -> Option<&'def Self> {
        let table = TableDef::lookup(def, key)?;
        table.schedule.as_ref()
    }
}

impl DefLookup for TableDef {
    type Parent = ();
    type Key = Identifier; // table name

    fn key(&self, _: &()) -> Self::Key {
        self.table_name.clone()
    }

    fn lookup<'def>(def: &'def DatabaseDef, key: &'_ Self::Key) -> Option<&'def Self> {
        def.tables.get(key)
    }
}

// Mildly silly base case
impl DefLookup for () {
    type Parent = ();
    type Key = ();

    fn key(&self, _: &()) -> Self::Key {}

    fn lookup<'def>(_: &'def DatabaseDef, _: &()) -> Option<&'def Self> {
        None
    }
}
