//! ABI Version 10 of the raw module definitions.
//!
//! This is a refactored version of V9 with a section-based structure.
//! V10 moves schedules, lifecycle reducers, and default values out of their V9 locations
//! into dedicated sections for cleaner organization.
//! It allows easier future extensibility to add new kinds of definitions.

use crate::db::raw_def::v9::{Lifecycle, RawIndexAlgorithm, TableAccess, TableType};
use core::fmt;
use spacetimedb_primitives::{ColId, ColList};
use spacetimedb_sats::raw_identifier::RawIdentifier;
use spacetimedb_sats::typespace::TypespaceBuilder;
use spacetimedb_sats::{AlgebraicType, AlgebraicTypeRef, AlgebraicValue, ProductType, SpacetimeType, Typespace};
use std::any::TypeId;
use std::collections::{btree_map, BTreeMap};

/// A possibly-invalid raw module definition.
///
/// ABI Version 10.
///
/// These "raw definitions" may contain invalid data, and are validated by the `validate` module
/// into a proper `spacetimedb_schema::ModuleDef`, or a collection of errors.
///
/// The module definition maintains the same logical global namespace as V9, mapping `Identifier`s to:
///
/// - database-level objects:
///     - logical schema objects: tables, constraints, sequence definitions
///     - physical schema objects: indexes
/// - module-level objects: reducers, procedures, schedule definitions
/// - binding-level objects: type aliases
///
/// All of these types of objects must have unique names within the module.
/// The exception is columns, which need unique names only within a table.
#[derive(Default, Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawModuleDefV10 {
    /// The sections comprising this module definition.
    ///
    /// Sections can appear in any order and are optional.
    pub sections: Vec<RawModuleDefV10Section>,
}

/// A section of a V10 module definition.
///
/// New variants MUST be added to the END of this enum, to maintain ABI compatibility.
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
#[non_exhaustive]
pub enum RawModuleDefV10Section {
    /// The `Typespace` used by the module.
    ///
    /// `AlgebraicTypeRef`s in other sections refer to this typespace.
    /// See [`crate::db::raw_def::v9::RawModuleDefV9::typespace`] for validation requirements.
    Typespace(Typespace),

    /// Type definitions exported by the module.
    Types(Vec<RawTypeDefV10>),

    /// Table definitions.
    Tables(Vec<RawTableDefV10>),

    /// Reducer definitions.
    Reducers(Vec<RawReducerDefV10>),

    /// Procedure definitions.
    Procedures(Vec<RawProcedureDefV10>),

    /// View definitions.
    Views(Vec<RawViewDefV10>),

    /// Schedule definitions.
    ///
    /// Unlike V9 where schedules were embedded in table definitions,
    /// V10 stores them in a dedicated section.
    Schedules(Vec<RawScheduleDefV10>),

    /// Lifecycle reducer assignments.
    ///
    /// Unlike V9 where lifecycle was a field on reducers,
    /// V10 stores lifecycle-to-reducer mappings separately.
    LifeCycleReducers(Vec<RawLifeCycleReducerDefV10>),

    RowLevelSecurity(Vec<RawRowLevelSecurityDefV10>), //TODO: Add section for Event tables, and Case conversion before exposing this from module

    /// Case conversion policy for identifiers in this module.
    CaseConversionPolicy(CaseConversionPolicy),

    /// Names provided explicitly by the user that do not follow from the case conversion policy.
    ExplicitNames(ExplicitNames),
}

#[derive(Debug, Clone, Copy, Default, SpacetimeType)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
#[sats(crate = crate)]
#[non_exhaustive]
pub enum CaseConversionPolicy {
    /// No conversion - names used verbatim as canonical names
    None,
    /// Convert to snake_case (SpacetimeDB default)
    #[default]
    SnakeCase,
    /// Convert to camelCase
    CamelCase,
    /// Convert to PascalCase (UpperCamelCase)
    PascalCase,
}

#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, Ord, PartialOrd))]
#[non_exhaustive]
pub struct NameMapping {
    /// The original name as defined or generated inside module.
    ///
    /// Generated as:
    /// - Tables: value from `#[spacetimedb::table(accessor = ...)]`.
    /// - Reducers/Procedures/Views: function name
    /// - Indexes: `{table_name}_{column_names}_idx_{algorithm}`
    ///
    /// During validation, this may be replaced by `canonical_name`
    /// if an explicit or policy-based name is applied.
    pub source_name: RawIdentifier,

    /// The canonical identifier used in system tables and client code generation.
    ///
    /// Set via:
    /// - `#[spacetimedb::table(name = "...")]` for tables
    /// - `#[spacetimedb::reducer(name = "...")]` for reducers
    /// - `#[name("...")]` for other entities
    ///
    /// If not explicitly provided, this defaults to `source_name`
    /// after validation. No particular format should be assumed.
    pub canonical_name: RawIdentifier,
}

#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, Ord, PartialOrd))]
#[non_exhaustive]
pub enum ExplicitNameEntry {
    Table(NameMapping),
    Function(NameMapping),
    Index(NameMapping),
}

