use super::RowId;
use crate::{db::datastore::traits::IndexSchema, error::DBError};
use indexmap::IndexMap;
use nonempty::NonEmpty;
use smallvec::SmallVec;
use spacetimedb_lib::data_key::ToDataKey;
use spacetimedb_primitives::{ColId, IndexId, TableId};
use spacetimedb_sats::{AlgebraicValue, ProductValue};

/// An iterator for the rows that match a value [AlgebraicValue] on the
/// [HashIndex]
/// We unify iterators over Option<&SmallVec<[&RowId; 1]>> and Option<&RowId>
/// into a common denominator - an iterator over &[RowId] - to avoid overhead
/// of an extra enum around different variants.
pub type HashIndexSeekIter<'a> = std::slice::Iter<'a, RowId>;

const fn _assert_index_seek_iter(arg: HashIndexSeekIter<'_>) -> impl Iterator<Item = &'_ RowId> {
    arg
}

enum HashIdx {
    // If we know the key is unique, we can reduce size of the index by always storing just one RowId.
    Unique(IndexMap<AlgebraicValue, RowId>),
    // Otherwise we store a SmallVec of RowIds to avoid allocation for the common case of still
    // having just one row with given key.
    MaybeUnique(IndexMap<AlgebraicValue, SmallVec<[RowId; 1]>>),
}

pub(crate) struct HashIndex {
    pub(crate) index_id: IndexId,
    pub(crate) table_id: TableId,
    pub(crate) cols: NonEmpty<ColId>,
    pub(crate) name: String,
    hash_idx: HashIdx,
}

impl HashIndex {
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
            hash_idx: if is_unique {
                HashIdx::Unique(Default::default())
            } else {
                HashIdx::MaybeUnique(Default::default())
            },
        }
    }

    pub(crate) fn is_unique(&self) -> bool {
        matches!(self.hash_idx, HashIdx::Unique(_))
    }

    pub(crate) fn get_fields(&self, row: &ProductValue) -> Result<AlgebraicValue, DBError> {
        let fields = row.project_not_empty(&self.cols)?;
        Ok(fields)
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn insert(&mut self, row: &ProductValue) -> Result<(), DBError> {
        let col_value = self.get_fields(row)?;
        let row_id = RowId(row.to_data_key());
        match &mut self.hash_idx {
            HashIdx::Unique(x) => {
                x.insert(col_value, row_id);
            }
            HashIdx::MaybeUnique(x) => {
                x.entry(col_value).or_default().push(row_id);
            }
        }
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn delete(&mut self, col_value: &AlgebraicValue, row_id: &RowId) {
        match &mut self.hash_idx {
            HashIdx::Unique(x) => {
                x.remove(col_value);
            }
            HashIdx::MaybeUnique(x) => {
                if let Some(row_ids) = x.get_mut(col_value) {
                    if let Some(idx) = row_ids.iter().position(|x| x == row_id) {
                        row_ids.swap_remove(idx);
                    }
                }
            }
        }
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn get_row_that_violates_unique_constraint<'a>(&'a self, row: &AlgebraicValue) -> Option<&'a RowId> {
        match &self.hash_idx {
            HashIdx::Unique(x) => x.get(row),
            _ => None,
        }
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn violates_unique_constraint(&self, row: &AlgebraicValue) -> bool {
        self.get_row_that_violates_unique_constraint(row).is_some()
    }

    /// Returns `true` if the [HashIndex] contains a value for the specified `value`.
    #[tracing::instrument(skip_all)]
    pub(crate) fn contains_any(&self, value: &AlgebraicValue) -> bool {
        match &self.hash_idx {
            HashIdx::Unique(x) => x.contains_key(value),
            HashIdx::MaybeUnique(x) => x.get(value).map_or(false, |x| !x.is_empty()),
        }
    }

    /// Returns an iterator over the `RowId`s in the [HashIndex]
    #[tracing::instrument(skip_all)]
    pub(crate) fn scan(&self) -> impl Iterator<Item = &'_ RowId> {
        match &self.hash_idx {
            HashIdx::Unique(x) => itertools::Either::Left(x.values()),
            HashIdx::MaybeUnique(x) => itertools::Either::Right(x.values().flatten()),
        }
    }

    /// Returns an iterator over the [HashIndex] that yields all the `RowId`s
    /// that fall within the specified `range`.
    #[tracing::instrument(skip_all)]
    pub(crate) fn seek<'a>(&'a self, value: &AlgebraicValue) -> HashIndexSeekIter<'a> {
        match &self.hash_idx {
            HashIdx::Unique(x) => x.get(value).map(std::slice::from_ref),
            HashIdx::MaybeUnique(x) => x.get(value).map(SmallVec::as_slice),
        }
        .unwrap_or_default()
        .iter()
    }

    /// Construct the [HashIndex] from the rows.
    #[tracing::instrument(skip_all)]
    pub(crate) fn build_from_rows<'a>(&mut self, rows: impl Iterator<Item = &'a ProductValue>) -> Result<(), DBError> {
        for row in rows {
            self.insert(row)?;
        }
        Ok(())
    }
}

impl From<&HashIndex> for IndexSchema {
    fn from(x: &HashIndex) -> Self {
        IndexSchema {
            index_id: x.index_id,
            table_id: x.table_id,
            cols: x.cols.clone(),
            is_unique: x.is_unique(),
            index_name: x.name.clone(),
        }
    }
}
