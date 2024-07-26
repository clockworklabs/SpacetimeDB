use crate::error::ValidationErrors;
use crate::identifier::Identifier;
use spacetimedb_data_structures::map::HashMap;
use spacetimedb_lib::db::auth::{StAccess, StTableType};
use spacetimedb_lib::db::raw_def;
use spacetimedb_lib::{ProductTypeElement, RawModuleDef};
use spacetimedb_primitives::{ColId, ColList};
use spacetimedb_sats::AlgebraicType;
use spacetimedb_sats::{AlgebraicTypeRef, Typespace};

pub mod validate;

// We may eventually want to reorganize this module to look more
// like the system tables, with numeric IDs used for lookups
// in addition to `Identifier`s.
//
// If that path is taken, it might be possible to have this type
// entirely subsume the various `Schema` types, which would be cool.

/// A validated, canonicalized, immutable module definition.
///
/// Cannot be created directly. Instead, create/deserialize a [spacetimedb_lib::RawModuleDef] and call [ModuleDef::validate].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ModuleDef {
    /// The tables of the module definition.
    tables: HashMap<Identifier, TableDef>,

    /// The reducers of the module definition.
    reducers: HashMap<Identifier, ReducerDef>,

    /// The type definitions of the module definition.
    types: HashMap<Identifier, TypeDef>,

    /// The typespace of the module definition.
    ///
    /// Must be in nominal normal form.
    typespace: Typespace,

    /// The global namespace of the module:
    /// tables, indexes, constraints, schedules, and sequences live in the global namespace.
    /// Concretely, though, they're stored in the `TableDef` data structures.
    /// This map allows looking up which `TableDef` stores the `Def` you're looking for.
    stored_in_table_def: HashMap<Identifier, Identifier>,
}

impl ModuleDef {
    /// Construct a `ModuleDef` by validating a `RawModuleDef`.
    /// This is the only way to construct a `ModuleDef`.
    /// (The `TryFrom` impls for this type just call this method.)
    pub fn validate(raw: RawModuleDef) -> Result<Self, ValidationErrors> {
        match raw {
            RawModuleDef::V8BackCompat(v8_mod) => validate::v8::validate(v8_mod),
            RawModuleDef::V9(v9_mod) => validate::v9::validate(v9_mod),
            _ => unimplemented!(),
        }
    }

    /// The tables of the module definition.
    pub fn tables(&self) -> impl Iterator<Item = &TableDef> {
        self.tables.values()
    }

    /// The typespace of the module def.
    pub fn typespace(&self) -> &Typespace {
        &self.typespace
    }

    /// The `TableDef` an entity in the global namespace is stored in, if any.
    fn stored_in_table_def(&self, name: &Identifier) -> Option<&TableDef> {
        self.stored_in_table_def
            .get(name)
            .and_then(|table_name| self.tables.get(table_name))
    }
}

impl TryFrom<RawModuleDef> for ModuleDef {
    type Error = ValidationErrors;

    fn try_from(raw: RawModuleDef) -> Result<Self, Self::Error> {
        Self::validate(raw)
    }
}
impl TryFrom<raw_def::v8::RawModuleDefV8> for ModuleDef {
    type Error = ValidationErrors;

    fn try_from(v8_mod: raw_def::v8::RawModuleDefV8) -> Result<Self, Self::Error> {
        Self::validate(RawModuleDef::V8BackCompat(v8_mod))
    }
}
impl TryFrom<raw_def::v9::RawModuleDefV9> for ModuleDef {
    type Error = ValidationErrors;

    fn try_from(v9_mod: raw_def::v9::RawModuleDefV9) -> Result<Self, Self::Error> {
        Self::validate(RawModuleDef::V9(v9_mod))
    }
}

