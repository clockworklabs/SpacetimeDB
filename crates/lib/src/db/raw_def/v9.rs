use std::any::TypeId;
use std::collections::btree_map;
use std::collections::BTreeMap;

use crate::db::auth::{StAccess, StTableType};
use itertools::Itertools;
use spacetimedb_primitives::*;
use spacetimedb_sats::typespace::TypespaceBuilder;
use spacetimedb_sats::AlgebraicType;
use spacetimedb_sats::AlgebraicTypeRef;
use spacetimedb_sats::ProductTypeElement;
use spacetimedb_sats::SpacetimeType;
use spacetimedb_sats::{de, ser, Typespace};

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
#[derive(Debug, Clone, Default, ser::Serialize, de::Deserialize)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawModuleDefV9 {
    /// The `Typespace` used by the module.
    ///
    /// `AlgebraicTypeRef`s in the table, reducer, and type alias declarations refer to this typespace.
    ///
    /// The typespace must satisfy `Typespace::is_nominal_normal_form`. That is, all types referenced by this typespace must either:
    /// 1. satisfy `AlgebraicType::is_nominal_normal_form`,
    /// 2. or be a `Sum`/`ProductType`s whose fields satisfy the same.
    ///
    /// Types satisfying condition 2 are called *non-nominal types*.
    /// Non-nominal types correspond to generated classes in client code.
    /// Every non-nominal type in this typespace MUST have a corresponding `RawTypeDefV9` declaration in the `types` field, with a module-unique name.
    ///
    /// (Nominal types are allowed to have `RawTypeDefV9` declarations, but do not require them.)
    ///
    /// All non-nominal in this typespace MUST have the [default element ordering](crate::db::default_element_ordering) UNLESS they declare a custom ordering via their `RawTypeDefV9`.
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

impl RawModuleDefV9 {
    /// Creates a new, empty [RawDatabaseDef] instance with no types in its typespace.
    pub fn new() -> Self {
        Default::default()
    }
}

/// The definition of a database table.
///
/// This struct holds information about the table, including its name, columns, indexes,
/// constraints, sequences, type, and access rights.
///
/// Validation rules:
/// - The table name must be a valid [crate::db::identifier::Identifier].
/// - The table's indexes, constraints, and sequences need not be sorted; they will be sorted according to their respective ordering rules.
/// - The table's column types may refer only to types in the containing RawDatabaseDef's typespace.
/// - The table's column names must be unique.
#[derive(Debug, Clone, ser::Serialize, de::Deserialize)]
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
    /// Like all types in the module, this must have the [default element ordering](crate::db::default_element_ordering), UNLESS a custom ordering is declared via `ModuleDef.misc_exports` for this type.
    pub product_type_ref: AlgebraicTypeRef,
    /// The indices of the table.
    pub indexes: Vec<RawIndexDefV9>,
    /// Any unique constraints on the table.
    pub unique_constraints: Vec<RawUniqueConstraintDefV9>,
    /// The sequences for the table.
    pub sequences: Vec<RawSequenceDefV9>,
    /// The schedule for the table.
    pub schedule: Option<RawScheduleDefV9>,
    /// Whether this is a system- or user-created table.
    pub table_type: StTableType,
    /// Whether this table is public or private.
    pub table_access: StAccess,
}

/// A sequence definition for a database table column.
#[derive(Debug, Clone, ser::Serialize, de::Deserialize)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawSequenceDefV9 {
    /// The name of the sequence. Must be unique within the containing `RawDatabaseDef`.
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
}

/// The definition of a database index.
#[derive(Debug, Clone, ser::Serialize, de::Deserialize)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawIndexDefV9 {
    /// The name of the index.
    ///
    /// This can be overridden by the user and should NOT be assumed to follow
    /// any particular format.
    ///
    /// Unique within the containing `DatabaseDef`.
    pub name: RawIdentifier,

    /// The algorithm parameters for the index.
    pub algorithm: RawIndexAlgorithm,
}

/// Data specifying an index algorithm.
#[non_exhaustive]
#[derive(Debug, Clone, ser::Serialize, de::Deserialize)]
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
#[derive(Debug, Clone, ser::Serialize, de::Deserialize)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawUniqueConstraintDefV9 {
    /// The name of the unique constraint. Must be unique within the containing `RawDatabaseDef`.
    pub name: RawIdentifier,

    /// The columns that must be unique.
    pub columns: ColList,
}

/// Marks a table as a timer table for a scheduled reducer.
///
/// The table must have columns:
/// - `scheduled_id` of type `u64`.
/// - `scheduled_at` of type `ScheduleAt`.
#[derive(Debug, Clone, ser::Serialize, de::Deserialize)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawScheduleDefV9 {
    /// The name of the schedule. Must be unique within the containing `RawDatabaseDef`.
    pub name: RawIdentifier,

    /// The name of the reducer to call.
    pub reducer_name: RawIdentifier,
}

/// A miscellaneous module export.
#[derive(Debug, Clone, ser::Serialize, de::Deserialize)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
#[non_exhaustive]
pub enum RawMiscModuleExportV9 {}

/// A type alias.
///
/// Exactly of these must be attached to every `Product` and `Sum` type used by a module.
#[derive(Debug, Clone, de::Deserialize, ser::Serialize)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawTypeDefV9 {
    /// The name of the type. This must be unique within the module.
    ///
    /// Eventually, we may add more information to this, such as the module name and generic arguments.
    pub name: RawIdentifier,

    /// The type to which the alias refers.
    pub ty: AlgebraicTypeRef,

    /// Whether this type has a custom ordering.
    pub custom_ordering: bool,
}

