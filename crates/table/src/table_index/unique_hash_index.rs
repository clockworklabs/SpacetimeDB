use super::{Index, KeySize};
use crate::table_index::uniquemap::UniqueMapPointIter;
use crate::{indexes::RowPointer, table_index::key_size::KeyBytesStorage};
use core::hash::Hash;
use spacetimedb_data_structures::map::hash_map::Entry;
use spacetimedb_sats::memory_usage::MemoryUsage;

// Faster than ahash, so we use this explicitly.
use foldhash::fast::RandomState;
use hashbrown::HashMap;

/// A "unique map" that relates a `K` to a `RowPointer`
///
/// (This is just a `HashMap<K, RowPointer>`) with a slightly modified interface.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct UniqueHashIndex<K: KeySize + Eq + Hash> {
    /// The map is backed by a `HashMap` for relating a key to a value.
    map: HashMap<K, RowPointer, RandomState>,
    /// Storage for [`Index::num_key_bytes`].
    num_key_bytes: K::MemoStorage,
}

impl<K: KeySize + Eq + Hash> Default for UniqueHashIndex<K> {
    fn default() -> Self {
        Self {
            map: <_>::default(),
            num_key_bytes: <_>::default(),
        }
    }
}

impl<K: KeySize + Eq + Hash + MemoryUsage> MemoryUsage for UniqueHashIndex<K> {
    fn heap_usage(&self) -> usize {
        let Self { map, num_key_bytes } = self;
        map.heap_usage() + num_key_bytes.heap_usage()
    }
}

// SAFETY: The implementations of all constructing
// and mutating methods uphold the invariant,
// assuming the caller requirements of the `unsafe` methods are upheld,
// that every `key -> ptr` pair only ever occurs once.
// In fact, given that this is a unique index,
// this is statically guaranteed as one `key` only has one slot for one `ptr`.
unsafe impl<K: KeySize + Eq + Hash> Index for UniqueHashIndex<K> {
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
        match self.map.entry(key) {
            // `key` not found in index, let's add it.
            Entry::Vacant(e) => {
                self.num_key_bytes.add_to_key_bytes::<Self>(e.key());
                e.insert(ptr);
                Ok(())
            }
            // Unique constraint violation!
            Entry::Occupied(e) => Err(*e.into_mut()),
        }
    }

    unsafe fn merge_from(&mut self, src_index: Self, mut translate: impl FnMut(RowPointer) -> RowPointer) {
        // Merge `num_key_bytes`.
        self.num_key_bytes.merge(src_index.num_key_bytes);

        // Move over the `key -> ptr` pairs
        // and translate all row pointers in `src` to fit `self`.
        let src = src_index.map;
        self.map.reserve(src.len());
        for (key, src) in src {
            // SAFETY: Given `(key, ptr)` in `src`,
            // `(key, translate(ptr))` does not exist in `self`.
            // As each index is a unique index,
            // it follows that if `key` exist in `src`
            // it does not also exist in `self`.
            unsafe { self.map.insert_unique_unchecked(key, translate(src)) };
        }
    }

    fn delete(&mut self, key: &Self::Key, _: RowPointer) -> bool {
        let ret = self.map.remove(key).is_some();
        if ret {
            self.num_key_bytes.sub_from_key_bytes::<Self>(key);
        }
        ret
    }

    fn clear(&mut self) {
        self.map.clear();
        self.num_key_bytes.reset_to_zero();
    }

    // =========================================================================
    // Querying
    // =========================================================================

    fn can_merge(&self, other: &Self, ignore: impl Fn(&RowPointer) -> bool) -> Result<(), RowPointer> {
        let Some(found) = other
            .map
            .keys()
            .find_map(|key| self.map.get(key).filter(|val| !ignore(val)))
        else {
            return Ok(());
        };
        Err(*found)
    }

    fn num_keys(&self) -> usize {
        self.map.len()
    }

    fn num_key_bytes(&self) -> u64 {
        self.num_key_bytes.get(self)
    }

    type PointIter<'a>
        = UniqueMapPointIter<'a>
    where
        Self: 'a;

    fn seek_point(&self, point: &Self::Key) -> Self::PointIter<'_> {
        let iter = self.map.get(point).into_iter();
        UniqueMapPointIter { iter }
    }
}
