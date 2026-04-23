use super::same_key_entry::{same_key_iter, SameKeyEntry, SameKeyEntryIter};
use super::{key_size::KeyBytesStorage, Index, KeySize, RangedIndex};
use crate::indexes::RowPointer;
use crate::table_index::same_key_entry::ManySameKeyEntryIter;
use core::borrow::Borrow;
use core::ops::RangeBounds;
use spacetimedb_sats::memory_usage::MemoryUsage;
use std::collections::btree_map::{BTreeMap, Range, Values};

/// A multi map that relates a `K` to a *set* of `RowPointer`s.
#[derive(Debug, PartialEq, Eq)]
pub struct BTreeIndex<K> {
    /// The map is backed by a `BTreeMap` for relating keys to values.
    ///
    /// A value set is stored as a `SmallVec`.
    /// This is an optimization over a `Vec<_>`
    /// as we allow a single element to be stored inline
    /// to improve performance for the common case of one element.
    map: BTreeMap<K, SameKeyEntry>,
    /// The memoized number of rows indexed in `self.map`.
    num_rows: usize,
    /// Storage for [`Index::num_key_bytes`].
    num_key_bytes: u64,
}

impl<K> Default for BTreeIndex<K> {
    fn default() -> Self {
        Self {
            map: <_>::default(),
            num_rows: <_>::default(),
            num_key_bytes: <_>::default(),
        }
    }
}

impl<K: MemoryUsage> MemoryUsage for BTreeIndex<K> {
    fn heap_usage(&self) -> usize {
        let Self {
            map,
            num_rows,
            num_key_bytes,
        } = self;
        map.heap_usage() + num_rows.heap_usage() + num_key_bytes.heap_usage()
    }
}

impl<K: Ord + KeySize> Index for BTreeIndex<K> {
    type Key = K;

    fn clone_structure(&self) -> Self {
        <_>::default()
    }

    /// Inserts the relation `key -> ptr` to this multimap.
    ///
    /// The map does not check whether `key -> ptr` was already in the map.
    /// It's assumed that the same `ptr` is never added twice,
    /// and multimaps do not bind one `key` to the same `ptr`.
    fn insert(&mut self, key: Self::Key, ptr: RowPointer) -> Result<(), RowPointer> {
        self.num_rows += 1;
        self.num_key_bytes.add_to_key_bytes(&key);
        self.map.entry(key).or_default().push(ptr);
        Ok(())
    }

    /// Deletes `key -> ptr` from this multimap.
    ///
    /// Returns whether `key -> ptr` was present.
    fn delete(&mut self, key: &K, ptr: RowPointer) -> bool {
        self.delete(key, ptr)
    }

    type PointIter<'a>
        = SameKeyEntryIter<'a>
    where
        Self: 'a;

    fn seek_point(&self, point: &Self::Key) -> Self::PointIter<'_> {
        self.seek_point(point)
    }

    type Iter<'a>
        = BTreeIndexIter<'a, K>
    where
        Self: 'a;

    fn iter(&self) -> Self::Iter<'_> {
        BTreeIndexIter::new(self.map.values())
    }

    fn num_keys(&self) -> usize {
        self.map.len()
    }

    fn num_key_bytes(&self) -> u64 {
        self.num_key_bytes
    }

    fn num_rows(&self) -> usize {
        self.num_rows
    }

    /// Deletes all entries from the multimap, leaving it empty.
    /// This will not deallocate the outer map.
    fn clear(&mut self) {
        self.map.clear();
        self.num_rows = 0;
        self.num_key_bytes.reset_to_zero();
    }

    fn can_merge(&self, _: &Self, _: impl Fn(&RowPointer) -> bool) -> Result<(), RowPointer> {
        // `self.insert` always returns `Ok(_)`.
        Ok(())
    }

    const IS_RANGED: bool = true;
}

