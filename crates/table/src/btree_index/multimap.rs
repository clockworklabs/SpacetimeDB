use core::ops::RangeBounds;
use core::slice;
use smallvec::SmallVec;
use std::collections::btree_map::{BTreeMap, Range};

use crate::MemoryUsage;

/// A multi map that relates a `K` to a *set* of `V`s.
#[derive(Default)]
pub struct MultiMap<K, V> {
    /// The map is backed by a `BTreeMap` for relating keys to values.
    ///
    /// A value set is stored as a *sorted* `SmallVec`.
    /// This is an optimization over a sorted `Vec<_>`
    /// as we allow a single element to be stored inline
    /// to improve performance for the common case of one element.
    map: BTreeMap<K, SmallVec<[V; 1]>>,
}

impl<K: MemoryUsage, V: MemoryUsage> MemoryUsage for MultiMap<K, V> {
    fn heap_usage(&self) -> usize {
        let Self { map } = self;
        map.heap_usage()
    }
}

impl<K: Ord, V: Ord> MultiMap<K, V> {
    /// Returns an empty multi map.
    pub fn new() -> Self {
        Self { map: BTreeMap::new() }
    }

    /// Inserts the relation `key -> val` to this multimap.
    ///
    /// The map does not check whether `key -> val` was already in the map.
    pub fn insert(&mut self, key: K, val: V) {
        self.map.entry(key).or_default().push(val);
    }

    /// Inserts the relation `key -> val` to this multimap.
    ///
    /// Returns back the value if the `key` was already present in the map.
    pub fn insert_unique(&mut self, key: K, val: V) -> Option<&V> {
        // TODO(perf, centril): don't use a multimap at all for unique indices.
        let vals = self.map.entry(key).or_default();
        if vals.is_empty() {
            vals.push(val);
            None
        } else {
            Some(&vals[0])
        }
    }

    /// Deletes `key -> val` from this multimap.
    ///
    /// Returns whether `key -> val` was present.
    pub fn delete(&mut self, key: &K, val: &V) -> bool {
        if let Some(vset) = self.map.get_mut(key) {
            // The `vset` is not sorted, so we have to do a linear scan first.
            if let Some(idx) = vset.iter().position(|v| v == val) {
                vset.swap_remove(idx);
                return true;
            }
        }
        false
    }

    /// Returns an iterator over the multimap that yields all the `V`s
    /// of the `K`s that fall within the specified `range`.
    pub fn values_in_range(&self, range: &impl RangeBounds<K>) -> MultiMapRangeIter<'_, K, V> {
        MultiMapRangeIter {
            outer: self.map.range((range.start_bound(), range.end_bound())),
            inner: None,
        }
    }

    /// Returns the number of unique keys in the multimap.
    pub fn num_keys(&self) -> usize {
        self.map.len()
    }

    /// Returns the total number of entries in the multimap.
    #[allow(unused)] // No use for this currently.
    pub fn len(&self) -> usize {
        self.map.values().map(|ptrs| ptrs.len()).sum()
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
pub struct MultiMapRangeIter<'a, K, V> {
    /// The outer iterator seeking for matching keys in the range.
    outer: Range<'a, K, SmallVec<[V; 1]>>,
    /// The inner iterator for the value set for a found key.
    inner: Option<slice::Iter<'a, V>>,
}

impl<'a, K, V> Iterator for MultiMapRangeIter<'a, K, V> {
    type Item = &'a V;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(inner) = self.inner.as_mut() {
                if let Some(val) = inner.next() {
                    // While the inner iterator has elements, yield them.
                    return Some(val);
                }
            }

            // This makes the iterator fused.
            self.inner = None;
            // Advance and get a new inner, if possible, or quit.
            // We'll come back and yield elements from it in the next iteration.
            let (_, next) = self.outer.next()?;
            self.inner = Some(next.iter());
        }
    }
}
