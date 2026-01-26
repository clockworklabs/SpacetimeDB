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
pub enum SmallHashMap<K, V, const N: usize, const M: usize> {
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
