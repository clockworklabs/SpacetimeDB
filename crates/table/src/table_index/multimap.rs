use super::same_key_entry::{same_key_iter, SameKeyEntry, SameKeyEntryIter};
use super::{key_size::KeyBytesStorage, Index, KeySize, RangedIndex};
use crate::indexes::RowPointer;
use core::ops::RangeBounds;
use spacetimedb_sats::memory_usage::MemoryUsage;
use std::collections::btree_map::{BTreeMap, Range};

/// A multi map that relates a `K` to a *set* of `RowPointer`s.
#[derive(Debug, PartialEq, Eq)]
pub struct MultiMap<K> {
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

impl<K> Default for MultiMap<K> {
    fn default() -> Self {
        Self {
            map: <_>::default(),
            num_rows: <_>::default(),
            num_key_bytes: <_>::default(),
        }
    }
}

impl<K: MemoryUsage> MemoryUsage for MultiMap<K> {
    fn heap_usage(&self) -> usize {
        let Self {
            map,
            num_rows,
            num_key_bytes,
        } = self;
        map.heap_usage() + num_rows.heap_usage() + num_key_bytes.heap_usage()
    }
}

impl<K: Ord + KeySize> Index for MultiMap<K> {
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
        self.num_key_bytes.add_to_key_bytes::<Self>(&key);
        self.map.entry(key).or_default().push(ptr);
        Ok(())
    }

    /// Deletes `key -> ptr` from this multimap.
    ///
    /// Returns whether `key -> ptr` was present.
    fn delete(&mut self, key: &K, ptr: RowPointer) -> bool {
        let Some(vset) = self.map.get_mut(key) else {
            return false;
        };

        let (deleted, is_empty) = vset.delete(ptr);

        if is_empty {
            self.map.remove(key);
        }

        if deleted {
            self.num_rows -= 1;
            self.num_key_bytes.sub_from_key_bytes::<Self>(key);
        }

        deleted
    }

    type PointIter<'a>
        = SameKeyEntryIter<'a>
    where
        Self: 'a;

    fn seek_point(&self, key: &Self::Key) -> Self::PointIter<'_> {
        same_key_iter(self.map.get(key))
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

impl<K: Ord + KeySize> RangedIndex for MultiMap<K> {
    type RangeIter<'a>
        = MultiMapRangeIter<'a, K>
    where
        Self: 'a;

    /// Returns an iterator over the multimap that yields all the `V`s
    /// of the `K`s that fall within the specified `range`.
    fn seek_range(&self, range: &impl RangeBounds<Self::Key>) -> Self::RangeIter<'_> {
        MultiMapRangeIter {
            outer: self.map.range((range.start_bound(), range.end_bound())),
            inner: SameKeyEntry::empty_iter(),
        }
    }
}

/// An iterator over values in a [`MultiMap`] where the keys are in a certain range.
#[derive(Clone)]
pub struct MultiMapRangeIter<'a, K> {
    /// The outer iterator seeking for matching keys in the range.
    outer: Range<'a, K, SameKeyEntry>,
    /// The inner iterator for the value set for a found key.
    inner: SameKeyEntryIter<'a>,
}

impl<K> Iterator for MultiMapRangeIter<'_, K> {
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
