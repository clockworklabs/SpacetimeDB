use crate::db::relational_db::TableIter;
use crate::error::DBError;
use spacetimedb_sats::relation::DbTable;

/// Common wrapper for relational iterators that work like cursors.
pub struct TableCursor<'a> {
    pub table: DbTable,
    pub iter: TableIter<'a>,
}

impl<'a> TableCursor<'a> {
    pub fn new(table: DbTable, iter: TableIter<'a>) -> Result<Self, DBError> {
        Ok(Self { table, iter })
    }
}
