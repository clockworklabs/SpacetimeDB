//! ABI Version 9 of the raw module definitions.
//!
//! This is the ABI that will be used for 1.0.
//! We are keeping around the old ABI (v8) for now, to allow ourselves to convert the codebase
//! a component-at-a-time.

use std::any::TypeId;
use std::collections::btree_map;
use std::collections::BTreeMap;
use std::fmt;

use itertools::Itertools;
use spacetimedb_primitives::*;
use spacetimedb_sats::typespace::TypespaceBuilder;
use spacetimedb_sats::AlgebraicType;
use spacetimedb_sats::AlgebraicTypeRef;
use spacetimedb_sats::ProductType;
use spacetimedb_sats::ProductTypeElement;
use spacetimedb_sats::SpacetimeType;
use spacetimedb_sats::Typespace;

use crate::db::auth::StAccess;
use crate::db::auth::StTableType;

/// A not-yet-validated identifier.
pub type RawIdentifier = Box<str>;

/// A possibly-invalid raw module definition.
///
/// ABI Version 9.
///
/// These "raw definitions" may contain invalid data, and are validated by the `validate` module into a proper `spacetimedb_schema::ModuleDef`, or a collection of errors.
///
/// The module definition has a single logical global namespace, which maps `Identifier`s to:
///
/// - database-level objects:
///     - logical schema objects:
///         - tables
///         - constraints
///         - sequence definitions
///     - physical schema objects:
///         - indexes
/// - module-level objects:
///     - reducers
///     - schedule definitions
/// - binding-level objects:
///     - type aliases
///
/// All of these types of objects must have unique names within the module.
/// The exception is columns, which need unique names only within a table.
#[derive(Debug, Clone, Default, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawModuleDefV9 {
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
    pub typespace: Typespace,

    /// The tables of the database definition used in the module.
    ///
    /// Each table must have a unique name.
    pub tables: Vec<RawTableDefV9>,

    /// The reducers exported by the module.
    pub reducers: Vec<RawReducerDefV9>,

    /// The types exported by the module.
    pub types: Vec<RawTypeDefV9>,

    /// Miscellaneous additional module exports.
    pub misc_exports: Vec<RawMiscModuleExportV9>,
}

/// The definition of a database table.
///
/// This struct holds information about the table, including its name, columns, indexes,
/// constraints, sequences, type, and access rights.
///
/// Validation rules:
/// - The table name must be a valid [`crate::db::identifier::Identifier`].
/// - The table's indexes, constraints, and sequences need not be sorted; they will be sorted according to their respective ordering rules.
/// - The table's column types may refer only to types in the containing `RawModuleDefV9`'s typespace.
/// - The table's column names must be unique.
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawTableDefV9 {
    /// The name of the table.
    /// Unique within a module, acts as the table's identifier.
    /// Must be a valid [crate::db::identifier::Identifier].
    pub name: RawIdentifier,

    /// A reference to a `ProductType` containing the columns of this table.
    /// This is the single source of truth for the table's columns.
    /// All elements of the `ProductType` must have names.
    ///
    /// Like all types in the module, this must have the [default element ordering](crate::db::default_element_ordering),
    /// UNLESS a custom ordering is declared via a `RawTypeDefv9` for this type.
    pub product_type_ref: AlgebraicTypeRef,

    /// The primary key of the table, if present. Must refer to a valid column.
    ///
    /// Currently, there must be a unique constraint and an index corresponding to the primary key.
    /// Eventually, we may remove the requirement for an index.
    ///
    /// The database engine does not actually care about this, but client code generation does.
    pub primary_key: Option<ColId>,

    /// The indices of the table.
    pub indexes: Vec<RawIndexDefV9>,

    /// Any unique constraints on the table.
    pub constraints: Vec<RawConstraintDefV9>,

    /// The sequences for the table.
    pub sequences: Vec<RawSequenceDefV9>,

    /// The schedule for the table.
    pub schedule: Option<RawScheduleDefV9>,

    /// Whether this is a system- or user-created table.
    pub table_type: TableType,

    /// Whether this table is public or private.
    pub table_access: TableAccess,
}

