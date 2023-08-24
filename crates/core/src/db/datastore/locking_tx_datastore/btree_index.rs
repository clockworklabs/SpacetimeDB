use super::RowId;
use crate::{
    db::datastore::traits::{IndexId, IndexSchema},
    error::DBError,
};
use nonempty::NonEmpty;
use spacetimedb_lib::{data_key::ToDataKey, DataKey};
use spacetimedb_sats::{AlgebraicValue, ProductValue};
use std::{
    collections::{btree_set, BTreeSet},
    ops::{Bound, RangeBounds},
};

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
}

impl Iterator for BTreeIndexRangeIter<'_> {
    type Item = RowId;

    #[tracing::instrument(skip_all)]
    fn next(&mut self) -> Option<Self::Item> {
        self.range_iter.next().map(|key| key.row_id)
    }
}

pub(crate) struct BTreeIndex {
    pub(crate) index_id: IndexId,
    pub(crate) table_id: u32,
    pub(crate) cols: NonEmpty<u32>,
    pub(crate) name: String,
    pub(crate) is_unique: bool,
    idx: BTreeSet<IndexKey>,
}

impl BTreeIndex {
    pub(crate) fn new(index_id: IndexId, table_id: u32, cols: NonEmpty<u32>, name: String, is_unique: bool) -> Self {
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
        Ok(AlgebraicValue::Product(fields))
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
        row: &'a ProductValue,
    ) -> Option<BTreeIndexRangeIter<'a>> {
        self.is_unique.then(|| {
            let value = self.get_fields(row).unwrap();
            self.seek(&value)
        })
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
    pub(crate) fn scan_range(&self, range: impl RangeBounds<AlgebraicValue>) -> BTreeIndexRangeIter {
        let map = |bound, datakey| match bound {
            Bound::Included(x) => Bound::Included(IndexKey::from_row(x, datakey)),
            Bound::Excluded(x) => Bound::Excluded(IndexKey::from_row(x, datakey)),
            Bound::Unbounded => Bound::Unbounded,
        };
        let start = map(range.start_bound(), DataKey::min_datakey());
        let end = map(range.end_bound(), DataKey::max_datakey());
        BTreeIndexRangeIter {
            range_iter: self.idx.range((start, end)),
        }
    }

    /// Returns an iterator over the [BTreeIndex] that yields all the `RowId`s
    /// that match the specified `value` in the indexed column.
    ///
    /// Matches is defined by `Ord for AlgebraicValue`.
    ///
    /// For a unique index this will always yield at most one `RowId`.
    #[tracing::instrument(skip_all)]
    pub(crate) fn seek<'a>(&'a self, value: &'a AlgebraicValue) -> BTreeIndexRangeIter<'a> {
        let k_start = IndexKey::from_row(value, DataKey::min_datakey());
        let k_end = IndexKey::from_row(value, DataKey::max_datakey());
        BTreeIndexRangeIter {
            range_iter: self.idx.range(k_start..k_end),
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
}

impl From<&BTreeIndex> for IndexSchema {
    fn from(x: &BTreeIndex) -> Self {
        IndexSchema {
            index_id: x.index_id.0,
            table_id: x.table_id,
            cols: x.cols.clone(),
            is_unique: x.is_unique,
            index_name: x.name.clone(),
        }
    }
}
