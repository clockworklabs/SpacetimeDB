use super::{Index, KeySize, RangedIndex};
use crate::{indexes::RowPointer, table_index::key_size::KeyBytesStorage};
use core::{borrow::Borrow, ops::RangeBounds, option::IntoIter};
use spacetimedb_sats::memory_usage::MemoryUsage;
use std::collections::btree_map::{BTreeMap, Entry, Range};

/// A "unique map" that relates a `K` to a `RowPointer`.
///
/// (This is just a `BTreeMap<K, RowPointer>`) with a slightly modified interface.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct UniqueBTreeIndex<K: KeySize> {
    /// The map is backed by a `BTreeMap` for relating a key to a value.
    map: BTreeMap<K, RowPointer>,
    /// Storage for [`Index::num_key_bytes`].
    num_key_bytes: K::MemoStorage,
}

impl<K: KeySize> Default for UniqueBTreeIndex<K> {
    fn default() -> Self {
        Self {
            map: <_>::default(),
            num_key_bytes: <_>::default(),
        }
    }
}

impl<K: KeySize + MemoryUsage> MemoryUsage for UniqueBTreeIndex<K> {
    fn heap_usage(&self) -> usize {
        let Self { map, num_key_bytes } = self;
        map.heap_usage() + num_key_bytes.heap_usage()
    }
}

impl<K: Ord + KeySize> Index for UniqueBTreeIndex<K> {
    type Key = K;

    fn clone_structure(&self) -> Self {
        Self::default()
    }

    fn insert(&mut self, key: K, val: RowPointer) -> Result<(), RowPointer> {
        match self.map.entry(key) {
            Entry::Vacant(e) => {
                self.num_key_bytes.add_to_key_bytes(e.key());
                e.insert(val);
                Ok(())
            }
            Entry::Occupied(e) => Err(*e.into_mut()),
        }
    }

    fn delete(&mut self, key: &K, ptr: RowPointer) -> bool {
        self.delete(key, ptr)
    }

    fn num_keys(&self) -> usize {
        self.map.len()
    }

    fn num_key_bytes(&self) -> u64 {
        self.num_key_bytes.get(self)
    }

    type PointIter<'a>
        = UniquePointIter
    where
        Self: 'a;

    fn seek_point(&self, point: &Self::Key) -> Self::PointIter<'_> {
        UniquePointIter::new(self.map.get(point).copied())
    }

    /// Deletes all entries from the map, leaving it empty.
    ///
    /// Unfortunately, this will drop the existing allocation.
    fn clear(&mut self) {
        self.map.clear();
        self.num_key_bytes.reset_to_zero();
    }

    fn can_merge(&self, other: &Self, ignore: impl Fn(&RowPointer) -> bool) -> Result<(), RowPointer> {
        let Some(found) = other
            .map
            .keys()
            .find_map(|key| self.map.get(key).filter(|val| !ignore(val)))
        else {
            return Ok(());
        };
        Err(*found)
    }
}

impl<K: KeySize + Ord> UniqueBTreeIndex<K> {
    /// See [`Index::delete`].
    /// This version has relaxed bounds.
    pub fn delete<Q>(&mut self, key: &Q, _: RowPointer) -> bool
    where
        Q: ?Sized + KeySize + Ord,
        <Self as Index>::Key: Borrow<Q>,
    {
        let ret = self.map.remove(key).is_some();
        if ret {
            self.num_key_bytes.sub_from_key_bytes(key);
        }
        ret
    }

    /// See [`Index::seek_point`].
    /// This version has relaxed bounds.
    pub fn seek_point<Q>(&self, point: &Q) -> <Self as Index>::PointIter<'_>
    where
        Q: ?Sized + Ord,
        <Self as Index>::Key: Borrow<Q>,
    {
        UniquePointIter::new(self.map.get(point).copied())
    }
}

/// An iterator over the potential value in a unique index for a given key.
pub struct UniquePointIter {
    /// The iterator seeking for matching keys in the range.
    pub(super) iter: IntoIter<RowPointer>,
}

impl UniquePointIter {
    /// Returns a new iterator over the possibly found row pointer.
    pub fn new(point: Option<RowPointer>) -> Self {
        let iter = point.into_iter();
        Self { iter }
    }
}

impl Iterator for UniquePointIter {
    type Item = RowPointer;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

impl<K: Ord + KeySize> RangedIndex for UniqueBTreeIndex<K> {
    type RangeIter<'a>
        = UniqueBTreeIndexRangeIter<'a, K>
    where
        Self: 'a;

    fn seek_range(&self, range: &impl RangeBounds<Self::Key>) -> Self::RangeIter<'_> {
        self.seek_range(range)
    }
}

impl<K: KeySize + Ord> UniqueBTreeIndex<K> {
    /// See [`RangedIndex::seek_range`].
    /// This version has relaxed bounds.
    pub fn seek_range<Q: ?Sized + Ord>(&self, range: &impl RangeBounds<Q>) -> <Self as RangedIndex>::RangeIter<'_>
    where
        <Self as Index>::Key: Borrow<Q>,
    {
        UniqueBTreeIndexRangeIter {
            iter: self.map.range((range.start_bound(), range.end_bound())),
        }
    }
}

/// An iterator over values in a [`UniqueBTreeIndex`] where the keys are in a certain range.
#[derive(Clone)]
pub struct UniqueBTreeIndexRangeIter<'a, K> {
    /// The iterator seeking for matching keys in the range.
    iter: Range<'a, K, RowPointer>,
}

impl<'a, K> Iterator for UniqueBTreeIndexRangeIter<'a, K> {
    type Item = RowPointer;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|(_, v)| *v)
    }
}