/// Whether the table was created by the system or the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, SpacetimeType)]
#[sats(crate = crate)]
pub enum TableType {
    /// Created by the system.
    System,
    /// Created by the user.
    User,
}
impl From<StTableType> for TableType {
    fn from(t: StTableType) -> Self {
        match t {
            StTableType::System => TableType::System,
            StTableType::User => TableType::User,
        }
    }
}
impl From<TableType> for StTableType {
    fn from(t: TableType) -> Self {
        match t {
            TableType::System => StTableType::System,
            TableType::User => StTableType::User,
        }
    }
}

/// The visibility of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, SpacetimeType)]
#[sats(crate = crate)]
pub enum TableAccess {
    /// Visible to all
    Public,
    /// Visible only to the owner
    Private,
}
impl From<StAccess> for TableAccess {
    fn from(t: StAccess) -> Self {
        match t {
            StAccess::Public => TableAccess::Public,
            StAccess::Private => TableAccess::Private,
        }
    }
}
impl From<TableAccess> for StAccess {
    fn from(t: TableAccess) -> Self {
        match t {
            TableAccess::Public => StAccess::Public,
            TableAccess::Private => StAccess::Private,
        }
    }
}

/// A sequence definition for a database table column.
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawSequenceDefV9 {
    /// The name of the sequence. Must be unique within the containing `RawModuleDefV9`.
    pub name: RawIdentifier,

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

    /// The increment used when updating the SequenceDef.
    pub increment: i128,
}

/// The definition of a database index.
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawIndexDefV9 {
    /// The name of the index.
    ///
    /// Currently, this is always set automatically, but that may not be the case in the future.
    ///
    /// Unique within the containing `DatabaseDef`.
    pub name: RawIdentifier,

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
    pub accessor_name: Option<RawIdentifier>,

    /// The algorithm parameters for the index.
    pub algorithm: RawIndexAlgorithm,
}

/// Data specifying an index algorithm.
/// New fields MUST be added to the END of this enum, to maintain ABI compatibility.
#[non_exhaustive]
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub enum RawIndexAlgorithm {
    /// Implemented using a B-Tree.
    ///
    /// Currently, this uses a rust `std::collections::BTreeMap`.
    BTree {
        /// The columns to index on. These are ordered.
        columns: ColList,
    },
    /// Currently forbidden.
    Hash {
        /// The columns to index on. These are ordered.
        columns: ColList,
    },
}

/// Requires that the projection of the table onto these `columns` is a bijection.
///
/// That is, there must be a one-to-one relationship between a row and the `columns` of that row.
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawUniqueConstraintDefV9 {
    /// The name of the unique constraint. Must be unique within the containing `RawModuleDefV9`.
    pub name: RawIdentifier,

    /// The columns that must be unique.
    pub columns: ColList,
}

/// Marks a table as a timer table for a scheduled reducer.
///
/// The table must have columns:
/// - `scheduled_id` of type `u64`.
/// - `scheduled_at` of type `ScheduleAt`.
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawScheduleDefV9 {
    /// The name of the schedule. Must be unique within the containing `RawModuleDefV9`.
    pub name: RawIdentifier,

    /// The name of the reducer to call.
    pub reducer_name: RawIdentifier,
}

/// A constraint definition attached to a table.
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawConstraintDefV9 {
    /// The name of the constraint. Must be unique within the containing `RawModuleDefV9`.
    pub name: RawIdentifier,

    /// The data for the constraint.
    pub data: RawConstraintDataV9,
}

/// Raw data attached to a constraint.
/// New fields MUST be added to the END of this enum, to maintain ABI compatibility.
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
#[non_exhaustive]
pub enum RawConstraintDataV9 {
    Unique(RawUniqueConstraintDataV9),
}

