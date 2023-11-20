use super::RowId;
use crate::db::datastore::locking_tx_datastore::table::Table;
use crate::error::{DBError, IndexError};
use spacetimedb_primitives::{ColId, IndexId, TableId};
use spacetimedb_sats::data_key::{DataKey, ToDataKey};
use spacetimedb_sats::db::def::{IndexSchema, IndexType};
use spacetimedb_sats::{AlgebraicValue, ProductValue, SatsNonEmpty, SatsString};
use std::{
    collections::{btree_set, BTreeSet},
    ops::{Bound, RangeBounds},
};

/// ## Index Key Composition
///
/// [IndexKey] use an [AlgebraicValue] to optimize for the common case of *single columns* as key.
///
/// See [ProductValue::project] for the logic.
///
/// ### SQL Examples
///
/// To illustrate the concept of single and multiple column indexes, consider the following SQL examples:
///
/// ```sql
/// CREATE INDEX a ON t1 (column_i32); -- Creating a single column index, a common case.
/// CREATE INDEX b ON t1 (column_i32, column_i32); -- Creating a multiple column index for more complex requirements.
/// ```
/// Will be on memory:
///
/// ```rust,ignore
/// [AlgebraicValue::I32(0)] = Row(ProductValue(...))
/// [AlgebraicValue::Product(AlgebraicValue::I32(0), AlgebraicValue::I32(1))] = Row(ProductValue(...))
/// ```
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd)]
struct IndexKey {
    value: AlgebraicValue,
    row_id: RowId,
}

impl IndexKey {
    #[tracing::instrument(skip_all)]
    pub(crate) fn from_row(value: &AlgebraicValue, row_id: DataKey) -> Self {
        Self {
            value: value.clone(),
            row_id: RowId(row_id),
        }
    }
}

pub struct BTreeIndexIter<'a> {
    iter: btree_set::Iter<'a, IndexKey>,
}

impl Iterator for BTreeIndexIter<'_> {
    type Item = RowId;

    #[tracing::instrument(skip_all)]
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|key| key.row_id)
    }
}

/// An iterator for the rows that match a value [AlgebraicValue] on the
/// [BTreeIndex]
pub struct BTreeIndexRangeIter<'a> {
    range_iter: btree_set::Range<'a, IndexKey>,
    num_keys_scanned: u64,
}

impl<'a> Iterator for BTreeIndexRangeIter<'a> {
    type Item = &'a RowId;

    #[tracing::instrument(skip_all)]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(key) = self.range_iter.next() {
            self.num_keys_scanned += 1;
            Some(&key.row_id)
        } else {
            None
        }
    }
}

impl BTreeIndexRangeIter<'_> {
    /// Returns the current number of keys the iterator has scanned.
    pub fn keys_scanned(&self) -> u64 {
        self.num_keys_scanned
    }
}

pub(crate) struct BTreeIndex {
    pub(crate) index_id: IndexId,
    pub(crate) table_id: TableId,
    pub(crate) cols: SatsNonEmpty<ColId>,
    pub(crate) name: SatsString,
    pub(crate) is_unique: bool,
    idx: BTreeSet<IndexKey>,
}

impl BTreeIndex {
    pub(crate) fn new(
        index_id: IndexId,
        table_id: TableId,
        cols: SatsNonEmpty<ColId>,
        name: SatsString,
        is_unique: bool,
    ) -> Self {
        Self {
            index_id,
            table_id,
            cols,
            name,
            is_unique,
            idx: BTreeSet::new(),
        }
    }

    pub(crate) fn get_fields(&self, row: &ProductValue) -> Result<AlgebraicValue, DBError> {
        let fields = row.project_not_empty(&self.cols)?;
        Ok(fields)
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn insert(&mut self, row: &ProductValue) -> Result<(), DBError> {
        let col_value = self.get_fields(row)?;
        let key = IndexKey::from_row(&col_value, row.to_data_key());
        self.idx.insert(key);
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn delete(&mut self, col_value: &AlgebraicValue, row_id: &RowId) {
        let key = IndexKey::from_row(col_value, row_id.0);
        self.idx.remove(&key);
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn violates_unique_constraint(&self, row: &ProductValue) -> bool {
        if self.is_unique {
            let col_value = self.get_fields(row).unwrap();
            return self.contains_any(&col_value);
        }
        false
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn get_rows_that_violate_unique_constraint<'a>(
        &'a self,
        row: &'a AlgebraicValue,
    ) -> Option<BTreeIndexRangeIter<'a>> {
        self.is_unique.then(|| self.seek(row))
    }

    /// Returns `true` if the [BTreeIndex] contains a value for the specified `value`.
    #[tracing::instrument(skip_all)]
    pub(crate) fn contains_any(&self, value: &AlgebraicValue) -> bool {
        self.seek(value).next().is_some()
    }

    /// Returns an iterator over the `RowId`s in the [BTreeIndex]
    #[tracing::instrument(skip_all)]
    pub(crate) fn scan(&self) -> BTreeIndexIter<'_> {
        BTreeIndexIter { iter: self.idx.iter() }
    }

    /// Returns an iterator over the [BTreeIndex] that yields all the `RowId`s
    /// that fall within the specified `range`.
    #[tracing::instrument(skip_all)]
    pub(crate) fn seek(&self, range: &impl RangeBounds<AlgebraicValue>) -> BTreeIndexRangeIter<'_> {
        let map = |bound, datakey| match bound {
            Bound::Included(x) => Bound::Included(IndexKey::from_row(x, datakey)),
            Bound::Excluded(x) => Bound::Excluded(IndexKey::from_row(x, datakey)),
            Bound::Unbounded => Bound::Unbounded,
        };
        let start = map(range.start_bound(), DataKey::min_datakey());
        let end = map(range.end_bound(), DataKey::max_datakey());
        BTreeIndexRangeIter {
            range_iter: self.idx.range((start, end)),
            num_keys_scanned: 0,
        }
    }

    /// Construct the [BTreeIndex] from the rows.
    #[tracing::instrument(skip_all)]
    pub(crate) fn build_from_rows<'a>(&mut self, rows: impl Iterator<Item = &'a ProductValue>) -> Result<(), DBError> {
        for row in rows {
            self.insert(row)?;
        }
        Ok(())
    }

    pub(crate) fn build_error_unique(&self, table: &Table, value: AlgebraicValue) -> IndexError {
        IndexError::UniqueConstraintViolation {
            constraint_name: self.name.clone(),
            table_name: table.schema.table_name.clone(),
            cols: self
                .cols
                .iter()
                .map(|&x| table.schema.columns[usize::from(x)].col_name.clone())
                .collect(),
            value,
        }
    }
}

impl From<&BTreeIndex> for IndexSchema {
    fn from(x: &BTreeIndex) -> Self {
        IndexSchema {
            index_id: x.index_id,
            table_id: x.table_id,
            cols: x.cols.clone(),
            is_unique: x.is_unique,
            index_name: x.name.clone(),
            index_type: IndexType::BTree,
        }
    }
}