#[derive(Debug, Default, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, Ord, PartialOrd))]
#[non_exhaustive]
pub struct ExplicitNames {
    /// Explicit name mappings defined in the module.
    ///
    /// These override policy-based or auto-generated names
    /// during schema validation.
    entries: Vec<ExplicitNameEntry>,
}

impl ExplicitNames {
    fn insert(&mut self, entry: ExplicitNameEntry) {
        self.entries.push(entry);
    }

    pub fn insert_table(&mut self, source_name: impl Into<RawIdentifier>, canonical_name: impl Into<RawIdentifier>) {
        self.insert(ExplicitNameEntry::Table(NameMapping {
            source_name: source_name.into(),
            canonical_name: canonical_name.into(),
        }));
    }

    pub fn insert_function(&mut self, source_name: impl Into<RawIdentifier>, canonical_name: impl Into<RawIdentifier>) {
        self.insert(ExplicitNameEntry::Function(NameMapping {
            source_name: source_name.into(),
            canonical_name: canonical_name.into(),
        }));
    }

    pub fn insert_index(&mut self, source_name: impl Into<RawIdentifier>, canonical_name: impl Into<RawIdentifier>) {
        self.insert(ExplicitNameEntry::Index(NameMapping {
            source_name: source_name.into(),
            canonical_name: canonical_name.into(),
        }));
    }

    pub fn merge(&mut self, other: ExplicitNames) {
        self.entries.extend(other.entries);
    }

    pub fn into_entries(self) -> Vec<ExplicitNameEntry> {
        self.entries
    }
}

pub type RawRowLevelSecurityDefV10 = crate::db::raw_def::v9::RawRowLevelSecurityDefV9;

/// The definition of a database table.
///
/// This struct holds information about the table, including its name, columns, indexes,
/// constraints, sequences, type, and access rights.
///
/// Validation rules are the same as V9, except:
/// - Default values are stored inline rather than in `MiscModuleExport`
/// - Schedules are stored in a separate section rather than embedded here
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawTableDefV10 {
    /// The name of the table.
    /// Unique within a module, acts as the table's identifier.
    /// Must be a valid `spacetimedb_schema::identifier::Identifier`.
    pub source_name: RawIdentifier,

    /// A reference to a `ProductType` containing the columns of this table.
    /// This is the single source of truth for the table's columns.
    /// All elements of the `ProductType` must have names.
    ///
    /// Like all types in the module, this must have the [default element ordering](crate::db::default_element_ordering),
    /// UNLESS a custom ordering is declared via a `RawTypeDefV10` for this type.
    pub product_type_ref: AlgebraicTypeRef,

    /// The primary key of the table, if present. Must refer to a valid column.
    ///
    /// Currently, there must be a unique constraint and an index corresponding to the primary key.
    /// Eventually, we may remove the requirement for an index.
    ///
    /// The database engine does not actually care about this, but client code generation does.
    ///
    /// A list of length 0 means no primary key. Currently, a list of length >1 is not supported.
    pub primary_key: ColList,

    /// The indices of the table.
    pub indexes: Vec<RawIndexDefV10>,

    /// Any unique constraints on the table.
    pub constraints: Vec<RawConstraintDefV10>,

    /// The sequences for the table.
    pub sequences: Vec<RawSequenceDefV10>,

    /// Whether this is a system- or user-created table.
    pub table_type: TableType,

    /// Whether this table is public or private.
    pub table_access: TableAccess,

    /// Default values for columns in this table.
    pub default_values: Vec<RawColumnDefaultValueV10>,

    /// Whether this is an event table.
    ///
    /// Event tables are write-only: their rows are persisted to the commitlog
    /// but are NOT merged into committed state. They are only visible to V2
    /// subscribers in the transaction that inserted them.
    pub is_event: bool,
}

/// Marks a particular table column as having a particular default value.
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawColumnDefaultValueV10 {
    /// Identifies which column has the default value.
    pub col_id: ColId,

    /// A BSATN-encoded [`AlgebraicValue`] valid at the column's type.
    /// (We cannot use `AlgebraicValue` directly as it isn't `SpacetimeType`.)
    pub value: Box<[u8]>,
}

/// A reducer definition.
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawReducerDefV10 {
    /// The name of the reducer.
    pub source_name: RawIdentifier,

    /// The types and optional names of the parameters, in order.
    /// This `ProductType` need not be registered in the typespace.
    pub params: ProductType,

    /// Whether this reducer is callable from clients or is internal-only.
    pub visibility: FunctionVisibility,

    /// The type of the `Ok` return value.
    pub ok_return_type: AlgebraicType,

    /// The type of the `Err` return value.
    pub err_return_type: AlgebraicType,
}

/// The visibility of a function (reducer or procedure).
#[derive(Debug, Copy, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub enum FunctionVisibility {
    /// Not callable by arbitrary clients.
    ///
    /// Still callable by the module owner, collaborators,
    /// and internal module code.
    ///
    /// Enabled for lifecycle reducers and scheduled functions by default.
    Private,

    /// Callable from client code.
    ClientCallable,
}