/// Requires that the projection of the table onto these `columns` is a bijection.
///
/// That is, there must be a one-to-one relationship between a row and the `columns` of that row.
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawUniqueConstraintDataV9 {
    /// The columns that must be unique.
    pub columns: ColList,
}

/// A miscellaneous module export.
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
#[non_exhaustive]
pub enum RawMiscModuleExportV9 {}

/// A type declaration.
///
/// Exactly of these must be attached to every `Product` and `Sum` type used by a module.
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawTypeDefV9 {
    /// The name of the type declaration.
    pub name: RawScopedTypeNameV9,

    /// The type to which the declaration refers.
    /// This must point to an `AlgebraicType::Product` or an `AlgebraicType::Sum` in the module's typespace.
    pub ty: AlgebraicTypeRef,

    /// Whether this type has a custom ordering.
    pub custom_ordering: bool,
}

/// A scoped type name, in the form `scope0::scope1::...::scopeN::name`.
///
/// These are the names that will be used *in client code generation*, NOT the names used for types
/// in the module source code.
#[derive(Clone, SpacetimeType, PartialEq, Eq, PartialOrd, Ord)]
#[sats(crate = crate)]
pub struct RawScopedTypeNameV9 {
    /// The scope for this type.
    ///
    /// Empty unless a sats `name` attribute is used, e.g.
    /// `#[sats(name = "namespace.name")]` in Rust.
    pub scope: Box<[RawIdentifier]>,

    /// The name of the type. This must be unique within the module.
    ///
    /// Eventually, we may add more information to this, such as generic arguments.
    pub name: RawIdentifier,
}

impl fmt::Debug for RawScopedTypeNameV9 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for module in self.scope.iter() {
            fmt::Debug::fmt(module, f)?;
            f.write_str("::")?;
        }
        fmt::Debug::fmt(&self.name, f)?;
        Ok(())
    }
}

/// A reducer definition.
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawReducerDefV9 {
    /// The name of the reducer.
    pub name: RawIdentifier,

    /// The types and optional names of the parameters, in order.
    /// This `ProductType` need not be registered in the typespace.
    pub params: ProductType,

    /// If the reducer has a special role in the module lifecycle, it should be marked here.
    pub lifecycle: Option<Lifecycle>,
}

/// Special roles a reducer can play in the module lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, SpacetimeType)]
#[sats(crate = crate)]
#[non_exhaustive]
pub enum Lifecycle {
    /// The reducer will be invoked upon module initialization.
    Init,
    /// The reducer will be invoked when a client connects.
    OnConnect,
    /// The reducer will be invoked when a client disconnects.
    OnDisconnect,
}

/// A builder for a [`ModuleDef`].
#[derive(Default)]
pub struct RawModuleDefV9Builder {
    /// The module definition.
    module: RawModuleDefV9,
    /// The type map from `T: 'static` Rust types to sats types.
    type_map: BTreeMap<TypeId, AlgebraicTypeRef>,
}

impl RawModuleDefV9Builder {
    /// Create a new, empty `RawModuleDefBuilder`.
    pub fn new() -> Self {
        Default::default()
    }

    /// Add a type to the in-progress module.
    ///
    /// The returned type must satisfy `AlgebraicType::is_valid_for_client_type_definition` or  `AlgebraicType::is_valid_for_client_type_use` .
    pub fn add_type<T: SpacetimeType>(&mut self) -> AlgebraicType {
        TypespaceBuilder::add_type::<T>(self)
    }

    /// Create a table builder.
    ///
    /// Does not validate that the product_type_ref is valid; this is left to the module validation code.
    pub fn build_table(
        &mut self,
        name: impl Into<RawIdentifier>,
        product_type_ref: AlgebraicTypeRef,
    ) -> RawTableDefBuilder {
        let name = name.into();
        RawTableDefBuilder {
            module_def: &mut self.module,
            table: RawTableDefV9 {
                name,
                product_type_ref,
                indexes: vec![],
                constraints: vec![],
                sequences: vec![],
                schedule: None,
                primary_key: None,
                table_type: TableType::User,
                table_access: TableAccess::Public,
            },
        }
    }

