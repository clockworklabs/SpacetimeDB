use core::ops::RangeBounds;
use core::slice;
use smallvec::SmallVec;
use std::collections::btree_map::{BTreeMap, Range};

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

impl<K: Ord, V: Ord> MultiMap<K, V> {
    /// Returns an empty multi map.
    pub fn new() -> Self {
        Self { map: BTreeMap::new() }
    }

    /// Inserts the relation `key -> val` to this multimap.
    ///
    /// Returns false if `key -> val` was already in the map.
    pub fn insert(&mut self, key: K, val: V) -> bool {
        let vset = self.map.entry(key).or_default();
        // Use binary search to maintain the sort order.
        // This is used to determine in `O(log(vset.len()))` whether `val` was already present.
        let Err(idx) = vset.binary_search(&val) else {
            return false;
        };
        vset.insert(idx, val);
        true
    }

    /// Deletes `key -> val` from this multimap.
    ///
    /// Returns whether `key -> val` was present.
    pub fn delete(&mut self, key: &K, val: &V) -> bool {
        if let Some(vset) = self.map.get_mut(key) {
            // The `vset` is sorted so we can binary search.
            if let Ok(idx) = vset.binary_search(val) {
                // Maintain the sorted order. Don't use `swap_remove`!
                vset.remove(idx);
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