impl<K: KeySize + Ord> BTreeIndex<K> {
    /// See [`Index::delete`].
    ///
    /// This version has relaxed bounds
    /// where relaxed means that the key type can be borrowed from the index's key type
    /// and need not be `Index::Key` itself.
    /// This allows e.g., queries with `&str` rather than providing an owned string key.
    /// This can be exploited to avoid heap alloctions in some situations,
    /// e.g., borrowing the input directly from BSATN.
    /// This is similar to the bounds on [`BTreeMap::remove`].
    pub fn delete<Q>(&mut self, key: &Q, ptr: RowPointer) -> bool
    where
        Q: ?Sized + KeySize + Ord,
        <Self as Index>::Key: Borrow<Q>,
    {
        let Some(vset) = self.map.get_mut(key) else {
            return false;
        };

        let (deleted, is_empty) = vset.delete(ptr);

        if is_empty {
            self.map.remove(key);
        }

        if deleted {
            self.num_rows -= 1;
            self.num_key_bytes.sub_from_key_bytes(key);
        }

        deleted
    }

    /// See [`Index::seek_point`].
    ///
    /// This version has relaxed bounds
    /// where relaxed means that the key type can be borrowed from the index's key type
    /// and need not be `Index::Key` itself.
    /// This allows e.g., queries with `&str` rather than providing an owned string key.
    /// This can be exploited to avoid heap alloctions in some situations,
    /// e.g., borrowing the input directly from BSATN.
    /// This is similar to the bounds on [`BTreeMap::get`].
    pub fn seek_point<Q>(&self, point: &Q) -> <Self as Index>::PointIter<'_>
    where
        Q: ?Sized + Ord,
        <Self as Index>::Key: Borrow<Q>,
    {
        same_key_iter(self.map.get(point))
    }
}

impl<K: Ord + KeySize> RangedIndex for BTreeIndex<K> {
    type RangeIter<'a>
        = BTreeIndexRangeIter<'a, K>
    where
        Self: 'a;

    /// Returns an iterator over the multimap that yields all the `V`s
    /// of the `K`s that fall within the specified `range`.
    fn seek_range(&self, range: &impl RangeBounds<Self::Key>) -> Self::RangeIter<'_> {
        self.seek_range(range)
    }
}

impl<K: KeySize + Ord> BTreeIndex<K> {
    /// See [`RangedIndex::seek_range`].
    ///
    /// This version has relaxed bounds
    /// where relaxed means that the key type can be borrowed from the index's key type
    /// and need not be `Index::Key` itself.
    /// This allows e.g., queries with `&str` rather than providing an owned string key.
    /// This can be exploited to avoid heap alloctions in some situations,
    /// e.g., borrowing the input directly from BSATN.
    /// This is similar to the bounds on [`BTreeMap::range`].
    pub fn seek_range<Q: ?Sized + Ord>(&self, range: &impl RangeBounds<Q>) -> <Self as RangedIndex>::RangeIter<'_>
    where
        <Self as Index>::Key: Borrow<Q>,
    {
        BTreeIndexRangeIter::new(RangeValues(self.map.range((range.start_bound(), range.end_bound()))))
    }
}

/// An iterator over all the values in a [`BTreeIndex`].
pub type BTreeIndexIter<'a, K> = ManySameKeyEntryIter<'a, Values<'a, K, SameKeyEntry>>;

/// An iterator over values in a [`BTreeIndex`] where the keys are in a certain range.
pub type BTreeIndexRangeIter<'a, K> = ManySameKeyEntryIter<'a, RangeValues<'a, K, SameKeyEntry>>;

/// An iterator over a key range in a [`BTreeMap`] providing only the values.
pub struct RangeValues<'a, K, V>(Range<'a, K, V>);

impl<K, V> Clone for RangeValues<'_, K, V> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<'a, K, V> Iterator for RangeValues<'a, K, V> {
    type Item = &'a V;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(_, v)| v)
    }
}
