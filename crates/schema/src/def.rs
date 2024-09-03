//! Canonicalized module definitions.
//!
//! This module contains a set of types that represent the canonical form of SpacetimeDB module definitions.
//! These types are immutable to prevent accidental introduction of errors.
//! The internal data structures of this module are not considered public API or ABI and may change
//! at any time.
//!
//! Different module ABI versions correspond to different submodules of `spacetimedb_lib::db::raw_def`.
//! All of these ABI versions can be converted to the standard form in this module via `TryFrom`.
//! We provide streams of errors in case the conversion fails, to provide as much information
//! to the user as possible about why their module is invalid.
//!
//! The `ModuleDef` type is the main type in this module. It contains all the information about a module, including its tables, reducers, typespace, and type metadata.
//!
//! After validation, a `ModuleDef` can be converted to the `*Schema` types in `crate::schema` for use in the database.
//! (Eventually, we may unify these types...)

use std::fmt::{self, Debug, Write};
use std::hash::Hash;

use crate::error::{IdentifierError, ValidationErrors};
use crate::identifier::Identifier;
use hashbrown::Equivalent;
use itertools::Itertools;
use spacetimedb_data_structures::error_stream::{CollectAllErrors, CombineErrors, ErrorStream};
use spacetimedb_data_structures::map::HashMap;
use spacetimedb_lib::db::raw_def;
use spacetimedb_lib::db::raw_def::v9::{
    Lifecycle, RawIdentifier, RawIndexAlgorithm, RawIndexDefV9, RawModuleDefV9, RawReducerDefV9, RawScheduleDefV9,
    RawScopedTypeNameV9, RawSequenceDefV9, RawTableDefV9, RawTypeDefV9, RawUniqueConstraintDefV9, TableAccess,
    TableType,
};
use spacetimedb_lib::{ProductType, RawModuleDef};
use spacetimedb_primitives::{ColId, ColList};
use spacetimedb_sats::AlgebraicType;
use spacetimedb_sats::{AlgebraicTypeRef, Typespace};

pub mod validate;

/// A map from `Identifier`s to values of type `T`.
pub type IdentifierMap<T> = HashMap<Identifier, T>;

// We may eventually want to reorganize this module to look more
// like the system tables, with numeric IDs used for lookups
// in addition to `Identifier`s.
//
// If that path is taken, it might be possible to have this type
// entirely subsume the various `Schema` types, which would be cool.

