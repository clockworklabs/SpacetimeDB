use crate::MemoryUsage;
use core::{ops::RangeBounds, option::IntoIter};
use std::collections::btree_map::{BTreeMap, Entry, Range};

/// A "unique map" that relates a `K` to a `V`.
///
/// (This is just a `BTreeMap<K, V>`) with a slightly modified interface.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct UniqueMap<K, V> {
    /// The map is backed by a `BTreeMap` for relating a key to a value.
    map: BTreeMap<K, V>,
}

impl<K, V> Default for UniqueMap<K, V> {
    fn default() -> Self {
        Self { map: BTreeMap::new() }
    }
}

impl<K: MemoryUsage, V: MemoryUsage> MemoryUsage for UniqueMap<K, V> {
    fn heap_usage(&self) -> usize {
        let Self { map } = self;
        map.heap_usage()
    }
}

impl<K: Ord, V: Ord> UniqueMap<K, V> {
    /// Inserts the relation `key -> val` to this map.
    ///
    /// If `key` was already present in the map, does not add an association with `val`.
    /// Returns the existing associated value instead.
    pub fn insert(&mut self, key: K, val: V) -> Result<(), &V> {
        match self.map.entry(key) {
            Entry::Vacant(e) => {
                e.insert(val);
                Ok(())
            }
            Entry::Occupied(e) => Err(e.into_mut()),
        }
    }

    /// Deletes `key` from this map.
    ///
    /// Returns whether `key` was present.
    pub fn delete(&mut self, key: &K) -> bool {
        self.map.remove(key).is_some()
    }

    /// Returns an iterator over the map that yields all the `V`s
    /// of the `K`s that fall within the specified `range`.
    pub fn values_in_range(&self, range: &impl RangeBounds<K>) -> UniqueMapRangeIter<'_, K, V> {
        UniqueMapRangeIter {
            iter: self.map.range((range.start_bound(), range.end_bound())),
        }
    }

    /// Returns an iterator over the map that yields the potential `V` of the `key: &K`.
    pub fn values_in_point(&self, key: &K) -> UniqueMapPointIter<'_, V> {
        let iter = self.map.get(key).into_iter();
        UniqueMapPointIter { iter }
    }

    /// Returns the number of unique keys in the map.
    pub fn num_keys(&self) -> usize {
        self.len()
    }

    /// Returns the total number of entries in the map.s
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Returns whether there are any entries in the map.
    #[allow(unused)] // No use for this currently.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Deletes all entries from the map, leaving it empty.
    /// This will not deallocate the outer map.
    pub fn clear(&mut self) {
        self.map.clear();
    }

    /// Returns whether `other` can be merged into `self`
    /// with an error containing the element in `self` that caused the violation.
    pub(crate) fn can_merge(&self, other: &UniqueMap<K, V>) -> Result<(), &V> {
        let Some(found) = other.map.keys().find_map(|key| self.map.get(key)) else {
            return Ok(());
        };
        Err(found)
    }
}

/// An iterator over the potential value in a [`UniqueMap`] for a given key.
pub struct UniqueMapPointIter<'a, V> {
    /// The iterator seeking for matching keys in the range.
    iter: IntoIter<&'a V>,
}

impl<'a, V> Iterator for UniqueMapPointIter<'a, V> {
    type Item = &'a V;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

/// An iterator over values in a [`UniqueMap`] where the keys are in a certain range.
pub struct UniqueMapRangeIter<'a, K, V> {
    /// The iterator seeking for matching keys in the range.
    iter: Range<'a, K, V>,
}

impl<'a, K, V> Iterator for UniqueMapRangeIter<'a, K, V> {
    type Item = &'a V;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|(_, v)| v)
    }
}
