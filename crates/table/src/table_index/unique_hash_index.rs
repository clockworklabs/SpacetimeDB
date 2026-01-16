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

impl<K: KeySize + Eq + Hash> Index for UniqueHashIndex<K> {
    type Key = K;

    fn clone_structure(&self) -> Self {
        <_>::default()
    }

    fn insert(&mut self, key: Self::Key, ptr: RowPointer) -> Result<(), RowPointer> {
        match self.map.entry(key) {
            Entry::Vacant(e) => {
                self.num_key_bytes.add_to_key_bytes::<Self>(e.key());
                e.insert(ptr);
                Ok(())
            }
            Entry::Occupied(e) => Err(*e.into_mut()),
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