/// A validated, canonicalized, immutable module definition.
///
/// Cannot be created directly. Instead, create/deserialize a [spacetimedb_lib::RawModuleDef] and call [ModuleDef::try_from].
///
/// ```rust
/// use spacetimedb_lib::RawModuleDef;
/// use spacetimedb_schema::def::{ModuleDef, TableDef, IndexDef, TypeDef, ModuleDefLookup, ScopedTypeName};
/// use spacetimedb_schema::identifier::Identifier;
///
/// fn read_raw_module_def_from_file() -> RawModuleDef {
///     // ...
/// #   RawModuleDef::V9(Default::default())
/// }
///
/// let raw_module_def = read_raw_module_def_from_file();
/// let module_def = ModuleDef::try_from(raw_module_def).expect("valid module def");
///
/// let table_name = Identifier::new("my_table".into()).expect("valid identifier");
/// let index_name = Identifier::new("my_index".into()).expect("valid identifier");
/// let scoped_type_name = ScopedTypeName::try_new([], "MyType").expect("valid scoped type name");
///
/// let table: Option<&TableDef> = module_def.lookup(&table_name);
/// let index: Option<&IndexDef> = module_def.lookup(&index_name);
/// let type_def: Option<&TypeDef> = module_def.lookup(&scoped_type_name);
/// // etc.
/// ```
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ModuleDef {
    /// The tables of the module definition.
    tables: IdentifierMap<TableDef>,

    /// The reducers of the module definition.
    reducers: IdentifierMap<ReducerDef>,

    /// The type definitions of the module definition.
    types: HashMap<ScopedTypeName, TypeDef>,

    /// The typespace of the module definition.
    typespace: Typespace,

    /// The global namespace of the module:
    /// tables, indexes, constraints, schedules, and sequences live in the global namespace.
    /// Concretely, though, they're stored in the `TableDef` data structures.
    /// This map allows looking up which `TableDef` stores the `Def` you're looking for.
    stored_in_table_def: IdentifierMap<Identifier>,
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

    /// The `Typespace` used by the module.
    ///
    /// `AlgebraicTypeRef`s in the table, reducer, and type alias declarations refer to this typespace.
    ///
    /// The typespace must satisfy `Typespace::is_valid_for_client_code_generation`. That is, all types stored in the typespace must either:
    /// 1. satisfy `AlgebraicType::is_valid_for_client_type_definition`
    /// 2. and/or `AlgebraicType::is_valid_for_client_type_use`.
    ///
    /// Types satisfying condition 1 correspond to generated classes in client code.
    /// (Types satisfying condition 2 are an artifact of the module bindings, and do not affect the semantics of the module definition.)
    ///
    /// Types satisfying condition 1 are required to have corresponding `RawTypeDefV9` declarations in the module.
    pub fn typespace(&self) -> &Typespace {
        &self.typespace
    }

    /// The `TableDef` an entity in the global namespace is stored in, if any.
    ///
    /// Generally, you will want to use the `lookup` method on the entity type instead.
    pub fn stored_in_table_def(&self, name: &Identifier) -> Option<&TableDef> {
        self.stored_in_table_def
            .get(name)
            .and_then(|table_name| self.tables.get(table_name))
    }

    /// Lookup a definition by its key in `self`.
    pub fn lookup<T: ModuleDefLookup>(&self, key: T::Key<'_>) -> Option<&T> {
        T::lookup(self, key)
    }

    /// Convenience method to look up a table, possibly by a string.
    pub fn table<K: ?Sized + Hash + Equivalent<Identifier>>(&self, name: &K) -> Option<&TableDef> {
        // If the string IS a valid identifier, we can just look it up.
        self.tables.get(name)
    }

    /// Generate indexes for the module definition.
    /// We guarantee that all `unique` constraints have an index generated for them.
    /// This will be removed once another enforcement mechanism is implemented.
    /// This is a noop if there are already usable indexes present.
    fn generate_indexes(&mut self) {
        for table in self.tables.values_mut() {
            for constraint in table.unique_constraints.values() {
                // if we have a constraint for the index, we're fine.
                if table.indexes.values().any(|index| {
                    let IndexDef {
                        algorithm: IndexAlgorithm::BTree { columns },
                        ..
                    } = index;

                    columns == &constraint.columns
                }) {
                    continue;
                }

                let column_names = constraint
                    .columns
                    .iter()
                    .map(|col_id| &*table.get_column(col_id).expect("validated unique constraint").name)
                    .join("_");

                // TODO(1.0): ensure generated index names are identical when upgrading the Rust module bindings.
                let mut index_name =
                    Identifier::new(format!("idx_{}_{}_{}_unique", table.name, column_names, constraint.name).into())
                        .expect("validated identifier parts");

                // incredibly janky loop to avoid name collisions.
                // hey, somebody could be being malicious.
                while self.stored_in_table_def.contains_key(&index_name) {
                    index_name =
                        Identifier::new(format!("{}_1", index_name).into()).expect("validated identifier parts");
                }

                table.indexes.insert(
                    index_name.clone(),
                    IndexDef {
                        name: index_name.clone(),
                        algorithm: IndexAlgorithm::BTree {
                            columns: constraint.columns.clone(),
                        },
                        accessor_name: None, // this is a generated index.
                    },
                );
                self.stored_in_table_def.insert(index_name, table.name.clone());
            }
        }
    }

    /// Lookup a definition by its key in `self`, panicking if it is not found.
    pub fn expect_lookup<T: ModuleDefLookup>(&self, key: T::Key<'_>) -> &T {
        if let Some(result) = T::lookup(self, key) {
            result
        } else {
            panic!("expected ModuleDef to contain {:?}, but it does not", key);
        }
    }

    /// Expect that this module definition contains a definition.
    pub fn expect_contains<Def: ModuleDefLookup>(&self, def: &Def) {
        if let Some(my_def) = self.lookup(def.key()) {
            assert_eq!(
                def as *const Def, my_def as *const Def,
                "expected ModuleDef to contain {:?}, but it contained {:?}",
                def, my_def
            );
        } else {
            panic!("expected ModuleDef to contain {:?}, but it does not", def.key());
        }
    }
}

