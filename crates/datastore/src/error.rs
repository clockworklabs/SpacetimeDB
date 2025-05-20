use enum_as_inner::EnumAsInner;
use spacetimedb_lib::{db::{error::LibError, raw_def::v9::RawSql}, ProductValue};
use spacetimedb_lib::buffer::DecodeError;
use spacetimedb_primitives::{ColId, TableId};
use spacetimedb_table::{bflatn_to, read_column, table};
use thiserror::Error;

use crate::system_tables::SystemTable;

#[derive(Error, Debug, EnumAsInner)]
pub enum TableError {
    #[error("Table with name `{0}` start with 'st_' and that is reserved for internal system tables.")]
    System(Box<str>),
    #[error("Table with name `{0}` already exists.")]
    Exist(String),
    #[error("Table with name `{0}` not found.")]
    NotFound(String),
    #[error("Table with ID `{1}` not found in `{0}`.")]
    IdNotFound(SystemTable, u32),
    #[error("Sql `{1}` not found in `{0}`.")]
    RawSqlNotFound(SystemTable, RawSql),
    #[error("Table with ID `{0}` not found in `TxState`.")]
    IdNotFoundState(TableId),
    #[error("Column `{0}.{1}` is missing a name")]
    ColumnWithoutName(String, ColId),
    #[error("schema_for_table: Table has invalid schema: {0} Err: {1}")]
    InvalidSchema(TableId, LibError),
    #[error("Row has invalid row type for table: {0} Err: {1}", table_id, row.to_satn())]
    RowInvalidType { table_id: TableId, row: ProductValue },
    #[error("failed to decode row in table")]
    RowDecodeError(DecodeError),
    #[error("Column with name `{0}` already exists")]
    DuplicateColumnName(String),
    #[error("Column `{0}` not found")]
    ColumnNotFound(ColId),
    #[error(
        "DecodeError for field `{0}.{1}`, expect `{2}` but found `{3}`",
        table,
        field,
        expect,
        found
    )]
    DecodeField {
        table: String,
        field: Box<str>,
        expect: String,
        found: String,
    },
    #[error(transparent)]
    Bflatn(#[from] bflatn_to::Error),
    #[error(transparent)]
    Duplicate(#[from] table::DuplicateError),
    #[error(transparent)]
    ReadColTypeError(#[from] read_column::TypeError),
}
