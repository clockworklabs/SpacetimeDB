use crate::map::{HashCollectionExt as _, HashMap};
use core::hash::Hash;
use core::mem;
use either::Either;
use smallvec::SmallVec;

/// A hash map optimized for small sizes,
/// with a small size optimization for up to `N` entries
/// and then a vector for up to `M`
/// and then finally falling back to a real hash map after `M`.
///
/// The inline and heap based vectors use linear scans,
/// which is faster than hashing as long as the map is small.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SmallHashMap<K: Eq + Hash, V, const N: usize, const M: usize> {
    Small(SmallVec<[(K, V); N]>),
    Large(HashMap<K, V>),
}

#[cfg(feature = "memory-usage")]
impl<
        K: Eq + Hash + spacetimedb_memory_usage::MemoryUsage,
        V: spacetimedb_memory_usage::MemoryUsage,
        const N: usize,
        const M: usize,
    > spacetimedb_memory_usage::MemoryUsage for SmallHashMap<K, V, N, M>
{
    fn heap_usage(&self) -> usize {
        match self {
            SmallHashMap::Small(vec) => vec.heap_usage(),
            SmallHashMap::Large(map) => map.heap_usage(),
        }
    }
}

impl<K: Eq + Hash, V, const N: usize, const M: usize> Default for SmallHashMap<K, V, N, M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Eq + Hash, V, const N: usize, const M: usize> SmallHashMap<K, V, N, M> {
    pub fn new() -> Self {
        Self::Small(SmallVec::new())
    }

    /// Inserts the association `key -> value` into the map,
    /// returning the previous value for `key`, if any.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        // Possibly convert to large map first.
        self.maybe_convert_to_large();

        match self {
            Self::Small(list) => {
                if let Some(idx) = Self::key_pos(list, &key) {
                    // SAFETY: `idx` was given by `key_pos`, so it must be in-bounds.
                    let (_, val) = unsafe { list.get_unchecked_mut(idx) };
                    return Some(mem::replace(val, value));
                }

                list.push((key, value));
                None
            }
            Self::Large(map) => map.insert(key, value),
        }
    }

    /// Returns either the existing value for `key`
    /// or inserts into `key` using `or_insert`.
    pub fn get_or_insert(&mut self, key: K, or_insert: impl FnOnce() -> V) -> &mut V {
        // Possibly convert to large map first.
        self.maybe_convert_to_large();

        match self {
            Self::Small(list) => {
                if let Some(idx) = Self::key_pos(list, &key) {
                    // SAFETY: `idx` was given by `key_pos`, so it must be in-bounds.
                    let (_, val) = unsafe { list.get_unchecked_mut(idx) };
                    return val;
                }

                list.push((key, or_insert()));
                let last = list.last_mut();
                // SAFETY: just inserted one element so `list` cannot be empty.
                let (_, val) = unsafe { last.unwrap_unchecked() };
                val
            }
            Self::Large(map) => map.entry(key).or_insert_with(or_insert),
        }
    }

    #[inline]
    fn maybe_convert_to_large(&mut self) {
        if let Self::Small(list) = self {
            if list.len() > M {
                let list = mem::take(list);
                self.convert_to_large(list);
            }
        }
    }

    #[cold]
    #[inline(never)]
    fn convert_to_large(&mut self, list: SmallVec<[(K, V); N]>) {
        let mut map = HashMap::with_capacity(list.len());
        map.extend(list);
        *self = Self::Large(map);
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        match self {
            Self::Small(list) => Self::key_pos(list, key).map(|idx| list.swap_remove(idx).1),
            Self::Large(map) => map.remove(key),
        }
    }

    /// Returns the position of `key` in `list`, if any.
    fn key_pos(list: &[(K, V)], key: &K) -> Option<usize> {
        list.iter().position(|(k, _)| k == key)
    }

    /// Clears all entries from the map.
    pub fn clear(&mut self) {
        match self {
            Self::Small(list) => list.clear(),
            Self::Large(map) => map.clear(),
        }
    }

    /// Returns the number of entries in the map.
    pub fn len(&self) -> usize {
        match self {
            Self::Small(list) => list.len(),
            Self::Large(map) => map.len(),
        }
    }

    /// Returns whether the map is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns an iterator over all the key-value pairs in the map.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&K, &V)> {
        match self {
            Self::Small(list) => Either::Left(list.iter().map(|(k, v)| (k, v))),
            Self::Large(map) => Either::Right(map.iter()),
        }
    }

    /// Returns an iterator over all the keys in the map.
    pub fn keys(&self) -> impl ExactSizeIterator<Item = &K> {
        match self {
            Self::Small(list) => Either::Left(list.iter().map(|(k, _)| k)),
            Self::Large(map) => Either::Right(map.keys()),
        }
    }

    /// Returns an iterator over all the values in the map.
    pub fn values(&self) -> impl ExactSizeIterator<Item = &V> {
        match self {
            Self::Small(list) => Either::Left(list.iter().map(|(_, v)| v)),
            Self::Large(map) => Either::Right(map.values()),
        }
    }

    /// Returns whether `key` is in the map.
    pub fn contains_key(&self, key: &K) -> bool {
        match self {
            Self::Small(list) => list.iter().any(|(k, _)| k == key),
            Self::Large(map) => map.contains_key(key),
        }
    }

    /// Returns the value for `key`, if any.
    pub fn get(&self, key: &K) -> Option<&V> {
        match self {
            Self::Small(list) => list.iter().find_map(|(k, v)| (k == key).then_some(v)),
            Self::Large(map) => map.get(key),
        }
    }

    /// Returns the value for `key`, mutably, if any.
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        match self {
            Self::Small(list) => list.iter_mut().find_map(|(k, v)| (k == key).then_some(v)),
            Self::Large(map) => map.get_mut(key),
        }
    }
}