    /// Build a new table with a product type.
    /// Adds the type to the module.
    pub fn build_table_with_new_type(
        &mut self,
        table_name: impl Into<RawIdentifier>,
        product_type: spacetimedb_sats::ProductType,
        custom_ordering: bool,
    ) -> RawTableDefBuilder {
        let table_name = table_name.into();
        let product_type_ref = self.add_algebraic_type([], table_name.clone(), product_type.into(), custom_ordering);

        self.build_table(table_name, product_type_ref)
    }

    /// Add a type to the typespace, along with a type alias declaring its name.
    /// This method should only be use for `AlgebraicType`s not corresponding to a Rust
    /// type that implements `SpacetimeType`.
    ///
    /// Returns a reference to the newly-added type.
    ///
    /// NOT idempotent, calling this twice with the same name will cause errors during
    /// validation.
    ///
    /// You must set `custom_ordering` if you're not using the default element ordering.
    pub fn add_algebraic_type(
        &mut self,
        scope: impl IntoIterator<Item = RawIdentifier>,
        name: impl Into<RawIdentifier>,
        ty: spacetimedb_sats::AlgebraicType,
        custom_ordering: bool,
    ) -> AlgebraicTypeRef {
        let ty = self.module.typespace.add(ty);
        let scope = scope.into_iter().collect();
        let name = name.into();
        self.module.types.push(RawTypeDefV9 {
            name: RawScopedTypeNameV9 { name, scope },
            ty,
            custom_ordering,
        });
        // We don't add a `TypeId` to `self.type_map`, because there may not be a corresponding Rust type! e.g. if we are randomly generating types in proptests.
        ty
    }

    /// Add a reducer to the in-progress module.
    /// Accepts a `ProductType` of reducer arguments for convenience.
    /// The `ProductType` need not be registered in the typespace.
    ///
    /// Importantly, if the reducer's first argument is a `ReducerContext`, that
    /// information should not be provided to this method.
    /// That is an implementation detail handled by the module bindings and can be ignored.
    /// As far as the module definition is concerned, the reducer's arguments
    /// start with the first non-`ReducerContext` argument.
    ///
    /// (It is impossible, with the current implementation of `ReducerContext`, to
    /// have more than one `ReducerContext` argument, at least in Rust.
    /// This is because `SpacetimeType` is not implemented for `ReducerContext`,
    /// so it can never act like an ordinary argument.)
    pub fn add_reducer(
        &mut self,
        name: impl Into<RawIdentifier>,
        params: spacetimedb_sats::ProductType,
        lifecycle: Option<Lifecycle>,
    ) {
        self.module.reducers.push(RawReducerDefV9 {
            name: name.into(),
            params,
            lifecycle,
        });
    }

    /// Get the typespace of the module.
    pub fn typespace(&self) -> &Typespace {
        &self.module.typespace
    }

    /// Finish building, consuming the builder and returning the module.
    /// The module should be validated before use.
    pub fn finish(self) -> RawModuleDefV9 {
        self.module
    }
}

/// Convert a string from a sats type-name annotation like `#[sats(name = "namespace.name")]` to a `RawScopedTypeNameV9`.
/// We split the input on the strings `"::"` and `"."` to split up module paths.
///
/// TODO(1.0): build namespacing directly into the bindings macros so that we don't need to do this.
pub fn sats_name_to_scoped_name(sats_name: &str) -> RawScopedTypeNameV9 {
    // We can't use `&[char]: Pattern` for `split` here because "::" is not a char :/
    let mut scope: Vec<RawIdentifier> = sats_name.split("::").flat_map(|s| s.split('.')).map_into().collect();
    // Unwrapping to "" will result in a validation error down the line, which is exactly what we want.
    let name = scope.pop().unwrap_or_default();
    RawScopedTypeNameV9 {
        scope: scope.into(),
        name,
    }
}