impl TryFrom<RawModuleDef> for ModuleDef {
    type Error = ValidationErrors;

    fn try_from(raw: RawModuleDef) -> Result<Self, Self::Error> {
        match raw {
            RawModuleDef::V8BackCompat(v8_mod) => Self::try_from(v8_mod),
            RawModuleDef::V9(v9_mod) => Self::try_from(v9_mod),
            _ => unimplemented!(),
        }
    }
}
impl TryFrom<raw_def::v8::RawModuleDefV8> for ModuleDef {
    type Error = ValidationErrors;

    fn try_from(v8_mod: raw_def::v8::RawModuleDefV8) -> Result<Self, Self::Error> {
        // it is not necessary to generate indexes for a v8 mod, since the validation
        // handles index generation.
        validate::v8::validate(v8_mod)
    }
}
impl TryFrom<raw_def::v9::RawModuleDefV9> for ModuleDef {
    type Error = ValidationErrors;

    fn try_from(v9_mod: raw_def::v9::RawModuleDefV9) -> Result<Self, Self::Error> {
        let mut result = validate::v9::validate(v9_mod)?;
        result.generate_indexes();
        Ok(result)
    }
}
impl From<ModuleDef> for RawModuleDefV9 {
    fn from(val: ModuleDef) -> Self {
        let ModuleDef {
            tables,
            reducers,
            types,
            typespace,
            stored_in_table_def: _,
        } = val;

        RawModuleDefV9 {
            tables: to_raw(tables, |table: &RawTableDefV9| &table.name),
            reducers: to_raw(reducers, |reducer: &RawReducerDefV9| &reducer.name),
            types: to_raw(types, |type_: &RawTypeDefV9| &type_.name),
            misc_exports: vec![],
            typespace,
        }
    }
}

/// Implemented by definitions stored in a `ModuleDef`.
/// Allows looking definitions up in a `ModuleDef`, and across
/// `ModuleDef`s during migrations.
pub trait ModuleDefLookup: Sized + Debug + 'static {
    /// A reference to a definition of this type within a module def. This reference should be portable across migrations.
    type Key<'a>: Debug + Copy;

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

    /// The primary key of the table, if present. Must refer to a valid column.
    ///
    /// Currently, there must be a unique constraint and an index corresponding to the primary key.
    /// Eventually, we may remove the requirement for an index.
    ///
    /// The database engine does not actually care about this, but client code generation does.
    pub primary_key: Option<ColId>,

    /// The columns of this table. This stores the information in
    /// `product_type_ref` in a more convenient-to-access format.
    pub columns: Vec<ColumnDef>,

    /// The indices on the table, indexed by name.
    pub indexes: IdentifierMap<IndexDef>,

    /// The constraints on the table, indexed by name.
    pub constraints: IdentifierMap<ConstraintDef>,

    /// The sequences for the table, indexed by name.
    pub sequences: IdentifierMap<SequenceDef>,

    /// The schedule for the table, if present.
    pub schedule: Option<ScheduleDef>,

    /// Whether this is a system- or user-created table.
    pub table_type: TableType,

    /// Whether this table is public or private.
    pub table_access: TableAccess,
}

