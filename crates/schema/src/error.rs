use spacetimedb_data_structures::error_stream::ErrorStream;
use spacetimedb_lib::db::raw_def::v9::RawIdentifier;
use spacetimedb_lib::ProductType;
use spacetimedb_primitives::{ColId, ColList};
use spacetimedb_sats::{typespace::TypeRefError, AlgebraicType, AlgebraicTypeRef};
use std::borrow::Cow;
use std::fmt;

use crate::identifier::Identifier;

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
    DuplicateName { name: Identifier }, // use an Identifier because this is checked for validated names.
    #[error("module contains invalid identifier: {error}")]
    IdentifierError { error: IdentifierError },
    #[error("table `{}` has unnamed column `{}`, which is forbidden.", column.table, column.column)]
    UnnamedColumn { column: RawColumnName },
    #[error("type `{type_name}` is not annotated with a custom ordering, but uses one: {bad_type:?}")]
    TypeHasIncorrectOrdering {
        type_name: RawIdentifier,
        ref_: AlgebraicTypeRef,
        /// Could be a sum or product.
        bad_type: AlgebraicType,
    },
    #[error("column `{column}` referenced by def {def} not found in table `{table}`")]
    ColumnNotFound {
        table: RawIdentifier,
        def: RawIdentifier,
        column: ColId,
    },
    #[error("column `{column}` has `Constraints::unset()`")]
    ConstraintUnset { column: RawColumnName, name: RawIdentifier },
    #[error("Attempt to define a column `{column}` with more than 1 auto_inc sequence")]
    OneAutoInc { column: RawColumnName },
    #[error("Only Btree Indexes are supported: index `{index}` is not a btree")]
    OnlyBtree { index: RawIdentifier },
    #[error("def `{def}` has duplicate columns: {columns:?}")]
    DuplicateColumns { def: RawIdentifier, columns: ColList },
    #[error("invalid sequence column type: `{column}` with type `{column_type:?}` in sequence `{sequence}`")]
    InvalidSequenceColumnType {
        sequence: RawIdentifier,
        column: RawColumnName,
        column_type: AlgebraicType,
    },
    #[error("invalid sequence range information: expected {min_value:?} <= {start:?} <= {max_value:?} in sequence `{sequence}`")]
    InvalidSequenceRange {
        sequence: RawIdentifier,
        min_value: Option<i128>,
        start: Option<i128>,
        max_value: Option<i128>,
    },
    #[error("Table {table} has invalid product_type_ref {ref_}")]
    InvalidProductTypeRef {
        table: RawIdentifier,
        ref_: AlgebraicTypeRef,
    },
    #[error("Type {type_name} has invalid ref: {ref_}")]
    InvalidTypeRef {
        type_name: RawIdentifier,
        ref_: AlgebraicTypeRef,
    },
    #[error("A scheduled table must have columns `scheduled_id: u64` and `scheduled_at: ScheduledAt`, but table `{table}` has columns {columns:?}")]
    ScheduledIncorrectColumns { table: RawIdentifier, columns: ProductType },
    #[error("{location} is not in nominal normal form: {ty:?}")]
    NotNominalNormalForm {
        location: TypeLocation<'static>,
        ty: AlgebraicType,
    },
    #[error("Type {ty:?} failed to resolve")]
    ResolutionFailure {
        location: TypeLocation<'static>,
        ty: AlgebraicType,
        error: TypeRefError,
    },
    #[error("Missing type definition for ref: {ref_}")]
    MissingTypeDef { ref_: AlgebraicTypeRef },
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
                write!(f, "reducer `{}` argument {}", reducer_name, position)?;
                if let Some(arg_name) = arg_name {
                    write!(f, " (`{}`)", arg_name)?;
                }
                Ok(())
            }
            TypeLocation::InTypespace { ref_ } => {
                write!(f, "typespace ref `{}`", ref_)
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

#[derive(thiserror::Error, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum IdentifierError {
    #[error("Identifier `{name}` is not canonicalized according to Unicode Annexreserved by spacetimedb and cannot be used for table, column, or reducer names.")]
    NotCanonicalized { name: RawIdentifier },

    #[error("Identifier `{name}` is reserved by spacetimedb and cannot be used for table, column, or reducer names.")]
    Reserved { name: RawIdentifier },

    #[error("Identifier `{name}`'s starting character '{invalid_start}' does not start with an underscore or Unicode XID start character (according to Unicode Standard Annex 31).")]
    InvalidStart { name: RawIdentifier, invalid_start: char },

    #[error("Identifier `{name}` contains a character '{invalid_continue}' that is not a Unicode XID continue character (according to Unicode Standard Annex 31).")]
    InvalidContinue {
        name: RawIdentifier,
        invalid_continue: char,
    },

    // This is not a particularly useful error without a link to WHICH identifier is empty.
    #[error("Identifier is empty.")]
    Empty {},
}
