use super::same_key_entry::{same_key_iter, SameKeyEntry, SameKeyEntryIter};
use core::{hash::Hash, ops::RangeBounds};
use spacetimedb_sats::memory_usage::MemoryUsage;
use std::collections::btree_map::{BTreeMap, Range};

/// A multi map that relates a `K` to a *set* of `V`s.
#[derive(Debug, PartialEq, Eq)]
pub struct MultiMap<K, V: Eq + Hash> {
    /// The map is backed by a `BTreeMap` for relating keys to values.
    ///
    /// A value set is stored as a `SmallVec`.
    /// This is an optimization over a `Vec<_>`
    /// as we allow a single element to be stored inline
    /// to improve performance for the common case of one element.
    map: BTreeMap<K, SameKeyEntry<V>>,
}

impl<K, V: Eq + Hash> Default for MultiMap<K, V> {
    fn default() -> Self {
        Self { map: BTreeMap::new() }
    }
}

impl<K: MemoryUsage, V: MemoryUsage + Eq + Hash> MemoryUsage for MultiMap<K, V> {
    fn heap_usage(&self) -> usize {
        let Self { map } = self;
        map.heap_usage()
    }
}

impl<K: Ord, V: Ord + Hash> MultiMap<K, V> {
    /// Inserts the relation `key -> val` to this multimap.
    ///
    /// The map does not check whether `key -> val` was already in the map.
    /// It's assumed that the same `val` is never added twice,
    /// and multimaps do not bind one `key` to the same `val`.
    pub fn insert(&mut self, key: K, val: V) {
        self.map.entry(key).or_default().push(val);
    }

    /// Deletes `key -> val` from this multimap.
    ///
    /// Returns whether `key -> val` was present.
    pub fn delete(&mut self, key: &K, val: &V) -> bool {
        let Some(vset) = self.map.get_mut(key) else {
            return false;
        };

        let (deleted, is_empty) = vset.delete(val);

        if is_empty {
            self.map.remove(key);
        }

        deleted
    }

    /// Returns an iterator over the multimap that yields all the `V`s
    /// of the `K`s that fall within the specified `range`.
    pub fn values_in_range(&self, range: &impl RangeBounds<K>) -> MultiMapRangeIter<'_, K, V> {
        MultiMapRangeIter {
            outer: self.map.range((range.start_bound(), range.end_bound())),
            inner: SameKeyEntry::empty_iter(),
        }
    }

    /// Returns an iterator over the multimap that yields all the `V`s of the `key: &K`.
    pub fn values_in_point(&self, key: &K) -> SameKeyEntryIter<'_, V> {
        same_key_iter(self.map.get(key))
    }

    /// Returns the number of unique keys in the multimap.
    pub fn num_keys(&self) -> usize {
        self.map.len()
    }

    /// Returns the total number of entries in the multimap.
    #[allow(unused)] // No use for this currently.
    pub fn len(&self) -> usize {
        self.map.values().map(|vals: &SameKeyEntry<V>| vals.len()).sum()
    }

    /// Returns whether there are any entries in the multimap.
    #[allow(unused)] // No use for this currently.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Deletes all entries from the multimap, leaving it empty.
    /// This will not deallocate the outer map.
    pub fn clear(&mut self) {
        self.map.clear();
    }
}

/// An iterator over values in a [`MultiMap`] where the keys are in a certain range.
pub struct MultiMapRangeIter<'a, K, V: Eq + Hash> {
    /// The outer iterator seeking for matching keys in the range.
    outer: Range<'a, K, SameKeyEntry<V>>,
    /// The inner iterator for the value set for a found key.
    inner: SameKeyEntryIter<'a, V>,
}

impl<'a, K, V: Eq + Hash> Iterator for MultiMapRangeIter<'a, K, V> {
    type Item = &'a V;

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