/// A schedule definition.
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawScheduleDefV10 {
    /// In the future, the user may FOR SOME REASON want to override this.
    /// Even though there is ABSOLUTELY NO REASON TO.
    /// If `None`, a nicely-formatted unique default will be chosen.
    pub source_name: Option<RawIdentifier>,

    /// The name of the table containing the schedule.
    pub table_name: RawIdentifier,

    /// The column of the `scheduled_at` field in the table.
    pub schedule_at_col: ColId,

    /// The name of the reducer or procedure to call.
    pub function_name: RawIdentifier,
}

/// A lifecycle reducer assignment.
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawLifeCycleReducerDefV10 {
    /// Which lifecycle event this reducer handles.
    pub lifecycle_spec: Lifecycle,

    /// The name of the reducer to call for this lifecycle event.
    pub function_name: RawIdentifier,
}

/// A procedure definition.
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawProcedureDefV10 {
    /// The name of the procedure.
    pub source_name: RawIdentifier,

    /// The types and optional names of the parameters, in order.
    /// This `ProductType` need not be registered in the typespace.
    pub params: ProductType,

    /// The type of the return value.
    ///
    /// If this is a user-defined product or sum type,
    /// it should be registered in the typespace and indirected through an [`AlgebraicType::Ref`].
    pub return_type: AlgebraicType,

    /// Whether this procedure is callable from clients or is internal-only.
    pub visibility: FunctionVisibility,
}

/// A sequence definition for a database table column.
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawSequenceDefV10 {
    /// In the future, the user may FOR SOME REASON want to override this.
    /// Even though there is ABSOLUTELY NO REASON TO.
    /// If `None`, a nicely-formatted unique default will be chosen.
    pub source_name: Option<RawIdentifier>,

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
pub struct RawIndexDefV10 {
    /// In the future, the user may FOR SOME REASON want to override this.
    /// Even though there is ABSOLUTELY NO REASON TO.
    /// TODO: Remove Option, must not be empty.
    pub source_name: Option<RawIdentifier>,

    // not to be used in v10
    pub accessor_name: Option<RawIdentifier>,

    /// The algorithm parameters for the index.
    pub algorithm: RawIndexAlgorithm,
}

/// A constraint definition attached to a table.
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawConstraintDefV10 {
    /// In the future, the user may FOR SOME REASON want to override this.
    /// Even though there is ABSOLUTELY NO REASON TO.
    pub source_name: Option<RawIdentifier>,

    /// The data for the constraint.
    pub data: RawConstraintDataV10,
}

type RawConstraintDataV10 = crate::db::raw_def::v9::RawConstraintDataV9;
type RawUniqueConstraintDataV10 = crate::db::raw_def::v9::RawUniqueConstraintDataV9;

/// A type declaration.
///
/// Exactly of these must be attached to every `Product` and `Sum` type used by a module.
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawTypeDefV10 {
    /// The name of the type declaration.
    pub source_name: RawScopedTypeNameV10,

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
pub struct RawScopedTypeNameV10 {
    /// The scope for this type.
    ///
    /// Empty unless a sats `name` attribute is used, e.g.
    /// `#[sats(name = "namespace.name")]` in Rust.
    pub scope: Box<[RawIdentifier]>,

    /// The name of the type. This must be unique within the module.
    ///
    /// Eventually, we may add more information to this, such as generic arguments.
    pub source_name: RawIdentifier,
}

impl fmt::Debug for RawScopedTypeNameV10 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for module in self.scope.iter() {
            fmt::Debug::fmt(module, f)?;
            f.write_str("::")?;
        }
        fmt::Debug::fmt(&self.source_name, f)?;
        Ok(())
    }
}

/// A view definition.
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawViewDefV10 {
    /// The name of the view function as defined in the module
    pub source_name: RawIdentifier,

    /// The index of the view in the module's list of views.
    pub index: u32,

    /// Is this a public or a private view?
    /// Currently only public views are supported.
    /// Private views may be supported in the future.
    pub is_public: bool,

    /// Is this view anonymous?
    /// An anonymous view does not know who called it.
    /// Specifically, it is a view that has an `AnonymousViewContext` as its first argument.
    /// This type does not have access to the `Identity` of the caller.
    pub is_anonymous: bool,

    /// The types and optional names of the parameters, in order.
    /// This `ProductType` need not be registered in the typespace.
    pub params: ProductType,

    /// The return type of the view.
    /// Either `T`, `Option<T>`, or `Vec<T>` where `T` is a `SpacetimeType`.
    ///
    /// More strictly `T` must be a SATS `ProductType`,
    /// however this will be validated by the server on publish.
    ///
    /// This is the single source of truth for the views's columns.
    /// All elements of the inner `ProductType` must have names.
    /// This again will be validated by the server on publish.
    pub return_type: AlgebraicType,
}