/// Implemented by definitions stored in a `ModuleDef`.
/// ALlows looking definitions up in a `ModuleDef`, and across
/// `ModuleDef`s during migrations.
pub trait Def: Sized + 'static {
    /// A reference to a definition of this type within a module def. This reference should be portable across migrations.
    type Key<'a>;

    /// Get a reference to this definition.
    fn key(&self) -> Self::Key<'_>;

    /// Look up this entity in the module def.
    fn lookup<'a>(module_def: &'a ModuleDef, key: Self::Key<'_>) -> Option<&'a Self>;
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
#[derive(Debug, Clone, Eq, PartialEq)]
#[non_exhaustive]
pub struct TableDef {
    /// The name of the table.
    /// Unique within a module, acts as the table's identifier.
    /// Must be a valid [crate::db::identifier::Identifier].
    pub name: Identifier,
    /// A reference to a `ProductType` containing the columns of this table.
    /// This is the single source of truth for the table's columns.
    /// All elements of the `ProductType` must have names.
    ///
    /// Like all types in the module, this must have the [default element ordering](crate::db::default_element_ordering), UNLESS a custom ordering is declared via `ModuleDef.misc_exports` for this type.
    pub product_type_ref: AlgebraicTypeRef,
    /// The columns of this table. This stores the information in
    /// `product_type_ref` in a more convenient-to-access format.
    pub columns: Vec<ColumnDef>,
    /// The indices on the table, indexed by name.
    pub indexes: HashMap<Identifier, IndexDef>,
    /// The unique constraints on the table, indexed by name.
    pub unique_constraints: HashMap<Identifier, UniqueConstraintDef>,
    /// The sequences for the table, indexed by name.
    pub sequences: HashMap<Identifier, SequenceDef>,
    /// The schedule for the table, if present.
    pub schedule: Option<ScheduleDef>,
    /// Whether this is a system- or user-created table.
    pub table_type: StTableType,
    /// Whether this table is public or private.
    pub table_access: StAccess,
}

impl TableDef {
    /// Get a column of the `TableDef`.
    pub fn get_column(&self, id: ColId) -> Option<&ColumnDef> {
        self.columns.get(id.0 as usize)
    }
    /// Get a column by the column's name.
    pub fn get_column_by_name(&self, name: &Identifier) -> Option<&ColumnDef> {
        self.columns.iter().find(|c| &c.name == name)
    }
}

/// A sequence definition for a database table column.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SequenceDef {
    /// The name of the sequence. Must be unique within the containing `RawDatabaseDef`.
    pub name: Identifier,

    /// The position of the column associated with this sequence.
    /// This refers to a column in the same `RawTableDef` that contains this `RawSequenceDef`.
    /// The column must have integral type.
    /// This must be the unique `RawSequenceDef` for this column.
    pub column: ColId,

    /// The value to start assigning to this column.
    /// Will be incremented by 1 for each new row.
    /// If not present, an arbitrary start point may be selected.
    pub start: Option<i128>,

    /// The minimum allowed value in this column.
    /// If not present, no minimum.
    pub min_value: Option<i128>,

    /// The maximum allowed value in this column.
    /// If not present, no maximum.
    pub max_value: Option<i128>,
}

/// A struct representing the validated definition of a database index.
///
/// Cannot be created directly. Instead, add a [spacetimedb_sats::db::raw_def::RawIndexDef] to a [spacetimedb_sats::db::raw_def::RawDatabaseDef] and call [spacetimedb_sats::db::raw_def::RawDatabaseDef::validate].
#[derive(Debug, Clone, Eq, PartialEq)]
#[non_exhaustive]
pub struct IndexDef {
    /// The name of the index.
    ///
    /// This can be overridden by the user and should NOT be assumed to follow
    /// any particular format.
    ///
    /// Unique within the containing `DatabaseDef`.
    pub name: Identifier,

    /// The algorithm parameters for the index.
    pub algorithm: IndexAlgorithm,

    /// Currently, we automatically generate indexes for fields with `unique` constraints.
    /// This flag indicates whether the index was generated automatically.
    /// It will eventually be removed once a different enforcement mechanism is in place.
    pub generated: bool,
}

/// Data specifying a supported index algorithm.
#[non_exhaustive]
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum IndexAlgorithm {
    /// Implemented using a rust `std::collections::BTreeMap`.
    BTree {
        /// The columns to index on. These are ordered.
        columns: ColList,
    },
}

/// A struct representing the validated definition of a database column.
///
/// Cannot be created directly. Instead, add a [spacetimedb_sats::db::raw_def::RawColumnDef] to a [spacetimedb_sats::db::raw_def::RawDatabaseDef] and call [spacetimedb_sats::db::raw_def::RawDatabaseDef::validate].
#[derive(Debug, Clone, Eq, PartialEq)]
#[non_exhaustive]
pub struct ColumnDef {
    /// The name of the column.
    /// Unique within the containing `TableDef`, but
    /// NOT within the containing `ModuleDef`.
    pub name: Identifier,

    /// The ID of this column.
    pub col_id: ColId,

    /// The type of the column.
    pub ty: AlgebraicType,

    /// The table this `ColumnDef` is stored in.
    pub table_name: Identifier,
}

/// Requires that the projection of the table onto these columns is an bijection.
#[derive(Debug, Clone, Eq, PartialEq)]
#[non_exhaustive]
pub struct UniqueConstraintDef {
    /// The name of the unique constraint. Must be unique within the containing `RawDatabaseDef`.
    pub name: Identifier,

    /// The columns on the containing `TableDef`
    pub columns: ColList,
}