impl TableDef {
    /// Get a column of the `TableDef`.
    pub fn get_column(&self, id: ColId) -> Option<&ColumnDef> {
        self.columns.get(id.idx())
    }
    /// Get a column by the column's name.
    pub fn get_column_by_name(&self, name: &Identifier) -> Option<&ColumnDef> {
        self.columns.iter().find(|c| &c.name == name)
    }
}

impl From<TableDef> for RawTableDefV9 {
    fn from(val: TableDef) -> Self {
        let TableDef {
            name,
            product_type_ref,
            primary_key,
            columns: _, // will be reconstructed from the product type.
            indexes,
            unique_constraints,
            sequences,
            schedule,
            table_type,
            table_access,
        } = val;

        RawTableDefV9 {
            name: name.into(),
            product_type_ref,
            primary_key,
            indexes: to_raw(indexes, |index: &RawIndexDefV9| &index.name),
            constraints: to_raw(unique_constraints, |constraint: &RawUniqueConstraintDefV9| {
                &constraint.name
            }),
            sequences: to_raw(sequences, |sequence: &RawSequenceDefV9| &sequence.name),
            schedule: schedule.map(Into::into),
            table_type,
            table_access,
        }
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

    /// The increment to use when updating the sequence.
    pub increment: i128,
}

impl From<SequenceDef> for RawSequenceDefV9 {
    fn from(val: SequenceDef) -> Self {
        RawSequenceDefV9 {
            name: val.name.into(),
            column: val.column,
            start: val.start,
            min_value: val.min_value,
            max_value: val.max_value,
            increment: val.increment,
        }
    }
}

/// A struct representing the validated definition of a database index.
///
/// Cannot be created directly. Instead, add a [spacetimedb_sats::db::raw_def::RawIndexDef] to a [spacetimedb_sats::db::raw_def::RawDatabaseDef] and call [spacetimedb_sats::db::raw_def::RawDatabaseDef::validate].
#[derive(Debug, Clone, Eq, PartialEq)]
#[non_exhaustive]
pub struct IndexDef {
    /// The name of the index. Must be unique within the containing `RawDatabaseDef`.
    pub name: Identifier,

    /// Accessor name for the index used in client codegen.
    ///
    /// This is set the user and should not be assumed to follow
    /// any particular format.
    ///
    /// May be set to `None` if this is an auto-generated index for which the user
    /// has not supplied a name. In this case, no client code generation for this index
    /// will be performed.
    ///
    /// This name is not visible in the system tables, it is only used for client codegen.
    pub accessor_name: Option<Identifier>,

    /// The algorithm parameters for the index.
    pub algorithm: IndexAlgorithm,
}

impl IndexDef {
    /// Whether this index was generated by the system.
    pub fn generated(&self) -> bool {
        self.accessor_name.is_none()
    }
}

impl From<IndexDef> for RawIndexDefV9 {
    fn from(val: IndexDef) -> Self {
        RawIndexDefV9 {
            name: val.name.into(),
            algorithm: match val.algorithm {
                IndexAlgorithm::BTree { columns } => RawIndexAlgorithm::BTree { columns },
            },
            accessor_name: val.accessor_name.map(Into::into),
        }
    }
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

impl From<IndexAlgorithm> for RawIndexAlgorithm {
    fn from(val: IndexAlgorithm) -> Self {
        match val {
            IndexAlgorithm::BTree { columns } => RawIndexAlgorithm::BTree { columns },
        }
    }
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

    /// The type of this column. May refer to the containing `ModuleDef`'s `Typespace`.
    /// Must satisfy `AlgebraicType::is_valid_for_client_type_use`.
    ///
    /// Will always correspond to the corresponding element of this table's
    /// `product_type_ref`, that is, the element at index `col_id.idx()`
    /// with name `Some(name.as_str())`.
    pub ty: AlgebraicType,

