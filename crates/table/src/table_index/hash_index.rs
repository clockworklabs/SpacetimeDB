use super::{
    multimap::MultiMapStatistics,
    same_key_entry::{same_key_iter, SameKeyEntry, SameKeyEntryIter},
    Index, KeySize,
};
use crate::indexes::RowPointer;
use core::hash::Hash;
use spacetimedb_data_structures::map::hash_map::EntryRef;
use spacetimedb_sats::memory_usage::MemoryUsage;

// Faster than ahash, so we use this explicitly.
use foldhash::fast::RandomState;
use hashbrown::HashMap;

/// A multi map that relates a `K` to a *set* of `RowPointer`s.
#[derive(Debug, PartialEq, Eq)]
pub struct HashIndex<K: Eq + Hash> {
    /// The map is backed by a `HashMap` for relating keys to values.
    ///
    /// A value set is stored as a `SmallVec`.
    /// This is an optimization over a `Vec<_>`
    /// as we allow a single element to be stored inline
    /// to improve performance for the common case of one element.
    map: HashMap<K, SameKeyEntry, RandomState>,
    /// Stats for `num_rows` and `num_key_bytes`.
    stats: MultiMapStatistics,
}

impl<K: Eq + Hash> Default for HashIndex<K> {
    fn default() -> Self {
        Self {
            map: <_>::default(),
            stats: <_>::default(),
        }
    }
}

impl<K: MemoryUsage + Eq + Hash> MemoryUsage for HashIndex<K> {
    fn heap_usage(&self) -> usize {
        let Self { map, stats } = self;
        map.heap_usage() + stats.heap_usage()
    }
}

// SAFETY: The implementations of all constructing
// and mutating methods uphold the invariant,
// assuming the caller requirements of the `unsafe` methods are upheld,
// that every `key -> ptr` pair only ever occurs once.
unsafe impl<K: KeySize + Eq + Hash> Index for HashIndex<K> {
    type Key = K;

    // =========================================================================
    // Construction
    // =========================================================================

    fn clone_structure(&self) -> Self {
        <_>::default()
    }

    // =========================================================================
    // Mutation
    // =========================================================================

    unsafe fn insert(&mut self, key: Self::Key, ptr: RowPointer) -> Result<(), RowPointer> {
        self.debug_ensure_key_ptr_not_included(&key, ptr);

        self.stats.add::<Self>(&key);
        let entry = self.map.entry(key).or_default();
        // SAFETY: caller promised that `(key, ptr)` does not exist in the index
        // so it also does not exist in `entry`.
        unsafe { entry.push(ptr) };
        Ok(())
    }

    unsafe fn merge_from(&mut self, src_index: Self, mut translate: impl FnMut(RowPointer) -> RowPointer) {
        // Merge `stats`.
        self.stats.merge_from(src_index.stats);

        // Move over the `key -> ptr` pairs
        // and translate `ptr`s from `src_index` to fit `self`.
        for (key, src) in src_index.map {
            let dst = self.map.entry(key).or_default();

            // SAFETY: Given `(key, ptr)` in `src_index`,
            // `(key, translate(ptr))` does not exist in `self`.
            // It follows that the `dst ∩ translate(src) = ∅`.
            unsafe { dst.merge_from(src, &mut translate) };
        }
    }

    fn delete(&mut self, key: &K, ptr: RowPointer) -> bool {
        let EntryRef::Occupied(mut entry) = self.map.entry_ref(key) else {
            return false;
        };

        let (deleted, is_empty) = entry.get_mut().delete(ptr);

        if deleted {
            self.stats.delete::<Self>(key);
        }

        if is_empty {
            entry.remove();
        }

        deleted
    }

    fn clear(&mut self) {
        // This will not deallocate the outer map.
        self.map.clear();
        self.stats.clear();
    }

    // =========================================================================
    // Querying
    // =========================================================================

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
        self.stats.num_key_bytes
    }

    fn num_rows(&self) -> usize {
        self.stats.num_rows
    }

    fn can_merge(&self, _: &Self, _: impl Fn(&RowPointer) -> bool) -> Result<(), RowPointer> {
        // `self.insert` always returns `Ok(_)`.
        Ok(())
    }
}
