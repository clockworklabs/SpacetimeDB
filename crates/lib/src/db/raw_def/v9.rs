use crate::db::auth::{StAccess, StTableType};
use itertools::Itertools;
use spacetimedb_primitives::*;
use spacetimedb_sats::AlgebraicType;
use spacetimedb_sats::AlgebraicTypeRef;
use spacetimedb_sats::ProductTypeElement;
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
    /// The types used in the module.
    ///
    /// `AlgebraicTypeRef`s in the table, reducer, and type alias declarations refer to this typespace.
    ///
    /// Any `Product` or `Sum` types used transitively by the module MUST be declared in this typespace.
    ///
    /// Every `Product`, `Sum`, and `Ref` type in this typespace MUST have a corresponding `TypeAlias` declaration in the `misc_exports` field, with a module-unique name.
    ///
    /// All product and sum types in this typespace MUST have the [default element ordering](crate::db::default_element_ordering) UNLESS they declare a custom ordering.
    /// Custom orderings are declared in `misc_exports`.
    ///
    /// It is permitted but not required to refer to `Builtin` or "primitive" types via this typespace.
    ///
    /// The typespace must satisfy `Typespace::is_nominal`. That is, it is not permitted to refer to `Sum` or `Product` types in this typespace except via `AlgebraicType::Ref`.
    pub typespace: Typespace,

    /// The tables of the database definition used in the module.
    ///
    /// Each table must have a unique name.
    pub tables: Vec<RawTableDefV9>,

    /// The reducers used by the module.
    pub reducers: Vec<RawReducerDefV9>,

    /// Miscellaneous additional module exports.
    pub misc_exports: Vec<RawMiscModuleExportV9>,
}

impl RawModuleDefV9 {
    /// Creates a new, empty [RawDatabaseDef] instance with no types in its typespace.
    pub fn new() -> Self {
        Default::default()
    }

    /// Build a new table.
    ///
    /// Does not validate that the product_type_ref is valid; this is left to the module validation code.
    pub fn build_table(&mut self, name: RawIdentifier, product_type_ref: AlgebraicTypeRef) -> RawTableDefBuilder {
        RawTableDefBuilder {
            module_def: self,
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
    /// This is a convenience method for tests, since in real modules, the product type is initialized automatically by `ModuleBuilder`.
    #[cfg(feature = "test")]
    pub fn build_table_with_product_type(
        &mut self,
        table_name: RawIdentifier,
        product_type: spacetimedb_sats::ProductType,
        custom_ordering: bool,
    ) -> RawTableDefBuilder {
        let product_type_ref = self.add_product_for_tests(table_name.clone(), product_type, custom_ordering);

        self.build_table(table_name, product_type_ref)
    }

    /// Add a product type to the typespace, along with a type alias declaring its name.
    /// This is a convenience method for tests, since the actual module code uses ModuleBuilder.
    ///
    /// NOT idempotent, calling this twice with the same name will cause errors during
    /// validation.
    ///
    /// `custom_ordering` must be set correctly, otherwise an error will result during validation.
    ///
    /// Returns an AlgebraicType::Ref.
    #[cfg(feature = "test")]
    pub fn add_product_for_tests(
        &mut self,
        name: impl Into<RawIdentifier>,
        product_type: impl Into<spacetimedb_sats::ProductType>,
        custom_ordering: bool,
    ) -> AlgebraicTypeRef {
        let ref_ = self.typespace.add(product_type.into().into());
        self.misc_exports.push(RawMiscModuleExportV9::TypeAlias(RawTypeAliasV9 {
            name: name.into(),
            ty: ref_,
        }));
        if custom_ordering {
            self.misc_exports
                .push(RawMiscModuleExportV9::CustomTypeOrdering(RawCustomTypeOrderingV9 {
                    ty: ref_,
                }));
        }
        ref_
    }

    /// Add a product type to the typespace, along with a type alias declaring its name.
    ///
    /// NOT idempotent, calling this twice with the same name will cause errors during
    /// validation.
    ///
    /// Returns an AlgebraicType::Ref.
    #[cfg(feature = "test")]
    pub fn add_sum_for_tests(
        &mut self,
        name: impl Into<RawIdentifier>,
        sum_type: impl Into<spacetimedb_sats::SumType>,
    ) -> AlgebraicTypeRef {
        let ref_ = self.typespace.add(sum_type.into().into());
        self.misc_exports.push(RawMiscModuleExportV9::TypeAlias(RawTypeAliasV9 {
            name: name.into(),
            ty: ref_,
        }));
        ref_
    }
}

/// The definition of a database table.
///
/// This struct holds information about the table, including its name, columns, indexes,
/// constraints, sequences, type, and access rights.
///
/// Validation rules:
/// - The table name must be a valid [crate::db::identifier::Identifier].
/// - The table's columns MUST be sorted according to [crate::db::ordering::canonical_ordering].
///   This is a sanity check to ensure that modules know the correct ordering to use for their tables.
///     - TODO(jgilles): add a test-only validation method that allows tables with unusual
///       column orderings. This will enable more test coverage in the `table` crate,
///       in case we allow more orderings in the future.
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
    /// A reference to a product type containing the columns of this table.
    /// This is the single source of truth for the table's columns.
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
#[derive(Debug, Clone, ser::Serialize, de::Deserialize)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawScheduleDefV9 {
    /// The name of the schedule. Must be unique within the containing `RawDatabaseDef`.
    pub name: RawIdentifier,

    /// The column that stores the desired invocation time.
    pub at_column: ColId,

    /// The name of the reducer to call.
    pub reducer_name: RawIdentifier,
}

/// A miscellaneous module export.
#[derive(Debug, Clone, ser::Serialize, de::Deserialize)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
#[non_exhaustive]
pub enum RawMiscModuleExportV9 {
    /// A type alias, declaring a name for an `AlgebraicTypeRef`.
    TypeAlias(RawTypeAliasV9),
    /// Annotates a type as possessing a custom ordering.
    /// If this is not present, the type is required to have the [default ordering](crate::db::default_element_ordering).
    /// Only `ProductType`s are allowed to have custom orderings.
    CustomTypeOrdering(RawCustomTypeOrderingV9),
}

/// A type alias.
///
/// Exactly of these must be attached to every `Product` and `Sum` type used by a module.
#[derive(Debug, Clone, de::Deserialize, ser::Serialize)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawTypeAliasV9 {
    /// The name of the type. This must be unique within the module.
    ///
    /// Eventually, we may add more information to this, such as the module name and generic arguments.
    pub name: RawIdentifier,

    /// The type to which the alias refers.
    pub ty: AlgebraicTypeRef,
}

/// Marks a type as possessing a custom ordering.
///
/// Types not marked with this are required to have the default ordering.
#[derive(Debug, Clone, de::Deserialize, ser::Serialize)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawCustomTypeOrderingV9 {
    pub ty: AlgebraicTypeRef,
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
    pub fn with_column_sequence(mut self, column: ColId, name: Option<RawIdentifier>) -> Self {
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
    /// The `at` column must be (TODO).
    pub fn with_schedule(mut self, at_column: ColId, reducer_name: RawIdentifier, name: Option<RawIdentifier>) -> Self {
        let name = name.unwrap_or_else(|| self.generate_schedule_name());
        self.table.schedule = Some(RawScheduleDefV9 {
            name,
            at_column,
            reducer_name,
        });
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