/// Marks a table as a timer table for a scheduled reducer.
#[derive(Debug, Clone, Eq, PartialEq)]
#[non_exhaustive]
pub struct ScheduleDef {
    /// The name of the schedule. Must be unique within the containing `RawDatabaseDef`.
    pub name: Identifier,

    /// The name of the column that stores the desired invocation time.
    ///
    /// Must be named `scheduled_at` and be of type `ScheduleAt`.
    pub at_column: ColId,

    /// The name of the column that stores the invocation ID.
    ///
    /// Must be named `scheduled_id` and be of type `u64`.
    pub id_column: ColId,

    /// The name of the reducer to call. Not yet an `Identifier` because
    /// reducer names are not currently validated.
    pub reducer_name: Identifier,
}

/// A type exported by the module.
#[derive(Debug, Clone, Eq, PartialEq)]
#[non_exhaustive]
pub struct TypeDef {
    /// The name of the type. This must be unique within the module.
    ///
    /// Eventually, we may add more information to this, such as the module name and generic arguments.
    pub name: Identifier,

    /// The type to which the alias refers.
    pub ty: AlgebraicTypeRef,

    /// Whether this type has a custom ordering.
    pub custom_ordering: bool,
}

/// A type exported by the module.
#[derive(Debug, Clone, Eq, PartialEq)]
#[non_exhaustive]
pub struct ReducerDef {
    /// The name of the reducer. This must be unique within the module.
    pub name: Identifier,

    /// The parameters of the reducer.
    ///
    /// `AlgebraicTypeRef`s in here refer to the containing `ModuleDef`'s `Typespace`.
    pub params: Vec<ProductTypeElement>,
}

impl Def for TableDef {
    type Key<'a> = &'a Identifier;

    fn key(&self) -> Self::Key<'_> {
        &self.name
    }

    fn lookup<'a>(module_def: &'a ModuleDef, key: Self::Key<'_>) -> Option<&'a Self> {
        module_def.tables.get(key)
    }
}

impl Def for SequenceDef {
    type Key<'a> = &'a Identifier;

    fn key(&self) -> Self::Key<'_> {
        &self.name
    }

    fn lookup<'a>(module_def: &'a ModuleDef, key: Self::Key<'_>) -> Option<&'a Self> {
        module_def.stored_in_table_def(key)?.sequences.get(key)
    }
}

impl Def for IndexDef {
    type Key<'a> = &'a Identifier;

    fn key(&self) -> Self::Key<'_> {
        &self.name
    }

    fn lookup<'a>(module_def: &'a ModuleDef, key: Self::Key<'_>) -> Option<&'a Self> {
        module_def.stored_in_table_def(key)?.indexes.get(key)
    }
}

impl Def for ColumnDef {
    // (table_name, column_name).
    // We don't use `ColId` here because we want this to be portable
    // across migrations.
    type Key<'a> = (&'a Identifier, &'a Identifier);

    fn key(&self) -> Self::Key<'_> {
        (&self.table_name, &self.name)
    }

    fn lookup<'a>(module_def: &'a ModuleDef, (table_name, name): Self::Key<'_>) -> Option<&'a Self> {
        module_def
            .tables
            .get(table_name)
            .and_then(|table| table.get_column_by_name(name))
    }
}

impl Def for UniqueConstraintDef {
    type Key<'a> = &'a Identifier;

    fn key(&self) -> Self::Key<'_> {
        &self.name
    }

    fn lookup<'a>(module_def: &'a ModuleDef, key: Self::Key<'_>) -> Option<&'a Self> {
        module_def.stored_in_table_def(key)?.unique_constraints.get(key)
    }
}

impl Def for ScheduleDef {
    type Key<'a> = &'a Identifier;

    fn key(&self) -> Self::Key<'_> {
        &self.name
    }

    fn lookup<'a>(module_def: &'a ModuleDef, key: Self::Key<'_>) -> Option<&'a Self> {
        let schedule = module_def.stored_in_table_def(key)?.schedule.as_ref()?;
        if &schedule.name == key {
            Some(schedule)
        } else {
            None
        }
    }
}

impl Def for TypeDef {
    type Key<'a> = &'a Identifier;

    fn key(&self) -> Self::Key<'_> {
        &self.name
    }

    fn lookup<'a>(module_def: &'a ModuleDef, key: Self::Key<'_>) -> Option<&'a Self> {
        module_def.types.get(key)
    }
}

impl Def for ReducerDef {
    type Key<'a> = &'a Identifier;

    fn key(&self) -> Self::Key<'_> {
        &self.name
    }

    fn lookup<'a>(module_def: &'a ModuleDef, key: Self::Key<'_>) -> Option<&'a Self> {
        module_def.reducers.get(key)
    }
}