impl<K: Eq + Hash, V, const N: usize, const M: usize> Extend<(K, V)> for SmallHashMap<K, V, N, M> {
    fn extend<T: IntoIterator<Item = (K, V)>>(&mut self, iter: T) {
        for (k, v) in iter {
            self.insert(k, v);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::SmallHashMap;
    use proptest::collection::hash_set;
    use proptest::prelude::*;

    type K = u32;
    type V = u32;
    type Map<const N: usize, const M: usize> = SmallHashMap<K, V, N, M>;

    /// Asserts that the map behaves consistently as an empty map.
    fn assert_empty<const N: usize, const M: usize>(map: &mut Map<N, { M }>, key: &K, val: V) {
        assert!(map.is_empty());
        assert_eq!(map.len(), 0);

        assert_eq!(map.iter().count(), 0);
        assert_eq!(map.keys().count(), 0);
        assert_eq!(map.values().count(), 0);

        assert_eq!(Map::<N, M>::key_pos(&[], key), None);
        assert!(!map.contains_key(key));
        assert_eq!(map.get(key), None);
        assert_eq!(map.get_mut(key), None);

        assert_eq!(map.remove(key), None);
        assert_eq!(map.insert(*key, val), None);
    }

    /// Asserts that the map behaves consistently as a non-empty map.
    fn assert_not_empty<const N: usize, const M: usize>(map: &mut Map<N, M>, len: usize) {
        assert!(!map.is_empty());
        assert_eq!(map.len(), len);

        assert_eq!(map.iter().count(), len);
        assert_eq!(map.keys().count(), len);
        assert_eq!(map.values().count(), len);
    }

    /// Extends the map with entries and then clears it,
    /// asserting correct behavior before and after.
    fn extend_clear<const N: usize, const M: usize>(map: &mut Map<N, M>, entries: &HashSet<(K, V)>) {
        map.extend(entries.iter().cloned());
        assert_not_empty(map, entries.len());

        map.clear();
        assert_empty(map, &0, 0);
    }

    /// Asserts that the map contains `key` with value `val`.
    fn assert_key_eq<const N: usize, const M: usize>(map: &mut Map<N, M>, key: K, mut val: V) {
        assert!(map.contains_key(&key));
        assert_eq!(map.get(&key), Some(&val));
        assert_eq!(map.get_mut(&key), Some(&mut val));
    }

    /// Asserts that the map does not contain `key`.
    fn assert_key_none<const N: usize, const M: usize>(map: &mut Map<N, M>, key: K) {
        assert!(!map.contains_key(&key));
        assert_eq!(map.get(&key), None);
        assert_eq!(map.get_mut(&key), None);
    }

    /// Inserting `key` twice returns the old value.
    fn insert_returns_old_inner<const N: usize, const M: usize>(key: K, val1: V, val2: V, val3: V) {
        let mut map = Map::<N, M>::new();

        assert_key_none(&mut map, key);

        assert_eq!(map.insert(key, val1), None);
        assert_key_eq(&mut map, key, val1);

        assert_eq!(map.insert(key, val2), Some(val1));
        assert_key_eq(&mut map, key, val2);

        assert_eq!(map.get_or_insert(key, || val3), &val2);
    }

    /// Mutating the value via `get_mut` has effect.
    fn mutation_via_get_mut_inner<const N: usize, const M: usize>(key: K, val1: V, val2: V) {
        let mut map = Map::<N, M>::new();

        assert_eq!(map.insert(key, val1), None);
        if let Some(slot) = map.get_mut(&key) {
            *slot = val2;
        }

        assert_key_eq(&mut map, key, val2);
    }

    /// Mutating the value via `get_or_insert` has effect.
    fn mutation_via_get_or_insert_inner<const N: usize, const M: usize>(key: K, val1: V, val2: V) {
        let mut map = Map::<N, M>::new();

        assert_eq!(map.insert(key, val1), None);
        let slot = map.get_or_insert(key, || val2);
        *slot = val2;

        assert_eq!(map.get(&key), Some(&val2));
    }

    /// Collects `iter` into a sorted `Vec<T>`.
    fn sorted<T: Ord>(iter: impl Iterator<Item = T>) -> Vec<T> {
        let mut vec: Vec<T> = iter.collect();
        vec.sort();
        vec
    }

    /// Tests insertion, retrieval, and deletion together.
    fn insert_get_remove_inner<const N: usize, const M: usize>(entries: &[(K, V)]) {
        let mut map = Map::<N, M>::new();

        // Initially all keys are absent.
        for (k, _) in entries.iter() {
            assert_key_none(&mut map, *k);
        }

        // Insert all entries.
        for (k, v) in entries.iter() {
            map.insert(*k, *v);
        }

        // Now all keys are present.
        assert_not_empty(&mut map, entries.len());
        for (k, v) in entries.iter().cloned() {
            assert_key_eq(&mut map, k, v);
        }

        // Iterators return all entries.
        assert_eq!(
            sorted(map.iter().map(|(k, v)| (*k, *v))),
            sorted(entries.iter().cloned())
        );
        assert_eq!(sorted(map.keys().cloned()), sorted(entries.iter().map(|(k, _)| *k)));
        assert_eq!(sorted(map.values().cloned()), sorted(entries.iter().map(|(_, v)| *v)));

        // Removal results in absence.
        for (k, _) in entries.iter() {
            assert!(map.remove(k).is_some());
            assert_eq!(map.get(k), None);
        }

        // Finally the map is empty again.
        assert!(map.is_empty());
    }

    #[test]
    fn new_is_same_as_default() {
        assert_eq!(Map::<8, 16>::new(), <_>::default());
        assert_eq!(Map::<0, 16>::new(), <_>::default());
        assert_eq!(Map::<0, 0>::new(), <_>::default());
    }

    proptest! {
        #[test]
        fn new_is_empty(key: K, val: V) {
            assert_empty(&mut Map::<4, 8>::new(), &key, val);
            assert_empty(&mut Map::<0, 8>::new(), &key, val);
            assert_empty(&mut Map::<0, 0>::new(), &key, val);
        }

        #[test]
        fn cleared_is_empty(entries in hash_set(any::<(K, V)>(), 1..50)) {
            extend_clear(&mut Map::<4, 8>::new(), &entries);
            extend_clear(&mut Map::<0, 8>::new(), &entries);
            extend_clear(&mut Map::<0, 0>::new(), &entries);
        }

        #[test]
        fn insert_returns_old(key: K, val1: V, val2: V) {
            insert_returns_old_inner::<4, 8>(key, val1, val2, val2);
            insert_returns_old_inner::<0, 8>(key, val1, val2, val2);
            insert_returns_old_inner::<0, 0>(key, val1, val2, val2);
        }

        #[test]
        fn mutation_via_get_mut(key: K, val1: V, val2: V) {
            mutation_via_get_mut_inner::<4, 8>(key, val1, val2);
            mutation_via_get_mut_inner::<0, 8>(key, val1, val2);
            mutation_via_get_mut_inner::<0, 0>(key, val1, val2);
        }

        #[test]
        fn mutation_via_get_or_insert(key: K, val1: V, val2: V) {
            mutation_via_get_or_insert_inner::<4, 8>(key, val1, val2);
            mutation_via_get_or_insert_inner::<0, 8>(key, val1, val2);
            mutation_via_get_or_insert_inner::<0, 0>(key, val1, val2);
        }

        #[test]
        fn insert_get_remove(entries in hash_set(any::<(K, V)>(), 1..50)) {
            let entries: Vec<(K, V)> = entries.into_iter().collect();
            insert_get_remove_inner::<4, 8>(&entries);
            insert_get_remove_inner::<0, 8>(&entries);
            insert_get_remove_inner::<0, 0>(&entries);
        }
    }
}
