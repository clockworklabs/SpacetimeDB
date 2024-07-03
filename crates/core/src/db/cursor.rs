use super::datastore::locking_tx_datastore::{Iter, IterByColRange};
use crate::error::DBError;
use core::ops::RangeBounds;
use spacetimedb_lib::relation::DbTable;
use spacetimedb_sats::AlgebraicValue;

/// Common wrapper for relational iterators that work like cursors.
pub struct TableCursor<'a> {
    pub table: &'a DbTable,
    pub iter: Iter<'a>,
}

impl<'a> TableCursor<'a> {
    pub fn new(table: &'a DbTable, iter: Iter<'a>) -> Result<Self, DBError> {
        Ok(Self { table, iter })
    }
}

/// A relational iterator wrapping a storage level index iterator.
/// A relational iterator returns [RelValue]s whereas storage iterators return [DataRef]s.
pub struct IndexCursor<'a, R: RangeBounds<AlgebraicValue>> {
    pub table: &'a DbTable,
    pub iter: IterByColRange<'a, R>,
}

impl<'a, R: RangeBounds<AlgebraicValue>> IndexCursor<'a, R> {
    pub fn new(table: &'a DbTable, iter: IterByColRange<'a, R>) -> Result<Self, DBError> {
        Ok(Self { table, iter })
    }
}