    /// The table this `ColumnDef` is stored in.
    pub table_name: Identifier,
}

/// A constraint definition.
pub struct ConstraintDef {
    /// The name of the constraint.
    pub name: Identifier,
    /// The data for the constraint.
    pub data: ConstraintData,
}

/// Data for a constraint.
#[non_exhaustive]
pub enum ConstraintData {
    Unique(UniqueConstraintDef),
}

/// Requires that the projection of the table onto these columns is an bijection.
///
/// That is, there must be a one-to-one relationship between a row and the `columns` of that row.
#[derive(Debug, Clone, Eq, PartialEq)]
#[non_exhaustive]
pub struct UniqueConstraintDef {
    /// The columns on the containing `TableDef`
    pub columns: ColList,
}

impl From<UniqueConstraintDef> for RawUniqueConstraintDefV9 {
    fn from(val: UniqueConstraintDef) -> Self {
        RawUniqueConstraintDefV9 {
            name: val.name.into(),
            columns: val.columns,
        }
    }
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

impl From<ScheduleDef> for RawScheduleDefV9 {
    fn from(val: ScheduleDef) -> Self {
        RawScheduleDefV9 {
            name: val.name.into(),
            reducer_name: val.reducer_name.into(),
        }
    }
}

/// A type exported by the module.
#[derive(Debug, Clone, Eq, PartialEq)]
#[non_exhaustive]
pub struct TypeDef {
    /// The (scoped) name of the type.
    pub name: ScopedTypeName,

    /// The type to which the alias refers.
    pub ty: AlgebraicTypeRef,

    /// Whether this type has a custom ordering.
    pub custom_ordering: bool,
}
impl From<TypeDef> for RawTypeDefV9 {
    fn from(val: TypeDef) -> Self {
        RawTypeDefV9 {
            name: val.name.into(),
            ty: val.ty,
            custom_ordering: val.custom_ordering,
        }
    }
}

/// A scoped type name, in the form `scope0::scope1::...::scopeN::name`.
///
/// These are the names that will be used *in client code generation*, NOT the names used for types
/// in the module source code.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopedTypeName {
    /// The scope for this type.
    ///
    /// Empty unless a sats `name` attribute is used, e.g.
    /// `#[sats(name = "namespace.name")]` in Rust.
    scope: Box<[Identifier]>,

    /// The name of the type.
    ///
    /// Eventually, we may add more information to this, such as generic arguments.
    name: Identifier,
}
impl ScopedTypeName {
    /// Create a new `ScopedTypeName` from a scope and a name.
    pub fn new(scope: Box<[Identifier]>, name: Identifier) -> Self {
        ScopedTypeName { scope, name }
    }

    /// Try to create a new `ScopedTypeName` from a scope and a name.
    /// Errors if the scope or name are invalid.
    pub fn try_new(
        scope: impl IntoIterator<Item = RawIdentifier>,
        name: impl Into<RawIdentifier>,
    ) -> Result<Self, ErrorStream<IdentifierError>> {
        let scope = scope
            .into_iter()
            .map(|chunk| Identifier::new(chunk).map_err(ErrorStream::from))
            .collect_all_errors();
        let name = Identifier::new(name.into()).map_err(ErrorStream::from);
        let (scope, name) = (scope, name).combine_errors()?;
        Ok(ScopedTypeName { scope, name })
    }

    /// Create a new `ScopedTypeName` with an empty scope.
    pub fn from_name(name: Identifier) -> Self {
        ScopedTypeName {
            scope: Box::new([]),
            name,
        }
    }
}
impl fmt::Debug for ScopedTypeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // we can wrap this in a pair of double quotes, since we know
        // none of its elements contain quotes.
        f.write_char('"')?;
        for scope in &*self.scope {
            write!(f, "{}::", scope)?;
        }
        write!(f, "{}\"", self.name)
    }
}
impl fmt::Display for ScopedTypeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for scope in &*self.scope {
            write!(f, "{}::", scope)?;
        }
        fmt::Display::fmt(&self.name, f)
    }
}
impl TryFrom<RawScopedTypeNameV9> for ScopedTypeName {
    type Error = ErrorStream<IdentifierError>;