impl RawModuleDefV10 {
    /// Get the types section, if present.
    pub fn types(&self) -> Option<&Vec<RawTypeDefV10>> {
        self.sections.iter().find_map(|s| match s {
            RawModuleDefV10Section::Types(types) => Some(types),
            _ => None,
        })
    }

    /// Get the tables section, if present.
    pub fn tables(&self) -> Option<&Vec<RawTableDefV10>> {
        self.sections.iter().find_map(|s| match s {
            RawModuleDefV10Section::Tables(tables) => Some(tables),
            _ => None,
        })
    }

    /// Get the typespace section, if present.
    pub fn typespace(&self) -> Option<&Typespace> {
        self.sections.iter().find_map(|s| match s {
            RawModuleDefV10Section::Typespace(ts) => Some(ts),
            _ => None,
        })
    }

    /// Get the reducers section, if present.
    pub fn reducers(&self) -> Option<&Vec<RawReducerDefV10>> {
        self.sections.iter().find_map(|s| match s {
            RawModuleDefV10Section::Reducers(reducers) => Some(reducers),
            _ => None,
        })
    }

    /// Get the procedures section, if present.
    pub fn procedures(&self) -> Option<&Vec<RawProcedureDefV10>> {
        self.sections.iter().find_map(|s| match s {
            RawModuleDefV10Section::Procedures(procedures) => Some(procedures),
            _ => None,
        })
    }

    /// Get the views section, if present.
    pub fn views(&self) -> Option<&Vec<RawViewDefV10>> {
        self.sections.iter().find_map(|s| match s {
            RawModuleDefV10Section::Views(views) => Some(views),
            _ => None,
        })
    }

    /// Get the schedules section, if present.
    pub fn schedules(&self) -> Option<&Vec<RawScheduleDefV10>> {
        self.sections.iter().find_map(|s| match s {
            RawModuleDefV10Section::Schedules(schedules) => Some(schedules),
            _ => None,
        })
    }

    /// Get the lifecycle reducers section, if present.
    pub fn lifecycle_reducers(&self) -> Option<&Vec<RawLifeCycleReducerDefV10>> {
        self.sections.iter().find_map(|s| match s {
            RawModuleDefV10Section::LifeCycleReducers(lcrs) => Some(lcrs),
            _ => None,
        })
    }

    pub fn tables_mut_for_tests(&mut self) -> &mut Vec<RawTableDefV10> {
        self.sections
            .iter_mut()
            .find_map(|s| match s {
                RawModuleDefV10Section::Tables(tables) => Some(tables),
                _ => None,
            })
            .expect("Tables section must exist for tests")
    }

    // Get the row-level security section, if present.
    pub fn row_level_security(&self) -> Option<&Vec<RawRowLevelSecurityDefV10>> {
        self.sections.iter().find_map(|s| match s {
            RawModuleDefV10Section::RowLevelSecurity(rls) => Some(rls),
            _ => None,
        })
    }

    pub fn case_conversion_policy(&self) -> CaseConversionPolicy {
        self.sections
            .iter()
            .find_map(|s| match s {
                RawModuleDefV10Section::CaseConversionPolicy(policy) => Some(*policy),
                _ => None,
            })
            .unwrap_or_default()
    }

    pub fn explicit_names(&self) -> Option<&ExplicitNames> {
        self.sections.iter().find_map(|s| match s {
            RawModuleDefV10Section::ExplicitNames(names) => Some(names),
            _ => None,
        })
    }
}

/// A builder for a [`RawModuleDefV10`].
#[derive(Default)]
pub struct RawModuleDefV10Builder {
    /// The module definition being built.
    module: RawModuleDefV10,

    /// The type map from `T: 'static` Rust types to sats types.
    type_map: BTreeMap<TypeId, AlgebraicTypeRef>,
}

impl RawModuleDefV10Builder {
    /// Create a new, empty `RawModuleDefV10Builder`.
    pub fn new() -> Self {
        Default::default()
    }

    /// Get mutable access to the typespace section, creating it if missing.
    fn typespace_mut(&mut self) -> &mut Typespace {
        let idx = self
            .module
            .sections
            .iter()
            .position(|s| matches!(s, RawModuleDefV10Section::Typespace(_)))
            .unwrap_or_else(|| {
                self.module
                    .sections
                    .push(RawModuleDefV10Section::Typespace(Typespace::EMPTY.clone()));
                self.module.sections.len() - 1
            });

        match &mut self.module.sections[idx] {
            RawModuleDefV10Section::Typespace(ts) => ts,
            _ => unreachable!("Just ensured Typespace section exists"),
        }
    }

    /// Get mutable access to the reducers section, creating it if missing.
    fn reducers_mut(&mut self) -> &mut Vec<RawReducerDefV10> {
        let idx = self
            .module
            .sections
            .iter()
            .position(|s| matches!(s, RawModuleDefV10Section::Reducers(_)))
            .unwrap_or_else(|| {
                self.module.sections.push(RawModuleDefV10Section::Reducers(Vec::new()));
                self.module.sections.len() - 1
            });

        match &mut self.module.sections[idx] {
            RawModuleDefV10Section::Reducers(reducers) => reducers,
            _ => unreachable!("Just ensured Reducers section exists"),
        }
    }

