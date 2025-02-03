use crate::MemoryUsage;
use core::ops::RangeBounds;
use core::option::IntoIter;
use core::{hash::Hash, ops::Bound};
use foldhash::fast::RandomState;
use spacetimedb_data_structures::map::{Entry, HashMap};

/// A "unique map" that relates a `K` to a `V`.
///
/// (This is just a `HashMap<K, V>`) with a slightly modified interface.
#[derive(Debug, Clone)]
pub struct UniqueMap<K, V> {
    /// The map is backed by a `HashMap` for relating a key to a value.
    map: HashMap<K, V, RandomState>,
}

impl<K, V> Default for UniqueMap<K, V> {
    fn default() -> Self {
        Self { map: HashMap::new() }
    }
}

impl<K: MemoryUsage + Eq + Hash, V: MemoryUsage> MemoryUsage for UniqueMap<K, V> {
    fn heap_usage(&self) -> usize {
        let Self { map } = self;
        map.heap_usage()
    }
}

impl<K: Eq + Hash, V: PartialEq> Eq for UniqueMap<K, V> {}
impl<K: Eq + Hash, V: PartialEq> PartialEq for UniqueMap<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.map.eq(&other.map)
    }
}

impl<K: Eq + Hash, V> UniqueMap<K, V> {
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
    pub fn values_in_range(&self, range: &impl RangeBounds<K>) -> UniqueMapIter<'_, V> {
        let Bound::Included(key) = range.start_bound() else {
            unreachable!();
        };
        let iter = self.map.get(key).into_iter();
        UniqueMapIter { iter }
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
}

/// An iterator over value in [`UniqueMap`] where the key matches exactly.
pub struct UniqueMapIter<'a, V> {
    /// The iterator seeking for the matching key.
    iter: IntoIter<&'a V>,
}

impl<'a, V> Iterator for UniqueMapIter<'a, V> {
    type Item = &'a V;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}