    fn try_from(value: RawScopedTypeNameV9) -> Result<Self, Self::Error> {
        Self::try_new(value.scope.into_vec(), value.name)
    }
}
impl From<ScopedTypeName> for RawScopedTypeNameV9 {
    fn from(val: ScopedTypeName) -> Self {
        RawScopedTypeNameV9 {
            scope: val.scope.into_vec().into_iter().map_into().collect(),
            name: val.name.into(),
        }
    }
}

/// A reducer exported by the module.
#[derive(Debug, Clone, Eq, PartialEq)]
#[non_exhaustive]
pub struct ReducerDef {
    /// The name of the reducer. This must be unique within the module.
    pub name: Identifier,

    /// The parameters of the reducer.
    ///
    /// This `ProductType` need not be registered in the module's `Typespace`.
    pub params: ProductType,

    /// The special role of this reducer in the module lifecycle, if any.
    pub lifecycle: Option<Lifecycle>,
}

impl From<ReducerDef> for RawReducerDefV9 {
    fn from(val: ReducerDef) -> Self {
        RawReducerDefV9 {
            name: val.name.into(),
            params: val.params,
            lifecycle: val.lifecycle,
        }
    }
}

impl ModuleDefLookup for TableDef {
    type Key<'a> = &'a Identifier;

    fn key(&self) -> Self::Key<'_> {
        &self.name
    }

    fn lookup<'a>(module_def: &'a ModuleDef, key: Self::Key<'_>) -> Option<&'a Self> {
        module_def.tables.get(key)
    }
}

impl ModuleDefLookup for SequenceDef {
    type Key<'a> = &'a Identifier;

    fn key(&self) -> Self::Key<'_> {
        &self.name
    }

    fn lookup<'a>(module_def: &'a ModuleDef, key: Self::Key<'_>) -> Option<&'a Self> {
        module_def.stored_in_table_def(key)?.sequences.get(key)
    }
}

impl ModuleDefLookup for IndexDef {
    type Key<'a> = &'a Identifier;

    fn key(&self) -> Self::Key<'_> {
        &self.name
    }

    fn lookup<'a>(module_def: &'a ModuleDef, key: Self::Key<'_>) -> Option<&'a Self> {
        module_def.stored_in_table_def(key)?.indexes.get(key)
    }
}

impl ModuleDefLookup for ColumnDef {
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

impl ModuleDefLookup for UniqueConstraintDef {
    type Key<'a> = &'a Identifier;

    fn key(&self) -> Self::Key<'_> {
        &self.name
    }

    fn lookup<'a>(module_def: &'a ModuleDef, key: Self::Key<'_>) -> Option<&'a Self> {
        module_def.stored_in_table_def(key)?.unique_constraints.get(key)
    }
}

impl ModuleDefLookup for ScheduleDef {
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

impl ModuleDefLookup for TypeDef {
    type Key<'a> = &'a ScopedTypeName;

    fn key(&self) -> Self::Key<'_> {
        &self.name
    }

    fn lookup<'a>(module_def: &'a ModuleDef, key: Self::Key<'_>) -> Option<&'a Self> {
        module_def.types.get(key)
    }
}

impl ModuleDefLookup for ReducerDef {
    type Key<'a> = &'a Identifier;

    fn key(&self) -> Self::Key<'_> {
        &self.name
    }

    fn lookup<'a>(module_def: &'a ModuleDef, key: Self::Key<'_>) -> Option<&'a Self> {
        module_def.reducers.get(key)
    }
}

fn to_raw<Def, RawDef, Name, RawName>(data: HashMap<Name, Def>, f: impl Fn(&RawDef) -> &RawName) -> Vec<RawDef>
where
    Def: ModuleDefLookup + Into<RawDef>,
    RawName: Eq + Ord + 'static,
{
    let mut result: Vec<RawDef> = data.into_iter().map(|(_, def)| def.into()).collect();
    result.sort_by(|a, b| f(a).cmp(f(b)));
    result
}
