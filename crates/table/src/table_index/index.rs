use crate::{indexes::RowPointer, table_index::KeySize};
use core::{mem, ops::RangeBounds};

pub trait Index {
    /// The type of keys indexed.
    type Key: KeySize;

    // =========================================================================
    // Construction
    // =========================================================================

    /// Clones the structure of this index but not the indexed elements,
    /// returning an empty index.
    fn clone_structure(&self) -> Self;

    // =========================================================================
    // Mutation
    // =========================================================================

    /// Inserts the relation `key -> ptr` to this map.
    ///
    /// If `key` was already present in the index,
    /// does not add an association with val.
    /// Returns the existing associated pointer instead.
    ///
    /// Returns [`Despecialize`]
    /// if inserting `key` is not compatible with the index
    /// or if it would be profitable to replace the index with a B-Tree.
    /// The provided default implementation does not return `Despecialize`.
    fn insert_maybe_despecialize(
        &mut self,
        key: Self::Key,
        ptr: RowPointer,
    ) -> Result<Result<(), RowPointer>, Despecialize> {
        Ok(self.insert(key, ptr))
    }

    /// Inserts the relation `key -> ptr` to this map.
    ///
    /// If `key` was already present in the index,
    /// does not add an association with val.
    /// Returns the existing associated pointer instead.
    fn insert(&mut self, key: Self::Key, ptr: RowPointer) -> Result<(), RowPointer> {
        self.insert_maybe_despecialize(key, ptr).unwrap()
    }

    /// Deletes `key -> ptr` from this index.
    ///
    /// Returns whether `key -> ptr` was present.
    ///
    /// Implementations are free to ignore `ptr`
    /// if there can only ever be one `key`,
    /// as is the case for unique indices.
    fn delete(&mut self, key: &Self::Key, ptr: RowPointer) -> bool;

    /// Clears all the rows and keys from the index,
    /// leaving it empty.
    fn clear(&mut self);

    // =========================================================================
    // Querying
    // =========================================================================

    /// Returns whether `other` can be merged into `self`
    /// with an error containing the element in `self` that caused the violation.
    ///
    /// The closure `ignore` indicates whether a row in `self` should be ignored.
    fn can_merge(&self, other: &Self, ignore: impl Fn(&RowPointer) -> bool) -> Result<(), RowPointer>;

    /// Returns the number of keys indexed.
    ///
    /// This method runs in constant time.
    fn num_keys(&self) -> usize;

    /// The number of bytes stored in keys in this index.
    ///
    /// For non-unique indexes, duplicate keys are counted once for each row that refers to them,
    /// even though the internal storage may deduplicate them as an optimization.
    ///
    /// This method runs in constant time.
    ///
    /// See the [`KeySize`](super::KeySize) trait for more details on how this method computes its result.
    ///
    /// The provided implementation assumes
    /// that the key takes up exactly `size_of::<Self::Key>()` bytes
    /// and has no dynamic component.
    /// If that is not correct, you should override the implementation.
    fn num_key_bytes(&self) -> u64 {
        (self.num_keys() * mem::size_of::<Self::Key>()) as u64
    }

    /// Returns the number of rows indexed.
    ///
    /// When `self.num_keys() == 0` then `self.num_values() == 0`.
    ///
    /// Note that, for non-unique indexes, this may be larger than [`Index::num_keys`].
    ///
    /// This method runs in constant time.
    ///
    /// The provided implementation assumes the index is unique
    /// and uses [`Index::num_keys`].
    fn num_rows(&self) -> usize {
        self.num_keys()
    }

    /// Returns whether the index has no key or values.
    ///
    /// When `self.is_empty()`
    /// then `self.num_keys() == 0` and `self.num_values() == 0`.
    ///
    /// The provided implementation uses [`Index::num_keys`].
    fn is_empty(&self) -> bool {
        self.num_keys() == 0
    }

    /// The type of iterator returned by [`Index::seek_point`].
    type PointIter<'a>: Iterator<Item = RowPointer>
    where
        Self: 'a;

    /// Seeks `point` in this index,
    /// returning an iterator over all the elements.
    ///
    /// If the index is unique, this will at most return one element.
    fn seek_point(&self, point: &Self::Key) -> Self::PointIter<'_>;
}

pub trait RangedIndex: Index {
    /// The type of iterator returned by [`Index::seek_range`].
    type RangeIter<'a>: Iterator<Item = RowPointer>
    where
        Self: 'a;

    /// Seeks the `range` in this index,
    /// returning an iterator over all the elements.
    ///
    /// Prefer [`Index::seek_point`] for point scans
    /// rather than providing a point `range`
    /// as it will be faster.
    fn seek_range(&self, range: &impl RangeBounds<Self::Key>) -> Self::RangeIter<'_>;
}

/// An error indicating that the index should be despecialized to a B-Tree.
#[derive(Debug)]
pub struct Despecialize;

/// An error indicating that the [`Index`] is not a [`RangedIndex`].
#[derive(Debug, PartialEq)]
pub struct IndexCannotSeekRange;

pub type IndexSeekRangeResult<T> = Result<T, IndexCannotSeekRange>;