impl TypespaceBuilder for RawModuleDefV9Builder {
    fn add(
        &mut self,
        typeid: TypeId,
        name: Option<&'static str>,
        make_ty: impl FnOnce(&mut Self) -> AlgebraicType,
    ) -> AlgebraicType {
        let r = match self.type_map.entry(typeid) {
            btree_map::Entry::Occupied(o) => *o.get(),
            btree_map::Entry::Vacant(v) => {
                // Bind a fresh alias to the unit type.
                let slot_ref = self.module.typespace.add(AlgebraicType::unit());
                // Relate `typeid -> fresh alias`.
                v.insert(slot_ref);

                // Alias provided? Relate `name -> slot_ref`.
                if let Some(sats_name) = name {
                    let name = sats_name_to_scoped_name(sats_name);

                    self.module.types.push(RawTypeDefV9 {
                        name,
                        ty: slot_ref,
                        // TODO(1.0): we need to update the `TypespaceBuilder` trait to include
                        // a `custom_ordering` parameter.
                        // For now, we assume all types have custom orderings, since the derive
                        // macro doesn't know about the default ordering yet.
                        custom_ordering: true,
                    });
                }

                // Borrow of `v` has ended here, so we can now convince the borrow checker.
                let ty = make_ty(self);
                self.module.typespace[slot_ref] = ty;
                slot_ref
            }
        };
        AlgebraicType::Ref(r)
    }
}

/// Builder for a `RawTableDef`.
pub struct RawTableDefBuilder<'a> {
    module_def: &'a mut RawModuleDefV9,
    table: RawTableDefV9,
}

impl<'a> RawTableDefBuilder<'a> {
    /// Sets the type of the table and return it.
    ///
    /// This is not about column algebraic types, but about whether the table
    /// was created by the system or the user.
    pub fn with_type(mut self, table_type: TableType) -> Self {
        self.table.table_type = table_type;
        self
    }

    /// Sets the access rights for the table and return it.
    pub fn with_access(mut self, table_access: TableAccess) -> Self {
        self.table.table_access = table_access;
        self
    }

    /// Generates a [UniqueConstraintDef] using the supplied `columns`.
    pub fn with_unique_constraint(mut self, columns: impl Into<ColList>, name: Option<RawIdentifier>) -> Self {
        let columns = columns.into();
        let name = name.unwrap_or_else(|| self.generate_unique_constraint_name(&columns));
        self.table.constraints.push(RawConstraintDefV9 {
            name,
            data: RawConstraintDataV9::Unique(RawUniqueConstraintDataV9 { columns }),
        });
        self
    }

    /// Adds a primary key to the table.
    /// You must also add a unique constraint on the primary key column.
    pub fn with_primary_key(mut self, column: impl Into<ColId>) -> Self {
        self.table.primary_key = Some(column.into());
        self
    }

    /// Generates a [RawIndexDef] using the supplied `columns`.
    pub fn with_index(
        mut self,
        algorithm: RawIndexAlgorithm,
        accessor_name: impl Into<RawIdentifier>,
        name: Option<RawIdentifier>,
    ) -> Self {
        let name = name.unwrap_or_else(|| self.generate_index_name(&algorithm));
        let accessor_name = accessor_name.into();

        self.table.indexes.push(RawIndexDefV9 {
            name,
            accessor_name: Some(accessor_name),
            algorithm,
        });
        self
    }

    /// Adds a [RawSequenceDef] on the supplied `column`.
    pub fn with_column_sequence(mut self, column: impl Into<ColId>, name: Option<RawIdentifier>) -> Self {
        let column = column.into();
        let name = name.unwrap_or_else(|| self.generate_sequence_name(column));
        self.table.sequences.push(RawSequenceDefV9 {
            name,
            column,
            start: None,
            min_value: None,
            max_value: None,
            increment: 1,
        });

        self
    }

