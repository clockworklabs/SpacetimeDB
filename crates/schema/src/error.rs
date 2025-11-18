use spacetimedb_data_structures::error_stream::ErrorStream;
use spacetimedb_lib::db::raw_def::v9::{Lifecycle, RawIdentifier, RawScopedTypeNameV9};
use spacetimedb_lib::{ProductType, SumType};
use spacetimedb_primitives::{ColId, ColList, ColSet};
use spacetimedb_sats::algebraic_type::fmt::fmt_algebraic_type;
use spacetimedb_sats::{bsatn::DecodeError, AlgebraicType, AlgebraicTypeRef};
use std::borrow::Cow;
use std::fmt;

use crate::def::{FunctionKind, ScopedTypeName};
use crate::identifier::Identifier;
use crate::type_for_generate::ClientCodegenError;

/// A stream of validation errors, defined using the `ErrorStream` type.
pub type ValidationErrors = ErrorStream<ValidationError>;

/// A single validation error.
///
/// Many variants of this enum store `RawIdentifier`s rather than `Identifier`s.
/// This is because we want to support reporting errors about module entities with invalid names.
#[derive(thiserror::Error, Debug, PartialOrd, Ord, PartialEq, Eq)]
#[non_exhaustive]
pub enum ValidationError {
    #[error("name `{name}` is used for multiple entities")]
    DuplicateName { name: Box<str> },
    #[error("name `{name}` is used for multiple types")]
    DuplicateTypeName { name: ScopedTypeName },
    #[error("Multiple reducers defined for lifecycle event {lifecycle:?}")]
    DuplicateLifecycle { lifecycle: Lifecycle },
    #[error("module contains invalid identifier: {error}")]
    IdentifierError { error: IdentifierError },
    #[error("table `{}` has unnamed column `{}`, which is forbidden.", column.table, column.column)]
    UnnamedColumn { column: RawColumnName },
    #[error("type `{type_name:?}` is not annotated with a custom ordering, but uses one: {bad_type}")]
    TypeHasIncorrectOrdering {
        type_name: RawScopedTypeNameV9,
        ref_: AlgebraicTypeRef,
        /// Could be a sum or product.
        bad_type: PrettyAlgebraicType,
    },
    #[error("column `{column}` referenced by def {def} not found in table `{table}`")]
    ColumnNotFound {
        table: RawIdentifier,
        def: RawIdentifier,
        column: ColId,
    },
    #[error(
        "{column} type {ty} does not match corresponding element at position {pos} in product type `{product_type}`"
    )]
    ColumnDefMalformed {
        column: RawColumnName,
        ty: PrettyAlgebraicType,
        pos: ColId,
        product_type: PrettyAlgebraicType,
    },
    #[error("table `{table}` has multiple primary key annotations")]
    RepeatedPrimaryKey { table: RawIdentifier },
    #[error("Attempt to define {column} with more than 1 auto_inc sequence")]
    OneAutoInc { column: RawColumnName },
    #[error("Hash indexes are not supported: `{index}` is a hash index")]
    HashIndexUnsupported { index: RawIdentifier },
    #[error("No index found to support unique constraint `{constraint}` for columns `{columns:?}`")]
    UniqueConstraintWithoutIndex { constraint: Box<str>, columns: ColSet },
    #[error("Direct index does not support type `{ty}` in column `{column}` in index `{index}`")]
    DirectIndexOnBadType {
        index: RawIdentifier,
        column: RawIdentifier,
        ty: PrettyAlgebraicType,
    },
    #[error("def `{def}` has duplicate columns: {columns:?}")]
    DuplicateColumns { def: RawIdentifier, columns: ColList },
    #[error("invalid sequence column type: {column} with type `{column_type:?}` in sequence `{sequence}`")]
    InvalidSequenceColumnType {
        sequence: RawIdentifier,
        column: RawColumnName,
        column_type: PrettyAlgebraicType,
    },
    #[error("invalid sequence range information: expected {min_value:?} <= {start:?} <= {max_value:?} in sequence `{sequence}`")]
    InvalidSequenceRange {
        sequence: RawIdentifier,
        min_value: Option<i128>,
        start: Option<i128>,
        max_value: Option<i128>,
    },
    #[error("View {view} has invalid return type {ty}")]
    InvalidViewReturnType {
        view: RawIdentifier,
        ty: PrettyAlgebraicType,
    },
    #[error("Table {table} has invalid product_type_ref {ref_}")]
    InvalidProductTypeRef {
        table: RawIdentifier,
        ref_: AlgebraicTypeRef,
    },
    #[error("Type {type_name:?} has invalid ref: {ref_}")]
    InvalidTypeRef {
        type_name: RawScopedTypeNameV9,
        ref_: AlgebraicTypeRef,
    },
    #[error("A scheduled table must have columns `scheduled_id: u64` and `scheduled_at: ScheduledAt`, but table `{table}` has columns {columns:?}")]
    ScheduledIncorrectColumns { table: RawIdentifier, columns: ProductType },
    #[error("error at {location}: {error}")]
    ClientCodegenError {
        location: TypeLocation<'static>,
        error: ClientCodegenError,
    },
    #[error("Missing type definition for ref: {ref_}, holds type: {ty}")]
    MissingTypeDef {
        ref_: AlgebraicTypeRef,
        ty: PrettyAlgebraicType,
    },
    #[error("{column} is primary key but has no unique constraint")]
    MissingPrimaryKeyUniqueConstraint { column: RawColumnName },
    #[error("Table {table} should have a type definition for its product_type_element, but does not")]
    TableTypeNameMismatch { table: Identifier },
    #[error("Schedule {schedule} refers to a scheduled reducer or procedure {function} that does not exist")]
    MissingScheduledFunction { schedule: Box<str>, function: Identifier },
    #[error("Scheduled {function_kind} {function_name} expected to have type {expected}, but has type {actual}")]
    IncorrectScheduledFunctionParams {
        function_name: RawIdentifier,
        function_kind: FunctionKind,
        expected: PrettyAlgebraicType,
        actual: PrettyAlgebraicType,
    },
    #[error("Table name is reserved for system use: {table}")]
    TableNameReserved { table: Identifier },
    #[error("Row-level security invalid: `{error}`, query: `{sql}")]
    InvalidRowLevelQuery { sql: String, error: String },
    #[error("Failed to deserialize default value for table {table} column {col_id}: {err}")]
    ColumnDefaultValueMalformed {
        table: RawIdentifier,
        col_id: ColId,
        err: DecodeError,
    },
    #[error("Multiple default values for table {table} column {col_id}")]
    MultipleColumnDefaultValues { table: RawIdentifier, col_id: ColId },
    #[error("Table {table} not found")]
    TableNotFound { table: RawIdentifier },
    #[error("Name {name} is used for multiple reducers, procedures and/or views")]
    DuplicateFunctionName { name: Identifier },
}