/// A reducer definition.
#[derive(Debug, Clone, de::Deserialize, ser::Serialize)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawReducerDefV9 {
    /// The name of the reducer.
    pub name: RawIdentifier,

    /// The types and optional names of the parameters, in order.
    /// Parameters are identified by their position in the list, not name.
    pub params: Vec<ProductTypeElement>,
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
    /// The returned type must satisfy `AlgebraicType::is_nominal_normal_form`
    pub fn add_type<T: SpacetimeType>(&mut self) -> AlgebraicType {
        TypespaceBuilder::add_type::<T>(self)
    }

    /// Create a table builder.
    ///
    /// Does not validate that the product_type_ref is valid; this is left to the module validation code.
    pub fn build_table(&mut self, name: RawIdentifier, product_type_ref: AlgebraicTypeRef) -> RawTableDefBuilder {
        RawTableDefBuilder {
            module_def: &mut self.module,
            table: RawTableDefV9 {
                name,
                product_type_ref,
                indexes: vec![],
                unique_constraints: vec![],
                sequences: vec![],
                schedule: None,
                table_type: StTableType::User,
                // TODO(1.0): make the default `Private` before 1.0.
                table_access: StAccess::Public,
            },
        }
    }

    /// Build a new table with a product type.
    ///
    /// This is a convenience method for tests, since in real modules, the product type is initialized via the `SpacetimeType` trait.
    #[cfg(feature = "test")]
    pub fn build_table_for_tests(
        &mut self,
        table_name: impl Into<RawIdentifier>,
        product_type: spacetimedb_sats::ProductType,
        custom_ordering: bool,
    ) -> RawTableDefBuilder {
        let table_name = table_name.into();
        let product_type_ref = self.add_type_for_tests(table_name.clone(), product_type.into(), custom_ordering);

        self.build_table(table_name, product_type_ref)
    }

    /// Add a type to the typespace, along with a type alias declaring its name.
    ///
    /// Returns a reference to the newly-added type.
    ///
    /// NOT idempotent, calling this twice with the same name will cause errors during
    /// validation.
    ///
    /// You must set `custom_ordering` if you're not using the default element ordering.
    ///
    /// This is a convenience method for tests, since in real modules, types are added to the
    /// typespace via the `SpacetimeType` trait.
    #[cfg(feature = "test")]
    pub fn add_type_for_tests(
        &mut self,
        name: impl Into<RawIdentifier>,
        ty: spacetimedb_sats::AlgebraicType,
        custom_ordering: bool,
    ) -> AlgebraicTypeRef {
        let ty = self.module.typespace.add(ty);

        let name = name.into();
        self.module.types.push(RawTypeDefV9 {
            name,
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
    pub fn add_reducer(&mut self, name: impl Into<RawIdentifier>, params: spacetimedb_sats::ProductType) {
        self.module.reducers.push(RawReducerDefV9 {
            name: name.into(),
            params: params.elements.into(),
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
                if let Some(name) = name {
                    self.module.types.push(RawTypeDefV9 {
                        name: name.to_owned().into(),
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
    pub fn with_type(mut self, table_type: StTableType) -> Self {
        self.table.table_type = table_type;
        self
    }

    /// Sets the access rights for the table and return it.
    pub fn with_access(mut self, table_access: StAccess) -> Self {
        self.table.table_access = table_access;
        self
    }

    /// Generates a [UniqueConstraintDef] using the supplied `columns`.
    pub fn with_unique_constraint(mut self, columns: ColList, name: Option<RawIdentifier>) -> Self {
        let name = name.unwrap_or_else(|| self.generate_unique_constraint_name(&columns));
        self.table
            .unique_constraints
            .push(RawUniqueConstraintDefV9 { name, columns });
        self
    }

    /// Generates a [RawIndexDef] using the supplied `columns`.
    pub fn with_index(mut self, algorithm: RawIndexAlgorithm, name: Option<RawIdentifier>) -> Self {
        let name = name.unwrap_or_else(|| self.generate_index_name(&algorithm));

        self.table.indexes.push(RawIndexDefV9 { name, algorithm });
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

    /// Build the table and add it to the module.
    pub fn finish(self) {
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
        format!("{table_name}_{label}_{column_names}").into()
    }

    /// YOU CANNOT RELY ON SEQUENCES HAVING THIS NAME FORMAT.
    fn generate_sequence_name(&self, column: ColId) -> RawIdentifier {
        let column_name = self.column_name(column);
        let table_name = &self.table.name;
        format!("{table_name}_seq_{column_name}").into()
    }

    /// YOU CANNOT RELY ON SCHEDULES HAVING THIS NAME FORMAT.
    fn generate_schedule_name(&self) -> RawIdentifier {
        let table_name = &self.table.name;
        format!("{table_name}_schedule").into()
    }

    /// YOU CANNOT RELY ON UNIQUE CONSTRAINTS HAVING THIS NAME FORMAT.
    fn generate_unique_constraint_name(&self, columns: &ColList) -> RawIdentifier {
        let column_names = self.concat_column_names(columns);
        let table_name = &self.table.name;
        format!("{table_name}_unique_{column_names}").into()
    }
}

impl Drop for RawTableDefBuilder<'_> {
    fn drop(&mut self) {
        self.module_def.tables.push(self.table.clone());
    }
}