    /// Adds a schedule definition to the table.
    ///
    /// The table must have the appropriate columns for a scheduled table.
    pub fn with_schedule(mut self, reducer_name: impl Into<RawIdentifier>, name: Option<RawIdentifier>) -> Self {
        let reducer_name = reducer_name.into();
        let name = name.unwrap_or_else(|| self.generate_schedule_name());
        self.table.schedule = Some(RawScheduleDefV9 { name, reducer_name });
        self
    }

    /// Build the table and add it to the module, returning the `product_type_ref` of the table.
    pub fn finish(self) -> AlgebraicTypeRef {
        self.table.product_type_ref
        // self is now dropped.
    }

    /// Get the column ID of the column with the specified name, if any.
    ///
    /// Returns `None` if this `TableDef` has been constructed with an invalid `ProductTypeRef`,
    /// or if no column exists with that name.
    pub fn find_col_pos_by_name(&self, column: impl AsRef<str>) -> Option<ColId> {
        let column = column.as_ref();
        self.columns()?
            .iter()
            .position(|x| x.name().is_some_and(|s| s == column))
            .map(|x| x.into())
    }

    /// Get the columns of this type.
    ///
    /// Returns `None` if this `TableDef` has been constructed with an invalid `ProductTypeRef`.
    fn columns(&self) -> Option<&[ProductTypeElement]> {
        self.module_def
            .typespace
            .get(self.table.product_type_ref)
            .and_then(|ty| ty.as_product())
            .map(|p| &p.elements[..])
    }

    /// Get the name of a column in the typespace.
    ///
    /// Only used for generating names for indexes, sequences, and unique constraints.
    ///
    /// Generates `col_{column}` if the column has no name or if the `RawTableDef`'s `product_type_ref`
    /// was initialized incorrectly.
    fn column_name(&self, column: ColId) -> String {
        self.columns()
            .and_then(|columns| columns.get(column.idx()))
            .and_then(|column| column.name().map(ToString::to_string))
            .unwrap_or_else(|| format!("col_{}", column.0))
    }

    /// Concatenate a list of column names.
    fn concat_column_names(&self, selected: &ColList) -> String {
        selected.iter().map(|col| self.column_name(col)).join("_")
    }

    /// YOU CANNOT RELY ON INDEXES HAVING THIS NAME FORMAT.
    fn generate_index_name(&self, algorithm: &RawIndexAlgorithm) -> RawIdentifier {
        let (label, columns) = match algorithm {
            RawIndexAlgorithm::BTree { columns } => ("btree", columns),
            RawIndexAlgorithm::Hash { columns } => ("hash", columns),
        };
        let column_names = self.concat_column_names(columns);
        let table_name = &self.table.name;
        format!("idx_{table_name}_{label}_{column_names}").into()
    }

    /// YOU CANNOT RELY ON SEQUENCES HAVING THIS NAME FORMAT.
    fn generate_sequence_name(&self, column: ColId) -> RawIdentifier {
        let column_name = self.column_name(column);
        let table_name = &self.table.name;
        format!("seq_{table_name}_{column_name}").into()
    }

    /// YOU CANNOT RELY ON SCHEDULES HAVING THIS NAME FORMAT.
    fn generate_schedule_name(&self) -> RawIdentifier {
        let table_name = &self.table.name;
        format!("schedule_{table_name}").into()
    }

    /// YOU CANNOT RELY ON UNIQUE CONSTRAINTS HAVING THIS NAME FORMAT.
    fn generate_unique_constraint_name(&self, columns: &ColList) -> RawIdentifier {
        let column_names = self.concat_column_names(columns);
        let table_name = &self.table.name;
        format!("ct_{table_name}_{column_names}_unique").into()
    }
}

impl Drop for RawTableDefBuilder<'_> {
    fn drop(&mut self) {
        self.module_def.tables.push(self.table.clone());
    }
}
