use super::RowId;
use crate::{
    db::datastore::traits::{IndexId, IndexSchema},
    error::DBError,
};
use nonempty::NonEmpty;
use spacetimedb_lib::data_key::ToDataKey;
use spacetimedb_sats::{AlgebraicValue, ProductValue};
use std::{
    collections::{btree_map, BTreeMap},
    ops::RangeBounds,
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

enum Rows {
    Unique(BTreeMap<AlgebraicValue, RowId>),
    NonUnique(BTreeMap<AlgebraicValue, indexmap::IndexSet<RowId>>),
}

pub struct ValueIter<I>(I);

impl<'a, K: 'a, V: 'a, I> Iterator for ValueIter<I>
where
    I: Iterator<Item = (&'a K, &'a V)>,
{
    type Item = &'a V;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(_, v)| v)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

pub type BTreeIndexRangeIter<'a> = std::iter::Copied<
    itertools::Either<
        ValueIter<btree_map::Range<'a, AlgebraicValue, RowId>>,
        std::iter::Flatten<ValueIter<btree_map::Range<'a, AlgebraicValue, indexmap::IndexSet<RowId>>>>,
    >,
>;

pub(crate) struct BTreeIndex {
    pub(crate) index_id: IndexId,
    pub(crate) table_id: u32,
    pub(crate) cols: NonEmpty<u32>,
    pub(crate) name: String,
    pub(crate) is_unique: bool,
    // todo: remove is_unique in favour of this
    idx: Rows,
}

impl BTreeIndex {
    pub(crate) fn new(index_id: IndexId, table_id: u32, cols: NonEmpty<u32>, name: String, is_unique: bool) -> Self {
        Self {
            index_id,
            table_id,
            cols,
            name,
            is_unique,
            idx: match is_unique {
                true => Rows::Unique(Default::default()),
                false => Rows::NonUnique(Default::default()),
            },
        }
    }

    pub(crate) fn get_fields(&self, row: &ProductValue) -> Result<AlgebraicValue, DBError> {
        let fields = row.project_not_empty(&self.cols)?;
        Ok(fields)
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn insert(&mut self, row: &ProductValue) -> Result<(), DBError> {
        let col_value = self.get_fields(row)?;
        let row_id = RowId(row.to_data_key());
        match &mut self.idx {
            Rows::Unique(rows) => {
                if rows.insert(col_value, row_id).is_some() {
                    tracing::error!("unique constraint violation that should have been checked by now");
                }
            }
            Rows::NonUnique(rows) => {
                rows.entry(col_value).or_default().insert(row_id);
            }
        }
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn delete(&mut self, col_value: &AlgebraicValue, row_id: &RowId) {
        match &mut self.idx {
            Rows::Unique(rows) => {
                rows.remove(col_value);
            }
            Rows::NonUnique(rows) => {
                if let Some(rows) = rows.get_mut(col_value) {
                    rows.remove(row_id);
                }
            }
        }
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn get_row_that_violates_unique_constraint<'a>(&'a self, row: &AlgebraicValue) -> Option<&'a RowId> {
        match &self.idx {
            Rows::Unique(rows) => rows.get(row),
            Rows::NonUnique(_) => None,
        }
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn violates_unique_constraint(&self, row: &ProductValue) -> bool {
        match &self.idx {
            Rows::Unique(rows) => {
                let col_value = self.get_fields(row).unwrap();
                rows.contains_key(&col_value)
            }
            Rows::NonUnique(_) => false,
        }
    }

    /// Returns `true` if the [BTreeIndex] contains a value for the specified `value`.
    #[tracing::instrument(skip_all)]
    pub(crate) fn contains_any(&self, value: &AlgebraicValue) -> bool {
        self.seek(value).next().is_some()
    }

    /// Returns an iterator over the `RowId`s in the [BTreeIndex]
    #[tracing::instrument(skip_all)]
    pub(crate) fn scan(&self) -> BTreeIndexRangeIter<'_> {
        self.seek(&(..))
    }

    /// Returns an iterator over the [BTreeIndex] that yields all the `RowId`s
    /// that fall within the specified `range`.
    #[tracing::instrument(skip_all)]
    pub(crate) fn seek<'a>(&'a self, range: &impl RangeBounds<AlgebraicValue>) -> BTreeIndexRangeIter<'a> {
        let range = (range.start_bound(), range.end_bound());
        match &self.idx {
            Rows::Unique(rows) => itertools::Either::Left(ValueIter(rows.range(range))),
            Rows::NonUnique(rows) => itertools::Either::Right(ValueIter(rows.range(range)).flatten()),
        }
        .copied()
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
