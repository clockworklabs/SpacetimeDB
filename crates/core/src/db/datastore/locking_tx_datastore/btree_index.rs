use super::RowId;
use crate::{db::datastore::traits::IndexSchema, error::DBError};
use indexmap::IndexMap;
use nonempty::NonEmpty;
use smallvec::SmallVec;
use spacetimedb_lib::{data_key::ToDataKey, DataKey};
use spacetimedb_primitives::{ColId, IndexId, TableId};
use spacetimedb_sats::{AlgebraicValue, ProductValue};
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

pub struct RangeRowIdIter<'a> {
    range_iter: btree_set::Range<'a, IndexKey>,
}

impl<'a> Iterator for RangeRowIdIter<'a> {
    type Item = &'a RowId;

    #[tracing::instrument(skip_all)]
    fn next(&mut self) -> Option<Self::Item> {
        self.range_iter.next().map(|key| &key.row_id)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.range_iter.size_hint()
    }
}

/// An iterator for the rows that match a value [AlgebraicValue] on the
/// [BTreeIndex]
pub type BTreeIndexRangeIter<'a> =
    itertools::Either<std::iter::Flatten<std::option::IntoIter<&'a SmallVec<[RowId; 1]>>>, RangeRowIdIter<'a>>;

const fn _assert_index_range_iter(arg: BTreeIndexRangeIter<'_>) -> impl Iterator<Item = &'_ RowId> {
    arg
}

pub(crate) struct BTreeIndex {
    pub(crate) index_id: IndexId,
    pub(crate) table_id: TableId,
    pub(crate) cols: NonEmpty<ColId>,
    pub(crate) name: String,
    pub(crate) is_unique: bool,
    idx: BTreeSet<IndexKey>,
    hash_idx: IndexMap<AlgebraicValue, SmallVec<[RowId; 1]>>,
}

impl BTreeIndex {
    pub(crate) fn new(
        index_id: IndexId,
        table_id: TableId,
        cols: NonEmpty<ColId>,
        name: String,
        is_unique: bool,
    ) -> Self {
        Self {
            index_id,
            table_id,
            cols,
            name,
            is_unique,
            idx: BTreeSet::new(),
            hash_idx: Default::default(),
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
        self.hash_idx
            .entry(col_value)
            .or_default()
            .push(RowId(row.to_data_key()));
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn delete(&mut self, col_value: &AlgebraicValue, row_id: &RowId) {
        let key = IndexKey::from_row(col_value, row_id.0);
        self.idx.remove(&key);
        if let Some(row_ids) = self.hash_idx.get_mut(col_value) {
            if let Some(idx) = row_ids.iter().position(|x| x == row_id) {
                row_ids.swap_remove(idx);
            }
        }
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
        self.hash_idx.get(value).map_or(false, |x| !x.is_empty())
    }

    /// Returns an iterator over the `RowId`s in the [BTreeIndex]
    #[tracing::instrument(skip_all)]
    pub(crate) fn scan(&self) -> impl Iterator<Item = &'_ RowId> {
        self.hash_idx.values().flatten()
    }

    /// Returns an iterator over the [BTreeIndex] that yields all the `RowId`s
    /// that fall within the specified `range`.
    #[tracing::instrument(skip_all)]
    pub(crate) fn seek<'a>(&'a self, range: &impl RangeBounds<AlgebraicValue>) -> BTreeIndexRangeIter<'a> {
        match (range.start_bound(), range.end_bound()) {
            (Bound::Included(start), Bound::Excluded(end)) if start == end && self.is_unique => {
                itertools::Either::Left(self.hash_idx.get(start).into_iter().flatten())
            }
            (start, end) => itertools::Either::Right({
                let map = |bound, datakey| match bound {
                    Bound::Included(x) => Bound::Included(IndexKey::from_row(x, datakey)),
                    Bound::Excluded(x) => Bound::Excluded(IndexKey::from_row(x, datakey)),
                    Bound::Unbounded => Bound::Unbounded,
                };
                let start = map(start, DataKey::min_datakey());
                let end = map(end, DataKey::max_datakey());
                RangeRowIdIter {
                    range_iter: self.idx.range((start, end)),
                }
            }),
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
            index_id: x.index_id,
            table_id: x.table_id,
            cols: x.cols.clone(),
            is_unique: x.is_unique,
            index_name: x.name.clone(),
        }
    }
}
