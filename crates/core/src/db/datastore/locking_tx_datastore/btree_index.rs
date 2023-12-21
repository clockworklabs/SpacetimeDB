use super::RowId;
use crate::error::DBError;
use core::ops::RangeBounds;
use core::slice;
use smallvec::SmallVec;
use spacetimedb_primitives::*;
use spacetimedb_sats::data_key::ToDataKey;
use spacetimedb_sats::{AlgebraicValue, ProductValue};
use std::collections::btree_map::{BTreeMap, Range};

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
type IndexKey = AlgebraicValue;
type RowIds = SmallVec<[RowId; 1]>;

/// An iterator for the rows that match a value [AlgebraicValue] on the
/// [BTreeIndex]
pub struct BTreeIndexRangeIter<'a> {
    outer: Range<'a, AlgebraicValue, RowIds>,
    inner: Option<slice::Iter<'a, RowId>>,
    num_keys_scanned: u64,
}

impl<'a> Iterator for BTreeIndexRangeIter<'a> {
    type Item = &'a RowId;

    #[tracing::instrument(skip_all)]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(inner) = self.inner.as_mut() {
                if let Some(ptr) = inner.next() {
                    self.num_keys_scanned += 1;
                    return Some(ptr);
                }
            }

            self.inner = None;
            let (_, next) = self.outer.next()?;
            self.inner = Some(next.iter());
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
    pub(crate) cols: ColList,
    pub(crate) name: String,
    pub(crate) is_unique: bool,
    idx: BTreeMap<IndexKey, RowIds>,
}

impl BTreeIndex {
    pub(crate) fn new(index_id: IndexId, table_id: TableId, cols: ColList, name: String, is_unique: bool) -> Self {
        Self {
            index_id,
            table_id,
            cols,
            name,
            is_unique,
            idx: BTreeMap::new(),
        }
    }

    pub(crate) fn get_fields(&self, row: &ProductValue) -> Result<AlgebraicValue, DBError> {
        let fields = row.project_not_empty(&self.cols)?;
        Ok(fields)
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn insert(&mut self, row: &ProductValue) -> Result<(), DBError> {
        let col_value = self.get_fields(row)?;
        self.idx.entry(col_value).or_default().push(RowId(row.to_data_key()));
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn delete(&mut self, col_value: &AlgebraicValue, row_id: &RowId) {
        let Some(entry) = self.idx.get_mut(col_value) else {
            return;
        };
        let Some(pos) = entry.iter().position(|x| x == row_id) else {
            return;
        };
        entry.swap_remove(pos);
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

    /// Returns an iterator over the [BTreeIndex] that yields all the `RowId`s
    /// that fall within the specified `range`.
    #[tracing::instrument(skip_all)]
    pub(crate) fn seek(&self, range: &impl RangeBounds<AlgebraicValue>) -> BTreeIndexRangeIter<'_> {
        BTreeIndexRangeIter {
            outer: self.idx.range((range.start_bound(), range.end_bound())),
            inner: None,
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
}