/// A wrapper around an `AlgebraicType` that implements `fmt::Display`.
#[derive(PartialOrd, Ord, PartialEq, Eq)]
pub struct PrettyAlgebraicType(pub AlgebraicType);

impl fmt::Display for PrettyAlgebraicType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_algebraic_type(&self.0).fmt(f)
    }
}
impl fmt::Debug for PrettyAlgebraicType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <Self as fmt::Display>::fmt(self, f)
    }
}
impl From<AlgebraicType> for PrettyAlgebraicType {
    fn from(ty: AlgebraicType) -> Self {
        Self(ty)
    }
}
impl From<ProductType> for PrettyAlgebraicType {
    fn from(ty: ProductType) -> Self {
        let ty: AlgebraicType = ty.into();
        Self(ty)
    }
}
impl From<SumType> for PrettyAlgebraicType {
    fn from(ty: SumType) -> Self {
        let ty: AlgebraicType = ty.into();
        Self(ty)
    }
}

/// A place a type can be located in a module.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TypeLocation<'a> {
    /// A reducer argument.
    ReducerArg {
        reducer_name: Cow<'a, str>,
        position: usize,
        arg_name: Option<Cow<'a, str>>,
    },
    /// A procedure argument.
    ProcedureArg {
        procedure_name: Cow<'a, str>,
        position: usize,
        arg_name: Option<Cow<'a, str>>,
    },
    /// A view argument.
    ViewArg {
        view_name: Cow<'a, str>,
        position: usize,
        arg_name: Option<Cow<'a, str>>,
    },
    /// A procedure return type.
    ProcedureReturn { procedure_name: Cow<'a, str> },
    /// A view return type.
    ViewReturn { view_name: Cow<'a, str> },
    /// A type in the typespace.
    InTypespace {
        /// The reference to the type within the typespace.
        ref_: AlgebraicTypeRef,
    },
}
impl TypeLocation<'_> {
    /// Make the lifetime of the location `'static`.
    /// This allocates.
    pub fn make_static(self) -> TypeLocation<'static> {
        match self {
            TypeLocation::ReducerArg {
                reducer_name,
                position,
                arg_name,
            } => TypeLocation::ReducerArg {
                reducer_name: reducer_name.to_string().into(),
                position,
                arg_name: arg_name.map(|s| s.to_string().into()),
            },
            TypeLocation::ProcedureArg {
                procedure_name,
                position,
                arg_name,
            } => TypeLocation::ProcedureArg {
                procedure_name: procedure_name.to_string().into(),
                position,
                arg_name: arg_name.map(|s| s.to_string().into()),
            },
            TypeLocation::ViewArg {
                view_name,
                position,
                arg_name,
            } => TypeLocation::ViewArg {
                view_name: view_name.to_string().into(),
                position,
                arg_name: arg_name.map(|s| s.to_string().into()),
            },
            Self::ProcedureReturn { procedure_name } => TypeLocation::ProcedureReturn {
                procedure_name: procedure_name.to_string().into(),
            },
            Self::ViewReturn { view_name } => TypeLocation::ViewReturn {
                view_name: view_name.to_string().into(),
            },
            // needed to convince rustc this is allowed.
            TypeLocation::InTypespace { ref_ } => TypeLocation::InTypespace { ref_ },
        }
    }
}