    /// Get mutable access to the procedures section, creating it if missing.
    fn procedures_mut(&mut self) -> &mut Vec<RawProcedureDefV10> {
        let idx = self
            .module
            .sections
            .iter()
            .position(|s| matches!(s, RawModuleDefV10Section::Procedures(_)))
            .unwrap_or_else(|| {
                self.module
                    .sections
                    .push(RawModuleDefV10Section::Procedures(Vec::new()));
                self.module.sections.len() - 1
            });

        match &mut self.module.sections[idx] {
            RawModuleDefV10Section::Procedures(procedures) => procedures,
            _ => unreachable!("Just ensured Procedures section exists"),
        }
    }

    /// Get mutable access to the views section, creating it if missing.
    fn views_mut(&mut self) -> &mut Vec<RawViewDefV10> {
        let idx = self
            .module
            .sections
            .iter()
            .position(|s| matches!(s, RawModuleDefV10Section::Views(_)))
            .unwrap_or_else(|| {
                self.module.sections.push(RawModuleDefV10Section::Views(Vec::new()));
                self.module.sections.len() - 1
            });

        match &mut self.module.sections[idx] {
            RawModuleDefV10Section::Views(views) => views,
            _ => unreachable!("Just ensured Views section exists"),
        }
    }

    /// Get mutable access to the schedules section, creating it if missing.
    fn schedules_mut(&mut self) -> &mut Vec<RawScheduleDefV10> {
        let idx = self
            .module
            .sections
            .iter()
            .position(|s| matches!(s, RawModuleDefV10Section::Schedules(_)))
            .unwrap_or_else(|| {
                self.module.sections.push(RawModuleDefV10Section::Schedules(Vec::new()));
                self.module.sections.len() - 1
            });

        match &mut self.module.sections[idx] {
            RawModuleDefV10Section::Schedules(schedules) => schedules,
            _ => unreachable!("Just ensured Schedules section exists"),
        }
    }

    /// Get mutable access to the lifecycle reducers section, creating it if missing.
    fn lifecycle_reducers_mut(&mut self) -> &mut Vec<RawLifeCycleReducerDefV10> {
        let idx = self
            .module
            .sections
            .iter()
            .position(|s| matches!(s, RawModuleDefV10Section::LifeCycleReducers(_)))
            .unwrap_or_else(|| {
                self.module
                    .sections
                    .push(RawModuleDefV10Section::LifeCycleReducers(Vec::new()));
                self.module.sections.len() - 1
            });

        match &mut self.module.sections[idx] {
            RawModuleDefV10Section::LifeCycleReducers(lcrs) => lcrs,
            _ => unreachable!("Just ensured LifeCycleReducers section exists"),
        }
    }

    /// Get mutable access to the types section, creating it if missing.
    fn types_mut(&mut self) -> &mut Vec<RawTypeDefV10> {
        let idx = self
            .module
            .sections
            .iter()
            .position(|s| matches!(s, RawModuleDefV10Section::Types(_)))
            .unwrap_or_else(|| {
                self.module.sections.push(RawModuleDefV10Section::Types(Vec::new()));
                self.module.sections.len() - 1
            });

        match &mut self.module.sections[idx] {
            RawModuleDefV10Section::Types(types) => types,
            _ => unreachable!("Just ensured Types section exists"),
        }
    }

    /// Add a type to the in-progress module.
    ///
    /// The returned type must satisfy `AlgebraicType::is_valid_for_client_type_definition` or `AlgebraicType::is_valid_for_client_type_use`.
    pub fn add_type<T: SpacetimeType>(&mut self) -> AlgebraicType {
        TypespaceBuilder::add_type::<T>(self)
    }

    /// Get mutable access to the row-level security section, creating it if missing.
    fn row_level_security_mut(&mut self) -> &mut Vec<RawRowLevelSecurityDefV10> {
        let idx = self
            .module
            .sections
            .iter()
            .position(|s| matches!(s, RawModuleDefV10Section::RowLevelSecurity(_)))
            .unwrap_or_else(|| {
                self.module
                    .sections
                    .push(RawModuleDefV10Section::RowLevelSecurity(Vec::new()));
                self.module.sections.len() - 1
            });

        match &mut self.module.sections[idx] {
            RawModuleDefV10Section::RowLevelSecurity(rls) => rls,
            _ => unreachable!("Just ensured RowLevelSecurity section exists"),
        }
    }

    /// Get mutable access to the case conversion policy, creating it if missing.
    fn explicit_names_mut(&mut self) -> &mut ExplicitNames {
        let idx = self
            .module
            .sections
            .iter()
            .position(|s| matches!(s, RawModuleDefV10Section::ExplicitNames(_)))
            .unwrap_or_else(|| {
                self.module
                    .sections
                    .push(RawModuleDefV10Section::ExplicitNames(ExplicitNames::default()));
                self.module.sections.len() - 1
            });

        match &mut self.module.sections[idx] {
            RawModuleDefV10Section::ExplicitNames(names) => names,
            _ => unreachable!("Just ensured ExplicitNames section exists"),
        }
    }

