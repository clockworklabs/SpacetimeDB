use super::same_key_entry::{same_key_iter, SameKeyEntry, SameKeyEntryIter};
use super::{key_size::KeyBytesStorage, Index, KeySize, RangedIndex};
use crate::indexes::RowPointer;
use core::borrow::Borrow;
use core::ops::RangeBounds;
use spacetimedb_sats::memory_usage::MemoryUsage;
use std::collections::btree_map::{BTreeMap, Range};

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
        BTreeIndexRangeIter {
            outer: self.map.range((range.start_bound(), range.end_bound())),
            inner: SameKeyEntry::empty_iter(),
        }
    }
}

impl<K: Ord + KeySize> BTreeIndex<K> {
    /// Returns an iterator over keys that have more than one row pointer,
    /// yielding `(&key, count)` for each duplicate key.
    pub(super) fn iter_duplicates(&self) -> impl Iterator<Item = (&K, usize)> {
        self.map.iter().filter_map(|(k, entry)| {
            let count = entry.count();
            if count > 1 {
                Some((k, count))
            } else {
                None
            }
        })
    }

    /// Check for duplicates and, if none, convert into a `BTreeMap<K, RowPointer>`.
    ///
    /// Returns `Ok(map)` if every key maps to exactly one row.
    /// Returns `Err((self, ptr))` with a witness `RowPointer` of a duplicate if any key
    /// maps to more than one row. The original `BTreeIndex` is returned intact on error.
    pub(super) fn check_and_into_unique(self) -> Result<BTreeMap<K, RowPointer>, (Self, RowPointer)> {
        // First pass: check for duplicates (borrows self.map immutably).
        let dup = self
            .map
            .values()
            .find_map(|entry| {
                if entry.count() > 1 {
                    Some(entry.iter().next().unwrap())
                } else {
                    None
                }
            });

        if let Some(ptr) = dup {
            return Err((self, ptr));
        }

        // No duplicates; conversion is infallible.
        let result = self
            .map
            .into_iter()
            .map(|(k, entry)| {
                let ptr = entry.iter().next().unwrap();
                (k, ptr)
            })
            .collect();

        Ok(result)
    }
}

/// An iterator over values in a [`BTreeIndex`] where the keys are in a certain range.
#[derive(Clone)]
pub struct BTreeIndexRangeIter<'a, K> {
    /// The outer iterator seeking for matching keys in the range.
    outer: Range<'a, K, SameKeyEntry>,
    /// The inner iterator for the value set for a found key.
    inner: SameKeyEntryIter<'a>,
}

impl<K> Iterator for BTreeIndexRangeIter<'_, K> {
    type Item = RowPointer;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // While the inner iterator has elements, yield them.
            if let Some(val) = self.inner.next() {
                return Some(val);
            }
            // Advance and get a new inner, if possible, or quit.
            // We'll come back and yield elements from it in the next iteration.
            let inner = self.outer.next().map(|(_, i)| i)?;
            self.inner = inner.iter();
        }
    }
}