impl fmt::Display for TypeLocation<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeLocation::ReducerArg {
                reducer_name,
                position,
                arg_name,
            } => {
                write!(f, "reducer `{reducer_name}` argument {position}")?;
                if let Some(arg_name) = arg_name {
                    write!(f, " (`{arg_name}`)")?;
                }
                Ok(())
            }
            TypeLocation::ProcedureArg {
                procedure_name,
                position,
                arg_name,
            } => {
                write!(f, "procedure `{procedure_name}` argument {position}")?;
                if let Some(arg_name) = arg_name {
                    write!(f, " (`{arg_name}`)")?;
                }
                Ok(())
            }
            TypeLocation::ViewArg {
                view_name,
                position,
                arg_name,
            } => {
                write!(f, "view `{view_name}` argument {position}")?;
                if let Some(arg_name) = arg_name {
                    write!(f, " (`{arg_name}`)")?;
                }
                Ok(())
            }
            TypeLocation::ProcedureReturn { procedure_name } => {
                write!(f, "procedure `{procedure_name}` return value")
            }
            TypeLocation::ViewReturn { view_name } => {
                write!(f, "view `{view_name}` return value")
            }
            TypeLocation::InTypespace { ref_ } => {
                write!(f, "typespace ref `{ref_}`")
            }
        }
    }
}

/// The name of a column.
#[derive(Debug, PartialOrd, Ord, PartialEq, Eq)]
pub struct RawColumnName {
    /// The table the column is in.
    pub table: RawIdentifier,
    /// The name of the column. This may be an integer if the column is unnamed.
    pub column: RawIdentifier,
}

impl RawColumnName {
    /// Create a new `RawColumnName`.
    pub fn new(table: impl Into<RawIdentifier>, column: impl Into<RawIdentifier>) -> Self {
        Self {
            table: table.into(),
            column: column.into(),
        }
    }
}

impl fmt::Display for RawColumnName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "table `{}` column `{}`", self.table, self.column)
    }
}

/// A reason that a string the user used is not allowed.
#[derive(thiserror::Error, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum IdentifierError {
    /// The identifier is not in Unicode Normalization Form C.
    ///
    /// TODO(1.0): We should *canonicalize* identifiers,
    /// rather than simply *rejecting* non-canonicalized identifiers.
    /// However, this will require careful testing of codegen in both modules and clients,
    /// to ensure that the canonicalization is done consistently.
    /// Otherwise, strange name errors will result.
    #[error(
        "Identifier `{name}` is not in normalization form C according to Unicode Standard Annex 15 \
        (http://www.unicode.org/reports/tr15/) and cannot be used for entities in a module."
    )]
    NotCanonicalized { name: RawIdentifier },

    /// The identifier is reserved.
    #[error("Identifier `{name}` is reserved by spacetimedb and cannot be used for entities in a module.")]
    Reserved { name: RawIdentifier },

    #[error(
        "Identifier `{name}`'s starting character '{invalid_start}' is neither an underscore ('_') nor a \
        Unicode XID_start character (according to Unicode Standard Annex 31, https://www.unicode.org/reports/tr31/) \
        and cannot be used for entities in a module."
    )]
    InvalidStart { name: RawIdentifier, invalid_start: char },

    #[error(
        "Identifier `{name}` contains a character '{invalid_continue}' that is not an XID_continue character \
        (according to Unicode Standard Annex 31, https://www.unicode.org/reports/tr31/) \
        and cannot be used for entities in a module."
    )]
    InvalidContinue {
        name: RawIdentifier,
        invalid_continue: char,
    },
    // This is not a particularly useful error without a link to WHICH identifier is empty.
    #[error("Empty identifiers are forbidden.")]
    Empty {},
}