    /// Create a table builder.
    ///
    /// Does not validate that the product_type_ref is valid; this is left to the module validation code.
    pub fn build_table(
        &mut self,
        source_name: impl Into<RawIdentifier>,
        product_type_ref: AlgebraicTypeRef,
    ) -> RawTableDefBuilderV10<'_> {
        let source_name = source_name.into();
        RawTableDefBuilderV10 {
            module: &mut self.module,
            table: RawTableDefV10 {
                source_name,
                product_type_ref,
                indexes: vec![],
                constraints: vec![],
                sequences: vec![],
                primary_key: ColList::empty(),
                table_type: TableType::User,
                table_access: TableAccess::Public,
                default_values: vec![],
                is_event: false,
            },
        }
    }

    /// Build a new table with a product type.
    /// Adds the type to the module.
    pub fn build_table_with_new_type(
        &mut self,
        table_name: impl Into<RawIdentifier>,
        product_type: impl Into<ProductType>,
        custom_ordering: bool,
    ) -> RawTableDefBuilderV10<'_> {
        let table_name = table_name.into();

        let product_type_ref = self.add_algebraic_type(
            [],
            table_name.clone(),
            AlgebraicType::from(product_type.into()),
            custom_ordering,
        );

        self.build_table(table_name, product_type_ref)
    }

    /// Build a new table with a product type, for testing.
    /// Adds the type to the module.
    pub fn build_table_with_new_type_for_tests(
        &mut self,
        table_name: impl Into<RawIdentifier>,
        mut product_type: ProductType,
        custom_ordering: bool,
    ) -> RawTableDefBuilderV10<'_> {
        self.add_expand_product_type_for_tests(&mut 0, &mut product_type);
        self.build_table_with_new_type(table_name, product_type, custom_ordering)
    }

    fn add_expand_type_for_tests(&mut self, name_gen: &mut usize, ty: &mut AlgebraicType) {
        if ty.is_valid_for_client_type_use() {
            return;
        }

        match ty {
            AlgebraicType::Product(prod_ty) => self.add_expand_product_type_for_tests(name_gen, prod_ty),
            AlgebraicType::Sum(sum_type) => {
                if let Some(wrapped) = sum_type.as_option_mut() {
                    self.add_expand_type_for_tests(name_gen, wrapped);
                } else {
                    for elem in sum_type.variants.iter_mut() {
                        self.add_expand_type_for_tests(name_gen, &mut elem.algebraic_type);
                    }
                }
            }
            AlgebraicType::Array(ty) => {
                self.add_expand_type_for_tests(name_gen, &mut ty.elem_ty);
                return;
            }
            _ => return,
        }

        // Make the type into a ref.
        let name = *name_gen;
        let add_ty = core::mem::replace(ty, AlgebraicType::U8);
        *ty = AlgebraicType::Ref(self.add_algebraic_type([], RawIdentifier::new(format!("gen_{name}")), add_ty, true));
        *name_gen += 1;
    }

    fn add_expand_product_type_for_tests(&mut self, name_gen: &mut usize, ty: &mut ProductType) {
        for elem in ty.elements.iter_mut() {
            self.add_expand_type_for_tests(name_gen, &mut elem.algebraic_type);
        }
    }

    /// Add a type to the typespace, along with a type alias declaring its name.
    /// This method should only be used for `AlgebraicType`s not corresponding to a Rust
    /// type that implements `SpacetimeType`.
    ///
    /// Returns a reference to the newly-added type.
    ///
    /// NOT idempotent, calling this twice with the same name will cause errors during validation.
    ///
    /// You must set `custom_ordering` if you're not using the default element ordering.
    pub fn add_algebraic_type(
        &mut self,
        scope: impl IntoIterator<Item = RawIdentifier>,
        source_name: impl Into<RawIdentifier>,
        ty: AlgebraicType,
        custom_ordering: bool,
    ) -> AlgebraicTypeRef {
        let ty_ref = self.typespace_mut().add(ty);
        let scope = scope.into_iter().collect();
        let source_name = source_name.into();
        self.types_mut().push(RawTypeDefV10 {
            source_name: RawScopedTypeNameV10 { source_name, scope },
            ty: ty_ref,
            custom_ordering,
        });
        // We don't add a `TypeId` to `self.type_map`, because there may not be a corresponding Rust type!
        // e.g. if we are randomly generating types in proptests.
        ty_ref
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
    pub fn add_reducer(&mut self, source_name: impl Into<RawIdentifier>, params: ProductType) {
        self.reducers_mut().push(RawReducerDefV10 {
            source_name: source_name.into(),
            params,
            visibility: FunctionVisibility::ClientCallable,
            ok_return_type: reducer_default_ok_return_type(),
            err_return_type: reducer_default_err_return_type(),
        });
    }

    /// Add a procedure to the in-progress module.
    ///
    /// Accepts a `ProductType` of arguments.
    /// The arguments `ProductType` need not be registered in the typespace.
    ///
    /// Also accepts an `AlgebraicType` return type.
    /// If this is a user-defined product or sum type,
    /// it should be registered in the typespace and indirected through an `AlgebraicType::Ref`.
    ///
    /// The `&mut ProcedureContext` first argument to the procedure should not be included in the `params`.
    pub fn add_procedure(
        &mut self,
        source_name: impl Into<RawIdentifier>,
        params: ProductType,
        return_type: AlgebraicType,
    ) {
        self.procedures_mut().push(RawProcedureDefV10 {
            source_name: source_name.into(),
            params,
            return_type,
            visibility: FunctionVisibility::ClientCallable,
        })
    }

    /// Add a view to the in-progress module.
    pub fn add_view(
        &mut self,
        source_name: impl Into<RawIdentifier>,
        index: usize,
        is_public: bool,
        is_anonymous: bool,
        params: ProductType,
        return_type: AlgebraicType,
    ) {
        self.views_mut().push(RawViewDefV10 {
            source_name: source_name.into(),
            index: index as u32,
            is_public,
            is_anonymous,
            params,
            return_type,
        });
    }

    /// Add a lifecycle reducer assignment to the module.
    ///
    /// The function must be a previously-added reducer.
    pub fn add_lifecycle_reducer(
        &mut self,
        lifecycle_spec: Lifecycle,
        function_name: impl Into<RawIdentifier>,
        params: ProductType,
    ) {
        let function_name = function_name.into();
        self.lifecycle_reducers_mut().push(RawLifeCycleReducerDefV10 {
            lifecycle_spec,
            function_name: function_name.clone(),
        });

        self.reducers_mut().push(RawReducerDefV10 {
            source_name: function_name,
            params,
            visibility: FunctionVisibility::Private,
            ok_return_type: reducer_default_ok_return_type(),
            err_return_type: reducer_default_err_return_type(),
        });
    }

    /// Add a schedule definition to the module.
    ///
    /// The `function_name` should name a reducer or procedure
    /// which accepts one argument, a row of the specified table.
    ///
    /// The table must have the appropriate columns for a scheduled table.
    pub fn add_schedule(
        &mut self,
        table: impl Into<RawIdentifier>,
        column: impl Into<ColId>,
        function: impl Into<RawIdentifier>,
    ) {
        self.schedules_mut().push(RawScheduleDefV10 {
            source_name: None,
            table_name: table.into(),
            schedule_at_col: column.into(),
            function_name: function.into(),
        });
    }

    /// Add a row-level security policy to the module.
    ///
    /// The `sql` expression should be a valid SQL expression that will be used to filter rows.
    ///
    /// **NOTE**: The `sql` expression must be unique within the module.
    pub fn add_row_level_security(&mut self, sql: &str) {
        self.row_level_security_mut()
            .push(RawRowLevelSecurityDefV10 { sql: sql.into() });
    }

    pub fn add_explicit_names(&mut self, names: ExplicitNames) {
        self.explicit_names_mut().merge(names);
    }

    /// Finish building, consuming the builder and returning the module.
    /// The module should be validated before use.
    pub fn finish(self) -> RawModuleDefV10 {
        self.module
    }
}

