use std::fmt;

use spacetimedb_primitives::ColList;
use spacetimedb_sats::{
    db::{error::DefType, raw_def::IndexType},
    typespace::TypeRefError,
    AlgebraicType,
};

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum SchemaError {
    #[error("table `{table}` has invalid name: {error}")]
    InvalidTableName { table: Box<str>, error: IdentifierError },
    #[error("table `{table}` has invalid column name `{column}`: {error}")]
    InvalidColumnName {
        table: Box<str>,
        column: Box<str>,
        error: IdentifierError,
    },
    #[error("table `{table}` column `{column}` has invalid type `{invalid:?}`: {error}")]
    InvalidColumnType {
        table: Box<str>,
        column: Box<str>,
        invalid: AlgebraicType,
        error: TypeRefError,
    },
    #[error("table `{table}`'s columns are not canonically ordered: expected {correct:?}, got {given:?}")]
    TableColumnsNotOrdered {
        table: Box<str>,
        correct: Vec<(Box<str>, AlgebraicType)>,
        given: Vec<(Box<str>, AlgebraicType)>,
    },
    #[error("column `{column}` not found in table `{table}`")]
    ColumnNotFound { table: Box<str>, column: Box<str> },
    #[error("table `{table}` {ty} should have name. {ty} id: {id}")]
    EmptyName { table: Box<str>, ty: DefType, id: u32 },
    #[error("table `{table}` have `Constraints::unset()` for columns: {columns:?}")]
    ConstraintUnset {
        table: Box<str>,
        name: Box<str>,
        columns: ColList,
    },
    #[error("Attempt to define a column with more than 1 auto_inc sequence: Table: `{table}`, Field: `{field}`")]
    OneAutoInc { table: Box<str>, field: Box<str> },
    #[error("Only Btree Indexes are supported: Table: `{table}`, Index on `{column_names:?}` is a `{index_type}`")]
    OnlyBtree {
        table: Box<str>,
        column_names: Vec<Box<str>>,
        index_type: IndexType,
    },
    #[error("{index_type} index definition on `{table}` has duplicate column names: {columns:?}")]
    IndexDefDuplicateColumnName {
        table: Box<str>,
        columns: Vec<Box<str>>,
        index_type: IndexType,
    },
    #[error("unique constraint definition on `{table}` has duplicate column names: {columns:?}")]
    UniqueConstraintDefDuplicateColumnName { table: Box<str>, columns: Vec<Box<str>> },
    #[error("invalid sequence column type: `{column}` with type `{column_type:?}` in table `{table}`")]
    InvalidSequenceColumnType {
        table: Box<str>,
        column: Box<str>,
        column_type: AlgebraicType,
    },
    #[error("Table {table} has uninitialized product type ref")]
    UninitializedProductTypeRef { table: Box<str> },
    #[error("Table {table} has incorrect product type element at column {column_index}")]
    ProductTypeColumnMismatch { table: Box<str>, column_index: usize },
}

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum IdentifierError {
    #[error("Identifier `{name}` is not canonicalized according to Unicode Annexreserved by spacetimedb and cannot be used for table, column, or reducer names.")]
    NotCanonicalized { name: Box<str> },

    #[error("Identifier `{name}` is reserved by spacetimedb and cannot be used for table, column, or reducer names.")]
    Reserved { name: Box<str> },

    #[error("Identifier `{name}`'s starting character '{invalid_start}' does not start with an underscore or Unicode XID start character (according to Unicode Standard Annex 31).")]
    InvalidStart { name: Box<str>, invalid_start: char },

    #[error("Identifier `{name}` contains a character '{invalid_continue}' that is not a Unicode XID continue character (according to Unicode Standard Annex 31).")]
    InvalidContinue { name: Box<str>, invalid_continue: char },

    // This is not a particularly useful error without a link to WHICH identifier is empty.
    #[error("Identifier is empty.")]
    Empty {},
}

#[derive(thiserror::Error, Debug, PartialEq)]
pub struct SchemaErrors(pub Vec<SchemaError>);

impl fmt::Display for SchemaErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.0.iter()).finish()
    }
}

impl SchemaErrors {
    /// Unpacks a result into the error stream, returning the value if it is Ok.
    pub(crate) fn unpack<T>(&mut self, result: Result<T, SchemaError>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(err) => {
                self.0.push(err);
                None
            }
        }
    }
}