/// Implement TypespaceBuilder for V10
impl TypespaceBuilder for RawModuleDefV10Builder {
    fn add(
        &mut self,
        typeid: TypeId,
        source_name: Option<&'static str>,
        make_ty: impl FnOnce(&mut Self) -> AlgebraicType,
    ) -> AlgebraicType {
        if let btree_map::Entry::Occupied(o) = self.type_map.entry(typeid) {
            AlgebraicType::Ref(*o.get())
        } else {
            let slot_ref = {
                let ts = self.typespace_mut();
                // Bind a fresh alias to the unit type.
                let slot_ref = ts.add(AlgebraicType::unit());
                // Relate `typeid -> fresh alias`.
                self.type_map.insert(typeid, slot_ref);

                // Alias provided? Relate `name -> slot_ref`.
                if let Some(sats_name) = source_name {
                    let source_name = sats_name_to_scoped_name_v10(sats_name);

                    self.types_mut().push(RawTypeDefV10 {
                        source_name,
                        ty: slot_ref,
                        // TODO(1.0): we need to update the `TypespaceBuilder` trait to include
                        // a `custom_ordering` parameter.
                        // For now, we assume all types have custom orderings, since the derive
                        // macro doesn't know about the default ordering yet.
                        custom_ordering: true,
                    });
                }
                slot_ref
            };

            // Borrow of `v` has ended here, so we can now convince the borrow checker.
            let ty = make_ty(self);
            self.typespace_mut()[slot_ref] = ty;
            AlgebraicType::Ref(slot_ref)
        }
    }
}

pub fn reducer_default_ok_return_type() -> AlgebraicType {
    AlgebraicType::unit()
}

pub fn reducer_default_err_return_type() -> AlgebraicType {
    AlgebraicType::String
}

/// Convert a string from a sats type-name annotation like `#[sats(name = "namespace.name")]` to a `RawScopedTypeNameV9`.
/// We split the input on the strings `"::"` and `"."` to split up module paths.
///
pub fn sats_name_to_scoped_name_v10(sats_name: &str) -> RawScopedTypeNameV10 {
    // We can't use `&[char]: Pattern` for `split` here because "::" is not a char :/
    let mut scope: Vec<RawIdentifier> = sats_name
        .split("::")
        .flat_map(|s| s.split('.'))
        .map(RawIdentifier::new)
        .collect();
    // Unwrapping to "" will result in a validation error down the line, which is exactly what we want.
    let source_name = scope.pop().unwrap_or_default();
    RawScopedTypeNameV10 {
        scope: scope.into(),
        source_name,
    }
}

/// Builder for a `RawTableDefV10`.
pub struct RawTableDefBuilderV10<'a> {
    module: &'a mut RawModuleDefV10,
    table: RawTableDefV10,
}

impl RawTableDefBuilderV10<'_> {
    /// Set the table type.
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

    /// Sets whether this table is an event table.
    pub fn with_event(mut self, is_event: bool) -> Self {
        self.table.is_event = is_event;
        self
    }

    /// Generates a `RawConstraintDefV10` using the supplied `columns`.
    pub fn with_unique_constraint(mut self, columns: impl Into<ColList>) -> Self {
        let columns = columns.into();
        self.table.constraints.push(RawConstraintDefV10 {
            source_name: None,
            data: RawConstraintDataV10::Unique(RawUniqueConstraintDataV10 { columns }),
        });

        self
    }

    /// Adds a primary key to the table.
    /// You must also add a unique constraint on the primary key column.
    pub fn with_primary_key(mut self, column: impl Into<ColId>) -> Self {
        self.table.primary_key = ColList::new(column.into());
        self
    }

    /// Adds a primary key to the table, with corresponding unique constraint and sequence definitions.
    /// You will also need to call [`Self::with_index`] to create an index on `column`.
    pub fn with_auto_inc_primary_key(self, column: impl Into<ColId>) -> Self {
        let column = column.into();
        self.with_primary_key(column)
            .with_unique_constraint(column)
            .with_column_sequence(column)
    }

    /// Generates a [RawIndexDefV10] using the supplied `columns`.
    pub fn with_index(mut self, algorithm: RawIndexAlgorithm, source_name: impl Into<RawIdentifier>) -> Self {
        self.table.indexes.push(RawIndexDefV10 {
            source_name: Some(source_name.into()),
            accessor_name: None,
            algorithm,
        });
        self
    }

    /// Generates a [RawIndexDefV10] using the supplied `columns` but with no `accessor_name`.
    pub fn with_index_no_accessor_name(mut self, algorithm: RawIndexAlgorithm) -> Self {
        self.table.indexes.push(RawIndexDefV10 {
            source_name: None,
            accessor_name: None,
            algorithm,
        });
        self
    }

    /// Adds a [RawSequenceDefV10] on the supplied `column`.
    pub fn with_column_sequence(mut self, column: impl Into<ColId>) -> Self {
        let column = column.into();
        self.table.sequences.push(RawSequenceDefV10 {
            source_name: None,
            column,
            start: None,
            min_value: None,
            max_value: None,
            increment: 1,
        });

        self
    }

    /// Adds a default value for a column.
    pub fn with_default_column_value(mut self, column: impl Into<ColId>, value: AlgebraicValue) -> Self {
        let col_id = column.into();
        self.table.default_values.push(RawColumnDefaultValueV10 {
            col_id,
            value: spacetimedb_sats::bsatn::to_vec(&value).unwrap().into(),
        });

        self
    }

    /// Build the table and add it to the module, returning the `product_type_ref` of the table.
    pub fn finish(self) -> AlgebraicTypeRef {
        let product_type_ref = self.table.product_type_ref;

        let tables = match self
            .module
            .sections
            .iter_mut()
            .find(|s| matches!(s, RawModuleDefV10Section::Tables(_)))
        {
            Some(RawModuleDefV10Section::Tables(t)) => t,
            _ => {
                self.module.sections.push(RawModuleDefV10Section::Tables(Vec::new()));
                match self.module.sections.last_mut().expect("Just pushed Tables section") {
                    RawModuleDefV10Section::Tables(t) => t,
                    _ => unreachable!(),
                }
            }
        };

        tables.push(self.table);
        product_type_ref
    }

    /// Find a column position by its name in the table's product type.
    pub fn find_col_pos_by_name(&self, column: impl AsRef<str>) -> Option<ColId> {
        let column = column.as_ref();

        let typespace = self.module.sections.iter().find_map(|s| {
            if let RawModuleDefV10Section::Typespace(ts) = s {
                Some(ts)
            } else {
                None
            }
        })?;

        typespace
            .get(self.table.product_type_ref)?
            .as_product()?
            .elements
            .iter()
            .position(|x| x.has_name(column.as_ref()))
            .map(|i| ColId(i as u16))
    }
}
